use crate::{
    commands::GossipEventPayload, // Import the payload struct
    fs_watcher::{FsEventPayload, FsEventType},
    state::AppState,
};
use anyhow::{Error, Result};
use futures_util::StreamExt; // Added import for try_next
use iroh::{protocol::Router, Endpoint, NodeAddr, RelayMode, SecretKey};
use iroh_blobs::{
    net_protocol::Blobs,
    rpc::client::blobs::WrapOption,
    store::{fs::Store, ExportFormat, ExportMode},
    ticket::BlobTicket,
    util::SetTagOption,
};
use iroh_gossip::{
    net::{Event as GossipNetEvent, Gossip, GossipEvent, GossipReceiver, GossipSender}, // Adjusted imports
    proto::TopicId,
};
use log::{error, info, warn}; // Added warn
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf; // Added import
use std::str::FromStr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
pub struct GossipTicket {
    pub topic: TopicId,
    pub nodes: Vec<NodeAddr>,
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

pub async fn setup<R: tauri::Runtime>(
    handle: tauri::AppHandle<R>,
    sync_path: PathBuf,
) -> Result<()> {
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
        .discovery_local_network()
        .relay_mode(RelayMode::Default)
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
        .spawn();

    let app_state = AppState {
        endpoint: Some(endpoint),
        blobs: Some(blobs),
        gossip: Some(gossip),
        router: Some(router),
        gossip_topic: Arc::new(Mutex::new(None)),
        gossip_sender: Arc::new(Mutex::new(None)), // Ensure Mutex is from tokio::sync
        sync_folder: sync_path,
        sync_task_handle: None,
    };

    handle.manage(app_state);
    Ok(())
}

pub async fn create_iroh_gossip_ticket(
    endpoint: Endpoint,
    topic_id: TopicId,
) -> Result<String, Error> {
    let me = endpoint.node_addr().await?;
    let ticket = GossipTicket {
        topic: topic_id,
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
    info!(
        "join_iroh_gossip (iroh_fns.rs) called with ticket: {}",
        str_gossip_ticket
    ); // New log

    let GossipTicket { topic, nodes } = GossipTicket::from_str(&str_gossip_ticket)?;
    info!(
        "Parsed ticket in join_iroh_gossip (iroh_fns.rs), topic: {:?}, nodes: {:?}",
        topic, nodes
    ); // New log

    let me = endpoint.node_id();

    // nodes without self node id
    let nodes = nodes
        .into_iter()
        .filter(|node_addr| node_addr.node_id != me)
        .collect::<Vec<_>>();
    info!("Attempting to connect to peers: {:?}", nodes); // New log

    let node_ids_to_join = nodes.iter().map(|p| p.node_id).collect::<Vec<_>>();
    if nodes.is_empty() {
        info!("No external peers in ticket, or only self. Waiting for others to join us on topic {:?}...", topic);
    // Changed println to info!
    } else {
        info!(
            "Trying to connect to {} nodes for topic {:?}...",
            nodes.len(),
            topic
        ); // Changed println to info!
        for node_addr in nodes.iter() {
            // Iterate over a reference
            info!("Adding node address: {:?}", node_addr); // New log
            endpoint.add_node_addr(node_addr.clone())?; // Clone node_addr if needed
        }
        info!("Finished adding node addresses."); // New log
    };

    info!("Calling gossip.subscribe for topic {:?}...", topic); // New log
    match gossip.subscribe(topic, node_ids_to_join) {
        Ok(subscription) => {
            info!("Successfully subscribed and joined topic {:?}.", topic); // New log
            let (sender, receiver) = subscription.split();
            Ok((sender, receiver))
        }
        Err(e) => {
            error!("Failed to subscribe topic {:?}: {:?}", topic, e); // New log
            Err(e.into())
        }
    }
}

pub async fn get_iroh_blob(
    blobs: Blobs<Store>,
    str_ticket: String,
    dest_path: PathBuf,
) -> Result<(), Error> {
    let blobs_client = blobs.client();
    let ticket: BlobTicket = str_ticket.parse()?;
    let download_req = blobs_client
        .download(ticket.hash(), ticket.node_addr().clone())
        .await?;
    download_req.finish().await?;

    blobs_client
        .export(
            ticket.hash(),
            dest_path,
            ExportFormat::Blob,
            ExportMode::Copy,
        )
        .await?
        .finish()
        .await?;
    Ok(())
}

pub fn handle_fs_payload(payload: FsEventPayload, handle: AppHandle) {
    let app_state: State<'_, AppState> = handle.state();

    match payload.event_type {
        FsEventType::Create => {
            let blobs_opt = app_state.blobs.clone();
            let endpoint_opt = app_state.endpoint.clone();
            let file_path = payload.path.clone();
            let sync_folder_path = app_state.sync_folder.clone();
            let gossip_sender_mutex = app_state.gossip_sender.clone();
            let gossip_topic_mutex = app_state.gossip_topic.clone();

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

                match create_iroh_ticket(blobs, endpoint.clone(), file_path.clone()).await {
                    Ok(iroh_ticket) => {
                        info!(
                            "Created Iroh Ticket Successfully for {:?}: {}",
                            file_path, iroh_ticket
                        );

                        info!(
                            "Attempting to lock gossip_topic_mutex for file: {:?}",
                            file_path
                        );
                        let gossip_topic_guard = gossip_topic_mutex.lock().await;
                        let topic_id = if let Some(topic) = &*gossip_topic_guard {
                            info!(
                                "Successfully locked and read gossip_topic_mutex for file: {:?}, topic: {:?}",
                                file_path, topic
                            );
                            topic.clone() // Clone the TopicId for use
                        } else {
                            warn!(
                                "Gossip topic not set in AppState. Ticket {} for file {:?} created but cannot be gossiped.",
                                iroh_ticket, file_path
                            );
                            return; // Exit if topic is not set
                        };
                        // Drop the guard for gossip_topic_mutex explicitly if needed, or let it go out of scope
                        drop(gossip_topic_guard);

                        info!(
                            "Attempting to lock gossip_sender_mutex for file: {:?}",
                            file_path
                        );
                        let sender_guard = gossip_sender_mutex.lock().await;
                        info!(
                            "Successfully locked gossip_sender_mutex for file: {:?}",
                            file_path
                        );

                        if let Some(sender) = &*sender_guard {
                            let relative_path = match file_path.strip_prefix(&sync_folder_path) {
                                Ok(p) => p.to_path_buf(),
                                Err(e) => {
                                    error!(
                                        "Failed to create relative path for {:?} from base {:?}: {}",
                                        file_path, sync_folder_path, e
                                    );
                                    // If we can't determine the relative path, we can't form a meaningful gossip message.
                                    return;
                                }
                            }.to_string_lossy().into_owned();

                            let file_name = match file_path.file_name() {
                                Some(name_os_str) => name_os_str.to_string_lossy().into_owned(),
                                None => {
                                    error!("Failed to get file name from path: {:?}", file_path);
                                    // If there's no file name, we can't form a meaningful gossip message.
                                    return;
                                }
                            };

                            let gossip_message = GossipEventPayload {
                                from: endpoint.node_id(),
                                topic: topic_id, // Use the cloned and verified topic_id
                                message_content: iroh_ticket.clone(),
                                file_name,     // This is now a String, shorthand is fine
                                relative_path, // This is now a PathBuf, shorthand is fine
                            };
                            info!("gossip message created {:?}", gossip_message);
                            match sender.broadcast(gossip_message.to_vec().into()).await {
                                Ok(_) => info!("Gossiped ticket: {}", iroh_ticket),
                                Err(e) => {
                                    error!("Failed to gossip ticket {}: {:?}", iroh_ticket, e);
                                }
                            }
                        } else {
                            warn!(
                                "Gossip sender not available. Ticket {} for file {:?} created but not gossiped.",
                                iroh_ticket, file_path
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
    blobs: Blobs<Store>,
    sync_path: PathBuf,
    mut receiver: GossipReceiver,
) -> Result<()> {
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
                    let payload = GossipEventPayload::from_bytes(&msg.content).unwrap();
                    let sync_path_clone = sync_path.clone();
                    info!("GossipEventPayload: {:?}", payload);
                    let payload_clone = payload.clone();
                    let blobs_clone = blobs.clone();
                    tauri::async_runtime::spawn(async move {
                        let str_iroh_ticket = payload_clone.message_content;
                        let dest_path = sync_path_clone.join(&payload_clone.relative_path);
                        match get_iroh_blob(blobs_clone, str_iroh_ticket, dest_path).await {
                            Ok(_) => {
                                info!("Fetching Iroh blob from the ticket");
                            }
                            Err(e) => {
                                error!("Error fetching iroh blob from the ticket {}", e);
                            }
                        }
                    });

                    if let Err(e) = app_handle.emit("gossip://message", payload) {
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
