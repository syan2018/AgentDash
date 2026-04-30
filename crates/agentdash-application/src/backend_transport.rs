use std::collections::HashMap;

use agent_client_protocol::SessionNotification;
use async_trait::async_trait;
use tokio::sync::mpsc;

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

/// relay prompt 传输端口 — `RelayAgentConnector` 通过此 trait 与远程后端交互。
///
/// 继承 `BackendTransport` 的在线探测能力，额外提供：
/// - prompt / cancel 命令发送
/// - 在线后端执行器枚举
/// - per-session 通知接收端注册（将 WebSocket 推送桥接为 `ExecutionStream`）
#[async_trait]
pub trait RelayPromptTransport: BackendTransport {
    /// 向指定后端发送 prompt 命令，返回 turn_id。
    async fn relay_prompt(
        &self,
        backend_id: &str,
        payload: RelayPromptRequest,
    ) -> Result<String, TransportError>;

    /// 取消远程会话。
    async fn relay_cancel(&self, backend_id: &str, session_id: &str) -> Result<(), TransportError>;

    /// 列出所有在线后端上报的执行器信息。
    async fn list_online_executors(&self) -> Vec<RemoteExecutorInfo>;

    /// 根据 executor_id + 可选 backend 提示解析应使用的后端。
    /// 优先策略：若提供 `preferred_backend_id`，要求该后端在线且提供对应 executor；
    /// 否则退化为在在线后端中按 executor 唯一匹配。
    async fn resolve_backend(
        &self,
        executor_id: &str,
        preferred_backend_id: Option<&str>,
    ) -> Result<String, TransportError>;

    /// 注册 per-session 通知接收端。
    /// WebSocket handler 收到 relay notification 时，通过此 channel 投递到 connector stream。
    fn register_session_sink(&self, session_id: &str, tx: mpsc::UnboundedSender<RelaySessionEvent>);

    /// 注销 per-session 通知接收端。
    fn unregister_session_sink(&self, session_id: &str);

    /// 检查指定 session 是否有已注册的通知接收端。
    fn has_session_sink(&self, session_id: &str) -> bool;
}

/// relay prompt 命令 payload — application 层抽象，不依赖 relay 协议。
#[derive(Debug, Clone)]
pub struct RelayPromptRequest {
    pub session_id: String,
    pub follow_up_session_id: Option<String>,
    pub prompt_blocks: Option<serde_json::Value>,
    pub mount_root_ref: String,
    pub working_dir: Option<String>,
    pub env: HashMap<String, String>,
    pub executor_config: Option<RelayExecutorConfig>,
    /// Cloud → remote 的完整 MCP 声明透传。
    ///
    /// Relay/remote agent 的 MCP 建联由远端第三方 agent 自行处理；
    /// cloud 侧不区分 direct / relay，也不在 relay connector 内私有缓存。
    pub mcp_servers: Vec<serde_json::Value>,
}

/// relay 执行器配置 — 对齐远程后端需要的最小字段集。
#[derive(Debug, Clone)]
pub struct RelayExecutorConfig {
    pub executor: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<String>,
    pub permission_policy: Option<String>,
}

/// 远程后端上报的执行器信息。
#[derive(Debug, Clone)]
pub struct RemoteExecutorInfo {
    pub backend_id: String,
    pub executor_id: String,
    pub executor_name: String,
    pub variants: Vec<String>,
    pub available: bool,
}

/// relay session 事件 — 由 WebSocket handler 投递到 connector stream。
#[derive(Debug)]
pub enum RelaySessionEvent {
    Notification(SessionNotification),
    Terminal {
        kind: RelayTerminalKind,
        message: Option<String>,
    },
}

/// relay session 终态类型。
#[derive(Debug, Clone, Copy)]
pub enum RelayTerminalKind {
    Completed,
    Failed,
    Interrupted,
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
