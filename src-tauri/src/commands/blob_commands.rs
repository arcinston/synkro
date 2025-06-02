use std::path::PathBuf;

use crate::{
    errors::CommandError, // Added
    iroh_fns::{create_iroh_ticket, get_iroh_blob},
    state::AppState,
};
use serde::Serialize;
use tauri::State;

#[derive(Clone, Serialize, Debug)]
pub struct FileEntryInfo {
    filename: String,
    hash: String,
    size: u64,
}

#[derive(Clone, Serialize, Debug)]
pub struct ShareTicketResponse {
    ticket: String,
}

#[tauri::command]
pub async fn get_blob(
    state: State<'_, AppState>,
    str_ticket: String,
    str_dest_path: String,
) -> Result<(), CommandError> {
    let blobs = state
        .blobs
        .clone()
        .ok_or_else(|| CommandError::IrohClientNotInitialized("blobs client".to_string()))?;
    let dest_path = PathBuf::from(str_dest_path);
    // Assuming get_iroh_blob will be updated to return Result<_, IrohError>
    // which can be converted to CommandError via From trait
    get_iroh_blob(blobs, str_ticket, dest_path).await?;

    Ok(())
}

#[tauri::command]
pub async fn create_ticket(state: State<'_, AppState>, filepath: String) -> Result<String, CommandError> {
    let path = PathBuf::from(filepath);
    if !path.exists() {
        return Err(CommandError::PathError(format!("File does not exist: {}", path.display())));
    }

    let blobs = state
        .blobs
        .clone()
        .ok_or_else(|| CommandError::IrohClientNotInitialized("blobs client".to_string()))?;

    let endpoint = state
        .endpoint
        .clone()
        .ok_or_else(|| CommandError::IrohClientNotInitialized("endpoint".to_string()))?;

    // Assuming create_iroh_ticket will be updated to return Result<_, IrohError>
    let str_ticket = create_iroh_ticket(blobs, endpoint, path).await?;

    Ok(str_ticket)
}
