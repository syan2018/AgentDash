/// CompositeConnector — 多连接器组合路由
///
/// 维护一组子连接器，将执行请求根据 executor ID 路由到正确的连接器。
/// discovery / list_executors 聚合所有子连接器的执行器列表。
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::BoxStream;

use crate::connector::{
    AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutionContext,
    ExecutionStream, ExecutorInfo, PromptPayload,
};

pub struct CompositeConnector {
    connectors: Vec<Arc<dyn AgentConnector>>,
    /// executor_id → connector 索引（在首次 build 或 list_executors 时填充）
    executor_routing: std::sync::RwLock<HashMap<String, usize>>,
}

impl CompositeConnector {
    pub fn new(connectors: Vec<Arc<dyn AgentConnector>>) -> Self {
        let routing = Self::build_routing(&connectors);
        Self {
            connectors,
            executor_routing: std::sync::RwLock::new(routing),
        }
    }

    fn build_routing(connectors: &[Arc<dyn AgentConnector>]) -> HashMap<String, usize> {
        let mut map = HashMap::new();
        for (idx, connector) in connectors.iter().enumerate() {
            for executor in connector.list_executors() {
                map.insert(executor.id, idx);
            }
        }
        map
    }

    fn refresh_routing(&self) {
        let new_routing = Self::build_routing(&self.connectors);
        *self.executor_routing.write().unwrap() = new_routing;
    }

    fn resolve_connector(&self, executor_id: &str) -> Option<Arc<dyn AgentConnector>> {
        let routing = self.executor_routing.read().unwrap();
        routing
            .get(executor_id)
            .and_then(|&idx| self.connectors.get(idx))
            .cloned()
    }

    pub fn sub_connectors(&self) -> &[Arc<dyn AgentConnector>] {
        &self.connectors
    }
}

#[async_trait::async_trait]
impl AgentConnector for CompositeConnector {
    fn connector_id(&self) -> &'static str {
        "composite"
    }

    fn connector_type(&self) -> ConnectorType {
        ConnectorType::LocalExecutor
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        let mut caps = ConnectorCapabilities::default();
        for c in &self.connectors {
            let sub = c.capabilities();
            caps.supports_cancel = caps.supports_cancel || sub.supports_cancel;
            caps.supports_discovery = caps.supports_discovery || sub.supports_discovery;
            caps.supports_variants = caps.supports_variants || sub.supports_variants;
            caps.supports_model_override =
                caps.supports_model_override || sub.supports_model_override;
            caps.supports_permission_policy =
                caps.supports_permission_policy || sub.supports_permission_policy;
        }
        caps
    }

    fn list_executors(&self) -> Vec<ExecutorInfo> {
        self.refresh_routing();
        let mut all = Vec::new();
        for c in &self.connectors {
            all.extend(c.list_executors());
        }
        all
    }

    async fn discover_options_stream(
        &self,
        executor: &str,
        variant: Option<&str>,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        let connector = self
            .resolve_connector(executor)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("未知执行器: {executor}")))?;
        connector
            .discover_options_stream(executor, variant, working_dir)
            .await
    }

    async fn prompt(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        let executor_id = &context.executor_config.executor;
        let connector = self.resolve_connector(executor_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "未知执行器 '{executor_id}'，无法路由到任何连接器"
            ))
        })?;
        connector
            .prompt(session_id, follow_up_session_id, prompt, context)
            .await
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        for c in &self.connectors {
            let _ = c.cancel(session_id).await;
        }
        Ok(())
    }

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        let mut last_error: Option<ConnectorError> = None;
        for connector in &self.connectors {
            match connector.approve_tool_call(session_id, tool_call_id).await {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ConnectorError::Runtime("当前没有可处理工具审批的连接器".to_string())
        }))
    }

    async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        let mut last_error: Option<ConnectorError> = None;
        for connector in &self.connectors {
            match connector
                .reject_tool_call(session_id, tool_call_id, reason.clone())
                .await
            {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ConnectorError::Runtime("当前没有可处理工具审批的连接器".to_string())
        }))
    }
}
