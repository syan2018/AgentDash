use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext};
use async_trait::async_trait;

use crate::vfs::inline_persistence::{InlineContentOverlay, InlineContentPersister};
use crate::vfs::service::VfsService;
use crate::vfs::tools::factory::{VfsToolFactory, VfsToolFactoryInput};
use crate::vfs::tools::fs::{ShellTerminalOwner, ShellTerminalRegistry};
use crate::vfs::{VfsMaterializationService, VfsMaterializationTransport};

use super::provider::{runtime_session_id_from_context, shared_runtime_vfs_from_context};

#[derive(Clone)]
pub struct VfsRuntimeToolProvider {
    service: Arc<VfsService>,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
    materialization: Option<Arc<VfsMaterializationService>>,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    terminal_registry: Arc<dyn ShellTerminalRegistry>,
}

impl VfsRuntimeToolProvider {
    pub fn new(
        service: Arc<VfsService>,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
        terminal_registry: Arc<dyn ShellTerminalRegistry>,
    ) -> Self {
        Self {
            service,
            inline_persister,
            materialization: None,
            shell_output_registry: None,
            terminal_registry,
        }
    }

    pub fn with_shell_output_registry(
        mut self,
        registry: Arc<agentdash_relay::ShellOutputRegistry>,
    ) -> Self {
        self.shell_output_registry = Some(registry);
        self
    }

    pub fn with_materialization_transport(
        mut self,
        transport: Arc<dyn VfsMaterializationTransport>,
    ) -> Self {
        self.materialization = Some(Arc::new(VfsMaterializationService::new(
            self.service.clone(),
            transport,
        )));
        self
    }

    pub fn with_materialization_service(mut self, service: Arc<VfsMaterializationService>) -> Self {
        self.materialization = Some(service);
        self
    }
}

#[async_trait]
impl RuntimeToolProvider for VfsRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let shared_vfs = shared_runtime_vfs_from_context(context)?;
        let overlay: Option<Arc<InlineContentOverlay>> = self
            .inline_persister
            .as_ref()
            .map(|p| Arc::new(InlineContentOverlay::new(p.clone())));
        let session_id = runtime_session_id_from_context(context)?;
        let platform_owner = context
            .turn
            .platform_tool_execution
            .as_ref()
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "缺少 Platform Tool typed owner context，无法注册 shell terminal".to_string(),
                )
            })?;
        let terminal_owner = ShellTerminalOwner {
            run_id: platform_owner.run_id,
            agent_id: platform_owner.agent_id,
            runtime_thread_id: platform_owner.runtime_thread_id.clone(),
        };

        Ok(
            VfsToolFactory::new(self.service.clone(), self.terminal_registry.clone())
                .with_materialization(self.materialization.clone())
                .with_shell_output_registry(self.shell_output_registry.clone())
                .build_tools(VfsToolFactoryInput {
                    shared_vfs,
                    overlay,
                    identity: context.session.identity.clone(),
                    session_id,
                    turn_id: context.session.turn_id.clone(),
                    terminal_owner,
                    flow: &context.turn.capability_state,
                }),
        )
    }
}
