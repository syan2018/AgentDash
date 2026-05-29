use std::collections::HashMap;
use std::sync::Arc;

use axum::{Json, extract::State};

use crate::dto::{AgentInfoResponse, ConnectorInfoResponse, DiscoveryResponse};
use crate::{app_state::AppState, rpc::ApiError};

pub async fn get_discovery(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DiscoveryResponse>, ApiError> {
    let connector = &state.services.connector;
    let connector_info = ConnectorInfoResponse {
        id: connector.connector_id().to_string(),
        connector_type: connector.connector_type(),
        capabilities: connector.capabilities(),
    };

    // CompositeConnector.list_executors() 现已包含 relay 执行器
    let mut merged: HashMap<String, AgentInfoResponse> = HashMap::new();
    for info in connector.list_executors() {
        merged.insert(
            info.id.clone(),
            AgentInfoResponse {
                id: info.id,
                name: info.name,
                variants: info.variants,
                available: info.available,
                backend_ids: Vec::new(),
            },
        );
    }

    // 丰富 backend_ids 信息（保留前端的"在哪个后端可用"展示）
    for backend in state.services.backend_registry.list_online().await {
        for ex in &backend.capabilities.executors {
            if let Some(existing) = merged.get_mut(&ex.id) {
                existing.backend_ids.push(backend.backend_id.clone());
            }
        }
    }

    let mut executors: Vec<AgentInfoResponse> = merged.into_values().collect();
    executors.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(DiscoveryResponse {
        connector: connector_info,
        executors,
    }))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new().route("/agents/discovery", axum::routing::get(get_discovery))
}
