use crate::state::{AppState, SyncState};
use anyhow::Result;
use iroh::{protocol::Router, Endpoint};
use iroh_blobs::net_protocol::Blobs;
use iroh_docs::{protocol::Docs, Author};
use iroh_gossip::net::Gossip;
use rand::rngs::OsRng;
use tauri::Manager;
use tokio::sync::Mutex;

pub async fn setup<R: tauri::Runtime>(handle: tauri::AppHandle<R>) -> Result<(AppState)> {
    let data_root = handle.path().app_data_dir()?;

    let blobs_root = data_root.join("blob_data");
    let docs_root = data_root.join("doc_data");

    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    println!("> our node id: {}", endpoint.node_id());

    let builder = Router::builder(endpoint.clone());

    // build the blobs protocol
    let blobs = Blobs::persistent(blobs_root)
        .await?
        .build(builder.endpoint());

    let gossip = Gossip::builder().spawn(endpoint.clone()).await?;

    // build the docs protocol
    let docs = Docs::persistent(docs_root).spawn(&blobs, &gossip).await?;

    let router = builder
        .accept(iroh_blobs::ALPN, blobs.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .accept(iroh_docs::ALPN, docs.clone())
        .spawn()
        .await?;

    let author = Author::new(&mut OsRng);

    let author = Author::new(&mut OsRng);
    let author_id = author.id(); // Get the ID

    // Get the router handle for shutdown management

    // Create the state struct with all necessary components
    let sync_state = SyncState {
        endpoint: Some(endpoint),
        blobs: Some(blobs),   // Store the handler
        gossip: Some(gossip), // Store the handler
        docs: Some(docs),     // Store the handler
        author_id: Some(author_id),
        router: Some(router), // Store the router
        // Initialize other fields as None initially
        // namespace_id: None,
        // gossip_topic: None,
        // sync_doc_handle: None,
        // gossip_discovery_handle: None,
        sync_folder: None,
        sync_task_handle: None,
    };

    // Wrap in the Tauri state wrapper
    let app_state = AppState(Mutex::new(sync_state));

    // Return the AppState instance to be managed by Tauri
    Ok(app_state)
}
