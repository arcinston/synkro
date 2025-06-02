use crate::{errors::CommandError, state::AppState};
use iroh::PublicKey;
use serde::Serialize;
use tauri::State;

#[derive(Clone, Serialize, Debug)]
pub struct NodeInfo {
    node_id: Option<PublicKey>,
}

#[tauri::command]
pub async fn get_node_info(state: State<'_, AppState>) -> Result<NodeInfo, CommandError> {
    let endpoint = state.endpoint.clone();

    let node_id = match endpoint {
        Some(e) => Some(e.node_id()),
        None => None,
    };

    Ok(NodeInfo { node_id })
}
