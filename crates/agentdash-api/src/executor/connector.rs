use std::{
    collections::HashMap,
    path::PathBuf,
    pin::Pin,
};

use agent_client_protocol::SessionNotification;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::Stream;
use serde::Serialize;
use thiserror::Error;

/// 连接器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    /// 本地子进程执行器（Claude Code, Codex, AMP 等）
    LocalExecutor,
    /// 远程 ACP 后端
    RemoteAcpBackend,
}

/// 连接器能力声明
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConnectorCapabilities {
    /// 是否支持取消执行
    pub supports_cancel: bool,
    /// 是否支持执行器发现（列出可用执行器、模型）
    pub supports_discovery: bool,
    /// 是否支持配置变体
    pub supports_variants: bool,
    /// 是否支持模型覆盖
    pub supports_model_override: bool,
    /// 是否支持权限策略选择
    pub supports_permission_policy: bool,
}

/// 连接器对外暴露的执行器选项（用于前端选择器渲染）
///
/// 重要：不要在 API 层硬编码名称/可用性判断；由连接器实现提供。
#[derive(Debug, Clone, Serialize)]
pub struct ExecutorInfo {
    /// 执行器 ID（例如 "CLAUDE_CODE"）
    pub id: String,
    /// 展示名称（由连接器决定，避免 API 层硬编码映射）
    pub name: String,
    /// 变体列表（例如 ["DEFAULT","PLAN"]）
    pub variants: Vec<String>,
    /// 是否可用（由连接器实现判断）
    pub available: bool,
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: executors::profile::ExecutorConfig,
}

pub type ExecutionStream = Pin<
    Box<dyn Stream<Item = Result<SessionNotification, ConnectorError>> + Send + 'static>,
>;

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("执行器配置无效: {0}")]
    InvalidConfig(String),
    #[error("执行器启动失败: {0}")]
    SpawnFailed(String),
    #[error("执行器运行错误: {0}")]
    Runtime(String),
    #[error("连接失败: {0}")]
    ConnectionFailed(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[async_trait]
pub trait AgentConnector: Send + Sync {
    fn connector_id(&self) -> &'static str;

    fn connector_type(&self) -> ConnectorType;

    fn capabilities(&self) -> ConnectorCapabilities;

    /// 列出当前连接器可用的执行器（给前端选择器使用）
    fn list_executors(&self) -> Vec<ExecutorInfo>;

    /// 获取当前连接器支持的执行选项流（对齐 vibe-kanban：JSON Patch over WebSocket）
    ///
    /// - `executor`: 执行器 ID（例如 "CLAUDE_CODE"）
    /// - `variant`: 可选变体（例如 "DEFAULT" / "PLAN"）
    async fn discover_options_stream(
        &self,
        executor: &str,
        variant: Option<&str>,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError>;

    async fn prompt(
        &self,
        session_id: &str,
        prompt: &str,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError>;

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError>;
}

