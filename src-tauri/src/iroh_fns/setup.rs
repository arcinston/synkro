use crate::{errors::IrohError, state::AppState}; // Changed
// use anyhow::Result; // Replaced by IrohError
use crate::clipboard_monitor::ClipboardMonitor; // Added
use iroh::{protocol::Router, Endpoint, RelayMode, SecretKey};
use iroh_blobs::{net_protocol::Blobs, store::fs::Store as BlobStore};
use iroh_gossip::net::Gossip;
use log::{error, info, warn}; // Added error, warn
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

pub async fn setup<R: tauri::Runtime>(
    handle: tauri::AppHandle<R>,
    sync_path: PathBuf,
) -> Result<(), IrohError> { // Changed from anyhow::Result
    let data_root = handle.path().app_data_dir().map_err(|e| {
        IrohError::General(format!("Failed to get app data dir: {}", e))
    })?;

    let blobs_root = data_root.join("blob_data");
    std::fs::create_dir_all(&blobs_root)?; // Ensure blobs_root exists

    let secret_key_path = data_root.join("secret_key");
    let secret_key = match secret_key_path.exists() {
        true => {
            info!("Loading secret key from {:?}", &secret_key_path);
            let bytes = std::fs::read(&secret_key_path)?;
            let key_bytes_array: [u8; 32] =
                bytes.as_slice().try_into().map_err(|_e| {
                    IrohError::General(format!(
                        "Secret key file {:?} has incorrect size: expected 32 bytes, found {}.",
                        secret_key_path,
                        bytes.len()
                    ))
                })?;
            SecretKey::from_bytes(&key_bytes_array) // This itself returns SecretKey, not Result
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
        .await?; // Returns Result<_, iroh::endpoint::Error>, uses From
    info!("> our node id: {}", endpoint.node_id()); // Changed println to info

    let builder = Router::builder(endpoint.clone());

    let blobs = Blobs::persistent(blobs_root)
        .await? // Returns Result<_, iroh::blobs::store::fs::PersistentError>, uses From
        .build(builder.endpoint());

    let gossip = Gossip::builder().spawn(endpoint.clone()).await?; // Returns Result<_, iroh_gossip::net::GossipError>, uses From

    let router = builder
        .accept(iroh_blobs::ALPN, blobs.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn()
        .await?; // Returns Result<_, iroh::endpoint::Error>, uses From

    // Initialize ClipboardMonitor
    let clipboard_monitor_instance = match ClipboardMonitor::new() {
        Ok(monitor) => Some(Arc::new(monitor)),
        Err(e) => {
            error!("Failed to initialize ClipboardMonitor: {:?}", e);
            None
        }
    };

    // Construct AppState once with all components
    let app_state = AppState {
        endpoint: Some(endpoint),
        blobs: Some(blobs),
        gossip: Some(gossip),
        router: Some(router),
        gossip_topic: Arc::new(Mutex::new(None)),
        gossip_sender: Arc::new(Mutex::new(None)),
        sync_folder, // from args
        sync_task_handle: None,
        clipboard_monitor: clipboard_monitor_instance.clone(), // Store the Arc
    };

    handle.manage(app_state);

    // Start clipboard monitoring if initialized
    if let Some(monitor_arc) = clipboard_monitor_instance {
        let app_handle_clone = handle.clone();
        tauri::async_runtime::spawn(async move {
            monitor_arc.start_monitoring(app_handle_clone).await;
        });
        info!("Clipboard monitoring task spawned.");
    } else {
        warn!("Clipboard monitor was not initialized, so not starting it.");
    }

    Ok(())
}
