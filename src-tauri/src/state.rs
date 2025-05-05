// src-tauri/src/state.rs

use iroh::endpoint::Endpoint;
use iroh::protocol::Router; // Use the specific handle type from Router::spawn
use iroh::NodeId;
// Import the specific handler types used in your setup.rs
use iroh_blobs::net_protocol::Blobs;
use iroh_gossip::net::Gossip;
// Import other types needed later (optional for now)
// use iroh_docs::{Doc, NamespaceId};
// use iroh_gossip::TopicId;
// use anyhow::Result;
use std::path::PathBuf;
// use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Holds the core state based on the setup function provided.
/// Stores the Endpoint and the protocol handlers needed for later interaction.
#[derive(Default)] // Handlers might not implement Debug easily
pub struct AppState {
    // --- Core Iroh Components ---
    /// The network endpoint managing connections and identity.
    pub endpoint: Option<Endpoint>,

    // --- Protocol Handlers ---
    /// Handler for the iroh-blobs protocol.
    pub blobs: Option<Blobs<iroh_blobs::store::fs::Store>>,

    /// Handler for the iroh-gossip protocol.
    pub gossip: Option<Gossip>,

    // --- Active Handles ---
    /// Handle for the main Iroh Router task. Essential for shutdown.
    pub router: Option<Router>,

    pub sync_folder: Option<PathBuf>,
    pub sync_task_handle: Option<JoinHandle<()>>,
}

impl AppState {
    /// Creates a new `AppState` with default (empty) `SyncState`.
    /// The actual values will be populated after setup completes.
    pub fn new() -> Self {
        AppState::default()
    }

    // Example helper to get NodeId
    pub fn node_id(&self) -> Option<NodeId> {
        let endpoint = self.endpoint.clone();
        match endpoint {
            Some(endpoint) => Some(endpoint.node_id()),
            None => None,
        }
    }
}
