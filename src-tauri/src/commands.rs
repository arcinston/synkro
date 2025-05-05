// src-tauri/src/commands.rs

use std::path::PathBuf;

use crate::state::AppState; // Import the AppState wrapper
use iroh::PublicKey;
// Import necessary types for blobs and docs interaction
use iroh_blobs::{rpc::client::blobs::WrapOption, ticket::BlobTicket, util::SetTagOption};
use log::info;
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

    let blobs_client = blobs.client();

    let ticket: BlobTicket = str_ticket
        .parse()
        .map_err(|e| format!("Failed to parse blob ticket: {}", e))?;

    let download_req = blobs_client
        .download(ticket.hash(), ticket.node_addr().clone())
        .await
        .map_err(|e| format!("Failed to initiate blob download: {}", e))?;

    download_req
        .finish()
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

    let blobs_client = blobs.client();

    let add_progress = blobs_client
        .add_from_path(path, true, SetTagOption::Auto, WrapOption::NoWrap)
        .await
        .map_err(|e| format!("Failed to find Blob from path {}", e))?;

    let blob = add_progress
        .finish()
        .await
        .map_err(|e| format!("Failed to add Blob from path {}", e))?;

    let endpoint = state
        .endpoint
        .clone()
        .ok_or_else(|| "Endpoint not initialized".to_string())?;

    let node_id = endpoint.node_id();

    let ticket = BlobTicket::new(node_id.into(), blob.hash, blob.format)
        .map_err(|e| format!("Error creating Ticket for the blob {}", e))?;

    let str_ticket = ticket.to_string();
    info!("created str ticket for ticket {}", str_ticket);

    Ok(str_ticket)
}
