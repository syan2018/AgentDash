use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext};
use async_trait::async_trait;

use crate::vfs::inline_persistence::{InlineContentOverlay, InlineContentPersister};
use crate::vfs::service::VfsService;
use crate::vfs::tools::factory::{VfsToolFactory, VfsToolFactoryInput};
use crate::vfs::tools::provider::{
    runtime_session_id_from_context, shared_runtime_vfs_from_context,
};
use crate::vfs::{VfsMaterializationService, VfsMaterializationTransport};

#[derive(Clone)]
pub struct VfsRuntimeToolProvider {
    service: Arc<VfsService>,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
    materialization: Option<Arc<VfsMaterializationService>>,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
}

impl VfsRuntimeToolProvider {
    pub fn new(
        service: Arc<VfsService>,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
    ) -> Self {
        Self {
            service,
            inline_persister,
            materialization: None,
            shell_output_registry: None,
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
        let session_id = runtime_session_id_from_context(context);

        Ok(VfsToolFactory::new(self.service.clone())
            .with_materialization(self.materialization.clone())
            .with_shell_output_registry(self.shell_output_registry.clone())
            .build_tools(VfsToolFactoryInput {
                shared_vfs,
                overlay,
                identity: context.session.identity.clone(),
                session_id,
                turn_id: context.session.turn_id.clone(),
                flow: &context.turn.capability_state,
            }))
    }
}
