use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::agent_run::AgentRunRuntimeSurfaceUpdateService;
use crate::session::{
    SessionControlService, SessionCoreService, SessionEventingService, SessionHookService,
    SessionLaunchService, SessionRuntimeTransitionService,
};
use crate::vfs::tools::fs::SharedRuntimeVfs;

#[derive(Clone)]
pub struct SessionToolServices {
    pub core: SessionCoreService,
    pub eventing: SessionEventingService,
    pub control: SessionControlService,
    pub launch: SessionLaunchService,
    pub hooks: SessionHookService,
    pub runtime_transition: SessionRuntimeTransitionService,
    pub runtime_surface_update: AgentRunRuntimeSurfaceUpdateService,
}

#[derive(Clone, Default)]
pub struct SharedSessionToolServicesHandle {
    inner: Arc<RwLock<Option<SessionToolServices>>>,
}

impl SharedSessionToolServicesHandle {
    pub async fn set(&self, services: SessionToolServices) {
        let mut guard = self.inner.write().await;
        *guard = Some(services);
    }

    pub async fn get(&self) -> Option<SessionToolServices> {
        self.inner.read().await.clone()
    }
}

#[derive(Clone, Default)]
pub struct SessionRuntimeToolComposer {
    providers: Vec<Arc<dyn RuntimeToolProvider>>,
}

impl SessionRuntimeToolComposer {
    pub fn new(providers: Vec<Arc<dyn RuntimeToolProvider>>) -> Self {
        Self { providers }
    }

    pub fn with_provider(mut self, provider: Arc<dyn RuntimeToolProvider>) -> Self {
        self.providers.push(provider);
        self
    }
}

#[async_trait]
impl RuntimeToolProvider for SessionRuntimeToolComposer {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let mut tools = Vec::new();
        let mut seen_names: BTreeMap<String, usize> = BTreeMap::new();
        for (provider_index, provider) in self.providers.iter().enumerate() {
            let provider_tools = provider.build_tools(context).await?;
            for tool in &provider_tools {
                let name = tool.name().to_string();
                if let Some(first_provider_index) = seen_names.get(&name) {
                    let duplicate_scope = if *first_provider_index == provider_index {
                        format!("同一 provider #{provider_index} 内重复")
                    } else {
                        format!("provider #{first_provider_index} 与 provider #{provider_index}")
                    };
                    return Err(ConnectorError::InvalidConfig(format!(
                        "runtime callable tool name `{name}` 重复（{duplicate_scope}）"
                    )));
                }
                seen_names.insert(name, provider_index);
            }
            tools.extend(provider_tools);
        }
        Ok(tools)
    }
}

pub(crate) fn shared_runtime_vfs_from_context(
    context: &ExecutionContext,
) -> Result<SharedRuntimeVfs, ConnectorError> {
    let vfs = context.session.vfs.clone().ok_or_else(|| {
        ConnectorError::InvalidConfig("缺少 vfs，无法构建统一访问工具".to_string())
    })?;
    Ok(SharedRuntimeVfs::new(vfs))
}

pub(crate) fn runtime_session_id_from_context(context: &ExecutionContext) -> String {
    context
        .turn
        .hook_runtime
        .as_ref()
        .map(|session| session.session_id().to_string())
        .unwrap_or_else(|| context.session.turn_id.clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent_types::{
        AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct SingleToolProvider {
        tool_name: &'static str,
    }

    #[async_trait]
    impl RuntimeToolProvider for SingleToolProvider {
        async fn build_tools(
            &self,
            _context: &ExecutionContext,
        ) -> Result<Vec<DynAgentTool>, ConnectorError> {
            Ok(vec![Arc::new(StubTool {
                name: self.tool_name,
            })])
        }
    }

    struct StubTool {
        name: &'static str,
    }

    #[async_trait]
    impl AgentTool for StubTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "stub"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({ "type": "object" })
        }

        async fn execute(
            &self,
            _tool_call_id: &str,
            _args: Value,
            _cancel: CancellationToken,
            _on_update: Option<ToolUpdateCallback>,
        ) -> Result<AgentToolResult, AgentToolError> {
            Ok(AgentToolResult {
                content: vec![ContentPart::text("ok")],
                is_error: false,
                details: None,
            })
        }
    }

    #[tokio::test]
    async fn composer_rejects_duplicate_callable_tool_names() {
        let composer = SessionRuntimeToolComposer::new(vec![
            Arc::new(SingleToolProvider {
                tool_name: "same_tool",
            }),
            Arc::new(SingleToolProvider {
                tool_name: "same_tool",
            }),
        ]);

        let context = ExecutionContext {
            session: agentdash_spi::ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory: std::path::PathBuf::from("."),
                environment_variables: std::collections::HashMap::new(),
                executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
                mcp_servers: Vec::new(),
                vfs: None,
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: agentdash_spi::ExecutionTurnFrame::default(),
        };

        let error = match composer.build_tools(&context).await {
            Ok(_) => panic!("duplicate tool name should fail composition"),
            Err(error) => error,
        };

        match error {
            ConnectorError::InvalidConfig(message) => {
                assert!(message.contains("same_tool"));
                assert!(message.contains("provider #0"));
                assert!(message.contains("provider #1"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
