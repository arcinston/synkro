// src-tauri/src/commands.rs

use std::{path::PathBuf, str::FromStr};

use crate::{
    fs_watcher,
    iroh_fns::{
        create_iroh_gossip_ticket,
        create_iroh_ticket,
        get_iroh_blob,
        join_iroh_gossip,
        setup,
        subscribe_loop,
        GossipTicket, // Make sure this is correctly imported
    },
    state::AppState, // Removed GossipState as gossip_sender is in AppState
};
use anyhow::Error;
use iroh::{NodeId, PublicKey};
use iroh_gossip::proto::TopicId;
// use iroh_gossip::net::GossipReceiver; // Not directly used here anymore
use log::{error, info}; // Added error
                        // Import necessary types for blobs and docs interaction
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_store::StoreExt; // Added Manager

// --- Frontend Event Payloads --- (Keep existing ones)

// #[derive(Clone, Serialize)]
// struct UploadProgress {
//     file_path: String,
//     #[serde(rename = "type")]
//     event_type: String, // "found", "progress", "done", "error"
//     size: Option<u64>,
//     offset: Option<u64>,
//     error: Option<String>,
// }

// #[derive(Clone, Serialize)]
// struct DownloadProgress {
//     filename: String,
//     #[serde(rename = "type")]
//     event_type: String, // "started", "progress", "complete", "error"
//     size: Option<u64>,
//     offset: Option<u64>,
//     error: Option<String>,
//     download_path: Option<String>,
// }

// --- New Gossip Event Payload (can also be in iroh_fns.rs) ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEventPayload {
    // Made public if subscribe_loop is in another module and needs this
    pub from: NodeId,
    pub topic: TopicId,
    pub file_name: String,
    pub relative_path: String,
    pub message_content: String,
}
impl GossipEventPayload {
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }
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
pub async fn get_blob(
    state: State<'_, AppState>,
    str_ticket: String,
    str_dest_path: String,
) -> Result<(), String> {
    let blobs = state
        .blobs
        .clone()
        .ok_or_else(|| "Iroh blobs client not initialized".to_string())?;
    let dest_path = PathBuf::from(str_dest_path);
    get_iroh_blob(blobs, str_ticket, dest_path)
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
pub async fn create_gossip_ticket(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let endpoint = state
        .endpoint
        .clone()
        .ok_or_else(|| "Endpoint not initialized".to_string())?;

    let store = app.store("store.json").map_err(|e| e.to_string())?;

    let topic_id: TopicId = match store.get("topic-id") {
        Some(json_value) => {
            // Attempt to deserialize the JsonValue to TopicId
            match serde_json::from_value::<TopicId>(json_value.clone()) {
                Ok(id) => {
                    info!("Found existing topic-id in store: {:?}", id);
                    id
                }
                Err(e) => {
                    error!("Failed to deserialize stored topic-id ({}). Generating and storing a new one.", e);
                    let new_topic_id = TopicId::from_bytes(rand::random());
                    info!("Generated and stored new topic-id: {:?}", new_topic_id);
                    new_topic_id
                }
            }
        }
        None => {
            // TopicId not found in store, generate a new one, store it, and use it.
            info!("No topic-id found in store. Generating and storing a new one.");
            let new_topic_id = TopicId::from_bytes(rand::random());
            new_topic_id
        }
    };

    store.set("topic-id", topic_id.clone().to_string());
    store.save().map_err(|e| e.to_string())?;
    store.close_resource();

    let str_gossip_ticket = create_iroh_gossip_ticket(endpoint, topic_id)
        .await
        .map_err(|e| format!("Endpoint not initialized {}", e))?;

    Ok(str_gossip_ticket)
}

#[tauri::command]
pub async fn join_gossip(
    app_handle: AppHandle,
    app_state: State<'_, AppState>,
    str_gossip_ticket: String,
) -> Result<bool, String> {
    info!("join_gossip command started.");

    let endpoint = app_state
        .endpoint
        .clone()
        .ok_or_else(|| "Endpoint not initialized".to_string())?;
    info!("Endpoint obtained.");

    let gossip = app_state
        .gossip
        .clone()
        .ok_or_else(|| "Gossip not initialized".to_string())?;
    info!("Gossip handler obtained.");

    let GossipTicket { topic, nodes: _ } =
        GossipTicket::from_str(&str_gossip_ticket).map_err(|e| {
            format!(
                " Failed to Parse Gossip ticket {}: {}",
                str_gossip_ticket, e
            )
        })?;
    info!("Gossip ticket parsed, topic: {:?}", topic);
    let store = app_handle.store("store.json").map_err(|e| e.to_string())?;
    store.set("topic-id", topic.clone().to_string());
    store.save().map_err(|e| e.to_string())?;
    store.close_resource();

    // Scope the lock for gossip_topic to release it before the await
    {
        info!("Attempting to lock gossip_topic in AppState.");
        let mut gossip_topic_guard = app_state.gossip_topic.lock().await;
        *gossip_topic_guard = Some(topic.clone()); // Clone topic for logging/potential reuse, ensure original moves
        info!("gossip_topic in AppState set and lock released.");
    } // gossip_topic_guard is dropped here, and the lock is released.

    info!("Calling join_iroh_gossip (iroh_fns.rs)...");
    let (sender, receiver) =
        match join_iroh_gossip(endpoint, gossip, str_gossip_ticket.clone()).await {
            Ok(res) => {
                info!("join_iroh_gossip (iroh_fns.rs) successful.");
                res
            }
            Err(e) => {
                error!("join_iroh_gossip (iroh_fns.rs) failed: {}", e);
                return Err(format!(
                    "Failed to create Gossip Sender and Receiver: {}",
                    e
                ));
            }
        };

    // Correctly store the sender in AppState's Mutex<Option<GossipSender>>
    {
        info!("Attempting to lock gossip_sender in AppState.");
        let mut gossip_sender_guard = app_state.gossip_sender.lock().await;
        *gossip_sender_guard = Some(sender);
        info!("gossip_sender in AppState set and lock released.");
    }

    // Spawn a task to handle incoming gossip messages
    // Pass the AppHandle to subscribe_loop so it can emit events
    let receiver_app_handle = app_handle.clone();
    let blobs = app_state
        .blobs
        .clone()
        .ok_or_else(|| "Iroh blobs client not initialized".to_string())?;
    let sync_path = app_state.sync_folder.clone();
    tauri::async_runtime::spawn(async move {
        info!("Gossip receiver task (subscribe_loop) started.");
        // Assuming subscribe_loop now takes AppHandle and GossipReceiver
        if let Err(e) = subscribe_loop(receiver_app_handle, blobs, sync_path, receiver).await {
            error!("Error in subscribe_loop: {:?}", e);
        }
        info!("Gossip receiver task (subscribe_loop) finished.");
    });
    info!("subscribe_loop task spawned.");

    // Emit an event to the frontend indicating that gossip is ready
    app_handle
        .emit("gossip-ready", ())
        .map_err(|e| format!("Failed to emit gossip-ready event: {}", e))?;
    info!("Emitted gossip-ready event to frontend.");

    Ok(true)
}

// Handle incoming events
#[tauri::command]
pub async fn setup_iroh_and_fs(app: AppHandle) -> Result<(), String> {
    handle_setup(app)
        .await
        .map_err(|e| format!("Iroh & Fs Setup failed {}", e))?;

    Ok(())
}

pub async fn handle_setup(handle: AppHandle) -> Result<(), Error> {
    let store = handle.store("store.json")?;
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
}
