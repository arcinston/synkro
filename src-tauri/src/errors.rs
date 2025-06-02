use thiserror::Error;
use iroh::blobs::store::fs::AddFromPathError;
use iroh::ticket::BlobTicketParseError;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Iroh client not initialized: {0}")]
    IrohClientNotInitialized(String),

    #[error("Filesystem path error: {0}")]
    PathError(String),

    #[error("Iroh operation failed: {0}")]
    IrohError(#[from] IrohError),

    #[error("Failed to generate ticket: {0}")]
    TicketGenerationError(String),

    #[error("Failed to parse ticket: {0}")]
    TicketParseError(String), // Consider if this should be #[from] BlobTicketParseError directly or via IrohError

    #[error("Store operation failed: {0}")]
    StoreError(String), // For tauri_plugin_store errors

    #[error("Gossip join failed: {0}")]
    GossipJoinError(String),

    #[error("Initialization failed: {0}")]
    InitializationError(String),

    #[error("Serialization/Deserialization error: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("Underlying Anyhow error: {0}")]
    AnyhowError(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum IrohError {
    #[error("Iroh endpoint error: {0}")]
    EndpointError(#[from] iroh::endpoint::Error),

    #[error("Iroh blobs RPC client error: {0}")]
    BlobsRpcError(#[from] iroh::blobs::rpc::client::blobs::Error),

    #[error("Iroh AddFromPathError: {0}")]
    AddFromPathError(#[from] AddFromPathError),

    #[error("Iroh export error: {0}")]
    ExportError(#[from] iroh::blobs::store::ExportError),

    #[error("Iroh ticket parse error: {0}")]
    TicketParseError(#[from] BlobTicketParseError),

    #[error("Iroh gossip error: {0}")]
    GossipError(#[from] iroh_gossip::net::GossipError),

    #[error("Iroh gossip subscription error: {0}")]
    GossipSubscribeError(String), // Placeholder, as iroh_gossip::net::Gossip::subscribe returns Result<_, GossipError>

    #[error("Iroh general error: {0}")]
    General(String), // For cases where a specific iroh error type isn't available or suitable

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Iroh persistent store error: {0}")]
    PersistentStoreError(#[from] iroh::blobs::store::fs::PersistentError),

    #[error("Iroh blob ticket format error: {0}")]
    BlobTicketFormatError(#[from] iroh::ticket::BlobTicketFormatError),

    #[error("Underlying Anyhow error in Iroh Fns: {0}")]
    AnyhowError(#[from] anyhow::Error), // For general anyhow errors within iroh_fns
}

// Convert CommandError to String for Tauri command results
// This is required because Tauri commands must return Result<T, String>
impl From<CommandError> for String {
    fn from(err: CommandError) -> Self {
        err.to_string()
    }
}

// It's generally better to handle IrohError within CommandError using #[from]
// and not convert IrohError directly to String for Tauri commands,
// but if needed for other purposes, it can be implemented.
// We'll rely on CommandError's From<IrohError> for the Tauri boundary.
