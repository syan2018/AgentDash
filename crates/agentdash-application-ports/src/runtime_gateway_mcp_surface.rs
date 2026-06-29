use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorError};
use agentdash_spi::{AuthIdentity, CapabilityState, RuntimeMcpServer, RuntimeVfsAccessPolicy, Vfs};
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeGatewayMcpSurfaceQueryPurpose {
    pub component: String,
}

impl RuntimeGatewayMcpSurfaceQueryPurpose {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeGatewayMcpSurface {
    pub runtime_session_id: String,
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub vfs_access_policy: RuntimeVfsAccessPolicy,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub active_turn_id: Option<String>,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone)]
pub struct RuntimeGatewayMcpSurfaceWithBackend {
    pub surface: RuntimeGatewayMcpSurface,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RuntimeGatewayMcpSurfaceQueryError {
    pub message: String,
    pub runtime_backend_anchor_error: Option<RuntimeBackendAnchorError>,
}

impl RuntimeGatewayMcpSurfaceQueryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            runtime_backend_anchor_error: None,
        }
    }

    pub fn with_runtime_backend_anchor_error(
        message: impl Into<String>,
        error: RuntimeBackendAnchorError,
    ) -> Self {
        Self {
            message: message.into(),
            runtime_backend_anchor_error: Some(error),
        }
    }
}

#[async_trait]
pub trait RuntimeGatewayMcpSurfaceQueryPort: Send + Sync {
    async fn current_runtime_mcp_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeGatewayMcpSurfaceQueryPurpose,
    ) -> Result<RuntimeGatewayMcpSurfaceWithBackend, RuntimeGatewayMcpSurfaceQueryError>;
}
