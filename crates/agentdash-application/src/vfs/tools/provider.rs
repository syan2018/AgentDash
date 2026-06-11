use std::sync::Arc;

use crate::session::{
    SessionCapabilityService, SessionControlService, SessionCoreService, SessionEventingService,
    SessionHookService, SessionLaunchService,
};
use agentdash_spi::DynAgentTool;
use agentdash_spi::ToolCluster;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::platform::tool_capability::{
    CAP_COLLABORATION, CAP_WORKFLOW, CAP_WORKSPACE_MODULE,
};
use agentdash_spi::{ConnectorError, ExecutionContext};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::companion::tool_context::CompanionToolContext;
use crate::companion::tools::{CompanionRequestTool, CompanionRespondTool};
use crate::platform_config::SharedPlatformConfig;
use crate::runtime_gateway::{ExtensionRuntimeChannelInvoker, RuntimeGateway};
use crate::vfs::inline_persistence::{InlineContentOverlay, InlineContentPersister};
use crate::vfs::service::VfsService;
use crate::vfs::tools::factory::{VfsToolFactory, VfsToolFactoryInput};
use crate::vfs::tools::fs::SharedRuntimeVfs;
use crate::vfs::{VfsMaterializationService, VfsMaterializationTransport};
use crate::workflow::tools::advance_node::CompleteLifecycleNodeTool;
use crate::workspace_module::{
    WorkspaceModuleCreateTool, WorkspaceModuleDescribeTool, WorkspaceModuleInvokeTool,
    WorkspaceModuleListTool, WorkspaceModulePresentTool, resolve_invocation_backend,
};
use agentdash_application_ports::extension_runtime::ExtensionRuntimeChannelTransport;
use uuid::Uuid;

/// `RuntimeGateway` 的延迟注入句柄。
///
/// gateway 在 app_state 装配序里晚于本 provider 构造（gateway 依赖 session_mcp_access，
/// 后者又依赖本 provider 产出的工具集），无法在 `new` 时传入。沿用
/// `SharedSessionToolServicesHandle` 的延迟注入模式：app_state 在 gateway 建好后 `set`。
#[derive(Clone, Default)]
pub struct SharedRuntimeGatewayHandle {
    inner: Arc<RwLock<Option<Arc<RuntimeGateway>>>>,
}

impl SharedRuntimeGatewayHandle {
    pub async fn set(&self, gateway: Arc<RuntimeGateway>) {
        *self.inner.write().await = Some(gateway);
    }

    pub async fn get(&self) -> Option<Arc<RuntimeGateway>> {
        self.inner.read().await.clone()
    }
}

#[derive(Clone)]
pub struct RelayRuntimeToolProvider {
    service: Arc<VfsService>,
    repos: crate::repository_set::RepositorySet,
    session_services_handle: SharedSessionToolServicesHandle,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
    function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    materialization: Option<Arc<VfsMaterializationService>>,
    runtime_gateway_handle: SharedRuntimeGatewayHandle,
    extension_channel_transport: Option<Arc<dyn ExtensionRuntimeChannelTransport>>,
}

impl RelayRuntimeToolProvider {
    pub fn new(
        service: Arc<VfsService>,
        repos: crate::repository_set::RepositorySet,
        session_services_handle: SharedSessionToolServicesHandle,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
        _platform_config: SharedPlatformConfig,
        function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    ) -> Self {
        Self {
            service,
            repos,
            session_services_handle,
            inline_persister,
            function_runner,
            shell_output_registry: None,
            materialization: None,
            runtime_gateway_handle: SharedRuntimeGatewayHandle::default(),
            extension_channel_transport: None,
        }
    }

    /// 注入 RuntimeGateway 延迟句柄（供 workspace_module_invoke 路由 runtime/canvas action）。
    pub fn with_runtime_gateway_handle(mut self, handle: SharedRuntimeGatewayHandle) -> Self {
        self.runtime_gateway_handle = handle;
        self
    }

    /// 注入 extension channel transport（供 workspace_module_invoke 的 protocol_channel 分支）。
    pub fn with_extension_channel_transport(
        mut self,
        transport: Arc<dyn ExtensionRuntimeChannelTransport>,
    ) -> Self {
        self.extension_channel_transport = Some(transport);
        self
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
        tools.extend(
            VfsToolFactory::new(self.service.clone())
                .with_materialization(self.materialization.clone())
                .with_shell_output_registry(self.shell_output_registry.clone())
                .build_tools(VfsToolFactoryInput {
                    shared_vfs: shared_vfs.clone(),
                    overlay: overlay.clone(),
                    identity: identity.clone(),
                    session_id: session_id.clone(),
                    turn_id: context.session.turn_id.clone(),
                    flow,
                }),
        );

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
                Some(self.function_runner.clone()),
                context,
            )));
        }

        // Collaboration 簇：Companion 协作 + Hook action 解析
        if clusters.contains(&ToolCluster::Collaboration) {
            let companion_tool_context = CompanionToolContext::resolve(context, &self.repos).await;
            if flow.is_capability_tool_enabled(
                CAP_COLLABORATION,
                "companion_request",
                Some(ToolCluster::Collaboration),
            ) {
                let companion_request_tool = CompanionRequestTool::new(
                    self.repos.project_agent_repo.clone(),
                    self.repos.clone(),
                    self.session_services_handle.clone(),
                    companion_tool_context.clone(),
                    context.session.executor_config.clone(),
                );
                tools.push(Arc::new(companion_request_tool));
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
        }

        // Workspace Module 簇：module 发现工具（只读，现取现算）
        if clusters.contains(&ToolCluster::WorkspaceModule) {
            if let Some(project_id) = project_id_from_context(context) {
                let visibility = flow.workspace_module.clone();
                if flow.is_capability_tool_enabled(
                    CAP_WORKSPACE_MODULE,
                    "workspace_module_list",
                    Some(ToolCluster::WorkspaceModule),
                ) {
                    tools.push(Arc::new(
                        WorkspaceModuleListTool::new(
                            self.repos.project_extension_installation_repo.clone(),
                            self.repos.canvas_repo.clone(),
                            project_id,
                            visibility.clone(),
                        )
                        .with_runtime_visibility(
                            self.session_services_handle.clone(),
                            session_id.clone(),
                        ),
                    ));
                }
                if flow.is_capability_tool_enabled(
                    CAP_WORKSPACE_MODULE,
                    "workspace_module_describe",
                    Some(ToolCluster::WorkspaceModule),
                ) {
                    tools.push(Arc::new(
                        WorkspaceModuleDescribeTool::new(
                            self.repos.project_extension_installation_repo.clone(),
                            self.repos.canvas_repo.clone(),
                            project_id,
                            visibility.clone(),
                        )
                        .with_runtime_visibility(
                            self.session_services_handle.clone(),
                            session_id.clone(),
                        ),
                    ));
                }
                if flow.is_capability_tool_enabled(
                    CAP_WORKSPACE_MODULE,
                    "workspace_module_create",
                    Some(ToolCluster::WorkspaceModule),
                ) {
                    tools.push(Arc::new(WorkspaceModuleCreateTool::new(
                        self.repos.canvas_repo.clone(),
                        project_id,
                        shared_vfs.clone(),
                        self.session_services_handle.clone(),
                        Some(session_id.clone()),
                    )));
                }

                // invoke：需 RuntimeGateway + channel transport 注入齐全才装配。
                if flow.is_capability_tool_enabled(
                    CAP_WORKSPACE_MODULE,
                    "workspace_module_invoke",
                    Some(ToolCluster::WorkspaceModule),
                ) {
                    match (
                        self.runtime_gateway_handle.get().await,
                        self.extension_channel_transport.as_ref(),
                    ) {
                        (Some(gateway), Some(transport)) => {
                            let backend = resolve_invocation_backend(
                                context.session.vfs.as_ref(),
                                context
                                    .session
                                    .backend_execution
                                    .as_ref()
                                    .map(|placement| placement.backend_id.as_str()),
                            );
                            let channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
                                self.repos.project_extension_installation_repo.clone(),
                                transport.clone(),
                            ));
                            tools.push(Arc::new(
                                WorkspaceModuleInvokeTool::new(
                                    self.repos.project_extension_installation_repo.clone(),
                                    self.repos.canvas_repo.clone(),
                                    project_id,
                                    visibility.clone(),
                                    session_id.clone(),
                                    // AgentFrame ID 不在 ExecutionContext 可达范围内（research/04），
                                    // runtime_action 派发不强制 agent_id（与 RuntimeActionToolSpec 一致）。
                                    None,
                                    backend,
                                    gateway,
                                    channel_invoker,
                                )
                                .with_runtime_visibility(self.session_services_handle.clone()),
                            ));
                        }
                        _ => {
                            tracing::warn!(
                                "workspace_module_invoke 未装配：缺少 RuntimeGateway 或 channel transport 注入"
                            );
                        }
                    }
                }

                // present：复用 session eventing 推 present 事件。
                if flow.is_capability_tool_enabled(
                    CAP_WORKSPACE_MODULE,
                    "workspace_module_present",
                    Some(ToolCluster::WorkspaceModule),
                ) {
                    tools.push(Arc::new(WorkspaceModulePresentTool::new(
                        self.repos.project_extension_installation_repo.clone(),
                        self.repos.canvas_repo.clone(),
                        project_id,
                        visibility.clone(),
                        shared_vfs.clone(),
                        self.session_services_handle.clone(),
                        session_id.clone(),
                        context.session.turn_id.clone(),
                    )));
                }
            } else {
                tracing::warn!(
                    "workspace module tools 注入失败：无法从 hook session 解析 project_id"
                );
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
