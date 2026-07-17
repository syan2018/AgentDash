use agentdash_domain::workspace::WorkspaceIdentityKind;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

/// Application 层端口：后端在线探测 + workspace 检测
///
/// API 层持有 WebSocket 连接池，实现此 trait；
/// Application 层通过此 trait 与远程后端交互，不直接依赖 WS/Relay 传输。
#[async_trait]
pub trait BackendTransport: Send + Sync {
    /// 检测后端是否在线
    async fn is_online(&self, backend_id: &str) -> bool;

    /// 列出所有在线后端 ID
    async fn list_online_backend_ids(&self) -> Vec<String>;

    /// 探测远程路径的工作空间事实（Git / P4 / Local）。
    async fn detect_workspace(
        &self,
        backend_id: &str,
        root: &str,
    ) -> Result<WorkspaceProbeInfo, TransportError>;

    /// 探测远程路径的 Git 仓库信息
    async fn detect_git_repo(
        &self,
        backend_id: &str,
        root: &str,
    ) -> Result<GitRepoInfo, TransportError> {
        Ok(self
            .detect_workspace(backend_id, root)
            .await?
            .git
            .unwrap_or_default())
    }

    /// 浏览远程后端上的目录入口。
    async fn browse_directory(
        &self,
        _backend_id: &str,
        _path: Option<&str>,
    ) -> Result<DirectoryBrowseInfo, TransportError> {
        Err(TransportError::OperationFailed(
            "backend transport 未实现 browse_directory".to_string(),
        ))
    }

    /// 按 Workspace identity 在目标本机 backend 上反向发现候选目录。
    async fn discover_workspace_by_identity(
        &self,
        _backend_id: &str,
        _workspaces: Vec<WorkspaceIdentityDiscoveryRequest>,
    ) -> Result<WorkspaceIdentityDiscoveryInfo, TransportError> {
        Err(TransportError::OperationFailed(
            "backend transport 未实现 workspace identity discovery".to_string(),
        ))
    }
}

#[derive(Debug, Clone, Default)]
pub struct GitRepoInfo {
    pub is_git_repo: bool,
    pub repo_root: Option<String>,
    pub source_repo: Option<String>,
    pub default_branch: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct P4WorkspaceInfo {
    pub is_p4_workspace: bool,
    pub workspace_root: Option<String>,
    pub client_name: Option<String>,
    pub server_address: Option<String>,
    pub user_name: Option<String>,
    pub stream: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceProbeInfo {
    pub git: Option<GitRepoInfo>,
    pub p4: Option<P4WorkspaceInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DirectoryBrowseInfo {
    pub current_path: String,
    pub entries: Vec<DirectoryEntryInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct DirectoryEntryInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct WorkspaceIdentityDiscoveryRequest {
    pub workspace_id: Uuid,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceIdentityDiscoveryInfo {
    pub candidates: Vec<WorkspaceIdentityDiscoveryCandidate>,
    pub skipped: Vec<WorkspaceIdentityDiscoverySkipped>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceIdentityDiscoveryCandidate {
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

#[derive(Debug, Clone)]
pub struct WorkspaceIdentityDiscoverySkipped {
    pub workspace_id: Uuid,
    pub identity_kind: WorkspaceIdentityKind,
    pub reason: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("后端不在线: {0}")]
    BackendOffline(String),
    #[error("操作失败: {0}")]
    OperationFailed(String),
    #[error("超时")]
    Timeout,
}
