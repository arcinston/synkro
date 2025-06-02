use crate::{
    // Use the specific path from the commands refactor for GossipEventPayload
    commands::gossip_commands::GossipEventPayload,
    clipboard_monitor::ClipboardPayload, // Added
    errors::IrohError,
    fs_watcher::{FsEventPayload, FsEventType}, // Corrected: remove if duplicated, ensure one exists
    iroh_fns::tickets::GossipTicket,
    state::AppState,
};
use futures_util::StreamExt;
// Ensure other necessary imports like NodeId, TopicId, etc., are present from previous steps
// For example, iroh::NodeId might be needed by ClipboardPayload if not already transitively available
use iroh::{Endpoint, NodeId};
use iroh_blobs::net_protocol::Blobs; // Added
use iroh_blobs::store::fs::Store as BlobStore; // Added and aliased
use iroh_gossip::{
    net::{Event as GossipNetEvent, Gossip, GossipEvent, GossipReceiver, GossipSender},
    proto::TopicId, // Added
};
use log::{error, info, warn};
use std::path::PathBuf;
use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager, Runtime, State}; // Added Runtime, kept Manager, State, Emitter
use tauri_plugin_store::StoreExt; // Added for store access

// Moved from tickets.rs, needed by join_iroh_gossip
// use crate::iroh_fns::tickets::GossipTicket; // This was already added above

// Moved from blob_ops.rs or main, needed by subscribe_loop if it calls get_iroh_blob directly
use super::blob_ops::get_iroh_blob; // Assuming get_iroh_blob will be in blob_ops.rs
use super::tickets::create_iroh_ticket; // For handle_fs_payload

pub async fn join_iroh_gossip(
    endpoint: Endpoint,
    gossip: Gossip,
    str_gossip_ticket: String,
) -> Result<(GossipSender, GossipReceiver), IrohError> { // Changed
    info!(
        "join_iroh_gossip called with ticket: {}",
        str_gossip_ticket
    );

    let GossipTicket { topic, nodes } = GossipTicket::from_str(&str_gossip_ticket)?; // Uses From<anyhow::Error> for IrohError
    info!(
        "Parsed ticket in join_iroh_gossip, topic: {:?}, nodes: {:?}",
        topic, nodes
    );

    let me = endpoint.node_id();

    let nodes_to_connect = nodes
        .into_iter()
        .filter(|node_addr| node_addr.node_id != me)
        .collect::<Vec<_>>();
    info!("Attempting to connect to peers: {:?}", nodes_to_connect);

    let node_ids_to_join = nodes_to_connect.iter().map(|p| p.node_id).collect::<Vec<_>>();
    if nodes_to_connect.is_empty() {
        info!("No external peers in ticket, or only self. Waiting for others to join us on topic {:?}...", topic);
    } else {
        info!(
            "Trying to connect to {} nodes for topic {:?}...",
            nodes_to_connect.len(),
            topic
        );
        for node_addr in nodes_to_connect.iter() {
            info!("Adding node address: {:?}", node_addr);
            endpoint.add_node_addr(node_addr.clone())?; // Uses From<iroh::endpoint::Error>
        }
        info!("Finished adding node addresses.");
    };

    info!("Calling gossip.subscribe for topic {:?}...", topic);
    let subscription = gossip.subscribe(topic.clone(), node_ids_to_join)?; // Uses From<iroh_gossip::net::GossipError>
    info!("Successfully subscribed and joined topic {:?}.", topic);
    let (sender, receiver) = subscription.split();
    Ok((sender, receiver))
}

pub fn handle_fs_payload<R: tauri::Runtime>(payload: FsEventPayload, handle: AppHandle<R>) {
    let app_state: State<'_, AppState> = handle.state();

    match payload.event_type {
        FsEventType::Create => {
            let blobs_opt = app_state.blobs.clone();
            let endpoint_opt = app_state.endpoint.clone();
            let file_path = payload.path.clone();
            let sync_folder_path = app_state.sync_folder.clone();
            let gossip_sender_mutex = app_state.gossip_sender.clone();
            let gossip_topic_mutex = app_state.gossip_topic.clone();
            // Need endpoint for node_id
            // let endpoint_for_node_id = app_state.endpoint.clone();


            tauri::async_runtime::spawn(async move {
                let current_endpoint = match endpoint_opt {
                    Some(ep) => ep,
                    None => {
                        error!("Endpoint not initialized for FS payload handling for {:?}", file_path);
                        return;
                    }
                };
                let current_blobs = match blobs_opt {
                    Some(b) => b,
                    None => {
                        error!("Blobs client not initialized for FS payload handling for {:?}.", file_path);
                        return;
                    }
                };

                match create_iroh_ticket(current_blobs, current_endpoint.clone(), file_path.clone()).await {
                    Ok(iroh_ticket) => {
                        info!(
                            "Created Iroh Ticket Successfully for {:?}: {}",
                            file_path, iroh_ticket
                        );

                        let topic_id: Option<TopicId> = { // Scope for topic_id lock
                            let guard = gossip_topic_mutex.lock().await;
                            (*guard).clone() // Clone if Some, else None
                        };

                        let current_topic_id = if let Some(id) = topic_id {
                            id
                        } else {
                            warn!( "Gossip topic not set. Ticket {} for {:?} created but cannot be gossiped.", iroh_ticket, file_path);
                            return;
                        };

                        let sender_guard = gossip_sender_mutex.lock().await;
                        if let Some(sender) = &*sender_guard {
                            let relative_path = match file_path.strip_prefix(&sync_folder_path) {
                                Ok(p) => p.to_string_lossy().into_owned(),
                                Err(e) => {
                                    error!("Failed to create relative path for {:?} from base {:?}: {}", file_path, sync_folder_path, e);
                                    return;
                                }
                            };

                            let file_name = match file_path.file_name() {
                                Some(name_os_str) => name_os_str.to_string_lossy().into_owned(),
                                None => {
                                    error!("Failed to get file name from path: {:?}", file_path);
                                    return;
                                }
                            };

                            let gossip_message = GossipEventPayload {
                                from: current_endpoint.node_id(), // Use the cloned endpoint
                                topic: current_topic_id,
                                message_content: iroh_ticket.clone(),
                                file_name,
                                relative_path,
                            };
                            info!("Gossip message created {:?}", gossip_message);
                            match sender.broadcast(gossip_message.to_vec().into()).await {
                                Ok(_) => info!("Gossiped ticket: {}", iroh_ticket),
                                Err(e) => {
                                    error!("Failed to gossip ticket {}: {:?}", iroh_ticket, e);
                                }
                            }
                        } else {
                            warn!("Gossip sender not available. Ticket {} for {:?} created but not gossiped.", iroh_ticket, file_path);
                        }
                    }
                    Err(err) => {
                        error!("Ticket Creation failed for {:?}: {}", file_path, err);
                    }
                }
            });
        }
        FsEventType::Remove => {
            info!("File system event: Remove for path {:?}", payload.path);
        }
        _ => {}
    }
}


pub async fn subscribe_loop<R: tauri::Runtime>(
    app_handle: AppHandle<R>,
    blobs: Blobs<BlobStore>,
    sync_path: PathBuf,
    mut receiver: GossipReceiver,
) -> Result<(), IrohError> { // Changed
    // Note: The errors inside this loop are logged, not propagated up from subscribe_loop
    // This is because subscribe_loop is typically spawned and its errors are handled within the task.
    // If subscribe_loop itself encounters a setup or unrecoverable stream error, it could return IrohError.
    while let Some(result) = receiver.next().await { // result is Result<GossipNetEvent, RecvError>
        match result {
            Ok(event) => { // event is GossipNetEvent
                match event {
                    GossipNetEvent::Gossip(GossipEvent::Received(msg)) => {
                        info!(
                            "Received gossip message from {:?} on topic {:?} ({} bytes)",
                            msg.delivered_from,
                            msg.scope,
                            msg.content.len()
                        );

                        let app_state_instance = app_handle.state::<AppState>();
                        let current_node_id_option = app_state_instance.endpoint.as_ref().map(|ep| ep.node_id());

                        // Try to deserialize as ClipboardPayload
                        if let Ok(clipboard_payload) = ClipboardPayload::from_bytes(&msg.content) {
                            info!("Deserialized as ClipboardPayload: {:?}", clipboard_payload);
                            if let Some(current_node_id) = current_node_id_option {
                                if clipboard_payload.from_node_id == current_node_id {
                                    info!("Ignoring self-sent clipboard payload.");
                                } else {
                                // Check if clipboard sharing is enabled before setting
                                match app_handle.store("store.json") {
                                    Ok(store) => {
                                        if store.get("clipboard_sharing_enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                                            if let Some(monitor_arc) = &app_state_instance.clipboard_monitor {
                                                match monitor_arc.set_local_clipboard_content(clipboard_payload.content) {
                                                    Ok(_) => info!("Successfully updated local clipboard from network."),
                                                    Err(e) => error!("Error updating local clipboard from network: {:?}", e),
                                                }
                                            } else {
                                                error!("ClipboardMonitor not found in AppState.");
                                            }
                                        } else {
                                            info!("Clipboard sharing disabled. Ignoring clipboard payload from network.");
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to access store in subscribe_loop: {}. Cannot check clipboard sharing status.", e);
                                        }
                                    }
                                }
                            } else {
                                 error!("Current NodeId not available, cannot process clipboard payload correctly.");
                            }
                        } else if let Ok(file_payload) = GossipEventPayload::from_bytes(&msg.content) {
                            // This is the existing file sync payload logic
                            info!("Deserialized as GossipEventPayload (file sync): {:?}", file_payload);
                            if let Err(e) = app_handle.emit("gossip://message", &file_payload) { // Pass by reference
                                error!("Failed to emit file gossip message to frontend: {}", e);
                            }

                            let sync_path_clone = sync_path.clone(); // sync_path is from subscribe_loop params
                            let blobs_clone = blobs.clone(); // blobs is from subscribe_loop params
                            tauri::async_runtime::spawn(async move {
                                let str_iroh_ticket = file_payload.message_content; // Use the cloned file_payload
                                let dest_path = sync_path_clone.join(&file_payload.relative_path);
                                if let Some(parent_dir) = dest_path.parent() {
                                    if !parent_dir.exists() {
                                        if let Err(e) = std::fs::create_dir_all(parent_dir) {
                                            error!("Failed to create directory {:?}: {}", parent_dir, e);
                                            return;
                                        }
                                        info!("Created directory {:?}", parent_dir);
                                    }
                                }
                                match get_iroh_blob(blobs_clone, str_iroh_ticket, dest_path).await { // get_iroh_blob is from super::blob_ops
                                    Ok(_) => {
                                        info!("Successfully downloaded blob for received file sync event.");
                                    }
                                    Err(e) => {
                                        error!("Error downloading blob for file sync event: {}", e.to_string());
                                    }
                                }
                            });
                        } else {
                            warn!(
                                "Failed to deserialize gossip message into known payload types (Clipboard or File). Content length: {}",
                                msg.content.len()
                            );
                        }
                    }
                    GossipNetEvent::Gossip(GossipEvent::NeighborUp(node_id)) => {
                        info!("Neighbor up: {:?}", node_id);
                        if let Err(e) = app_handle.emit("gossip://neighbor-up", node_id.to_string()) {
                            error!("Failed to emit neighbor-up event: {}", e);
                        }
                    }
                    GossipNetEvent::Gossip(GossipEvent::NeighborDown(node_id)) => {
                        info!("Neighbor down: {:?}", node_id);
                        if let Err(e) = app_handle.emit("gossip://neighbor-down", node_id.to_string()) {
                            error!("Failed to emit neighbor-down event: {}", e);
                        }
                    }
                    // Handle other GossipNetEvent variants if necessary
                    _ => {
                        info!("Received other gossip event: {:?}", event);
                    }
                }
            }
            Err(e) => { // e is RecvError
                error!("Gossip receiver stream error: {:?}", e);
                // This could be a point to return an IrohError if the stream is terminally broken.
                // For now, just logging, consistent with original behavior.
                // Example: return Err(IrohError::General(format!("Gossip stream failed: {}", e)));
            }
        }
    }
    info!("Gossip subscribe_loop finished gracefully.");
    Ok(())
}
