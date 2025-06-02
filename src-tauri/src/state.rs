// src-tauri/src/state.rs
use iroh::endpoint::Endpoint;
use iroh::protocol::Router;
use iroh_blobs::net_protocol::Blobs;
use iroh_gossip::{
    net::{Gossip, GossipSender},
    proto::TopicId,
};
use crate::clipboard_monitor::ClipboardMonitor; // Add this
use std::{path::PathBuf, sync::Arc};
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
    pub gossip_topic: Arc<Mutex<Option<TopicId>>>,
    pub gossip_sender: Arc<Mutex<Option<GossipSender>>>,
    // --- Active Handles ---
    /// Handle for the main Iroh Router task. Essential for shutdown.
    pub router: Option<Router>,

    pub sync_folder: PathBuf,
    pub sync_task_handle: Option<JoinHandle<()>>,
    pub clipboard_monitor: Option<Arc<ClipboardMonitor>>, // Add this
}
