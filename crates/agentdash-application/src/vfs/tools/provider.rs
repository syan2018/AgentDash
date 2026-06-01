use std::sync::Arc;

use crate::session::companion_wait::CompanionWaitRegistry;
use crate::session::{
    SessionCapabilityService, SessionControlService, SessionCoreService, SessionEventingService,
    SessionHookService, SessionLaunchService,
};
use agentdash_spi::DynAgentTool;
use agentdash_spi::ToolCluster;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::platform::tool_capability::{
    CAP_CANVAS, CAP_COLLABORATION, CAP_FILE_READ, CAP_FILE_WRITE, CAP_SHELL_EXECUTE, CAP_WORKFLOW,
};
use agentdash_spi::{ConnectorError, ExecutionContext};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::canvas::{BindCanvasDataTool, ListCanvasesTool, PresentCanvasTool, StartCanvasTool};
use crate::companion::tools::{CompanionRequestTool, CompanionRespondTool};
use crate::platform_config::SharedPlatformConfig;
use crate::vfs::inline_persistence::{InlineContentOverlay, InlineContentPersister};
use crate::vfs::service::VfsService;
use crate::vfs::tools::fs::{
    FsApplyPatchTool, FsGlobTool, FsGrepTool, FsReadTool, MountsListTool, SharedRuntimeVfs,
    ShellExecTool,
};
use crate::vfs::{VfsMaterializationService, VfsMaterializationTransport};
use crate::workflow::tools::advance_node::CompleteLifecycleNodeTool;
use uuid::Uuid;

#[derive(Clone)]
pub struct RelayRuntimeToolProvider {
    service: Arc<VfsService>,
    repos: crate::repository_set::RepositorySet,
    session_services_handle: SharedSessionToolServicesHandle,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
    platform_config: SharedPlatformConfig,
    function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    materialization: Option<Arc<VfsMaterializationService>>,
}

impl RelayRuntimeToolProvider {
    pub fn new(
        service: Arc<VfsService>,
        repos: crate::repository_set::RepositorySet,
        session_services_handle: SharedSessionToolServicesHandle,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
        platform_config: SharedPlatformConfig,
        function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    ) -> Self {
        Self {
            service,
            repos,
            session_services_handle,
            inline_persister,
            platform_config,
            function_runner,
            shell_output_registry: None,
            materialization: None,
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

#[derive(Clone)]
pub struct SessionToolServices {
    pub core: SessionCoreService,
    pub eventing: SessionEventingService,
    pub control: SessionControlService,
    pub launch: SessionLaunchService,
    pub hooks: SessionHookService,
    pub capability: SessionCapabilityService,
    pub companion_wait_registry: CompanionWaitRegistry,
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

#[async_trait]
impl RuntimeToolProvider for RelayRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let vfs = context.session.vfs.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig("缺少 vfs，无法构建统一访问工具".to_string())
        })?;
        let shared_vfs = SharedRuntimeVfs::new(vfs);

        let overlay: Option<Arc<InlineContentOverlay>> = self
            .inline_persister
            .as_ref()
            .map(|p| Arc::new(InlineContentOverlay::new(p.clone())));

        let identity = context.session.identity.clone();
        let session_id = context
            .turn
            .hook_runtime
            .as_ref()
            .map(|session| session.session_id().to_string())
            .unwrap_or_else(|| context.session.turn_id.clone());

        let clusters = &context.turn.capability_state.tool.enabled_clusters;

        let mut tools: Vec<DynAgentTool> = Vec::new();
        let session_services = self.session_services_handle.get().await;
        let flow = &context.turn.capability_state;

        // Read 簇：只读文件系统访问
        if clusters.contains(&ToolCluster::Read) {
            if flow.is_capability_tool_enabled(
                CAP_FILE_READ,
                "mounts_list",
                Some(ToolCluster::Read),
            ) {
                tools.push(Arc::new(MountsListTool::new(
                    self.service.clone(),
                    shared_vfs.clone(),
                )));
            }
            if flow.is_capability_tool_enabled(CAP_FILE_READ, "fs_read", Some(ToolCluster::Read)) {
                tools.push(Arc::new(FsReadTool::new(
                    self.service.clone(),
                    shared_vfs.clone(),
                    overlay.clone(),
                    identity.clone(),
                )));
            }
            if flow.is_capability_tool_enabled(CAP_FILE_READ, "fs_glob", Some(ToolCluster::Read)) {
                tools.push(Arc::new(FsGlobTool::new(
                    self.service.clone(),
                    shared_vfs.clone(),
                    overlay.clone(),
                    identity.clone(),
                )));
            }
            if flow.is_capability_tool_enabled(CAP_FILE_READ, "fs_grep", Some(ToolCluster::Read)) {
                tools.push(Arc::new(FsGrepTool::new(
                    self.service.clone(),
                    shared_vfs.clone(),
                    overlay.clone(),
                    identity.clone(),
                )));
            }
        }

        // Write 簇：文件写入
        if clusters.contains(&ToolCluster::Write)
            && flow.is_capability_tool_enabled(
                CAP_FILE_WRITE,
                "fs_apply_patch",
                Some(ToolCluster::Write),
            )
        {
            tools.push(Arc::new(FsApplyPatchTool::new(
                self.service.clone(),
                shared_vfs.clone(),
                overlay.clone(),
                identity.clone(),
            )));
        }

        // Execute 簇：命令执行
        if clusters.contains(&ToolCluster::Execute)
            && flow.is_capability_tool_enabled(
                CAP_SHELL_EXECUTE,
                "shell_exec",
                Some(ToolCluster::Execute),
            )
        {
            let mut shell_tool = ShellExecTool::new(self.service.clone(), shared_vfs.clone())
                .with_materialization_context(
                    self.materialization.clone(),
                    session_id.clone(),
                    Some(context.session.turn_id.clone()),
                    overlay.clone(),
                    identity.clone(),
                );
            if let Some(ref registry) = self.shell_output_registry {
                shell_tool = shell_tool.with_shell_output_registry(registry.clone());
            }
            tools.push(Arc::new(shell_tool));
        }

        // Workflow 簇：lifecycle node 推进
        if clusters.contains(&ToolCluster::Workflow)
            && flow.is_capability_tool_enabled(
                CAP_WORKFLOW,
                "complete_lifecycle_node",
                Some(ToolCluster::Workflow),
            )
        {
            tools.push(Arc::new(CompleteLifecycleNodeTool::new(
                self.repos.clone(),
                session_services.clone(),
                context,
                self.platform_config.clone(),
                self.function_runner.clone(),
            )));
        }

        // Collaboration 簇：Companion 协作 + Hook action 解析
        if clusters.contains(&ToolCluster::Collaboration) {
            if flow.is_capability_tool_enabled(
                CAP_COLLABORATION,
                "companion_request",
                Some(ToolCluster::Collaboration),
            ) {
                tools.push(Arc::new(CompanionRequestTool::new(
                    self.repos.project_agent_repo.clone(),
                    self.repos.clone(),
                    self.session_services_handle.clone(),
                    context,
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
                    context,
                )));
            }
        }

        // Canvas 簇：Canvas 资产工具
        if clusters.contains(&ToolCluster::Canvas) {
            if let Some(project_id) = project_id_from_context(context) {
                if flow.is_capability_tool_enabled(
                    CAP_CANVAS,
                    "canvases_list",
                    Some(ToolCluster::Canvas),
                ) {
                    tools.push(Arc::new(ListCanvasesTool::new(
                        self.repos.canvas_repo.clone(),
                        project_id,
                    )));
                }
                if flow.is_capability_tool_enabled(
                    CAP_CANVAS,
                    "canvas_start",
                    Some(ToolCluster::Canvas),
                ) {
                    tools.push(Arc::new(StartCanvasTool::new(
                        self.repos.canvas_repo.clone(),
                        project_id,
                        shared_vfs.clone(),
                        self.session_services_handle.clone(),
                        context
                            .turn
                            .hook_runtime
                            .as_ref()
                            .map(|session| session.session_id().to_string()),
                    )));
                }
                if flow.is_capability_tool_enabled(
                    CAP_CANVAS,
                    "bind_canvas_data",
                    Some(ToolCluster::Canvas),
                ) {
                    tools.push(Arc::new(BindCanvasDataTool::new(
                        self.repos.canvas_repo.clone(),
                        project_id,
                    )));
                }

                if let Some(session_id) = context
                    .turn
                    .hook_runtime
                    .as_ref()
                    .map(|session| session.session_id().to_string())
                {
                    if flow.is_capability_tool_enabled(
                        CAP_CANVAS,
                        "present_canvas",
                        Some(ToolCluster::Canvas),
                    ) {
                        tools.push(Arc::new(PresentCanvasTool::new(
                            self.repos.canvas_repo.clone(),
                            shared_vfs.clone(),
                            self.session_services_handle.clone(),
                            session_id,
                            context.session.turn_id.clone(),
                            project_id,
                        )));
                    }
                }
            } else {
                tracing::warn!("canvas tools 注入失败：无法从 hook session 解析 project_id");
            }
        }

        Ok(tools)
    }
}

fn project_id_from_context(context: &ExecutionContext) -> Option<Uuid> {
    if let Some(hook_runtime) = context.turn.hook_runtime.as_ref() {
        let snapshot = hook_runtime.snapshot();

        if let Some(run_context) = &snapshot.run_context {
            return Some(run_context.project_id);
        }
    }

    context
        .session
        .vfs
        .as_ref()
        .and_then(|space| space.source_project_id.as_deref())
        .and_then(|project_id| Uuid::parse_str(project_id).ok())
}
