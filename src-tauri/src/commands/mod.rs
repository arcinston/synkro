// This file will declare the sub-modules and re-export their public functions.
pub mod node_commands;
pub mod blob_commands;
pub mod gossip_commands;
pub mod setup_commands;
pub mod clipboard_commands; // Added

pub use node_commands::get_node_info;
pub use blob_commands::{get_blob, create_ticket};
pub use gossip_commands::{create_gossip_ticket, join_gossip};
pub use setup_commands::{setup_iroh_and_fs, handle_setup};
pub use clipboard_commands::{enable_clipboard_sharing, disable_clipboard_sharing, is_clipboard_sharing_enabled}; // Added
