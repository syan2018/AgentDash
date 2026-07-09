use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::platform::tool_capability::CAP_COLLABORATION;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext, ToolCluster};
use async_trait::async_trait;

use crate::companion::model_preflight::CompanionModelPreflightPort;
use crate::companion::tool_context::CompanionToolContext;
use crate::companion::tools::{CompanionRequestTool, CompanionRespondTool};
use crate::companion::workflow_script_preflight::CompanionWorkflowScriptPreflightPort;
use crate::runtime_tools::provider::SharedSessionToolServicesHandle;
use crate::wait_activity::WaitActivityService;

#[derive(Clone)]
pub struct CollaborationRuntimeToolProvider {
    repos: crate::repository_set::RepositorySet,
    session_services_handle: SharedSessionToolServicesHandle,
    wait_service: Option<WaitActivityService>,
    model_preflight: Option<Arc<dyn CompanionModelPreflightPort>>,
    workflow_script_preflight: Option<Arc<dyn CompanionWorkflowScriptPreflightPort>>,
}

impl CollaborationRuntimeToolProvider {
    pub fn new(
        repos: crate::repository_set::RepositorySet,
        session_services_handle: SharedSessionToolServicesHandle,
    ) -> Self {
        Self {
            repos,
            session_services_handle,
            wait_service: None,
            model_preflight: None,
            workflow_script_preflight: None,
        }
    }

    pub fn with_wait_service(mut self, wait_service: WaitActivityService) -> Self {
        self.wait_service = Some(wait_service);
        self
    }

    pub fn with_model_preflight(
        mut self,
        model_preflight: Arc<dyn CompanionModelPreflightPort>,
    ) -> Self {
        self.model_preflight = Some(model_preflight);
        self
    }

    pub fn with_workflow_script_preflight(
        mut self,
        workflow_script_preflight: Arc<dyn CompanionWorkflowScriptPreflightPort>,
    ) -> Self {
        self.workflow_script_preflight = Some(workflow_script_preflight);
        self
    }
}

#[async_trait]
impl RuntimeToolProvider for CollaborationRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let flow = &context.turn.capability_state;
        if !flow
            .tool
            .enabled_clusters
            .contains(&ToolCluster::Collaboration)
        {
            return Ok(Vec::new());
        }

        let mut tools: Vec<DynAgentTool> = Vec::new();
        let companion_tool_context = CompanionToolContext::from_execution_context(context);
        if flow.is_capability_tool_enabled(
            CAP_COLLABORATION,
            "companion_request",
            Some(ToolCluster::Collaboration),
        ) {
            let wait_service = self.wait_service.clone().ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "companion_request 需要 WaitActivityService 才能构建统一等待路径".to_string(),
                )
            })?;
            tools.push(Arc::new(CompanionRequestTool::new(
                self.repos.project_agent_repo.clone(),
                self.repos.clone(),
                self.session_services_handle.clone(),
                companion_tool_context.clone(),
                flow.companion.agents.clone(),
                wait_service,
                self.model_preflight.clone(),
                self.workflow_script_preflight.clone(),
            )));
        }
        if flow.is_capability_tool_enabled(
            CAP_COLLABORATION,
            "companion_respond",
            Some(ToolCluster::Collaboration),
        ) {
            tools.push(Arc::new(CompanionRespondTool::new(
                self.repos.clone(),
                self.session_services_handle.clone(),
                companion_tool_context,
            )));
        }
        Ok(tools)
    }
}
