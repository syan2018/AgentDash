use std::{
    collections::HashMap,
    path::PathBuf,
    pin::Pin,
};

use agent_client_protocol::SessionNotification;
use async_trait::async_trait;
use futures::Stream;
use thiserror::Error;

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
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[async_trait]
pub trait AgentConnector: Send + Sync {
    #[allow(dead_code)]
    fn connector_id(&self) -> &'static str;

    async fn prompt(
        &self,
        session_id: &str,
        prompt: &str,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError>;

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError>;
}

