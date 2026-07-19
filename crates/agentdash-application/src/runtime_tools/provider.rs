use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_platform_spi::RuntimeToolProvider;
use agentdash_platform_spi::{DynAgentTool, ExecutionContext, PlatformRuntimeError};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::vfs::compile_whole_mount_runtime_vfs_access_policy;
use crate::vfs::tools::fs::SharedRuntimeVfs;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductInputDeliveryPort, AgentRunProductLaunchService,
    AgentRunProductRuntimeBindingRepository,
};
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;

#[derive(Clone)]
pub struct RuntimeThreadToolServices {
    pub product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort>,
    pub product_runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    pub product_launch: Arc<AgentRunProductLaunchService>,
    pub frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
}

#[derive(Clone, Default)]
pub struct SharedRuntimeThreadToolServicesHandle {
    inner: Arc<RwLock<Option<RuntimeThreadToolServices>>>,
}

impl SharedRuntimeThreadToolServicesHandle {
    pub async fn set(&self, services: RuntimeThreadToolServices) {
        let mut guard = self.inner.write().await;
        *guard = Some(services);
    }

    pub async fn get(&self) -> Option<RuntimeThreadToolServices> {
        self.inner.read().await.clone()
    }
}

#[derive(Clone, Default)]
pub struct RuntimeThreadToolComposer {
    providers: Vec<Arc<dyn RuntimeToolProvider>>,
}

impl RuntimeThreadToolComposer {
    pub fn new(providers: Vec<Arc<dyn RuntimeToolProvider>>) -> Self {
        Self { providers }
    }

    pub fn with_provider(mut self, provider: Arc<dyn RuntimeToolProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Final Business Surface assembly has six product-owned provider slots. Keeping the
    /// arity in the constructor prevents bootstrap refactors from silently dropping a family.
    pub fn from_final_catalog_providers(providers: [Arc<dyn RuntimeToolProvider>; 6]) -> Self {
        Self {
            providers: Vec::from(providers),
        }
    }
}

#[async_trait]
impl RuntimeToolProvider for RuntimeThreadToolComposer {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, PlatformRuntimeError> {
        let mut tools = Vec::new();
        let mut seen_names: BTreeMap<String, usize> = BTreeMap::new();
        for (provider_index, provider) in self.providers.iter().enumerate() {
            let provider_tools = provider.build_tools(context).await?;
            for tool in &provider_tools {
                let name = tool.name().to_string();
                if tool.protocol_projector().is_none() {
                    return Err(PlatformRuntimeError::InvalidConfig(format!(
                        "runtime callable tool `{name}` 缺少 owner protocol projector"
                    )));
                }
                if tool
                    .protocol_fixture_id()
                    .is_none_or(|fixture| fixture.trim().is_empty())
                {
                    return Err(PlatformRuntimeError::InvalidConfig(format!(
                        "runtime callable tool `{name}` 缺少 main parity fixture"
                    )));
                }
                if let Some(first_provider_index) = seen_names.get(&name) {
                    let duplicate_scope = if *first_provider_index == provider_index {
                        format!("同一 provider #{provider_index} 内重复")
                    } else {
                        format!("provider #{first_provider_index} 与 provider #{provider_index}")
                    };
                    return Err(PlatformRuntimeError::InvalidConfig(format!(
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
) -> Result<SharedRuntimeVfs, PlatformRuntimeError> {
    let vfs = context.session.vfs.clone().ok_or_else(|| {
        PlatformRuntimeError::InvalidConfig("缺少 vfs，无法构建统一访问工具".to_string())
    })?;
    let access_policy = context
        .session
        .vfs_access_policy
        .clone()
        .unwrap_or_else(|| compile_whole_mount_runtime_vfs_access_policy(&vfs));
    Ok(SharedRuntimeVfs::new_with_policy(vfs, access_policy))
}

pub(crate) fn runtime_thread_id_from_context(
    context: &ExecutionContext,
) -> Result<String, PlatformRuntimeError> {
    context
        .turn
        .platform_tool_execution
        .as_ref()
        .map(|owner| owner.runtime_thread_id.to_string())
        .ok_or_else(|| {
            PlatformRuntimeError::InvalidConfig(
                "缺少 Platform Tool typed owner context，无法定位 runtime session".to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent::{
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
        ) -> Result<Vec<DynAgentTool>, PlatformRuntimeError> {
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

        fn protocol_projector(&self) -> Option<agentdash_agent::ToolProtocolProjector> {
            Some(agentdash_agent::ToolProtocolProjector::Dynamic { namespace: None })
        }

        fn protocol_fixture_id(&self) -> Option<String> {
            Some(format!("main_tool_{}_lifecycle", self.name))
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
    async fn final_catalog_constructor_enumerates_all_six_provider_slots() {
        let providers =
            ["vfs", "lifecycle", "companion", "task", "wait", "workspace"].map(|tool_name| {
                Arc::new(SingleToolProvider { tool_name }) as Arc<dyn RuntimeToolProvider>
            });
        let composer = RuntimeThreadToolComposer::from_final_catalog_providers(providers);
        let context = ExecutionContext {
            session: agentdash_platform_spi::ExecutionSessionFrame {
                turn_id: "turn-final-catalog".to_string(),
                working_directory: std::path::PathBuf::from("."),
                environment_variables: std::collections::HashMap::new(),
                executor_config: agentdash_platform_spi::AgentConfig::new("PI_AGENT"),
                mcp_servers: Vec::new(),
                vfs: None,
                vfs_access_policy: None,
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: agentdash_platform_spi::ExecutionTurnFrame::default(),
        };
        let tools = composer.build_tools(&context).await.expect("final catalog");
        assert_eq!(tools.len(), 6);
        assert!(tools.iter().all(|tool| tool.protocol_projector().is_some()));
        assert!(
            tools
                .iter()
                .all(|tool| tool.protocol_fixture_id().is_some())
        );
    }

    #[tokio::test]
    async fn composer_rejects_duplicate_callable_tool_names() {
        let composer = RuntimeThreadToolComposer::new(vec![
            Arc::new(SingleToolProvider {
                tool_name: "same_tool",
            }),
            Arc::new(SingleToolProvider {
                tool_name: "same_tool",
            }),
        ]);

        let context = ExecutionContext {
            session: agentdash_platform_spi::ExecutionSessionFrame {
                turn_id: "turn-1".to_string(),
                working_directory: std::path::PathBuf::from("."),
                environment_variables: std::collections::HashMap::new(),
                executor_config: agentdash_platform_spi::AgentConfig::new("PI_AGENT"),
                mcp_servers: Vec::new(),
                vfs: None,
                vfs_access_policy: None,
                backend_execution: None,
                runtime_backend_anchor: None,
                identity: None,
            },
            turn: agentdash_platform_spi::ExecutionTurnFrame::default(),
        };

        let error = match composer.build_tools(&context).await {
            Ok(_) => panic!("duplicate tool name should fail composition"),
            Err(error) => error,
        };

        match error {
            PlatformRuntimeError::InvalidConfig(message) => {
                assert!(message.contains("same_tool"));
                assert!(message.contains("provider #0"));
                assert!(message.contains("provider #1"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
