// src/fs_watcher.rs

use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver}, // Use standard library channels
    thread,
    time::Duration,
};

// Define a type alias for the events we'll send over the channel
// We send the whole Result to propagate potential watcher errors
pub type FileEventResult = NotifyResult<Event>;
pub type FileEventReceiver = Receiver<FileEventResult>;

// Define a simple serializable struct for the event payload
#[derive(Clone, Serialize)]
pub struct FsEventPayload {
    pub event_type: String, // e.g., "Create", "Modify", "Remove", "Error", "Other"
    pub paths: Vec<String>, // Paths affected, converted to strings
    pub message: Option<String>, // Optional extra info or error message
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
