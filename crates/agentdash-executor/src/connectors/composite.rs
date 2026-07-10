#![allow(clippy::items_after_test_module)]

/// CompositeConnector — 多连接器组合路由
///
/// 维护一组子连接器，将执行请求根据 executor ID 路由到正确的连接器。
/// discovery / list_executors 聚合所有子连接器的执行器列表。
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::BoxStream;

use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    DiscoveryContext, DynAgentTool, ExecutionContext, ExecutionStream, PromptPayload,
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
        supports_steering: bool,
        update_calls: Arc<AtomicUsize>,
        notification_calls: Arc<AtomicUsize>,
        steer_calls: Arc<AtomicUsize>,
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
            ConnectorCapabilities {
                supports_steering: self.supports_steering,
                ..Default::default()
            }
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

        async fn push_session_notification(
            &self,
            _session_id: &str,
            _message: String,
        ) -> Result<(), ConnectorError> {
            self.notification_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn steer_session(
            &self,
            _session_id: &str,
            _expected_turn_id: &str,
            _input: Vec<agentdash_agent_protocol::codex_app_server_protocol::UserInput>,
        ) -> Result<(), ConnectorError> {
            self.steer_calls.fetch_add(1, Ordering::SeqCst);
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
                notification_calls: Arc::new(AtomicUsize::new(0)),
                ..Default::default()
            }),
            Arc::new(StubConnector {
                live_session: Some("session-1".to_string()),
                update_calls: routed_calls.clone(),
                notification_calls: Arc::new(AtomicUsize::new(0)),
                ..Default::default()
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

        assert!(error.to_string().contains("live 连接器"));
    }

    #[tokio::test]
    async fn push_session_notification_routes_to_live_child() {
        let skipped_calls = Arc::new(AtomicUsize::new(0));
        let routed_calls = Arc::new(AtomicUsize::new(0));
        let composite = CompositeConnector::new(vec![
            Arc::new(StubConnector {
                live_session: Some("other".to_string()),
                update_calls: Arc::new(AtomicUsize::new(0)),
                notification_calls: skipped_calls.clone(),
                ..Default::default()
            }),
            Arc::new(StubConnector {
                live_session: Some("session-1".to_string()),
                update_calls: Arc::new(AtomicUsize::new(0)),
                notification_calls: routed_calls.clone(),
                ..Default::default()
            }),
        ]);

        composite
            .push_session_notification("session-1", "phase changed".to_string())
            .await
            .expect("live child should receive notification");

        assert_eq!(skipped_calls.load(Ordering::SeqCst), 0);
        assert_eq!(routed_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn steer_session_routes_to_live_steering_child() {
        let skipped_calls = Arc::new(AtomicUsize::new(0));
        let routed_calls = Arc::new(AtomicUsize::new(0));
        let composite = CompositeConnector::new(vec![
            Arc::new(StubConnector {
                live_session: Some("other".to_string()),
                supports_steering: true,
                steer_calls: skipped_calls.clone(),
                ..Default::default()
            }),
            Arc::new(StubConnector {
                live_session: Some("session-1".to_string()),
                supports_steering: true,
                steer_calls: routed_calls.clone(),
                ..Default::default()
            }),
        ]);

        assert!(composite.supports_session_steering("session-1").await);
        composite
            .steer_session(
                "session-1",
                "turn-1",
                vec![
                    agentdash_agent_protocol::codex_app_server_protocol::UserInput::Text {
                        text: "steer".to_string(),
                        text_elements: Vec::new(),
                    },
                ],
            )
            .await
            .expect("live child should receive steer");

        assert_eq!(skipped_calls.load(Ordering::SeqCst), 0);
        assert_eq!(routed_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn control_rejects_ambiguous_live_session_owner() {
        let composite = CompositeConnector::new(vec![
            Arc::new(StubConnector {
                live_session: Some("session-1".to_string()),
                ..Default::default()
            }),
            Arc::new(StubConnector {
                live_session: Some("session-1".to_string()),
                ..Default::default()
            }),
        ]);

        let error = composite
            .cancel("session-1")
            .await
            .expect_err("ambiguous owner must not broadcast cancel");
        assert!(error.to_string().contains("多个连接器"));
    }

    #[test]
    fn composite_does_not_or_child_capabilities() {
        let composite = CompositeConnector::new(vec![Arc::new(StubConnector {
            supports_steering: true,
            ..Default::default()
        })]);

        let capabilities = composite.capabilities();
        assert!(!capabilities.supports_cancel);
        assert!(!capabilities.supports_steering);
        assert!(!capabilities.supports_discovery);
        assert!(!capabilities.supports_variants);
        assert!(!capabilities.supports_model_override);
        assert!(!capabilities.supports_permission_policy);
        assert!(!capabilities.supports_source_session_title);
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

    async fn resolve_live_session_owner(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn AgentConnector>, ConnectorError> {
        let mut owner = None;
        for connector in &self.connectors {
            if connector.has_live_session(session_id).await {
                if owner.is_some() {
                    return Err(ConnectorError::Runtime(format!(
                        "session `{session_id}` 同时被多个连接器声明为 live owner"
                    )));
                }
                owner = Some(connector.clone());
            }
        }
        owner.ok_or_else(|| {
            ConnectorError::Runtime(format!(
                "当前没有持有 session `{session_id}` 的 live 连接器"
            ))
        })
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
        // A composite has no globally bound executor, so child capabilities cannot be combined
        // into a truthful session guarantee. Bound runtime capability comes from RuntimeOffer.
        ConnectorCapabilities::default()
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
        self.discover_options_stream_with_context(
            executor,
            DiscoveryContext {
                working_dir,
                identity: None,
            },
        )
        .await
    }

    async fn discover_options_stream_with_context(
        &self,
        executor: &str,
        context: DiscoveryContext,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        let connector = self
            .resolve_connector(executor)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("未知执行器: {executor}")))?;
        connector
            .discover_options_stream_with_context(executor, context)
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

    async fn supports_session_steering(&self, session_id: &str) -> bool {
        let Ok(connector) = self.resolve_live_session_owner(session_id).await else {
            return false;
        };
        connector.capabilities().supports_steering
            && connector.supports_session_steering(session_id).await
    }

    async fn prompt(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        let executor_id = &context.session.executor_config.executor;
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
        self.resolve_live_session_owner(session_id)
            .await?
            .cancel(session_id)
            .await
    }

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        self.resolve_live_session_owner(session_id)
            .await?
            .approve_tool_call(session_id, tool_call_id)
            .await
    }

    async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        self.resolve_live_session_owner(session_id)
            .await?
            .reject_tool_call(session_id, tool_call_id, reason)
            .await
    }

    async fn update_session_tools(
        &self,
        session_id: &str,
        tools: Vec<DynAgentTool>,
    ) -> Result<(), ConnectorError> {
        self.resolve_live_session_owner(session_id)
            .await?
            .update_session_tools(session_id, tools)
            .await
    }

    async fn push_session_notification(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<(), ConnectorError> {
        self.resolve_live_session_owner(session_id)
            .await?
            .push_session_notification(session_id, message)
            .await
    }

    async fn steer_session(
        &self,
        session_id: &str,
        expected_turn_id: &str,
        input: Vec<agentdash_agent_protocol::UserInputBlock>,
    ) -> Result<(), ConnectorError> {
        self.resolve_live_session_owner(session_id)
            .await?
            .steer_session(session_id, expected_turn_id, input)
            .await
    }
}
