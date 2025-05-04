// src-tauri/src/commands.rs

use crate::state::AppState; // Import the AppState wrapper
use anyhow::{anyhow, Context, Result};
use futures_lite::StreamExt;
// Import necessary types for blobs and docs interaction
use iroh::{
    base::node_addr::AddrInfoOptions, // For sharing options
    client::docs::ShareMode, // For sharing mode (even without client struct, types are needed)
};
use iroh_blobs::{export::ExportProgress, store::ExportFormat, util::SetTagOption, Tag};
use iroh_docs::{
    store::Query, // To query document entries
    sync::Entry,
    AuthorId,
    ContentStatus,
    Doc, // Import Doc handle type
    NamespaceId,
    PeerSource,
};
use log::{debug, error, info, trace};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State, Window};

// --- Frontend Event Payloads --- (Keep existing ones)

#[derive(Clone, Serialize)]
struct UploadProgress {
    file_path: String,
    #[serde(rename = "type")]
    event_type: String, // "found", "progress", "done", "error"
    size: Option<u64>,
    offset: Option<u64>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    filename: String,
    #[serde(rename = "type")]
    event_type: String, // "started", "progress", "complete", "error"
    size: Option<u64>,
    offset: Option<u64>,
    error: Option<String>,
    download_path: Option<String>,
}

// --- Command-Specific Structs --- (Keep existing ones)

#[derive(Clone, Serialize, Debug)]
pub struct FileEntryInfo {
    filename: String,
    hash: String,
    size: u64,
}

#[derive(Clone, Serialize, Debug)]
pub struct ShareTicketResponse {
    ticket: String,
}

#[derive(Clone, Serialize, Debug)]
pub struct NodeInfo {
    node_id: Option<String>,
    author_id: Option<String>,
    namespace_id: Option<String>,
    gossip_topic: Option<String>,
}

// --- Tauri Commands ---

/// **Helper function to get the specific Doc handle from state.**
/// This avoids repeating the logic in every command.
async fn get_doc_handle(state: &State<'_, AppState>) -> Result<Doc, String> {
    let state_guard = state.0.lock().await;
    let docs_handler = state_guard
        .docs // Access the stored DocsHandler
        .clone()
        .context("Docs handler not initialized")
        .map_err(|e| e.to_string())?;
    let namespace_id = state_guard
        .namespace_id // Access the stored NamespaceId
        .context("Namespace ID not initialized")
        .map_err(|e| e.to_string())?;

    // Use the handler to open the specific document
    let doc = docs_handler
        .open(namespace_id)
        .await
        .map_err(|e| format!("Failed to open doc {}: {}", namespace_id, e))?
        .ok_or(format!("Doc {} not found via handler", namespace_id))?;
    Ok(doc)
}

/// Imports a local file into the synchronized document.
#[tauri::command]
pub async fn send_file(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    file_path: String,
) -> Result<(), String> {
    info!("Received send_file request for: {}", file_path);
    let path = PathBuf::from(file_path.clone());

    if !path.is_file() {
        return Err(format!("Invalid file path: {}", file_path));
    }

    let filename = path
        .file_name()
        .ok_or_else(|| format!("Could not get filename from path: {}", file_path))?
        .to_string_lossy()
        .to_string();

    let key = filename.as_bytes().to_vec();

    // Get AuthorId from state
    let author_id = {
        let state_guard = state.0.lock().await;
        state_guard
            .author_id
            .context("Author ID not initialized")
            .map_err(|e| e.to_string())?
    };

    // Get the specific Doc handle using the helper
    let doc = get_doc_handle(&state).await?;

    info!("Importing file '{}' into doc {}", filename, doc.id());

    let tag = Tag::from_bytes(filename.as_bytes());

    // Import the file using the obtained Doc handle
    let mut import_stream = doc // <-- Use the doc handle directly
        .import_file(
            author_id,
            key,
            &path,
            SetTagOption::Named(tag),
            false, // in_place = false
        )
        .await
        .map_err(|e| format!("Failed to start import for {}: {}", filename, e))?;

    // Handle import progress events (logic remains the same)
    while let Some(progress) = import_stream.next().await {
        match progress {
            Ok(p) => {
                trace!("Import progress: {:?}", p);
                match p {
                    iroh::client::docs::ImportProgress::Found { name, size, .. } => {
                        let _ = app_handle.emit(
                            "upload-progress",
                            UploadProgress {
                                file_path: file_path.clone(),
                                event_type: "found".to_string(),
                                size: Some(size),
                                offset: None,
                                error: None,
                            },
                        );
                    }
                    iroh::client::docs::ImportProgress::Progress { offset, .. } => {
                        let _ = app_handle.emit(
                            "upload-progress",
                            UploadProgress {
                                file_path: file_path.clone(),
                                event_type: "progress".to_string(),
                                size: None,
                                offset: Some(offset),
                                error: None,
                            },
                        );
                    }
                    iroh::client::docs::ImportProgress::IngestDone { .. } => {
                        let _ = app_handle.emit(
                            "upload-progress",
                            UploadProgress {
                                file_path: file_path.clone(),
                                event_type: "done".to_string(),
                                size: None,
                                offset: None,
                                error: None,
                            },
                        );
                        info!("Successfully imported file '{}'", filename);
                    }
                    iroh::client::docs::ImportProgress::AllDone { .. } => {
                        info!("Import stream finished for file '{}'", filename);
                        break;
                    }
                    iroh::client::docs::ImportProgress::Abort(e) => {
                        error!("Import aborted for {}: {:?}", filename, e);
                        let _ = app_handle.emit(
                            "upload-progress",
                            UploadProgress {
                                file_path: file_path.clone(),
                                event_type: "error".to_string(),
                                size: None,
                                offset: None,
                                error: Some(format!("Import aborted: {}", e)),
                            },
                        );
                        return Err(format!("Import aborted: {}", e));
                    }
                }
            }
            Err(e) => {
                error!("Import error for {}: {:?}", filename, e);
                let _ = app_handle.emit(
                    "upload-progress",
                    UploadProgress {
                        file_path: file_path.clone(),
                        event_type: "error".to_string(),
                        size: None,
                        offset: None,
                        error: Some(format!("Import failed: {}", e)),
                    },
                );
                return Err(format!("Import failed: {}", e));
            }
        }
    }
    Ok(())
}

/// Retrieves basic information about the running Iroh node and sync setup.
#[tauri::command]
pub async fn get_node_info(state: State<'_, AppState>) -> Result<NodeInfo, String> {
    let state_guard = state.0.lock().await;
    // Access fields directly from the state guard
    let node_id = state_guard
        .endpoint // Access the endpoint stored in state
        .as_ref()
        .map(|ep| ep.node_id().to_string());
    let author_id = state_guard.author_id.map(|a| a.to_string());
    let namespace_id = state_guard.namespace_id.map(|n| n.to_string());
    let gossip_topic = state_guard.gossip_topic.map(|t| t.to_string());

    Ok(NodeInfo {
        node_id,
        author_id,
        namespace_id,
        gossip_topic,
    })
}

/// Lists all file entries currently present in the synchronized document.
#[tauri::command]
pub async fn list_files(state: State<'_, AppState>) -> Result<Vec<FileEntryInfo>, String> {
    info!("Received list_files request");

    // Get the specific Doc handle using the helper
    let doc = get_doc_handle(&state).await?;

    info!("Querying entries for doc {}", doc.id());

    // Query all entries using the Doc handle
    let mut entries_stream = doc // <-- Use the doc handle directly
        .get_many(Query::all())
        .await
        .map_err(|e| format!("Failed to query entries: {}", e))?;

    let mut file_list = Vec::new();
    while let Some(entry_result) = entries_stream.next().await {
        match entry_result {
            Ok(entry) => {
                let filename = String::from_utf8_lossy(entry.key()).to_string();
                let info = FileEntryInfo {
                    filename,
                    hash: entry.content_hash().to_string(),
                    size: entry.content_len(),
                };
                trace!("Found entry: {:?}", info);
                file_list.push(info);
            }
            Err(e) => {
                error!("Error retrieving entry: {}", e);
            }
        }
    }

    info!("Found {} file entries.", file_list.len());
    Ok(file_list)
}

/// Generates a shareable ticket for the current document.
#[tauri::command]
pub async fn get_share_ticket(state: State<'_, AppState>) -> Result<ShareTicketResponse, String> {
    info!("Received get_share_ticket request");

    // Get the specific Doc handle using the helper
    let doc = get_doc_handle(&state).await?;

    info!("Generating share ticket for doc {}", doc.id());

    // Share using the Doc handle
    let ticket = doc // <-- Use the doc handle directly
        .share(ShareMode::Read, AddrInfoOptions::default())
        .await
        .map_err(|e| format!("Failed to create share ticket: {}", e))?;

    let ticket_string = ticket.to_string();
    info!("Generated ticket: {}", ticket_string);

    Ok(ShareTicketResponse {
        ticket: ticket_string,
    })
}
