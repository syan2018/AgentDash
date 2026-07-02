use agentdash_domain::mcp_preset::{McpRoutePolicy, McpRuntimeBindingConfig, McpTransportConfig};
use agentdash_domain::workspace::{WorkspaceBinding, WorkspaceIdentityKind};
use agentdash_spi::AuthIdentity;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const MCP_PROBE_TRANSPORT_ACTION: &str = "mcp.probe_transport";
pub const WORKSPACE_BROWSE_DIRECTORY_ACTION: &str = "workspace.browse_directory";
pub const WORKSPACE_DETECT_ACTION: &str = "workspace.detect";
pub const WORKSPACE_DETECT_GIT_ACTION: &str = "workspace.detect_git";
pub const WORKSPACE_DISCOVER_BY_IDENTITY_ACTION: &str = "workspace.discover_by_identity";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpProbeTransportInput {
    pub transport: McpTransportConfig,
    #[serde(default)]
    pub route_policy: McpRoutePolicy,
    #[serde(default)]
    pub probe_target: McpProbeTarget,
    pub current_user: AuthIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_binding: Option<McpRuntimeBindingConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpProbeTarget {
    #[default]
    DefaultUserLocal,
    Backend {
        backend_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum McpProbeTransportOutput {
    Ok {
        latency_ms: u64,
        tools: Vec<McpProbeToolOutput>,
    },
    Error {
        error: String,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpProbeToolOutput {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDetectInput {
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceDetectOutput {
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub binding: WorkspaceBinding,
    pub confidence: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDetectGitInput {
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDetectGitOutput {
    pub resolved_root_ref: String,
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceBrowseDirectoryInput {
    pub backend_id: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceBrowseDirectoryOutput {
    pub current_path: String,
    pub entries: Vec<WorkspaceBrowseDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceBrowseDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceDiscoverByIdentityInput {
    pub backend_id: String,
    pub workspaces: Vec<WorkspaceDiscoverByIdentityWorkspaceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceDiscoverByIdentityWorkspaceInput {
    pub workspace_id: Uuid,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceDiscoverByIdentityOutput {
    pub candidates: Vec<WorkspaceDiscoverByIdentityCandidateOutput>,
    pub skipped: Vec<WorkspaceDiscoverByIdentitySkippedOutput>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceDiscoverByIdentityCandidateOutput {
    pub workspace_id: Uuid,
    pub root_ref: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub detected_facts: Value,
    pub confidence: String,
    pub display_name: Option<String>,
    pub client_name: Option<String>,
    pub server_address: Option<String>,
    pub stream: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceDiscoverByIdentitySkippedOutput {
    pub workspace_id: Uuid,
    pub identity_kind: WorkspaceIdentityKind,
    pub reason: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeGatewaySetupError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    BackendOffline(String),
    #[error("{0}")]
    TransportFailed(String),
    #[error("{0}")]
    ProviderFailed(String),
    #[error("超时")]
    Timeout,
}

#[async_trait]
pub trait McpProbeSetupPort: Send + Sync {
    async fn probe_transport(
        &self,
        input: McpProbeTransportInput,
    ) -> Result<McpProbeTransportOutput, RuntimeGatewaySetupError>;
}

#[async_trait]
pub trait WorkspaceDetectSetupPort: Send + Sync {
    async fn detect_workspace(
        &self,
        input: WorkspaceDetectInput,
    ) -> Result<WorkspaceDetectOutput, RuntimeGatewaySetupError>;
}

#[async_trait]
pub trait WorkspaceDetectGitSetupPort: Send + Sync {
    async fn detect_git(
        &self,
        input: WorkspaceDetectGitInput,
    ) -> Result<WorkspaceDetectGitOutput, RuntimeGatewaySetupError>;
}

#[async_trait]
pub trait WorkspaceBrowseDirectorySetupPort: Send + Sync {
    async fn browse_directory(
        &self,
        input: WorkspaceBrowseDirectoryInput,
    ) -> Result<WorkspaceBrowseDirectoryOutput, RuntimeGatewaySetupError>;
}

#[async_trait]
pub trait WorkspaceDiscoverByIdentitySetupPort: Send + Sync {
    async fn discover_by_identity(
        &self,
        input: WorkspaceDiscoverByIdentityInput,
    ) -> Result<WorkspaceDiscoverByIdentityOutput, RuntimeGatewaySetupError>;
}
