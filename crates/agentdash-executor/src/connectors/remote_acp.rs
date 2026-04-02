//! 远程 ACP 后端连接器（骨架实现）

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, PromptPayload,
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

    fn list_executors(&self) -> Vec<AgentInfo> {
        Vec::new()
    }

    async fn discover_options_stream(
        &self,
        _executor: &str,
        _working_dir: Option<std::path::PathBuf>,
    ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError> {
        Err(ConnectorError::ConnectionFailed(
            "远程 ACP 连接器不支持 discover_options_stream（尚未实现）".to_string(),
        ))
    }

    async fn prompt(
        &self,
        _session_id: &str,
        _follow_up_session_id: Option<&str>,
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

    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::ConnectionFailed(
            "远程 ACP 连接器尚未实现工具审批".to_string(),
        ))
    }

    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::ConnectionFailed(
            "远程 ACP 连接器尚未实现工具审批".to_string(),
        ))
    }
}
