use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::{
    app_state::AppState,
    rpc::ApiError,
};
use agentdash_executor::connector::{ConnectorCapabilities, ConnectorType, ExecutorInfo as ConnectorExecutorInfo};

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorInfoResponse {
    pub id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorInfoResponse {
    pub id: String,
    pub connector_type: ConnectorType,
    pub capabilities: ConnectorCapabilities,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryResponse {
    pub connector: ConnectorInfoResponse,
    pub executors: Vec<ExecutorInfoResponse>,
}

pub async fn get_discovery(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DiscoveryResponse>, ApiError> {
    let connector = &state.connector;
    let connector_info = ConnectorInfoResponse {
        id: connector.connector_id().to_string(),
        connector_type: connector.connector_type(),
        capabilities: connector.capabilities(),
    };

    let executors: Vec<ExecutorInfoResponse> = connector
        .list_executors()
        .into_iter()
        .map(|ConnectorExecutorInfo { id, name, variants, available }| ExecutorInfoResponse {
            id,
            name,
            variants,
            available,
        })
        .collect();

    Ok(Json(DiscoveryResponse {
        connector: connector_info,
        executors,
    }))
}
