// src/fs_watcher.rs

use anyhow::Result;
use log::{error, info, warn};
use notify::{
    event::{ModifyKind, RenameMode},
    Config, Error, Event, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver}, // Use standard library channels
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter};

use crate::iroh_fns::handle_fs_payload;

// Define a type alias for the events we'll send over the channel
// We send the whole Result to propagate potential watcher errors
pub type FileEventResult = NotifyResult<Event>;
pub type FileEventReceiver = Receiver<FileEventResult>;

#[derive(Clone, Serialize, Debug)]
pub enum FsEventType {
    Create,
    Modify,
    Remove,
    Error,
    Other,
}

// Define a simple serializable struct for the event payload
#[derive(Clone, Serialize, Debug)]
pub struct FsEventPayload {
    pub event_type: FsEventType, // e.g., "Create", "Modify", "Remove", "Error", "Other"
    pub path: PathBuf,           // Paths affected, converted to strings
}

/// Starts watching a directory recursively in a separate thread.
///
/// Returns a channel receiver to get filesystem events or errors.
pub fn start_watching(path_to_watch: PathBuf) -> Result<FileEventReceiver> {
    // Create a channel for communication
    let (tx, rx) = mpsc::channel();

    // --- Watcher Thread ---
    // Spawn a new thread to handle the filesystem watching.
    // Move ownership of the sender `tx` and `path_to_watch` into the thread.
    thread::spawn(move || {
        println!(
            "[FS Watcher] Watcher thread started for path: {:?}",
            path_to_watch
        );

        // Define the event handler closure.
        // It clones the sender and sends received events/errors through the channel.
        let handler = {
            let tx = tx.clone(); // Clone sender for the closure
            move |res: NotifyResult<Event>| {
                if tx.send(res).is_err() {
                    // If sending fails, the receiver has been dropped.
                    // The watcher thread can probably exit gracefully.
                    eprintln!("[FS Watcher] Receiver dropped. Watcher thread may stop.");
                    // We could add logic here to signal the watcher loop to stop,
                    // but often the watcher itself will error out or stop if the
                    // handler fails repeatedly or under certain conditions.
                }
            }
        };

        // --- The core watcher logic ---
        // This inner function helps manage the watcher's lifetime and error handling.
        let run_watcher = || -> Result<()> {
            // Create a new RecommendedWatcher. RecommendedWatcher automatically
            // selects the best backend available for the OS.
            let mut watcher = RecommendedWatcher::new(handler, Config::default())?;

            // Add the path to the watcher. Watch recursively.
            watcher.watch(&path_to_watch, RecursiveMode::Recursive)?;

            println!(
                "[FS Watcher] Successfully watching {:?} recursively.",
                path_to_watch
            );

            // Keep the watcher alive. The watcher runs in the background.
            // This thread just needs to stay alive to keep the `watcher` instance
            // in scope. A simple loop suffices. Add a sleep to prevent busy-waiting
            // if the underlying watcher mechanism doesn't block.
            // You might add a shutdown signal check here in a real app.
            loop {
                thread::sleep(Duration::from_secs(5));
                // In a real app, you might check an AtomicBool or another channel
                // here to see if shutdown has been requested.
            }
        }; // End of run_watcher closure

        // Execute the watcher logic. If it errors out, log it.
        if let Err(e) = run_watcher() {
            eprintln!("[FS Watcher] Watcher thread encountered an error: {:?}", e);
            // Optionally send the final error over the channel if needed
            // tx.send(Err(notify::Error::generic(format!("Watcher failed: {}", e)))).ok();
        }

        println!(
            "[FS Watcher] Watcher thread exiting for path: {:?}",
            path_to_watch
        );
    }); // End of thread::spawn

    // Return the receiver end of the channel to the caller
    Ok(rx)
}

pub fn handle_watcher(
    path_to_watch: PathBuf,
    fs_handle: AppHandle,
    receiver: Receiver<Result<Event, Error>>,
) {
    info!(
        "Filesystem watcher started successfully for {:?}",
        path_to_watch
    );

    // This task will now process events from the receiver channel.
    // We use spawn_blocking because receiver.recv() is blocking.
    let blocking_task_handle = fs_handle.clone(); // Clone handle for spawn_blocking
    tokio::task::spawn_blocking(move || {
        info!("FS Event processing loop started.");
        loop {
            match receiver.recv() {
                Ok(event_result) => {
                    // Process the received event or error
                    let payload = match event_result {
                        Ok(event) => {
                            info!(
                                "FS Event Received: Kind: {:?}, Paths: {:?}",
                                event.kind, event.paths
                            );

                            // Get the first path, if any. Handle empty paths gracefully.
                            // Some events (like AccessMode::Close) might not have paths.
                            let path = event.paths.get(0).cloned().unwrap_or_else(PathBuf::new);

                            // Determine FsEventType based on notify::EventKind
                            let event_type = match event.kind {
                                notify::EventKind::Create(_) => FsEventType::Create,
                                notify::EventKind::Remove(_) => FsEventType::Remove,
                                notify::EventKind::Modify(kind) => {
                                    match kind {
                                        ModifyKind::Data(_) => FsEventType::Modify, // File content changed
                                        ModifyKind::Metadata(_) => FsEventType::Modify, // Metadata changed
                                        ModifyKind::Name(rename_mode) => {
                                            // Handle different rename scenarios
                                            match rename_mode {
                                                RenameMode::To => FsEventType::Create, // Renamed *to* this path (appeared)
                                                RenameMode::From => FsEventType::Remove, // Renamed *from* this path (disappeared)
                                                RenameMode::Both => FsEventType::Modify, // Renamed within watched dir (path changes content/identity)
                                                RenameMode::Any => {
                                                    // Often used for create/delete on some backends
                                                    if path.exists() {
                                                        info!("-> State Change Create: {:?} appeared ", path);
                                                        FsEventType::Create
                                                    } else {
                                                        info!("-> State Change Remove: {:?} disappeared (Treat as Remove)", path);
                                                        FsEventType::Remove
                                                    }
                                                }
                                                RenameMode::Other => FsEventType::Other, // Unknown rename type
                                            }
                                        }
                                        ModifyKind::Any => FsEventType::Modify, // Generic modify event
                                        ModifyKind::Other => FsEventType::Other, // Unknown modify type
                                    }
                                }
                                notify::EventKind::Access(_) => {
                                    // Access events are often noisy and might not signify a change
                                    // relevant to the frontend. Map to Other or ignore.
                                    FsEventType::Other
                                }
                                notify::EventKind::Other => FsEventType::Other, // Explicitly Other kind from notify
                                // Use a wildcard arm to catch any future EventKind variants
                                _ => {
                                    warn!("Unhandled FS Event Kind: {:?}", event.kind);
                                    FsEventType::Other
                                }
                            };

                            // Construct the payload to send to the frontend
                            let payload = FsEventPayload { event_type, path };
                            info!("Payload Emitted {:?}", payload);

                            payload
                        }
                        Err(err) => {
                            // Handle errors from the notify watcher itself
                            warn!("FS Watcher Error: {:?}", err);
                            FsEventPayload {
                                event_type: FsEventType::Error,
                                path: PathBuf::new(), // No specific path for a watcher error
                            }
                        }
                    };
                    // handle iroh jobs to be performed based on the
                    handle_fs_payload(payload.clone(), blocking_task_handle.clone());
                    // Emit event to frontend
                    if let Err(e) = blocking_task_handle.emit("fs-event", payload) {
                        error!("Failed to emit Tauri event 'fs-event': {}", e);
                    }
                }
                Err(recv_error) => {
                    error!(
                        "FS Watcher channel error: {}. Watcher thread likely stopped.",
                        recv_error
                    );
                    // Emit a final error event?
                    let payload = FsEventPayload {
                        event_type: FsEventType::Other, // Or perhaps a specific Error type?
                        path: PathBuf::new(),
                    };
                    blocking_task_handle.emit("fs-event", payload).ok(); // Best effort emit
                    break; // Exit the loop
                }
            } // <-- Added missing semicolon
        }
        info!("FS Event processing loop finished.");
    }); // <-- Added missing semicolon
}
