use crate::state::AppState;
use anyhow::Result;
use iroh::{protocol::Router, Endpoint};
use iroh_blobs::net_protocol::Blobs;
use iroh_gossip::net::Gossip;
use tauri::Manager;

pub async fn setup<R: tauri::Runtime>(handle: tauri::AppHandle<R>) -> Result<()> {
    let data_root = handle.path().app_data_dir()?;

    let blobs_root = data_root.join("blob_data");

    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
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
