// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

mod iroh_setup;
mod state;
use iroh_setup::setup;
use log::{error, info, warn};
use state::AppState;
use tauri::{Emitter, Manager, State}; // Import State

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            #[cfg(debug_assertions)] // only include this code on debug builds
            {
                // Use app handle to get window to avoid lifetime issues if needed elsewhere
                let window = handle.get_webview_window("main").unwrap();
                window.open_devtools();
            }

            // Spawn the async Iroh setup task
            tauri::async_runtime::spawn(async move {
                info!("Starting Iroh setup...");
                // Clone handle again for use inside this task
                let task_handle = handle.clone();
                match setup(task_handle.clone()).await { // Pass cloned handle
                    Ok(app_state) => {
                        // Setup succeeded, manage the state using the app handle
                        task_handle.manage(app_state); // <-- Manage the state HERE
                        info!("✅ Iroh setup successful, AppState managed.");
                        task_handle.emit("iroh_ready", ()).unwrap_or_else(|e| {
                            error!("Failed to emit iroh_ready event: {}", e);
                        });
                    }
                    Err(err) => {
                        error!("❌❌❌ Iroh setup failed: {:?}", err);
                        task_handle
                            .emit("iroh_setup_failed", format!("{}", err))
                            .unwrap_or_else(|e| {
                                error!("Failed to emit iroh_setup_failed event: {}", e);
                            });
                        // Consider exiting or disabling features if setup fails critically
                    }
                }
            });

            Ok(())
        })
        // --- Graceful Shutdown --- (Essential)
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                info!("Window close requested. Initiating graceful shutdown...");
                api.prevent_close(); // Prevent immediate closing

            let app_state: State<'_, AppState> = window.state::<AppState>();
            let window_clone = window.clone();

                // Spawn a task for cleanup
                tauri::async_runtime::spawn(async move {
                    // --- FIX: Get state using the moved AppHandle ---
                    // Access the state via the AppHandle inside the async task.
                    // The state object obtained this way has a lifetime tied to the AppHandle
                    // within this task, which is valid.

                    // Lock state for cleanup
                    // Use try_lock or handle potential poisoning more robustly if needed
                    // Using .lock().await which returns a Result
                    info!("Attempting to lock state for shutdown...");
                    match app_state.0.try_lock() {
                        Ok(mut state) => { // Successfully acquired the lock
                            info!("Acquired state lock for shutdown.");
                            // --- Success Case (Lock Acquired) ---

                            // 1. Abort sync task (if running - added later)
                            if let Some(handle) = state.sync_task_handle.take() {
                                info!("Aborting sync task...");
                                handle.abort();
                                // It's often good practice to await the handle after aborting
                                // to ensure the task has actually stopped, but be wary of hangs.
                                // Consider adding a timeout here if needed.
                                // Use tokio::select! or tokio::time::timeout
                                match tokio::time::timeout(std::time::Duration::from_secs(2), handle).await {
                                    Ok(_) => info!("Sync task stopped gracefully."),
                                    Err(_) => warn!("Sync task did not stop within timeout after abort."),
                                }
                            }

                            // 2. Abort the Router task using its handle
                            if let Some(router) = state.router.take() {
                                info!("Shutting down Iroh router...");
                                // Assuming shutdown is synchronous or returns quickly.
                                // If it's async, you might need to await it (potentially with timeout).
                                router.shutdown();
                                info!("Iroh router shutdown initiated.");
                                // If router.shutdown() is async:
                                // match tokio::time::timeout(Duration::from_secs(5), router.shutdown()).await { ... }
                            } else {
                                info!("No active router task found.");
                            }

                            // 3. Close the Endpoint
                            if let Some(endpoint) = state.endpoint.take() {
                                endpoint.close();

                            } else {
                                info!("No active Iroh endpoint found to close.");
                            }

                            // Explicitly clear handlers (optional, depends on their Drop impl)
                            state.docs = None;
                            state.gossip = None;
                            state.blobs = None;

                            drop(state); // Release lock before closing window

                            info!("Cleanup finished. Closing window.");
                            // Use the cloned window handle to close
                            window_clone.close().expect("Failed to close window after cleanup");

                        }
                        Err(poisoned) => {
                             // --- Error Case (Failed to Lock Mutex, likely poisoned) ---
                            error!(
                                "Failed to lock state for shutdown (mutex poisoned: {}). Forcing close.",
                                poisoned
                            );
                            // Still try to close the window
                            window_clone.close().expect("Failed to close window after lock failure");
                        }
                    }
                });
            }
            _ => {}
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
