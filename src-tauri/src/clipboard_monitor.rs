use crate::state::AppState; // Added
use anyhow;
use arboard::Clipboard;
use iroh::NodeId;
// use iroh::Endpoint; // Not directly needed, NodeId comes from endpoint in AppState
use iroh_gossip::net::GossipSender;
use iroh_gossip::proto::TopicId;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime, State}; // Added Runtime
use tauri_plugin_store::StoreExt; // Added for store access
use tokio::time::{self, Duration};

pub struct ClipboardMonitor {
    clipboard: Arc<Mutex<Clipboard>>,
    last_content: Arc<Mutex<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub from_node_id: NodeId, // ID of the node that sent this clipboard content
    pub content: String,      // The actual text content from the clipboard
}

impl ClipboardPayload {
    pub fn new(from_node_id: NodeId, content: String) -> Self {
        Self { from_node_id, content }
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("ClipboardPayload serialization should not fail")
    }
}

impl ClipboardMonitor {
    pub fn new() -> Result<Self, arboard::Error> {
        let clipboard = Clipboard::new()?;
        Ok(Self {
            clipboard: Arc::new(Mutex::new(clipboard)), // std::sync::Mutex
            last_content: Arc::new(Mutex::new(String::new())), // std::sync::Mutex
        })
    }

    pub fn set_local_clipboard_content(&self, content: String) -> Result<(), arboard::Error> {
        // It's important that clipboard and last_content are locked briefly and together if possible,
        // but here clipboard.set_text might take some time.
        // Consider if last_content should be updated regardless of clipboard.set_text success,
        // or only on success. Current logic: only on success.

        // Check if sharing is enabled before setting
        let store = app_handle.store("store.json").map_err(|e| {
            // This error conversion is tricky because this function returns arboard::Error
            // For now, log and return a generic arboard error or the original if it can be mapped.
            error!("Failed to access store: {}", e);
            // Create a dummy arboard::Error or map if possible.
            // This is a limitation of not having a unified error type here.
            arboard::Error::Unknown // Placeholder for actual error mapping
        })?;

        if !store.get("clipboard_sharing_enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            info!("Clipboard sharing is disabled. Skipping setting local clipboard from network.");
            return Ok(());
        }

        let mut clipboard_guard = self.clipboard.lock().unwrap();
        match clipboard_guard.set_text(content.clone()) {
            Ok(_) => {
                let mut last_content_guard = self.last_content.lock().unwrap();
                *last_content_guard = content;
                info!("Successfully set local clipboard from network (len: {}).", last_content_guard.len());
                Ok(())
            }
            Err(e) => {
                error!("Failed to set local clipboard from network: {:?}", e);
                Err(e)
            }
        }
    }

    pub async fn start_monitoring<R: Runtime>( // Changed to tauri::Runtime
        &self,
        app_handle: AppHandle<R>,
    ) {
        info!("Clipboard monitoring started.");
        let clipboard_arc = Arc::clone(&self.clipboard);
        let last_content_arc = Arc::clone(&self.last_content);

        loop {
            time::sleep(Duration::from_secs(2)).await;

            // Check if clipboard sharing is enabled
            let store_result = app_handle.store("store.json");
            let store_is_accessible = store_result.is_ok();

            if !store_is_accessible {
                error!("Failed to access store in clipboard monitor. Skipping check.");
                continue;
            }
            let store = store_result.unwrap(); // Safe due to check above

            if !store.get("clipboard_sharing_enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                // info!("Clipboard sharing is disabled. Skipping clipboard check and broadcast.");
                continue;
            }

            let app_state_guard = app_handle.state::<AppState>();

            let endpoint_option = app_state_guard.endpoint.clone();
            let current_node_id = match endpoint_option {
                Some(ep) => ep.node_id(),
                None => {
                    // info!("Endpoint not available, skipping clipboard check.");
                    drop(app_state_guard); // Release AppState lock before continuing
                    continue;
                }
            };

            let gossip_sender_arc_mutex = app_state_guard.gossip_sender.clone();
            let topic_id_arc_mutex = app_state_guard.gossip_topic.clone();
            drop(app_state_guard); // Release AppState lock

            let gossip_sender_option: Option<GossipSender> = gossip_sender_arc_mutex.lock().await.clone();
            let topic_id_option: Option<TopicId> = topic_id_arc_mutex.lock().await.clone();

            if gossip_sender_option.is_none() || topic_id_option.is_none() {
                // info!("Gossip not ready, skipping clipboard broadcast check.");
                continue;
            }

            let gossip_sender = gossip_sender_option.unwrap();
            let topic_id = topic_id_option.unwrap();

            // Lock clipboard and last_content (std::sync::Mutex)
            // It's better to lock these for shorter periods.
            // Consider moving text fetching outside and only lock for comparison and update.
            let current_text_result = { // Scope for clipboard_guard
                let mut clipboard_guard = clipboard_arc.lock().unwrap(); // std::sync::Mutex
                clipboard_guard.get_text()
            };


            match current_text_result {
                Ok(current_text) => {
                    let mut last_content_guard = last_content_arc.lock().unwrap(); // std::sync::Mutex
                    if current_text != *last_content_guard && !current_text.is_empty() {
                        info!("New clipboard text detected (len: {}): {}", current_text.len(), &current_text[..std::cmp::min(current_text.len(), 50)]);

                        let payload = ClipboardPayload::new(current_node_id, current_text.clone());
                        match gossip_sender.broadcast_to_topic(topic_id, payload.to_vec().into()).await {
                            Ok(_) => {
                                info!("Clipboard content gossiped successfully.");
                                *last_content_guard = current_text;
                            }
                            Err(e) => {
                                error!("Failed to gossip clipboard content: {:?}", e);
                            }
                        }
                    }
                }
                Err(err) => {
                    let err_str = err.to_string();
                    if !err_str.contains("Clipboard is empty or contains non-text data") &&
                       !err_str.contains("The clipboard doesn't contain text") && // Linux Wayland (arboard uses this)
                       !err_str.contains("Could not find data of type TEXT") && // Linux X11
                       !err_str.contains("Format not available") && // Windows
                       !err_str.contains("failed to get text from clipboard: Empty") // MacOS
                    {
                         error!("Error reading clipboard: {} ({:?})", err_str, err);
                    }
                }
            }
        }
    }
}

// Old init_clipboard_monitor function is removed as per instructions.
// It will be started from iroh_fns::setup::setup after AppState is managed.
}
