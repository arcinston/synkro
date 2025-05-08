// src-tauri/src/state.rs
use iroh::endpoint::Endpoint;
use iroh::protocol::Router;
use iroh_blobs::net_protocol::Blobs;
use iroh_gossip::net::{Gossip, GossipReceiver, GossipSender};
use std::path::PathBuf;
use tokio::{sync::Mutex, task::JoinHandle};

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
    pub gossip_sender: Mutex<Option<GossipSender>>,
    // --- Active Handles ---
    /// Handle for the main Iroh Router task. Essential for shutdown.
    pub router: Option<Router>,

    pub sync_folder: Option<PathBuf>,
    pub sync_task_handle: Option<JoinHandle<()>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }
}
