use std::collections::HashMap;
use std::path::PathBuf; // Added import

use crate::{
    commands::GossipEventPayload, // Import the payload struct
    fs_watcher::{FsEventPayload, FsEventType},
    state::AppState,
};
use anyhow::{Error, Result};
use futures_util::StreamExt; // Added import for try_next
use iroh::{protocol::Router, Endpoint, NodeAddr, SecretKey};
use iroh_blobs::{
    net_protocol::Blobs, rpc::client::blobs::WrapOption, store::fs::Store, ticket::BlobTicket,
    util::SetTagOption,
};
use iroh_gossip::{
    net::{Event as GossipNetEvent, Gossip, GossipEvent, GossipReceiver, GossipSender}, // Adjusted imports
    proto::TopicId,
};
use log::{error, info, warn}; // Added warn
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use tauri::{AppHandle, Manager, State}; // Manager is already here
                                        // use tokio::sync::Mutex; // Mutex from tokio is not directly used in this file's changes

#[derive(Debug, Serialize, Deserialize)]
struct GossipTicket {
    topic: TopicId,
    nodes: Vec<NodeAddr>,
}

impl GossipTicket {
    /// Deserialize from a slice of bytes to a Ticket.
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    /// Serialize from a `Ticket` to a `Vec` of bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }
}

impl fmt::Display for GossipTicket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut text = data_encoding::BASE32_NOPAD.encode(&self.to_bytes()[..]);
        text.make_ascii_lowercase();
        write!(f, "{}", text)
    }
}

impl FromStr for GossipTicket {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = data_encoding::BASE32_NOPAD.decode(s.to_ascii_uppercase().as_bytes())?;
        Self::from_bytes(&bytes)
    }
}

pub async fn setup<R: tauri::Runtime>(handle: tauri::AppHandle<R>) -> Result<()> {
    let data_root = handle.path().app_data_dir()?;

    let blobs_root = data_root.join("blob_data");

    let secret_key_path = data_root.join("secret_key");
    let secret_key = match secret_key_path.exists() {
        true => {
            info!("Loading secret key from {:?}", &secret_key_path);
            let bytes = std::fs::read(&secret_key_path)?;
            let key_bytes_array: [u8; 32] = bytes.as_slice().try_into().map_err(|e| {
                anyhow::anyhow!(
                "Secret key file {:?} has incorrect size: expected 32 bytes, found {}. Error: {}",
                secret_key_path,
                bytes.len(),
                e
            )
            })?;
            SecretKey::from_bytes(&key_bytes_array)
        }
        false => {
            info!(
                "Generating new secret key and saving to {:?}",
                &secret_key_path
            );
            let new_secret_key = SecretKey::generate(rand::rngs::OsRng);
            if let Some(parent_dir) = secret_key_path.parent() {
                std::fs::create_dir_all(parent_dir)?;
            }
            std::fs::write(&secret_key_path, new_secret_key.to_bytes())?;
            new_secret_key
        }
    };

    let endpoint = Endpoint::builder()
        .secret_key(secret_key)
        .discovery_n0()
        .bind()
        .await?;
    println!("> our node id: {}", endpoint.node_id());

    let builder = Router::builder(endpoint.clone());

    let blobs = Blobs::persistent(blobs_root)
        .await?
        .build(builder.endpoint());

    let gossip = Gossip::builder().spawn(endpoint.clone()).await?;

    let router = builder
        .accept(iroh_blobs::ALPN, blobs.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn()
        .await?;

    let app_state = AppState {
        endpoint: Some(endpoint),
        blobs: Some(blobs),
        gossip: Some(gossip),
        router: Some(router),
        gossip_sender: tokio::sync::Mutex::new(None), // Ensure Mutex is from tokio::sync
        sync_folder: None,
        sync_task_handle: None,
    };

    handle.manage(app_state);
    Ok(())
}

pub async fn create_iroh_gossip_ticket(endpoint: Endpoint) -> Result<String, Error> {
    let me = endpoint.node_addr().await?;
    let ticket = GossipTicket {
        topic: TopicId::from_bytes(rand::random()),
        nodes: vec![me],
    };
    let str_gossip_ticket = ticket.to_string();
    info!("created str gossip ticket for ticket {}", str_gossip_ticket);
    Ok(str_gossip_ticket)
}

pub async fn create_iroh_ticket(
    blobs: Blobs<Store>,
    endpoint: Endpoint,
    path: PathBuf,
) -> Result<String, Error> {
    let blobs_client = blobs.client();
    let add_progress = blobs_client
        .add_from_path(path, true, SetTagOption::Auto, WrapOption::NoWrap)
        .await?;
    let blob = add_progress.finish().await?;
    let node_id = endpoint.node_id();
    let ticket = BlobTicket::new(node_id.into(), blob.hash, blob.format)?;
    let str_ticket = ticket.to_string();
    info!("created str ticket for ticket {}", str_ticket);
    Ok(str_ticket)
}

pub async fn join_iroh_gossip(
    endpoint: Endpoint,
    gossip: Gossip,
    str_gossip_ticket: String,
) -> Result<(GossipSender, GossipReceiver), Error> {
    let GossipTicket { topic, nodes } = GossipTicket::from_str(&str_gossip_ticket)?;
    let node_ids = nodes.iter().map(|p| p.node_id).collect();
    if nodes.is_empty() {
        println!("> waiting for nodes to join us...");
    } else {
        println!("> trying to connect to {} nodes...", nodes.len());
        for node in nodes.into_iter() {
            endpoint.add_node_addr(node)?;
        }
    };
    let (sender, receiver) = gossip.subscribe_and_join(topic, node_ids).await?.split();
    Ok((sender, receiver))
}

pub async fn get_iroh_blob(blobs: Blobs<Store>, str_ticket: String) -> Result<(), Error> {
    let blobs_client = blobs.client();
    let ticket: BlobTicket = str_ticket.parse()?;
    let download_req = blobs_client
        .download(ticket.hash(), ticket.node_addr().clone())
        .await?;
    download_req.finish().await?;
    Ok(())
}

pub fn handle_fs_payload(payload: FsEventPayload, handle: AppHandle) {
    let app_state: State<'_, AppState> = handle.state();

    match payload.event_type {
        FsEventType::Create => {
            let blobs_opt = app_state.blobs.clone();
            let endpoint_opt = app_state.endpoint.clone();
            let file_path = payload.path.clone();
            let gossip_sender_mutex = app_state.gossip_sender.clone(); // Clone the Arc<Mutex<_>>

            tauri::async_runtime::spawn(async move {
                let blobs = match blobs_opt {
                    Some(b) => b,
                    None => {
                        error!(
                            "Blobs client not initialized for ticket creation for {:?}.",
                            file_path
                        );
                        return;
                    }
                };
                let endpoint = match endpoint_opt {
                    Some(e) => e,
                    None => {
                        error!(
                            "Endpoint not initialized for ticket creation for {:?}.",
                            file_path
                        );
                        return;
                    }
                };

                match create_iroh_ticket(blobs, endpoint, file_path.clone()).await {
                    Ok(iroh_ticket) => {
                        info!(
                            "Created Iroh Ticket Successfully for {:?}: {}",
                            file_path, iroh_ticket
                        );

                        let gossip_sender_guard = gossip_sender_mutex.lock().await;
                        if let Some(sender) = &*gossip_sender_guard {
                            let message_content: Vec<u8> = iroh_ticket.as_bytes().to_vec();
                            match sender.broadcast(message_content.into()).await {
                                // .into() converts Vec<u8> to Bytes
                                Ok(_) => info!("Gossiped ticket: {}", iroh_ticket),
                                Err(e) => {
                                    error!("Failed to gossip ticket {}: {:?}", iroh_ticket, e)
                                }
                            }
                        } else {
                            warn!(
                                "Gossip sender not available. Ticket {} created but not gossiped.",
                                iroh_ticket
                            );
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
            // Consider sending a gossip message about the removal.
        }
        _ => {}
    }
}

// Updated subscribe_loop to accept AppHandle and emit events
pub async fn subscribe_loop<R: tauri::Runtime>(
    app_handle: AppHandle<R>,
    mut receiver: GossipReceiver,
) -> Result<()> {
    // The HashMap for names is specific to an example message format.
    // If your messages are different, you might not need it or need different logic.
    // For now, I'll keep it to show how one might handle structured messages if desired.
    let mut _names = HashMap::new(); // Kept for structure, but not used with current raw emit

    // Iterate over all events from the gossip receiver stream
    while let Some(result) = receiver.next().await {
        // Changed from try_next to next for typical stream handling
        match result {
            Ok(event) => {
                if let GossipNetEvent::Gossip(GossipEvent::Received(msg)) = event {
                    info!(
                        "Received gossip message from {:?} on topic {:?} ({} bytes)",
                        msg.delivered_from,
                        msg.scope,
                        msg.content.len()
                    );

                    // Emit the raw message (or a structured version) to the frontend
                    let payload = GossipEventPayload {
                        from: msg.from.to_string(),
                        topic: msg.topic.to_string(),
                        content_base64: base64::encode(&msg.content), // Using base64 crate
                    };

                    if let Err(e) = app_handle.emit_all("gossip://message", payload) {
                        error!("Failed to emit gossip message to frontend: {}", e);
                    }

                    // If you had a specific message protocol like the example:
                    // match Message::from_bytes(&msg.content) {
                    //     Ok(Message::AboutMe { from, name }) => {
                    //         names.insert(from, name.clone());
                    //         info!("> {} is now known as {}", from.fmt_short(), name);
                    //     }
                    //     Ok(Message::Message { from, text }) => {
                    //         let name_display = names
                    //             .get(&from)
                    //             .map_or_else(|| from.fmt_short(), String::to_string);
                    //         info!("{}: {}", name_display, text);
                    //     }
                    //     Err(e) => {
                    //         warn!("Failed to deserialize gossip message: {:?}", e);
                    //     }
                    // }
                } else if let GossipNetEvent::Gossip(GossipEvent::NeighborUp(node_id)) = event {
                    info!("Neighbor up: {:?}", node_id);
                    // Optionally emit this event to the frontend too
                    if let Err(e) = app_handle.emit("gossip://neighbor-up", node_id.to_string()) {
                        error!("Failed to emit neighbor-up event: {}", e);
                    }
                } else if let GossipNetEvent::Gossip(GossipEvent::NeighborDown(node_id)) = event {
                    info!("Neighbor down: {:?}", node_id);
                    if let Err(e) = app_handle.emit("gossip://neighbor-down", node_id.to_string()) {
                        error!("Failed to emit neighbor-down event: {}", e);
                    }
                }
                // Handle other GossipNetEvent variants as needed
            }
            Err(e) => {
                error!("Gossip receiver stream error: {:?}", e);
                // Depending on the error, you might want to break or implement retry logic
            }
        }
    }
    Ok(())
}
