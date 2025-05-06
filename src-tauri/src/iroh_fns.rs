use std::path::PathBuf;

use crate::state::AppState;
use anyhow::{Error, Result};
use iroh::{protocol::Router, Endpoint};
use iroh_blobs::{
    net_protocol::Blobs, rpc::client::blobs::WrapOption, store::fs::Store, ticket::BlobTicket,
    util::SetTagOption,
};
use iroh_gossip::net::Gossip;
use log::info;
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
