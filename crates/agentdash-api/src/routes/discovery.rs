use std::collections::HashMap;
use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::{app_state::AppState, rpc::ApiError};
use agentdash_executor::connector::{
    ConnectorCapabilities, ConnectorType, ExecutorInfo as ConnectorExecutorInfo,
};

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorInfoResponse {
    pub id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
    /// 该执行器可用的后端 ID 列表（空 = 仅本机）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backend_ids: Vec<String>,
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

    let mut merged: HashMap<String, ExecutorInfoResponse> = HashMap::new();

    for info in connector.list_executors() {
        let ConnectorExecutorInfo { id, name, variants, available } = info;
        merged.insert(id.clone(), ExecutorInfoResponse {
            id,
            name,
            variants,
            available,
            backend_ids: Vec::new(),
        });
    }

    for backend in state.backend_registry.list_online().await {
        for ex in &backend.capabilities.executors {
            match merged.get_mut(&ex.id) {
                Some(existing) => {
                    if ex.available && !existing.available {
                        existing.available = true;
                    }
                    existing.backend_ids.push(backend.backend_id.clone());
                }
                None => {
                    merged.insert(ex.id.clone(), ExecutorInfoResponse {
                        id: ex.id.clone(),
                        name: ex.name.clone(),
                        variants: ex.variants.clone(),
                        available: ex.available,
                        backend_ids: vec![backend.backend_id.clone()],
                    });
                }
            }
        }
    }

    let mut executors: Vec<ExecutorInfoResponse> = merged.into_values().collect();
    executors.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(DiscoveryResponse {
        connector: connector_info,
        executors,
    }))
}
