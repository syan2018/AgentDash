//! 远程 ACP 后端连接器（骨架实现）

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, ExecutorInfo, PromptPayload,
};

pub struct RemoteAcpConnector {
    endpoint: String,
    #[allow(dead_code)]
    auth_token: Option<String>,
    #[allow(dead_code)]
    active_sessions: Arc<Mutex<HashMap<String, ()>>>,
}

impl RemoteAcpConnector {
    #[allow(dead_code)]
    pub fn new(endpoint: String, auth_token: Option<String>) -> Self {
        Self {
            endpoint,
            auth_token,
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl AgentConnector for RemoteAcpConnector {
    fn connector_id(&self) -> &'static str {
        "remote-acp"
    }

    fn connector_type(&self) -> ConnectorType {
        ConnectorType::RemoteAcpBackend
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            supports_cancel: true,
            supports_discovery: false,
            supports_variants: false,
            supports_model_override: false,
            supports_permission_policy: false,
        }
    }

    fn list_executors(&self) -> Vec<ExecutorInfo> {
        Vec::new()
    }

    async fn discover_options_stream(
        &self,
        _executor: &str,
        _variant: Option<&str>,
        _working_dir: Option<std::path::PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Err(ConnectorError::ConnectionFailed(
            "远程 ACP 连接器不支持 discover_options_stream（尚未实现）".to_string(),
        ))
    }

    async fn prompt(
        &self,
        _session_id: &str,
        _prompt: &PromptPayload,
        _context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        Err(ConnectorError::ConnectionFailed(format!(
            "远程 ACP 连接器尚未实现 (endpoint: {})",
            self.endpoint
        )))
    }

    async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
        Err(ConnectorError::ConnectionFailed(
            "远程 ACP 连接器尚未实现".to_string(),
        ))
    }
}
