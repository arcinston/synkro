// src-tauri/src/commands.rs

use std::path::PathBuf;

use crate::{
    iroh_fns::{
        create_iroh_gossip_ticket,
        create_iroh_ticket,
        get_iroh_blob,
        join_iroh_gossip,
        subscribe_loop, // Make sure this is correctly imported
    },
    state::AppState, // Removed GossipState as gossip_sender is in AppState
};
use iroh::PublicKey;
// use iroh_gossip::net::GossipReceiver; // Not directly used here anymore
use log::{error, info}; // Added error
                        // Import necessary types for blobs and docs interaction
use serde::Serialize;
use tauri::{AppHandle, State}; // Added Manager

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

// --- New Gossip Event Payload (can also be in iroh_fns.rs) ---
#[derive(Clone, Serialize)]
pub struct GossipEventPayload {
    // Made public if subscribe_loop is in another module and needs this
    from: String,
    topic: String,
    content_base64: String, // Representing Vec<u8> as base64 for JSON compatibility
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
    let endpoint = state
        .endpoint
        .clone()
        .ok_or_else(|| "Endpoint not initialized".to_string())?;

    let str_gossip_ticket = create_iroh_gossip_ticket(endpoint)
        .await
        .map_err(|e| format!("Endpoint not initialized {}", e))?;

    Ok(str_gossip_ticket)
}

#[tauri::command]
pub async fn join_gossip(
    app_handle: AppHandle,          // Keep AppHandle
    app_state: State<'_, AppState>, // Renamed from `state` to `app_state` for clarity
    str_gossip_ticket: String,
) -> Result<bool, String> {
    let endpoint = app_state
        .endpoint
        .clone()
        .ok_or_else(|| "Endpoint not initialized".to_string())?;
    let gossip = app_state
        .gossip
        .clone()
        .ok_or_else(|| "Gossip not initialized".to_string())?;

    let (sender, receiver) = join_iroh_gossip(endpoint, gossip, str_gossip_ticket) // `receiver` is `mut` if `subscribe_loop` needs mutable access, but it takes ownership.
        .await
        .map_err(|e| format!("Failed to create Gossip Sender and Receiver: {}", e))?;

    // Correctly store the sender in AppState's Mutex<Option<GossipSender>>
    let mut gossip_sender_guard = app_state.gossip_sender.lock().await;
    *gossip_sender_guard = Some(sender);
    // Drop the guard explicitly if desired, or let it drop at the end of the scope.
    // drop(gossip_sender_guard);

    // Spawn a task to handle incoming gossip messages
    // Pass the AppHandle to subscribe_loop so it can emit events
    let receiver_app_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        info!("Gossip receiver task (subscribe_loop) started.");
        // Assuming subscribe_loop now takes AppHandle and GossipReceiver
        if let Err(e) = subscribe_loop(receiver_app_handle, receiver).await {
            error!("Error in subscribe_loop: {:?}", e);
        }
        info!("Gossip receiver task (subscribe_loop) finished.");
    });

    Ok(true)
}

// Handle incoming events
