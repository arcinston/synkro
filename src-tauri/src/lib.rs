// src-tauri/src/main.rs

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod iroh_setup;
mod state;

use iroh_setup::setup;
use log::{error, info, warn, LevelFilter}; // Import warn
use state::AppState;
use std::time::Duration; // For timeouts
use tauri::{Emitter, Manager}; // Keep State import if used elsewhere, but not needed for shutdown state access this way
use commands::{get_node_info,send_file,list_files,get_share_ticket}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // --- Logger Setup --- (Recommended)
    let log_plugin = tauri_plugin_log::Builder::new()
        .level(LevelFilter::Info)
        .level_for("iroh", LevelFilter::Warn)
        .level_for("my_p2p_app", LevelFilter::Trace) // Adjust your app name/level
        .build();

    tauri::Builder::default()
        .plugin(log_plugin) // Add logger first
        .setup(|app| {
            let handle = app.handle().clone(); // Clone handle for async task

            #[cfg(debug_assertions)]
            {
                if let Some(window) = handle.get_webview_window("main") {
                    window.open_devtools();
                    info!("Opened dev tools");
                } else {
                    warn!("Could not get main window to open dev tools");
                }
            }

            // Spawn the async Iroh setup task
            tauri::async_runtime::spawn(async move {
                info!("Starting Iroh setup...");
                // Clone handle again for use inside this task
                let task_handle = handle.clone();
                match setup(task_handle.clone()).await {
                    // Pass cloned handle
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

                // --- FIX: Get AppHandle and clone window OUTSIDE the async block ---
                let app_handle = window.app_handle().clone(); // Get the AppHandle
                let window_clone = window.clone(); // Clone the window for closing later

                // Spawn a task for cleanup
                tauri::async_runtime::spawn(async move {
                    // Move app_handle and window_clone
                    // --- FIX: Get state using the AppHandle INSIDE the async block ---
                    let app_state = app_handle.state::<AppState>(); // State lifetime is now tied to handle within this task

                    info!("Attempting to lock state for shutdown...");
                    // Using lock() directly as try_lock() might fail unnecessarily if briefly contended
                    match app_state.0.lock().await {
                        mut state => {
                            // Successfully acquired the lock (lock() returns the guard directly)
                            info!("Acquired state lock for shutdown.");
                            // --- Success Case (Lock Acquired) ---

                            // 1. Abort sync task (if running - added later)
                            if let Some(handle) = state.sync_task_handle.take() {
                                info!("Aborting sync task...");
                                handle.abort();
                                match tokio::time::timeout(Duration::from_secs(2), handle).await {
                                    Ok(_) => info!("Sync task stopped gracefully."),
                                    Err(_) => {
                                        warn!("Sync task did not stop within timeout after abort.")
                                    }
                                }
                            }

                            // 2. Abort the Router task using its handle
                            if let Some(router) = state.router.take() {
                                info!("Aborting Iroh router task...");
                                router
                                    .shutdown()
                                    .await
                                    .expect("Error in shutting down router");

                                info!("Iroh router task aborted.");
                            } else {
                                info!("No active router task found.");
                            }

                            // 3. Close the Endpoint
                            if let Some(endpoint) = state.endpoint.take() {
                                info!("Closing Iroh endpoint...");
                                // endpoint.close() is async, so await it (potentially with timeout)
                                endpoint.close().await;
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
                            window_clone
                                .close()
                                .expect("Failed to close window after cleanup");
                        } // Mutex::lock returns the guard directly or panics if poisoned.
                          // If you want to handle poisoning explicitly, use try_lock() and match on the Result.
                          // For simplicity here, we assume poisoning leads to panic (default Tokio Mutex behavior).
                    }
                });
            }
            _ => {}
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, get_node_info,send_file,list_files,get_share_ticket])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
