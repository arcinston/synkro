// src-tauri/src/commands.rs

use std::path::PathBuf;

use crate::{
    iroh_fns::{create_iroh_ticket, get_iroh_blob},
    state::AppState,
}; // Import the AppState wrapper
use iroh::PublicKey;
// Import necessary types for blobs and docs interaction
use iroh_blobs::ticket::BlobTicket;
use serde::Serialize;
use tauri::State;

// --- Frontend Event Payloads --- (Keep existing ones)

#[derive(Clone, Serialize)]
struct UploadProgress {
    file_path: String,
    #[serde(rename = "type")]
    event_type: String, // "found", "progress", "done", "error"
    size: Option<u64>,
    offset: Option<u64>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    filename: String,
    #[serde(rename = "type")]
    event_type: String, // "started", "progress", "complete", "error"
    size: Option<u64>,
    offset: Option<u64>,
    error: Option<String>,
    download_path: Option<String>,
}

// --- Command-Specific Structs --- (Keep existing ones)

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

#[derive(Clone, Serialize, Debug)]
pub struct NodeInfo {
    node_id: Option<PublicKey>,
}

#[tauri::command]
pub async fn get_node_info(state: State<'_, AppState>) -> Result<NodeInfo, String> {
    let endpoint = state.endpoint.clone();

    let node_id = match endpoint {
        Some(e) => Some(e.node_id()),
        None => None,
    };

    Ok(NodeInfo { node_id })
}

#[tauri::command]
pub async fn get_blob(state: State<'_, AppState>, str_ticket: String) -> Result<(), String> {
    let blobs = state
        .blobs
        .clone()
        .ok_or_else(|| "Iroh blobs client not initialized".to_string())?;

    get_iroh_blob(blobs, str_ticket)
        .await
        .map_err(|e| format!("Failed to complete blob download: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn create_ticket(state: State<'_, AppState>, filepath: String) -> Result<String, String> {
    let path: PathBuf = PathBuf::from(filepath);

    let blobs = state
        .blobs
        .clone()
        .ok_or_else(|| "Iroh blobs client not initialized".to_string())?;

    let endpoint = state
        .endpoint
        .clone()
        .ok_or_else(|| "Endpoint not initialized".to_string())?;

    let str_ticket = create_iroh_ticket(blobs, endpoint, path)
        .await
        .map_err(|e| format!("Endpoint not initialized {}", e))?;

    Ok(str_ticket)
}

#[tauri::command]
pub async fn create_gossip_ticket(state: State<'_, AppState>) -> Result<String, String> {
    let path: PathBuf = PathBuf::from(filepath);

    let gossip = state
        .gossip
        .clone()
        .ok_or_else(|| "Iroh blobs client not initialized".to_string())?;

    let endpoint = state
        .endpoint
        .clone()
        .ok_or_else(|| "Endpoint not initialized".to_string())?;

    let str_gossip_ticket = create_iroh_gossip_ticket(gossip, endpoint, path)
        .await
        .map_err(|e| format!("Endpoint not initialized {}", e))?;

    Ok(str_gossip_ticket)
}
