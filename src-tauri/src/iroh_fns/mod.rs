// This file (src-tauri/src/iroh_fns/mod.rs) now declares the sub-modules
// and re-exports their public functions and structs.

pub mod blob_ops;
pub mod gossip_ops;
pub mod setup;
pub mod tickets;

pub use blob_ops::get_iroh_blob;
pub use gossip_ops::{handle_fs_payload, join_iroh_gossip, subscribe_loop};
pub use setup::setup;
pub use tickets::{create_iroh_gossip_ticket, create_iroh_ticket, GossipTicket};

// Note: The GossipEventPayload is currently defined in `crate::commands::gossip_commands`
// If it were to be used more broadly by iroh_fns, it might be better to move it here
// or to a more general `crate::payloads` module.
// For now, `gossip_ops.rs` imports it from `crate::commands::gossip_commands`.

// Similarly, FsEventPayload and FsEventType are from `crate::fs_watcher`.
// These dependencies on other modules are fine as long as they don't create circular dependencies.
