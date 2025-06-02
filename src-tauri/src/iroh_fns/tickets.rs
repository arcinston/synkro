use crate::errors::IrohError; // Added
// use anyhow::{Error, Result}; // Replaced Error, Result kept for GossipTicket internal methods
use anyhow::Result; // For GossipTicket internal methods
use iroh::{Endpoint, NodeAddr};
use iroh_blobs::{
    net_protocol::Blobs,
    rpc::client::blobs::WrapOption,
    store::fs::Store,
    ticket::BlobTicket,
    util::SetTagOption,
};
use iroh_gossip::proto::TopicId;
use log::info;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize, Clone)] // Added Clone
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

pub async fn create_iroh_gossip_ticket(
    endpoint: Endpoint,
    topic_id: TopicId,
) -> Result<String, IrohError> { // Changed
    let me = endpoint.node_addr().await?; // Uses From<iroh::endpoint::Error>
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
) -> Result<String, IrohError> { // Changed
    let blobs_client = blobs.client();
    let add_progress = blobs_client
        .add_from_path(path, true, SetTagOption::Auto, WrapOption::NoWrap)
        .await?; // Uses From<AddFromPathError>
    let blob = add_progress.finish().await?; // Uses From<AddFromPathError>
    let node_id = endpoint.node_id();
    let ticket = BlobTicket::new(node_id.into(), blob.hash, blob.format)?; // Uses From<BlobTicketFormatError>
    let str_ticket = ticket.to_string();
    info!("created str ticket for ticket {}", str_ticket);
    Ok(str_ticket)
}
