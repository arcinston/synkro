// src-tauri/src/state.rs

use iroh::endpoint::Endpoint;
use iroh::protocol::Router; // Use the specific handle type from Router::spawn
use iroh::NodeId;
// Import the specific handler types used in your setup.rs
use iroh_blobs::net_protocol::Blobs;
use iroh_docs::{protocol::Docs, AuthorId};
use iroh_gossip::net::Gossip;
// Import other types needed later (optional for now)
// use iroh_docs::{Doc, NamespaceId};
// use iroh_gossip::TopicId;
// use anyhow::Result;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Holds the core state based on the setup function provided.
/// Stores the Endpoint and the protocol handlers needed for later interaction.
#[derive(Default)] // Handlers might not implement Debug easily
pub struct SyncState {
    // --- Core Iroh Components ---
    /// The network endpoint managing connections and identity.
    pub endpoint: Option<Endpoint>,

    // --- Protocol Handlers ---
    /// Handler for the iroh-blobs protocol.
    pub blobs: Option<Blobs<iroh_blobs::store::fs::Store>>,

    /// Handler for the iroh-gossip protocol.
    pub gossip: Option<Gossip>,

    /// Handler for the iroh-docs protocol.
    pub docs: Option<Docs<iroh_blobs::store::fs::Store>>,

    // --- Identity ---
    /// The Author ID created during setup.
    /// NOTE: Your current setup creates a *new random* author each time.
    /// Consider making this persistent if needed.
    pub author_id: Option<AuthorId>,

    // --- Active Handles ---
    /// Handle for the main Iroh Router task. Essential for shutdown.
    pub router: Option<Router>, // Store the router's handle

    // --- Fields for specific sync logic (Added later) ---
    // pub namespace_id: Option<NamespaceId>,
    // pub gossip_topic: Option<TopicId>,
    // pub sync_doc_handle: Option<Doc>,
    // pub gossip_discovery_handle: Option<JoinHandle<Result<()>>>,
    pub sync_folder: Option<PathBuf>,
    pub sync_task_handle: Option<JoinHandle<()>>,
}

/// Tauri managed state wrapper.
pub struct AppState(pub Mutex<SyncState>);

impl AppState {
    /// Creates a new `AppState` with default (empty) `SyncState`.
    /// The actual values will be populated after setup completes.
    pub fn new() -> Self {
        AppState(Mutex::new(SyncState::default()))
    }

    // Example helper to get NodeId
    pub fn node_id(&self) -> Option<NodeId> {
        self.0
            .try_lock()
            .ok()
            .and_then(|guard| guard.endpoint.as_ref().map(|ep| ep.node_id()))
    }
}
