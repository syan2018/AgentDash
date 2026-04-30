/// CompositeConnector — 多连接器组合路由
///
/// 维护一组子连接器，将执行请求根据 executor ID 路由到正确的连接器。
/// discovery / list_executors 聚合所有子连接器的执行器列表。
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::BoxStream;

use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType, DynAgentTool,
    ExecutionContext, ExecutionStream, PromptPayload,
};

pub struct CompositeConnector {
    connectors: Vec<Arc<dyn AgentConnector>>,
    /// executor_id → connector 索引（在首次 build 或 list_executors 时填充）
    executor_routing: std::sync::RwLock<HashMap<String, usize>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::stream;

    #[derive(Default)]
    struct StubConnector {
        live_session: Option<String>,
        update_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AgentConnector for StubConnector {
        fn connector_id(&self) -> &'static str {
            "stub"
        }

        fn connector_type(&self) -> ConnectorType {
            ConnectorType::LocalExecutor
        }

        fn capabilities(&self) -> ConnectorCapabilities {
            ConnectorCapabilities::default()
        }

        fn list_executors(&self) -> Vec<AgentInfo> {
            Vec::new()
        }

        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
            Ok(Box::pin(stream::empty()))
        }

        async fn has_live_session(&self, session_id: &str) -> bool {
            self.live_session.as_deref() == Some(session_id)
        }

        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: ExecutionContext,
        ) -> Result<ExecutionStream, ConnectorError> {
            Ok(Box::pin(stream::empty()))
        }

        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn update_session_tools(
            &self,
            _session_id: &str,
            _tools: Vec<DynAgentTool>,
        ) -> Result<(), ConnectorError> {
            self.update_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn update_session_tools_routes_to_live_child() {
        let skipped_calls = Arc::new(AtomicUsize::new(0));
        let routed_calls = Arc::new(AtomicUsize::new(0));
        let composite = CompositeConnector::new(vec![
            Arc::new(StubConnector {
                live_session: Some("other".to_string()),
                update_calls: skipped_calls.clone(),
            }),
            Arc::new(StubConnector {
                live_session: Some("session-1".to_string()),
                update_calls: routed_calls.clone(),
            }),
        ]);

        composite
            .update_session_tools("session-1", Vec::new())
            .await
            .expect("live child should receive update");

        assert_eq!(skipped_calls.load(Ordering::SeqCst), 0);
        assert_eq!(routed_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn update_session_tools_errors_without_live_child() {
        let composite = CompositeConnector::new(vec![Arc::new(StubConnector::default())]);

        let error = composite
            .update_session_tools("missing-session", Vec::new())
            .await
            .expect_err("missing live child should fail");

        assert!(error.to_string().contains("无法热更新工具"));
    }
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
        {
            let routing = self.executor_routing.read().unwrap();
            if let Some(connector) = routing
                .get(executor_id)
                .and_then(|&idx| self.connectors.get(idx))
            {
                return Some(connector.clone());
            }
        }
        // miss — relay 后端可能在 CompositeConnector 初始化后才上线，刷新重试
        self.refresh_routing();
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

    fn supports_repository_restore(&self, executor: &str) -> bool {
        self.resolve_connector(executor)
            .is_some_and(|connector| connector.supports_repository_restore(executor))
    }

    fn list_executors(&self) -> Vec<AgentInfo> {
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
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        let connector = self
            .resolve_connector(executor)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("未知执行器: {executor}")))?;
        connector
            .discover_options_stream(executor, working_dir)
            .await
    }

    async fn has_live_session(&self, session_id: &str) -> bool {
        for connector in &self.connectors {
            if connector.has_live_session(session_id).await {
                return true;
            }
        }
        false
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
        let mut any_success = false;
        let mut last_error: Option<ConnectorError> = None;
        for c in &self.connectors {
            match c.cancel(session_id).await {
                Ok(()) => any_success = true,
                Err(error) => last_error = Some(error),
            }
        }
        if any_success {
            return Ok(());
        }
        Err(last_error.unwrap_or_else(|| {
            ConnectorError::Runtime(format!("当前没有可取消 session `{session_id}` 的连接器"))
        }))
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

    async fn update_session_tools(
        &self,
        session_id: &str,
        tools: Vec<DynAgentTool>,
    ) -> Result<(), ConnectorError> {
        for connector in &self.connectors {
            if connector.has_live_session(session_id).await {
                return connector.update_session_tools(session_id, tools).await;
            }
        }

        Err(ConnectorError::Runtime(format!(
            "当前没有持有 session `{session_id}` 的连接器，无法热更新工具"
        )))
    }
}
