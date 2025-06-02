use std::path::PathBuf;

use crate::{
    errors::CommandError, // Added
    fs_watcher,
    iroh_fns::setup,
    // state::AppState,
};
// use anyhow::Error; // Replaced by CommandError
use log::{error, info};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

// Handle incoming events
#[tauri::command]
pub async fn setup_iroh_and_fs(app: AppHandle) -> Result<(), CommandError> {
    handle_setup(app).await?;
    Ok(())
}

pub async fn handle_setup(handle: AppHandle) -> Result<(), CommandError> {
    let store_plugin = handle.store("store.json").map_err(CommandError::StoreError)?;
    let path_to_watch_value = store_plugin
        .get("sync-folder-path")
        .ok_or_else(|| CommandError::StoreError("sync-folder-path not found in store".to_string()))?;

    let path_to_watch_str = path_to_watch_value.as_str().ok_or_else(|| {
        CommandError::InitializationError("sync-folder-path is not a valid string".to_string())
    })?;

    let path_to_watch = PathBuf::from(path_to_watch_str);
    // No need for path_to_watch_clone if path_to_watch is cloned where needed in spawns

    // store_plugin.close_resource(); // close_resource is not a method on Store

    // Spawn the async Iroh setup task
    let iroh_handle_clone = handle.clone();
    let path_for_iroh_setup = path_to_watch.clone();
    tauri::async_runtime::spawn(async move {
        info!("Starting Iroh setup...");
        // Assuming setup will return Result<_, IrohError>
        // Errors in spawned tasks are logged, not returned to the command caller directly.
        match setup(iroh_handle_clone, path_for_iroh_setup).await {
            Ok(()) => {
                info!("Iroh Setup successful");
            }
            Err(err) => {
                // If setup returns IrohError, it would be more specific.
                // For now, assume it's converted to string for logging.
                error!("❌❌❌ Iroh setup failed: {:?}", err.to_string());
            }
        }
    });

    // --- Spawn Filesystem Watcher Task ---
    let fs_handle_clone = handle.clone();
    let path_for_fs_watcher = path_to_watch.clone();
    tauri::async_runtime::spawn(async move {
        info!("Starting Filesystem Watcher setup...");

        if !path_for_fs_watcher.exists() {
            info!("Creating watch directory: {:?}", path_for_fs_watcher);
            if let Err(e) = std::fs::create_dir_all(&path_for_fs_watcher) {
                // This error is within a spawned task, so we log it.
                // It doesn't propagate to the CommandError of handle_setup.
                error!(
                    "Failed to create watch directory {:?}: {}",
                    path_for_fs_watcher, e
                );
                return;
            }
        }

        info!("Attempting to watch: {:?}", path_for_fs_watcher);

        // fs_watcher::start_watching returns anyhow::Result
        match fs_watcher::start_watching(path_for_fs_watcher.clone()) {
            Ok(receiver) => {
                // fs_watcher::handle_watcher is a sync function, runs in this spawned thread.
                fs_watcher::handle_watcher(path_for_fs_watcher, fs_handle_clone, receiver);
            }
            Err(err) => {
                // Log error from starting the watcher.
                error!(
                    "❌❌❌ Failed to start filesystem watcher for path {:?}: {:?}",
                    path_for_fs_watcher, err
                );
            }
        }
    });

    Ok(())
}
