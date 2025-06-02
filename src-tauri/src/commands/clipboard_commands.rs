use tauri::{AppHandle, command, Runtime}; // Removed State as it's not used directly in these commands
use tauri_plugin_store::StoreExt;
use crate::errors::CommandError; // Your custom error type
use log::info;

const CLIPBOARD_SHARING_KEY: &str = "clipboard_sharing_enabled";

#[command]
pub async fn enable_clipboard_sharing<R: Runtime>(app_handle: AppHandle<R>) -> Result<(), CommandError> {
    // Access the store plugin
    let store_plugin = app_handle.store("store.json").map_err(CommandError::StoreError)?;

    // Set the value
    store_plugin.set(CLIPBOARD_SHARING_KEY, serde_json::Value::Bool(true))
        .map_err(CommandError::StoreError)?;

    // Save the store to persist changes
    store_plugin.save().map_err(CommandError::StoreError)?;

    info!("Clipboard sharing enabled.");
    Ok(())
}

#[command]
pub async fn disable_clipboard_sharing<R: Runtime>(app_handle: AppHandle<R>) -> Result<(), CommandError> {
    let store_plugin = app_handle.store("store.json").map_err(CommandError::StoreError)?;
    store_plugin.set(CLIPBOARD_SHARING_KEY, serde_json::Value::Bool(false))
        .map_err(CommandError::StoreError)?;
    store_plugin.save().map_err(CommandError::StoreError)?;
    info!("Clipboard sharing disabled.");
    Ok(())
}

#[command]
pub async fn is_clipboard_sharing_enabled<R: Runtime>(app_handle: AppHandle<R>) -> Result<bool, CommandError> {
    let store_plugin = app_handle.store("store.json").map_err(CommandError::StoreError)?;
    let is_enabled = store_plugin
        .get(CLIPBOARD_SHARING_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false); // Default to false if not set or not a boolean
    Ok(is_enabled)
}
