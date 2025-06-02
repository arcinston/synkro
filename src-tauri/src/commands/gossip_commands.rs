use std::str::FromStr;

use crate::{
    errors::CommandError, // Added
    iroh_fns::{create_iroh_gossip_ticket, join_iroh_gossip, subscribe_loop, GossipTicket},
    state::AppState,
};
use iroh::{NodeId, PublicKey};
use iroh_gossip::proto::TopicId;
use log::{error, info};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_store::StoreExt;

// --- New Gossip Event Payload ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEventPayload {
    pub from: NodeId,
    pub topic: TopicId,
    pub file_name: String,
    pub relative_path: String,
    pub message_content: String,
}
impl GossipEventPayload {
    // Updated to use CommandError for consistency if this were to be a command itself,
    // but it's a payload. Original anyhow::Result is fine for internal use.
    // For now, keeping anyhow::Result as its usage is internal to subscribe_loop.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }
}

#[tauri::command]
pub async fn create_gossip_ticket(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let endpoint = state
        .endpoint
        .clone()
        .ok_or_else(|| CommandError::IrohClientNotInitialized("endpoint".to_string()))?;

    let store_plugin = app.store("store.json").map_err(CommandError::StoreError)?;

    let topic_id: TopicId = match store_plugin.get("topic-id") {
        Some(json_value) => {
            serde_json::from_value::<TopicId>(json_value.clone()).unwrap_or_else(|e| {
                error!("Failed to deserialize stored topic-id ({}). Generating new one.", e);
                TopicId::from_bytes(rand::random())
            })
        }
        None => {
            info!("No topic-id found in store. Generating new one.");
            TopicId::from_bytes(rand::random())
        }
    };

    store_plugin
        .set("topic-id", topic_id.clone().to_string())
        .map_err(CommandError::StoreError)?;
    store_plugin.save().map_err(CommandError::StoreError)?;
    // store_plugin.close_resource(); // close_resource is not a method on Store

    // Assuming create_iroh_gossip_ticket will return Result<_, IrohError>
    let str_gossip_ticket = create_iroh_gossip_ticket(endpoint, topic_id).await?;

    Ok(str_gossip_ticket)
}

#[tauri::command]
pub async fn join_gossip(
    app_handle: AppHandle,
    app_state: State<'_, AppState>,
    str_gossip_ticket: String,
) -> Result<bool, CommandError> {
    info!("join_gossip command started.");

    let endpoint = app_state
        .endpoint
        .clone()
        .ok_or_else(|| CommandError::IrohClientNotInitialized("endpoint".to_string()))?;
    info!("Endpoint obtained.");

    let gossip = app_state
        .gossip
        .clone()
        .ok_or_else(|| CommandError::IrohClientNotInitialized("gossip".to_string()))?;
    info!("Gossip handler obtained.");

    let GossipTicket { topic, nodes: _ } = GossipTicket::from_str(&str_gossip_ticket)
        .map_err(|e| CommandError::TicketParseError(e.to_string()))?;
    info!("Gossip ticket parsed, topic: {:?}", topic);

    let store_plugin = app_handle.store("store.json").map_err(CommandError::StoreError)?;
    store_plugin.set("topic-id", topic.clone().to_string()).map_err(CommandError::StoreError)?;
    store_plugin.save().map_err(CommandError::StoreError)?;
    // store_plugin.close_resource();

    {
        info!("Attempting to lock gossip_topic in AppState.");
        let mut gossip_topic_guard = app_state.gossip_topic.lock().await;
        *gossip_topic_guard = Some(topic.clone());
        info!("gossip_topic in AppState set and lock released.");
    }

    info!("Calling join_iroh_gossip (iroh_fns.rs)...");
    // Assuming join_iroh_gossip will return Result<_, IrohError>
    let (sender, receiver) = join_iroh_gossip(endpoint, gossip, str_gossip_ticket.clone()).await?;

    {
        info!("Attempting to lock gossip_sender in AppState.");
        let mut gossip_sender_guard = app_state.gossip_sender.lock().await;
        *gossip_sender_guard = Some(sender);
        info!("gossip_sender in AppState set and lock released.");
    }

    let receiver_app_handle = app_handle.clone();
    let blobs = app_state
        .blobs
        .clone()
        .ok_or_else(|| CommandError::IrohClientNotInitialized("blobs client".to_string()))?;
    let sync_path = app_state.sync_folder.clone();
    tauri::async_runtime::spawn(async move {
        info!("Gossip receiver task (subscribe_loop) started.");
        // Assuming subscribe_loop will be updated to return Result<_, CommandError> or IrohError
        match subscribe_loop(receiver_app_handle, blobs, sync_path, receiver).await {
            Ok(_) => info!("subscribe_loop finished successfully."),
            Err(e) => error!("Error in subscribe_loop: {:?}", e), // Log error, decide if it should panic or be handled
        }
        info!("Gossip receiver task (subscribe_loop) finished.");
    });
    info!("subscribe_loop task spawned.");

    app_handle
        .emit("gossip-ready", ())
        .map_err(|e| CommandError::GossipJoinError(format!("Failed to emit gossip-ready event: {}", e)))?;
    info!("Emitted gossip-ready event to frontend.");

    Ok(true)
}
