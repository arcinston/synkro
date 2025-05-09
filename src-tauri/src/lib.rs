// src-tauri/src/main.rs

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod fs_watcher;
mod iroh_fns;
mod state;

use std::path::PathBuf;

use commands::{create_gossip_ticket, create_ticket, get_blob, get_node_info, join_gossip};
use iroh_fns::setup;
use log::{error, info, warn, LevelFilter};

use tauri::Manager;
use tauri_plugin_store::StoreExt;

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

            let store = app.store("store.json")?;
            let path_to_watch_str = store
                .get("sync-folder-path")
                .expect("Failed to get path to watch from store");

            let path_to_watch_str = path_to_watch_str.as_str().unwrap();
            let path_to_watch = PathBuf::from(path_to_watch_str);
            let path_to_watch_clone = PathBuf::from(path_to_watch_str);

            // Remove the store from the resource table
            store.close_resource();

            // Spawn the async Iroh setup task
            let iroh_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                info!("Starting Iroh setup...");
                // Clone handle again for use inside this task
                match setup(iroh_handle.clone(), path_to_watch_clone).await {
                    // Pass cloned handle
                    Ok(()) => {
                        info!("Iroh Setup successful")
                    }
                    Err(err) => {
                        error!("❌❌❌ Iroh setup failed: {:?}", err);
                    }
                }
            });

            // --- Spawn Filesystem Watcher Task ---
            let fs_handle = handle.clone(); // Clone handle for FS Watcher task
            tauri::async_runtime::spawn(async move {
                info!("Starting Filesystem Watcher setup...");

                // Determine path to watch (e.g., Documents directory)
                // In a real app, get this from config or user selection

                // Ensure the directory exists (create if it doesn't)
                if !path_to_watch.exists() {
                    info!("Creating watch directory: {:?}", path_to_watch);
                    if let Err(e) = std::fs::create_dir_all(&path_to_watch) {
                        error!(
                            "Failed to create watch directory {:?}: {}",
                            path_to_watch, e
                        );
                        return; // Cannot proceed without the directory
                    }
                }

                info!("Attempting to watch: {:?}", path_to_watch);

                // Start the watcher (which runs in its own std::thread)
                match fs_watcher::start_watching(path_to_watch.clone()) {
                    Ok(receiver) => {
                        fs_watcher::handle_watcher(path_to_watch, fs_handle, receiver);
                    }
                    Err(err) => {
                        error!(
                            "❌❌❌ Failed to start filesystem watcher for path {:?}: {:?}",
                            path_to_watch, err
                        );
                    }
                }
            }); // End of FS Watcher spawn

            Ok(())
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_blob,
            create_ticket,
            create_gossip_ticket,
            join_gossip,
            get_node_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
