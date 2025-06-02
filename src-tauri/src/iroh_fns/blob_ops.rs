use crate::errors::IrohError; // Added
// use anyhow::{Error, Result}; // Replaced by IrohError
use iroh_blobs::{
    net_protocol::Blobs,
    // rpc::client::blobs::WrapOption, // Not used here
    store::{fs::Store, ExportFormat, ExportMode},
    ticket::BlobTicket,
    // util::SetTagOption, // This might not be needed here
};
// use iroh::Endpoint; // Not directly used by get_iroh_blob
use std::path::PathBuf;
// use log::info; // Not used in get_iroh_blob directly

pub async fn get_iroh_blob(
    blobs: Blobs<Store>,
    str_ticket: String,
    dest_path: PathBuf,
) -> Result<(), IrohError> { // Changed
    let blobs_client = blobs.client();
    let ticket: BlobTicket = str_ticket.parse()?; // Uses From<BlobTicketParseError>

    // Ensure parent directory exists
    if let Some(parent_dir) = dest_path.parent() {
        if !parent_dir.exists() {
            std::fs::create_dir_all(parent_dir)?; // Uses From<std::io::Error>
        }
    }

    let download_req = blobs_client
        .download(ticket.hash(), ticket.node_addr().clone())
        .await?; // Uses From<iroh::blobs::rpc::client::blobs::Error>
    download_req.finish().await?; // Uses From<iroh::blobs::rpc::client::blobs::Error>

    blobs_client
        .export(
            ticket.hash(),
            dest_path.clone(),
            ExportFormat::Blob,
            ExportMode::Copy,
        )
        .await? // Uses From<iroh::blobs::store::ExportError>
        .finish()
        .await?; // Uses From<iroh::blobs::store::ExportError>
    Ok(())
}
