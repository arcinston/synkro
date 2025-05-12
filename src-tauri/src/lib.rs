// src-tauri/src/main.rs

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod fs_watcher;
mod iroh_fns;
mod state;

use commands::{
    create_gossip_ticket, create_ticket, get_blob, get_node_info, join_gossip, setup_iroh_and_fs,
};
use log::LevelFilter;

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
        .setup(|_| {
            // let handle = app.handle().clone(); // Clone handle for async task

            // #[cfg(debug_assertions)]
            // {
            //     if let Some(window) = handle.get_webview_window("main") {
            //         window.open_devtools();
            //         info!("Opened dev tools");
            //     } else {
            //         warn!("Could not get main window to open dev tools");
            //     }
            // }

            Ok(())
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            setup_iroh_and_fs,
            get_blob,
            create_ticket,
            create_gossip_ticket,
            join_gossip,
            get_node_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
