use std::sync::Arc;

use agentdash_connector_contract::DynAgentTool;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
};
use agentdash_connector_contract::{ConnectorError, ExecutionContext};
use agentdash_executor::RuntimeToolProvider;
use crate::session::ExecutorHub;
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::address_space::inline_persistence::{InlineContentOverlay, InlineContentPersister};
use crate::address_space::relay_service::RelayAddressSpaceService;
use crate::address_space::tools::fs::{
    FsListTool, FsReadTool, FsSearchTool, FsWriteTool, MountsListTool, ShellExecTool,
};
use crate::task::tools::companion::{CompanionCompleteTool, CompanionDispatchTool};
use crate::task::tools::hook_action::ResolveHookActionTool;
use crate::workflow::tools::artifact_report::WorkflowArtifactReportTool;

#[derive(Clone)]
pub struct RelayRuntimeToolProvider {
    service: Arc<RelayAddressSpaceService>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    executor_hub_handle: SharedExecutorHubHandle,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
}

impl RelayRuntimeToolProvider {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        executor_hub_handle: SharedExecutorHubHandle,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
    ) -> Self {
        Self {
            service,
            session_binding_repo,
            workflow_definition_repo,
            lifecycle_definition_repo,
            lifecycle_run_repo,
            executor_hub_handle,
            inline_persister,
        }
    }
}

#[derive(Clone, Default)]
pub struct SharedExecutorHubHandle {
    inner: Arc<RwLock<Option<ExecutorHub>>>,
}

impl SharedExecutorHubHandle {
    pub async fn set(&self, hub: ExecutorHub) {
        let mut guard = self.inner.write().await;
        *guard = Some(hub);
    }

    pub async fn get(&self) -> Option<ExecutorHub> {
        self.inner.read().await.clone()
    }
}

#[async_trait]
impl RuntimeToolProvider for RelayRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let address_space = context.address_space.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig("缺少 address_space，无法构建统一访问工具".to_string())
        })?;

        let overlay: Option<Arc<InlineContentOverlay>> = self
            .inline_persister
            .as_ref()
            .map(|p| Arc::new(InlineContentOverlay::new(p.clone())));

        let mut tools: Vec<DynAgentTool> = vec![
            Arc::new(MountsListTool::new(
                self.service.clone(),
                address_space.clone(),
            )),
            Arc::new(FsReadTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(FsWriteTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(FsListTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(FsSearchTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(ShellExecTool::new(self.service.clone(), address_space)),
        ];

        let caps = &context.flow_capabilities;
        if caps.workflow_artifact {
            tools.push(Arc::new(WorkflowArtifactReportTool::new(
                self.workflow_definition_repo.clone(),
                self.lifecycle_definition_repo.clone(),
                self.lifecycle_run_repo.clone(),
                context,
            )));
        }
        if caps.companion_dispatch {
            tools.push(Arc::new(CompanionDispatchTool::new(
                self.session_binding_repo.clone(),
                self.executor_hub_handle.clone(),
                context,
            )));
        }
        if caps.companion_complete {
            tools.push(Arc::new(CompanionCompleteTool::new(
                self.executor_hub_handle.clone(),
                context,
            )));
        }
        if caps.resolve_hook_action {
            tools.push(Arc::new(ResolveHookActionTool::new(
                self.executor_hub_handle.clone(),
                context,
            )));
        }

        Ok(tools)
    }
}
