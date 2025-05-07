use std::path::PathBuf;

use crate::{
    fs_watcher::{FsEventPayload, FsEventType},
    state::AppState,
};
use anyhow::{Error, Result};
use iroh::{protocol::Router, Endpoint, NodeAddr, SecretKey};
use iroh_blobs::{
    net_protocol::Blobs, rpc::client::blobs::WrapOption, store::fs::Store, ticket::BlobTicket,
    util::SetTagOption,
};
use iroh_gossip::{net::Gossip, proto::TopicId};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::fmt;
use tauri::{AppHandle, Manager, State};

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

// The `Display` trait allows us to use the `to_string`
// method on `Ticket`.
impl fmt::Display for GossipTicket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut text = data_encoding::BASE32_NOPAD.encode(&self.to_bytes()[..]);
        text.make_ascii_lowercase();
        write!(f, "{}", text)
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

    // build the blobs protocol
    let blobs = Blobs::persistent(blobs_root)
        .await?
        .build(builder.endpoint());

    let gossip = Gossip::builder().spawn(endpoint.clone()).await?;

    // build the docs protocol

    let router = builder
        .accept(iroh_blobs::ALPN, blobs.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn()
        .await?;

    // Get the router handle for shutdown management

    // Create the state struct with all necessary components
    let app_state = AppState {
        endpoint: Some(endpoint),
        blobs: Some(blobs),   // Store the handler
        gossip: Some(gossip), // Store the handler
        router: Some(router), // Store the router
        sync_folder: None,
        sync_task_handle: None,
    };

    // Wrap in the Tauri state wrapper
    handle.manage(app_state);

    // Return the AppState instance to be managed by Tauri
    Ok(())
}

pub async fn create_iroh_gossip_ticket(
    gossip: Gossip,
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
    let state: State<'_, AppState> = handle.state();

    match payload.event_type {
        FsEventType::Create => {
            let blobs = state.blobs.clone().unwrap();
            let endpoint = state.endpoint.clone().unwrap();

            tauri::async_runtime::spawn(async move {
                let res = create_iroh_ticket(blobs, endpoint, payload.path.clone()).await;

                match res {
                    Ok(iroh_ticket) => {
                        info!("Created Iroh Ticket Successfully for {:?}", payload.path);
                        info!("Ticket is  {} ", iroh_ticket);

                        // TODO: Add Gossip here
                    }
                    Err(err) => {
                        error!("Ticket Creation failed {}", err)
                    }
                }
            });
        }
        FsEventType::Remove => {
            info!("Removing this file {:?}", payload.path);
        }
        _ => {}
    }
}
