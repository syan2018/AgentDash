use async_trait::async_trait;

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

    /// 探测远程路径的 Git 仓库信息
    async fn detect_git_repo(
        &self,
        backend_id: &str,
        root: &str,
    ) -> Result<GitRepoInfo, TransportError>;
}

#[derive(Debug, Clone, Default)]
pub struct GitRepoInfo {
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
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
