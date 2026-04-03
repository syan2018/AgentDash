use std::sync::Arc;

use crate::session::SessionHub;
use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
};
use agentdash_spi::DynAgentTool;
use agentdash_spi::ToolCluster;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, ExecutionContext};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::address_space::inline_persistence::{InlineContentOverlay, InlineContentPersister};
use crate::address_space::relay_service::RelayAddressSpaceService;
use crate::address_space::tools::fs::{
    FsApplyPatchTool, FsGlobTool, FsGrepTool, FsReadTool, MountsListTool,
    SharedRuntimeAddressSpace, ShellExecTool,
};
use crate::canvas::{BindCanvasDataTool, ListCanvasesTool, PresentCanvasTool, StartCanvasTool};
use crate::companion::tools::{CompanionRequestTool, CompanionRespondTool};
use crate::workflow::tools::artifact_report::WorkflowArtifactReportTool;
use uuid::Uuid;

#[derive(Clone)]
pub struct RelayRuntimeToolProvider {
    service: Arc<RelayAddressSpaceService>,
    canvas_repo: Arc<dyn CanvasRepository>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    session_hub_handle: SharedSessionHubHandle,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
}

impl RelayRuntimeToolProvider {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        canvas_repo: Arc<dyn CanvasRepository>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        session_hub_handle: SharedSessionHubHandle,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
    ) -> Self {
        Self {
            service,
            canvas_repo,
            session_binding_repo,
            workflow_definition_repo,
            lifecycle_definition_repo,
            lifecycle_run_repo,
            session_hub_handle,
            inline_persister,
        }
    }
}

#[derive(Clone, Default)]
pub struct SharedSessionHubHandle {
    inner: Arc<RwLock<Option<SessionHub>>>,
}

impl SharedSessionHubHandle {
    pub async fn set(&self, hub: SessionHub) {
        let mut guard = self.inner.write().await;
        *guard = Some(hub);
    }

    pub async fn get(&self) -> Option<SessionHub> {
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
        let shared_address_space = SharedRuntimeAddressSpace::new(address_space);

        let overlay: Option<Arc<InlineContentOverlay>> = self
            .inline_persister
            .as_ref()
            .map(|p| Arc::new(InlineContentOverlay::new(p.clone())));

        let identity = context.identity.clone();

        // 合并 session-type 默认簇 与 agent 级 tool_clusters 限制（交集）
        let session_clusters = &context.flow_capabilities.enabled_clusters;
        let effective_clusters = if let Some(ref agent_clusters) =
            context.executor_config.tool_clusters
        {
            let agent_set: std::collections::BTreeSet<ToolCluster> = agent_clusters
                .iter()
                .filter_map(|s| {
                    serde_json::from_value::<ToolCluster>(serde_json::Value::String(s.clone())).ok()
                })
                .collect();
            session_clusters
                .intersection(&agent_set)
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
        } else {
            session_clusters.clone()
        };
        let clusters = &effective_clusters;

        let mut tools: Vec<DynAgentTool> = Vec::new();

        // Read 簇：只读文件系统访问
        if clusters.contains(&ToolCluster::Read) {
            tools.push(Arc::new(MountsListTool::new(
                self.service.clone(),
                shared_address_space.clone(),
            )));
            tools.push(Arc::new(FsReadTool::new(
                self.service.clone(),
                shared_address_space.clone(),
                overlay.clone(),
                identity.clone(),
            )));
            tools.push(Arc::new(FsGlobTool::new(
                self.service.clone(),
                shared_address_space.clone(),
                overlay.clone(),
                identity.clone(),
            )));
            tools.push(Arc::new(FsGrepTool::new(
                self.service.clone(),
                shared_address_space.clone(),
                overlay.clone(),
                identity.clone(),
            )));
        }

        // Write 簇：文件写入
        if clusters.contains(&ToolCluster::Write) {
            tools.push(Arc::new(FsApplyPatchTool::new(
                self.service.clone(),
                shared_address_space.clone(),
                overlay.clone(),
                identity,
            )));
        }

        // Execute 簇：命令执行
        if clusters.contains(&ToolCluster::Execute) {
            tools.push(Arc::new(ShellExecTool::new(
                self.service.clone(),
                shared_address_space.clone(),
            )));
        }

        // Workflow 簇：工作流产出汇报
        if clusters.contains(&ToolCluster::Workflow) {
            tools.push(Arc::new(WorkflowArtifactReportTool::new(
                self.workflow_definition_repo.clone(),
                self.lifecycle_definition_repo.clone(),
                self.lifecycle_run_repo.clone(),
                context,
            )));
        }

        // Collaboration 簇：Companion 协作 + Hook action 解析
        if clusters.contains(&ToolCluster::Collaboration) {
            tools.push(Arc::new(CompanionRequestTool::new(
                self.session_binding_repo.clone(),
                self.session_hub_handle.clone(),
                context,
            )));
            tools.push(Arc::new(CompanionRespondTool::new(
                self.session_hub_handle.clone(),
                context,
            )));
        }

        // Canvas 簇：Canvas 资产工具
        if clusters.contains(&ToolCluster::Canvas) {
            if let Some(project_id) = project_id_from_context(context) {
                tools.push(Arc::new(ListCanvasesTool::new(
                    self.canvas_repo.clone(),
                    project_id,
                )));
                tools.push(Arc::new(StartCanvasTool::new(
                    self.canvas_repo.clone(),
                    project_id,
                    shared_address_space.clone(),
                    self.session_hub_handle.clone(),
                    context
                        .hook_session
                        .as_ref()
                        .map(|session| session.session_id().to_string()),
                )));
                tools.push(Arc::new(BindCanvasDataTool::new(
                    self.canvas_repo.clone(),
                    project_id,
                )));

                if let Some(session_id) = context
                    .hook_session
                    .as_ref()
                    .map(|session| session.session_id().to_string())
                {
                    tools.push(Arc::new(PresentCanvasTool::new(
                        self.canvas_repo.clone(),
                        self.session_hub_handle.clone(),
                        session_id,
                        context.turn_id.clone(),
                        project_id,
                    )));
                }
            } else {
                tracing::warn!("canvas tools 注入失败：无法从 hook session 解析 project_id");
            }
        }

        Ok(tools)
    }
}

fn project_id_from_context(context: &ExecutionContext) -> Option<Uuid> {
    if let Some(hook_session) = context.hook_session.as_ref() {
        let snapshot = hook_session.snapshot();

        // project owner 直接使用 owner_id；story/task owner 使用 owner.project_id。
        for owner in &snapshot.owners {
            let owner_type = owner.owner_type.as_str();
            if owner_type == "project" {
                if let Ok(project_id) = Uuid::parse_str(owner.owner_id.as_str()) {
                    return Some(project_id);
                }
            } else if (owner_type == "story" || owner_type == "task")
                && let Some(project_id) = owner.project_id.as_deref()
                && let Ok(project_id) = Uuid::parse_str(project_id)
            {
                return Some(project_id);
            }
        }
    }

    context
        .address_space
        .as_ref()
        .and_then(|space| space.source_project_id.as_deref())
        .and_then(|project_id| Uuid::parse_str(project_id).ok())
}
