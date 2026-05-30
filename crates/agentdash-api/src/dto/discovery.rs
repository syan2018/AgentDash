use serde::Serialize;

use agentdash_spi::connector::{ConnectorCapabilities, ConnectorType};

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfoResponse {
    pub id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
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
    pub executors: Vec<AgentInfoResponse>,
}
