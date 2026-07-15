use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::Result;
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::integrations::{builtin_integrations, collect_integration_registration};
use crate::project_projection_notification::ProjectProjectionNotificationPublisher;
use crate::relay::registry::BackendRegistry;
use agentdash_application::agent_run_list::{
    ProjectAgentRunListQuery, ProjectAgentRunListQueryDeps,
};
use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::context::{
    InMemoryContextAuditBus, SharedContextAuditBus, VfsDiscoveryRegistry,
};
use agentdash_application::platform_config::{PlatformConfig, SharedPlatformConfig};
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application::routine::RoutineExecutor;
use agentdash_application::scheduling::CronSchedulerHandle;
use agentdash_application::vfs_surface_resolver::{VfsSurfaceResolver, VfsSurfaceResolverDeps};
use agentdash_application_agentrun::agent_run::{
    AgentBusinessSurfaceContextDeps, AgentBusinessSurfaceSource, AgentRunControlEffectDeps,
    AgentRunControlEffectService, AgentRunJournalBindingResolver, AgentRunJournalService,
    AgentRunJournalSource, AgentRunJournalSourceSubscription, AgentRunProductDeliveryPort,
    AgentRunRuntime, AgentRunRuntimeSurfaceUpdateDeps, AgentRunRuntimeSurfaceUpdateService,
    BaseIdentitySource, BusinessFrameSurfaceQuery, BusinessFrameSurfaceQueryDeps,
    BusinessResourceSurfaceQuery, BusinessResourceSurfaceQueryDeps, ManagedAgentRunRuntime,
    RuntimeAgentRunMailbox, RuntimeMailboxTerminalConvergence,
};
use agentdash_application_hooks::AppExecutionHookProvider;
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_lifecycle::run_view_builder::LifecycleReadModelQueryAdapter;
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectPort, EmptyAgentRunHookEffectHandlerRegistry,
};
use agentdash_application_ports::agent_run_surface::{
    AgentRunEffectiveCapabilityPort, AgentRunResourceSurfaceQueryPort,
    AgentRunRuntimeSurfaceQueryPort,
};
use agentdash_application_ports::lifecycle_read_model::LifecycleReadModelQueryPort;
use agentdash_application_runtime_gateway::{
    CurrentSurfaceRuntimeMcpAccess, ExtensionRuntimeChannelInvoker, RuntimeGateway,
};
use agentdash_application_vfs::MountProviderRegistry;
use agentdash_application_vfs::{VfsMutationDispatcher, VfsService};
use agentdash_contracts::project::ProjectEventStreamEnvelope;
use agentdash_diagnostics::DiagnosticBuffer;
use agentdash_domain::llm_provider::LlmSecretCodec;
use agentdash_integration_api::AgentDashIntegration;
use agentdash_integration_api::AuthMode;
use agentdash_integration_api::MarketplaceSourceProvider;
use agentdash_integration_api::MemoryDiscoveryProvider;
use agentdash_integration_api::SkillDiscoveryProvider;
use agentdash_spi::extension_package::ExtensionPackageArtifactStorage;

const BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY: usize = 256;
const PROJECT_CONTROL_PLANE_EVENT_CHANNEL_CAPACITY: usize = 256;
const PLATFORM_MCP_BASE_URL_ENV: &str = "AGENTDASH_MCP_BASE_URL";

fn configured_platform_mcp_base_url() -> Option<String> {
    resolve_platform_mcp_base_url(std::env::var(PLATFORM_MCP_BASE_URL_ENV).ok())
}

fn resolve_platform_mcp_base_url(raw_value: Option<String>) -> Option<String> {
    raw_value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

struct RejectUndeclaredRuntimeCredentials;

struct CanonicalAgentRunJournalSource {
    repository: Arc<agentdash_infrastructure::PostgresRuntimeRepository>,
}

struct CanonicalAgentRunJournalBindingResolver {
    bindings:
        Arc<dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository>,
}

#[async_trait::async_trait]
impl AgentRunJournalBindingResolver for CanonicalAgentRunJournalBindingResolver {
    async fn resolve_thread(
        &self,
        target: &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget,
    ) -> Result<
        Option<agentdash_agent_runtime_contract::RuntimeThreadId>,
        agentdash_application_agentrun::WorkflowApplicationError,
    > {
        self.bindings
            .load(target)
            .await
            .map(|binding| binding.map(|binding| binding.thread_id))
            .map_err(|error| {
                agentdash_application_agentrun::WorkflowApplicationError::Internal(format!(
                    "AgentRun journal binding 读取失败: {error}"
                ))
            })
    }
}

fn agent_run_journal_ephemeral_epoch() -> u64 {
    static EPOCH: OnceLock<u64> = OnceLock::new();
    *EPOCH.get_or_init(|| uuid::Uuid::new_v4().as_u128() as u64)
}

pub(crate) fn ensure_agent_run_journal_full_history_available(
    earliest_available: agentdash_agent_runtime_contract::EventSequence,
    unavailable_context: &str,
) -> Result<(), agentdash_application_agentrun::WorkflowApplicationError> {
    if earliest_available.0 > 1 {
        return Err(
            agentdash_application_agentrun::WorkflowApplicationError::Conflict(format!(
                "AgentRun journal retained history starts at {}, {unavailable_context} is unavailable",
                earliest_available.0
            )),
        );
    }
    Ok(())
}

#[async_trait::async_trait]
impl AgentRunJournalSource for CanonicalAgentRunJournalSource {
    async fn durable_records(
        &self,
        thread_id: &agentdash_agent_runtime_contract::RuntimeThreadId,
    ) -> Result<
        Vec<agentdash_agent_runtime_contract::RuntimeJournalRecord>,
        agentdash_application_agentrun::WorkflowApplicationError,
    > {
        use agentdash_agent_runtime::RuntimeRepository;
        let batch = self
            .repository
            .journal_records_after(thread_id, None)
            .await
            .map_err(|error| {
                agentdash_application_agentrun::WorkflowApplicationError::Internal(format!(
                    "AgentRun journal durable 读取失败: {error}"
                ))
            })?;
        ensure_agent_run_journal_full_history_available(batch.earliest_available, "full refresh")?;
        Ok(batch.records)
    }

    async fn subscribe(
        &self,
        thread_id: &agentdash_agent_runtime_contract::RuntimeThreadId,
    ) -> Result<
        AgentRunJournalSourceSubscription,
        agentdash_application_agentrun::WorkflowApplicationError,
    > {
        use agentdash_agent_runtime::{RuntimeRepository, RuntimeTransientEvents};
        // 先订阅再取snapshot，保证commit发生在两者之间时只会在live中形成可去重重复，绝不漏事件。
        let live = self.repository.subscribe_presentation(thread_id).await;
        let durable_batch = self
            .repository
            .journal_records_after(thread_id, None)
            .await
            .map_err(|error| {
                agentdash_application_agentrun::WorkflowApplicationError::Internal(format!(
                    "AgentRun journal snapshot 读取失败: {error}"
                ))
            })?;
        ensure_agent_run_journal_full_history_available(
            durable_batch.earliest_available,
            "stream resume",
        )?;
        let durable_snapshot = durable_batch.records;
        let ephemeral_backlog = self
            .repository
            .read_presentation(thread_id, None, None)
            .await;
        Ok(AgentRunJournalSourceSubscription {
            ephemeral_epoch: agent_run_journal_ephemeral_epoch(),
            durable_snapshot,
            ephemeral_backlog,
            live,
        })
    }
}

struct CanonicalWorkspaceModulePresentationAppender {
    runtime: Arc<
        agentdash_agent_runtime::ManagedAgentRuntime<
            agentdash_infrastructure::PostgresRuntimeRepository,
        >,
    >,
}

struct CanonicalWorkspaceModuleAgentRunBridge {
    service: AgentRunRuntimeSurfaceUpdateService,
}

#[async_trait::async_trait]
impl agentdash_workspace_module::workspace_module::WorkspaceModuleAgentRunBridge
    for CanonicalWorkspaceModuleAgentRunBridge
{
    async fn effective_capability_view_for_agent_run_delivery(
        &self,
        runtime_thread_id: &str,
    ) -> Result<
        agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView,
        String,
    > {
        self.service
            .effective_capability_view_for_delivery_runtime(runtime_thread_id)
            .await
    }

    async fn apply_canvas_runtime_surface_update_to_agent_run(
        &self,
        runtime_thread_id: &str,
        canvas: &agentdash_domain::canvas::Canvas,
        current_user: Option<&agentdash_domain::project::ProjectAuthorizationContext>,
        request: agentdash_application_ports::agent_frame_materialization::RuntimeSurfaceUpdateRequest,
    ) -> Result<agentdash_application_vfs::tools::RuntimeVfsState, String> {
        self.service
            .apply_canvas_runtime_surface_update(runtime_thread_id, canvas, current_user, request)
            .await
    }
}

#[async_trait::async_trait]
impl agentdash_workspace_module::workspace_module::WorkspaceModulePresentationAppendPort
    for CanonicalWorkspaceModulePresentationAppender
{
    async fn append_presentation(
        &self,
        request: agentdash_agent_runtime_contract::RuntimePresentationAppendRequest,
    ) -> Result<agentdash_agent_runtime_contract::RuntimePresentationAppendReceipt, String> {
        self.runtime
            .append_presentation(request)
            .await
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
struct EffectiveProviderCompanionModelPreflight {
    repos: RepositorySet,
    llm_provider_secret: Arc<dyn LlmSecretCodec>,
}

#[async_trait::async_trait]
impl agentdash_application::companion::CompanionModelPreflightPort
    for EffectiveProviderCompanionModelPreflight
{
    async fn preflight_companion_model(
        &self,
        request: agentdash_application::companion::CompanionModelPreflightRequest,
    ) -> Result<(), agentdash_application::companion::CompanionModelPreflightError> {
        let provider_id = request
            .executor_config
            .provider_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let model_id = request
            .executor_config
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let catalog = agentdash_llm_provider::build_effective_profile_catalog_from_db(
            self.repos.llm_provider_repo.as_ref(),
            Some(self.repos.llm_provider_credential_repo.as_ref()),
            self.llm_provider_secret.as_ref(),
            request.identity.as_ref(),
        )
        .await;
        agentdash_llm_provider::preflight_effective_model_selection(
            &catalog.providers,
            provider_id,
            model_id,
        )
        .map_err(|reason| {
            agentdash_application::companion::CompanionModelPreflightError::new(format!(
                "SubAgent dispatch model preflight 失败：{reason}。companion_label=`{}`, agent_key=`{}`, project_id={}, project_agent_id={}, parent_run_id={}, parent_agent_id={}, provider_id={}, model_id={}",
                request.companion_label,
                request.selected_agent_key,
                request.project_id,
                request.selected_project_agent_id,
                request.parent_run_id,
                request.parent_agent_id,
                provider_id.unwrap_or("(missing)"),
                model_id.unwrap_or("(missing)")
            ))
        })
    }
}

#[async_trait::async_trait]
impl agentdash_integration_api::AgentRuntimeCredentialBroker
    for RejectUndeclaredRuntimeCredentials
{
    async fn resolve(
        &self,
        slot: &agentdash_integration_api::AgentRuntimeCredentialSlot,
        _reference: &agentdash_integration_api::AgentRuntimeCredentialRef,
        _purpose: &str,
    ) -> Result<
        agentdash_integration_api::CredentialLease,
        agentdash_integration_api::CredentialResolveError,
    > {
        Err(
            agentdash_integration_api::CredentialResolveError::Unavailable {
                slot: slot.clone(),
                reason: "no Host Integration registered a credential resolver for this slot"
                    .to_string(),
            },
        )
    }
}

/// 应用服务集合 — 执行引擎、连接器与各类注册表
pub struct ServiceSet {
    pub agent_run_runtime: Arc<dyn AgentRunRuntime>,
    pub agent_run_journal: Arc<AgentRunJournalService>,
    pub agent_run_product_delivery: Arc<dyn AgentRunProductDeliveryPort>,
    pub(crate) terminal_application_effect_worker:
        Arc<crate::agent_run_terminal_control::RuntimeTerminalApplicationEffectWorker>,
    pub project_agent_run_list_query: ProjectAgentRunListQuery,
    pub agent_runtime_host: Arc<agentdash_agent_runtime_host::IntegrationDriverHost>,
    pub agent_runtime_inventory: Arc<crate::relay::CloudRemoteRuntimeInventory>,
    pub runtime_surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    pub lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort>,
    pub resource_surface_query: Arc<dyn AgentRunResourceSurfaceQueryPort>,
    pub vfs_surface_resolver: VfsSurfaceResolver,
    /// 统一 VFS 访问服务 — 供 declared sources、runtime tools、workspace browse 共享
    pub vfs_service: Arc<VfsService>,
    /// VFS 写入分发器 — 统一 surface/tool mutation 与 inline_fs storage 坐标解析。
    pub vfs_mutation_dispatcher: Arc<VfsMutationDispatcher>,
    /// Host Integration 额外 skill 目录 — frame construction 阶段统一 discovery 后进入 session capabilities。
    pub extra_skill_dirs: Vec<std::path::PathBuf>,
    /// Host Integration 动态 skill discovery providers — frame construction 阶段统一聚合。
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    /// Host Integration 动态 memory discovery providers — 启动期统一聚合，供 frame construction 消费。
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    /// Host Integration Marketplace Source providers — 后续 external marketplace API 统一从这里读取来源。
    pub marketplace_source_providers: Vec<Arc<dyn MarketplaceSourceProvider>>,
    /// WebSocket 中继后端注册表 — 跟踪在线的本机后端
    pub backend_registry: Arc<BackendRegistry>,
    /// Backend runtime 在线/离线/能力变化事件 — 供全局事件流驱动前端刷新
    pub backend_runtime_events: broadcast::Sender<String>,
    /// Project scoped control-plane projection invalidation — 供 AgentRun list 等投影刷新
    pub project_control_plane_events: broadcast::Sender<ProjectEventStreamEnvelope>,
    /// 串行 Shell 流式输出路由 — ShellExecTool 注册，ws_handler 投递
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    /// AgentRun scope 终端运行时状态注册表
    pub terminal_registry: Arc<agentdash_application_agentrun::agent_run::AgentRunTerminalRegistry>,
    /// 寻址空间注册表 — 持有可用的资源引用能力提供者
    pub vfs_registry: VfsDiscoveryRegistry,
    /// Mount 级 I/O 提供者注册表（`inline_fs` / `relay_fs` 等）
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    /// Hook 提供者 — 供 API 层验证脚本等管理接口使用
    pub hook_provider: Arc<AppExecutionHookProvider>,
    /// 统一认证会话服务（application 层）
    pub auth_session_service: Arc<AuthSessionService>,
    /// Cron 调度器句柄 — 配置变更时调用 `notify_config_changed()` 触发热重载
    pub cron_scheduler: CronSchedulerHandle,
    /// Routine 执行器 — 统一处理定时/Webhook/Host Integration 触发
    pub routine_executor: Option<Arc<RoutineExecutor>>,
    /// Session 上下文审计总线 — Bundle / Fragment 产出与消费的可观测轨迹
    pub audit_bus: SharedContextAuditBus,
    /// 统一运行时能力网关 — Session/Setup runtime action 的共享入口
    pub runtime_gateway: Arc<RuntimeGateway>,
    pub extension_runtime_channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    /// Extension package archive object 存储端口 — API 只通过 application use case 消费。
    pub extension_package_artifact_storage: Arc<dyn ExtensionPackageArtifactStorage>,
    /// Workflow function/local-effect executor port — orchestration scheduler 共享。
    pub function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
}

/// 应用级配置
pub struct AppConfig {
    /// 进程级平台配置（MCP base URL 等不变量，`Arc` 共享避免逐层透传）
    pub platform_config: SharedPlatformConfig,
    /// 当前宿主配置的认证模式
    pub auth_mode: AuthMode,
}

pub struct SecretSet {
    pub llm_provider_secret: Arc<dyn LlmSecretCodec>,
}

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
/// 按职责分为 3 个子集：repos / services / config。
pub struct AppState {
    pub repos: RepositorySet,
    pub services: ServiceSet,
    pub config: AppConfig,
    pub secrets: SecretSet,
    /// 认证/授权提供者（由 Host Integration 注入，None 表示无认证）
    pub auth_provider: Option<Arc<dyn agentdash_integration_api::AuthProvider>>,
    /// 身份目录提供者（由 Host Integration 注入，None 表示仅使用本地 projection）
    pub identity_directory_provider:
        Option<Arc<dyn agentdash_integration_api::IdentityDirectoryProvider>>,
    /// 统一诊断环形缓冲句柄 — 供 `GET /api/diagnostics` 查询"近期"诊断。
    ///
    /// 仅 `agentdash-api` main 把它接进 tracing 订阅器（[`DiagnosticLayer`]）；
    /// 其它宿主（tauri/local）传入一个未接订阅器的空缓冲，查询端点返回空集，
    /// 行为与原先一致。
    pub diagnostics: DiagnosticBuffer,
}

impl AppState {
    pub async fn new(pool: PgPool) -> Result<Arc<Self>> {
        Self::new_with_integrations(pool, builtin_integrations(), DiagnosticBuffer::new(0)).await
    }

    /// 携带 Host Integration 列表构建 AppState
    ///
    /// `diagnostics` 为统一诊断环形缓冲句柄：`agentdash-api` main 传入已接进
    /// tracing 订阅器的缓冲，其它宿主传入空缓冲即可。
    ///
    /// 返回 `Arc<Self>` 以支持需要 AppState 引用的延迟装配。
    pub async fn new_with_integrations(
        pool: PgPool,
        integrations: Vec<Box<dyn AgentDashIntegration>>,
        diagnostics: DiagnosticBuffer,
    ) -> Result<Arc<Self>> {
        let integration_registration = collect_integration_registration(integrations)
            .map_err(|err| anyhow::anyhow!("Host Integration 注册失败: {err}"))?;
        let runtime_definition_registry = Arc::new(
            agentdash_agent_runtime_host::AgentServiceDefinitionRegistry::collect(
                integration_registration
                    .runtime_driver_contributions
                    .clone(),
            )
            .map_err(|err| anyhow::anyhow!("Agent Runtime Integration 注册失败: {err}"))?,
        );

        let (project_control_plane_events, _project_control_plane_rx) =
            broadcast::channel(PROJECT_CONTROL_PLANE_EVENT_CHANNEL_CAPACITY);
        let project_projection_notifications = Arc::new(
            ProjectProjectionNotificationPublisher::new(project_control_plane_events.clone()),
        );

        let runtime_pool = pool.clone();
        let runtime_provisioner_handle =
            agentdash_application_ports::agent_run_runtime::SharedAgentRunRuntimeProvisionerHandle::default();
        let frame_construction_handle =
            agentdash_application_ports::agent_frame_materialization::SharedAgentRunFrameConstructionHandle::default();
        let hook_plan_compiler_handle =
            agentdash_application_ports::agent_frame_hook_plan::SharedAgentFrameHookPlanCompiler::default();
        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(2000));
        let repository_bootstrap = crate::bootstrap::repositories::build_repositories(
            pool,
            integration_registration.library_asset_seeds.clone(),
            Some(project_projection_notifications.clone()),
            runtime_provisioner_handle.clone(),
            frame_construction_handle.clone(),
            hook_plan_compiler_handle.clone(),
        )
        .await?;
        let repos = repository_bootstrap.repos;
        let auth_session_service = repository_bootstrap.auth_session_service;
        let extension_package_artifact_storage =
            repository_bootstrap.extension_package_artifact_storage;
        let lifecycle_surface_projection = repository_bootstrap.lifecycle_surface_projection;
        let llm_provider_secret: Arc<dyn LlmSecretCodec> = Arc::new(
            agentdash_infrastructure::LlmProviderSecretCipher::from_env_or_create_default()?,
        );

        let platform_config: SharedPlatformConfig = Arc::new(PlatformConfig {
            mcp_base_url: configured_platform_mcp_base_url(),
        });

        let relay_bootstrap =
            crate::bootstrap::relay::build_relay_runtime(BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY);
        let backend_registry = relay_bootstrap.backend_registry;
        let backend_runtime_events = relay_bootstrap.backend_runtime_events;
        let mcp_probe_relay = relay_bootstrap.mcp_probe_relay;
        let setup_action_transport = relay_bootstrap.setup_action_transport;
        let shell_output_registry = relay_bootstrap.shell_output_registry;
        let terminal_registry = relay_bootstrap.terminal_registry;
        let function_runner: Arc<dyn agentdash_spi::FunctionRunner> =
            Arc::new(agentdash_infrastructure::DefaultFunctionRunner::new());

        let vfs_bootstrap = crate::bootstrap::vfs::build_vfs_kernel(
            repos.clone(),
            backend_registry.clone(),
            integration_registration.mount_providers,
        );
        let mount_provider_registry = vfs_bootstrap.mount_provider_registry;
        let vfs_service = vfs_bootstrap.vfs_service;
        let vfs_mutation_dispatcher = vfs_bootstrap.vfs_mutation_dispatcher;
        let vfs_materialization_service = vfs_bootstrap.vfs_materialization_service;
        let mcp_relay_provider = vfs_bootstrap.mcp_relay_provider;
        let agent_run_journal_reader = vfs_bootstrap.agent_run_journal_reader;
        let hook_preset_scripts = AppExecutionHookProvider::builtin_preset_scripts();
        let hook_provider = Arc::new(AppExecutionHookProvider::new(
            agentdash_application_hooks::AppExecutionHookProviderDeps {
                workflow_projection: repos.hook_workflow_projection_port(),
                script_evaluator: Arc::new(agentdash_infrastructure::RhaiHookScriptEvaluator::new(
                    &hook_preset_scripts,
                )),
            },
        ));
        hook_plan_compiler_handle
            .set(hook_provider.clone())
            .map_err(|_| anyhow::anyhow!("AgentFrame HookPlan compiler composition 重复绑定"))?;
        frame_construction_handle
            .set(Arc::new(
                agentdash_application::frame_construction::AgentRunProjectOwnerFrameConstructionAdapter::new(
                    agentdash_application::frame_construction::AgentRunProjectOwnerFrameConstructionDeps {
                        repos: repos.clone(),
                        vfs_service: vfs_service.clone(),
                        availability: backend_registry.clone(),
                        platform_config: platform_config.clone(),
                        lifecycle_surface_projection: lifecycle_surface_projection.clone(),
                        audit_bus: audit_bus.clone(),
                        hook_plan_compiler: hook_provider.clone(),
                    },
                ),
            ))
            .map_err(|_| anyhow::anyhow!("AgentRun frame-construction composition 重复绑定"))?;
        let mcp_tool_discovery: Arc<
            dyn agentdash_application_ports::mcp_discovery::McpToolDiscovery,
        > = Arc::new(agentdash_executor::mcp::ExecutorMcpToolDiscovery::new(
            Some(mcp_relay_provider.clone()),
        ));
        let extra_skill_dirs = integration_registration.extra_skill_dirs;
        let skill_discovery_providers = integration_registration.skill_discovery_providers;
        let memory_discovery_providers = integration_registration.memory_discovery_providers;
        let session_tool_services =
            agentdash_application::runtime_tools::SharedSessionToolServicesHandle::default();
        let workspace_module_agent_run_bridge =
            agentdash_workspace_module::workspace_module::SharedWorkspaceModuleAgentRunBridgeHandle::default();
        let workspace_module_presentation_append =
            agentdash_workspace_module::workspace_module::SharedWorkspaceModulePresentationAppendHandle::default();
        let workspace_module_runtime_gateway =
            agentdash_workspace_module::workspace_module::SharedWorkspaceModuleRuntimeGatewayHandle::default();
        let inline_persister: Arc<
            dyn agentdash_application_vfs::inline_persistence::InlineContentPersister,
        > = Arc::new(
            agentdash_application_vfs::inline_persistence::DbInlineContentPersister::new(
                repos.inline_file_repo.clone(),
            ),
        );
        let wait_service = agentdash_application::wait_activity::WaitActivityService::new(
            agentdash_application::wait_activity::WaitActivityDeps {
                repositories: repos.wait_activity_repositories(),
                terminal_registry: terminal_registry.clone(),
            },
        );
        let vfs_provider = agentdash_application::runtime_tools::VfsRuntimeToolProvider::new(
            vfs_service.clone(),
            Some(inline_persister),
        )
        .with_materialization_service(vfs_materialization_service)
        .with_shell_output_registry(shell_output_registry.clone());
        let workflow_provider =
            agentdash_application::runtime_tools::WorkflowRuntimeToolProvider::new(
                repos.lifecycle_orchestrator_deps(),
                agentdash_application_lifecycle::lifecycle::tools::SharedSessionToolServicesHandle,
                function_runner.clone(),
            );
        let collaboration_provider =
            agentdash_application::runtime_tools::CollaborationRuntimeToolProvider::new(
                repos.clone(),
                session_tool_services.clone(),
            )
            .with_wait_service(wait_service.clone())
            .with_model_preflight(Arc::new(EffectiveProviderCompanionModelPreflight {
                repos: repos.clone(),
                llm_provider_secret: llm_provider_secret.clone(),
            }))
            .with_workflow_script_preflight(Arc::new(
                agentdash_application::companion::ApplicationWorkflowScriptPreflightAdapter::new(
                    Arc::new(agentdash_infrastructure::RhaiWorkflowScriptEvaluator::new()),
                ),
            ));
        let task_provider =
            agentdash_application::runtime_tools::TaskRuntimeToolProvider::new(repos.clone());
        let wait_provider =
            agentdash_application::wait_activity::WaitRuntimeToolProvider::from_service(
                wait_service,
            );
        let workspace_module_provider =
            agentdash_workspace_module::workspace_module::WorkspaceModuleRuntimeToolProvider::new(
                repos.project_extension_installation_repo.clone(),
                repos.project_repo.clone(),
                repos.canvas_repo.clone(),
                repos.canvas_runtime_state_repo.clone(),
                repos.agent_run_runtime_binding_repo.clone(),
                workspace_module_agent_run_bridge.clone(),
                workspace_module_runtime_gateway.clone(),
            )
            .with_presentation_append_handle(workspace_module_presentation_append.clone())
            .with_extension_channel_transport(backend_registry.clone())
            .with_extension_backend_service_transport(backend_registry.clone());
        let runtime_tool_provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider> =
            Arc::new(agentdash_application::runtime_tools::SessionRuntimeToolComposer::from_final_catalog_providers([
                Arc::new(vfs_provider) as Arc<dyn agentdash_spi::connector::RuntimeToolProvider>,
                Arc::new(workflow_provider),
                Arc::new(collaboration_provider),
                Arc::new(task_provider),
                Arc::new(wait_provider),
                Arc::new(workspace_module_provider),
            ]));
        let runtime_surface_query = Arc::new(BusinessFrameSurfaceQuery::new(
            BusinessFrameSurfaceQueryDeps {
                binding_repo: repos.agent_run_runtime_binding_repo.clone(),
                run_repo: repos.lifecycle_run_repo.clone(),
                agent_repo: repos.lifecycle_agent_repo.clone(),
                frame_repo: repos.agent_frame_repo.clone(),
                permission_grant_repo: repos.permission_grant_repo.clone(),
            },
        ));
        let runtime_surface_query_port: Arc<dyn AgentRunRuntimeSurfaceQueryPort> =
            runtime_surface_query.clone();
        let agent_run_effective_capability: Arc<dyn AgentRunEffectiveCapabilityPort> =
            runtime_surface_query.clone();
        let tool_registry = Arc::new(
            crate::bootstrap::agent_runtime_surface::CompiledAgentRunToolRegistry::default(),
        );
        let base_identity = BaseIdentitySource::resolve(repos.settings_repo.as_ref()).await;
        let business_surface_source = Arc::new(AgentBusinessSurfaceSource::new(
            runtime_surface_query.clone(),
            repos.agent_frame_repo.clone(),
            runtime_tool_provider,
            hook_provider.clone(),
            AgentBusinessSurfaceContextDeps {
                vfs_service: vfs_service.clone(),
                extra_skill_dirs: extra_skill_dirs.clone(),
                skill_discovery_providers: skill_discovery_providers.clone(),
                memory_discovery_providers: memory_discovery_providers.clone(),
                settings_repository: repos.settings_repo.clone(),
                base_identity,
            },
        ));
        let surface_compiler = Arc::new(
            crate::bootstrap::agent_runtime_surface::AgentFrameSurfaceCompositionAdapter::new(
                business_surface_source,
                tool_registry.clone(),
            ),
        );
        tool_registry
            .bind_recovery(Arc::new(
                crate::bootstrap::agent_runtime_surface::CanonicalCompiledAgentRunToolBindingRecovery::new(
                    Arc::downgrade(&surface_compiler),
                    Arc::new(
                        agentdash_infrastructure::persistence::postgres::PostgresAgentRuntimeCompositionRepository::new(
                            runtime_pool.clone(),
                        ),
                    ),
                ),
            ))
            .map_err(anyhow::Error::msg)?;
        let callback_runtime_pool = runtime_pool.clone();
        let callback_tool_registry = tool_registry.clone();
        let callback_effective_capability = agent_run_effective_capability.clone();
        let callback_factory: crate::bootstrap::agent_runtime::AgentRuntimeCallbackFactory =
            Arc::new(move |runtime| {
                let tool_broker_resolver = Arc::new(
                    crate::bootstrap::agent_runtime_surface::PostgresAgentRunToolBrokerResolver::new(
                        callback_runtime_pool.clone(),
                        runtime.clone(),
                        callback_tool_registry.clone(),
                        callback_effective_capability.clone(),
                    ),
                );
                crate::bootstrap::agent_runtime::AgentRuntimeCallbacks {
                    tools: Arc::new(
                        crate::bootstrap::agent_runtime::PlatformAgentRuntimeToolCallback::new(
                            tool_broker_resolver,
                        ),
                    ),
                    hooks: Arc::new(
                        crate::bootstrap::agent_runtime_surface::CanonicalAgentRuntimeHookCallback::new(
                            runtime,
                            callback_tool_registry.clone(),
                        ),
                    ),
                }
            });
        let runtime_composition =
            crate::bootstrap::agent_runtime::build_native_agent_runtime_composition(
                crate::bootstrap::agent_runtime::NativeAgentRuntimeCompositionInput {
                    pool: runtime_pool.clone(),
                    provider_repository: repos.llm_provider_repo.clone(),
                    provider_credential_repository: repos.llm_provider_credential_repo.clone(),
                    secret_codec: llm_provider_secret.clone(),
                    surface_compiler: surface_compiler.clone(),
                    credential_broker: Arc::new(RejectUndeclaredRuntimeCredentials),
                    callback_factory,
                    application_presentation_projector: Arc::new(
                        agentdash_application_agentrun::agent_run::AgentRunRuntimeApplicationPresentationProjector,
                    ),
                    remote_definitions: runtime_definition_registry.definitions(),
                    remote_trust_manifests: integration_registration.runtime_trust_manifests,
                    remote_placements: Arc::new(
                        crate::relay::CloudRuntimeWirePlacementResolver::new(
                            backend_registry.clone(),
                            64,
                        ),
                    ),
                    node_id: "agentdash-api".to_string(),
                },
            )?;
        workspace_module_presentation_append
            .set(Arc::new(CanonicalWorkspaceModulePresentationAppender {
                runtime: runtime_composition.managed_runtime.clone(),
            }))
            .await;
        let runtime_surface_adopter = Arc::new(
            crate::bootstrap::agent_runtime_surface::CanonicalRuntimeSurfaceAdopter::new(
                surface_compiler,
                runtime_composition.surfaces.clone(),
                runtime_composition.bindings.clone(),
                runtime_composition.managed_runtime.clone(),
                tool_registry.clone(),
            ),
        );
        workspace_module_agent_run_bridge
            .set(Arc::new(CanonicalWorkspaceModuleAgentRunBridge {
                service: AgentRunRuntimeSurfaceUpdateService::new(
                    AgentRunRuntimeSurfaceUpdateDeps {
                        surface_query: runtime_surface_query_port.clone(),
                        frame_repo: repos.agent_frame_repo.clone(),
                        vfs_service: Some(vfs_service.clone()),
                        active_adopter: runtime_surface_adopter,
                        extra_skill_dirs: extra_skill_dirs.clone(),
                        skill_discovery_providers: skill_discovery_providers.clone(),
                    },
                ),
            }))
            .await;
        runtime_provisioner_handle
            .set(runtime_composition.provisioner.clone())
            .map_err(|_| anyhow::anyhow!("AgentRun runtime provisioner 重复绑定"))?;
        let agent_run_runtime: Arc<dyn AgentRunRuntime> = Arc::new(ManagedAgentRunRuntime::new(
            runtime_composition.gateway.clone(),
            runtime_composition.bindings.clone(),
            runtime_composition.provisioner.clone(),
            runtime_composition.presentation_plans.clone(),
            tool_registry.clone(),
        ));
        let agent_run_journal = Arc::new(AgentRunJournalService::new(
            repos.agent_run_lineage_repo.clone(),
            Arc::new(CanonicalAgentRunJournalBindingResolver {
                bindings: runtime_composition.bindings.clone(),
            }),
            Arc::new(CanonicalAgentRunJournalSource {
                repository: runtime_composition.runtime_repository.clone(),
            }),
        ));
        agent_run_journal_reader
            .bind(agent_run_journal.clone())
            .map_err(anyhow::Error::msg)?;
        runtime_composition
            .outbox_worker
            .clone()
            .spawn(tokio_util::sync::CancellationToken::new());
        runtime_composition
            .durable_workers
            .clone()
            .spawn(tokio_util::sync::CancellationToken::new());
        let runtime_mailbox_worker = Arc::new(RuntimeAgentRunMailbox::new(
            repos.agent_run_mailbox_repo.clone(),
            agent_run_runtime.clone(),
        ));
        repos
            .workflow_agent_run_delivery
            .set(runtime_mailbox_worker.clone())
            .await;
        let agent_run_product_delivery: Arc<dyn AgentRunProductDeliveryPort> =
            runtime_mailbox_worker.clone();
        session_tool_services
            .set(agentdash_application::runtime_tools::SessionToolServices {
                product_delivery: agent_run_product_delivery.clone(),
            })
            .await;
        let mailbox_recovery_worker = runtime_mailbox_worker.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                if let Err(error) = mailbox_recovery_worker.recover_pending_once().await {
                    diag!(Error, Subsystem::AgentRun,
                        error = %error,
                        "AgentRun runtime mailbox recovery failed"
                    );
                }
            }
        });
        let parent_mailbox_delivery = Arc::new(
            agentdash_application::companion::AgentRunCompanionMailboxDelivery::new(
                repos.clone(),
                agentdash_application::runtime_tools::SessionToolServices {
                    product_delivery: agent_run_product_delivery.clone(),
                },
            ),
        );
        let wait_convergence = Arc::new(
            crate::agent_run_terminal_control::RuntimeWaitProducerTerminalConvergence::new(
                runtime_composition.bindings.clone(),
                agentdash_application::gate_wait_policy::GateProducerTerminalConvergenceServiceAdapter::with_mailbox_wake_delivery(
                    repos.lifecycle_gate_repo.clone(),
                    runtime_composition.bindings.clone(),
                    Arc::new(agentdash_application::gate_wait_policy::CompanionGateMailboxWakeDelivery::new(parent_mailbox_delivery)),
                ),
                agent_run_journal.clone(),
            ),
        );
        let lifecycle_convergence = Arc::new(
            agentdash_application_lifecycle::LifecycleOrchestrator::new(
                repos.lifecycle_orchestrator_deps(),
            )
            .with_function_runner(function_runner.clone()),
        );
        let terminal_hooks = Arc::new(
            crate::agent_run_terminal_control::RuntimeTerminalHookEffects::new(
                tool_registry.clone(),
                Arc::new(EmptyAgentRunHookEffectHandlerRegistry),
            ),
        );
        let terminal_effects: Arc<dyn AgentRunControlEffectPort> = Arc::new(
            AgentRunControlEffectService::new(AgentRunControlEffectDeps {
                store: Arc::new(
                    agentdash_infrastructure::PostgresAgentRunControlEffectStore::new(
                        runtime_pool.clone(),
                    ),
                ),
                delivery: Arc::new(RuntimeMailboxTerminalConvergence::new(
                    runtime_composition.bindings.clone(),
                    runtime_mailbox_worker.as_ref().clone(),
                )),
                wait_producer: wait_convergence,
                lifecycle: lifecycle_convergence,
                terminal_hooks,
            }),
        );
        let terminal_application_effect_worker = Arc::new(
            crate::agent_run_terminal_control::RuntimeTerminalApplicationEffectWorker::new(
                runtime_composition.runtime_repository.clone(),
                terminal_effects,
                agentdash_agent_runtime::RuntimeWorkerId(format!(
                    "agentdash-api-terminal-effects-{}",
                    uuid::Uuid::new_v4()
                )),
                30_000,
                100,
            )?,
        );
        let agent_runtime_host = runtime_composition.host;
        let agent_runtime_inventory = Arc::new(crate::relay::CloudRemoteRuntimeInventory::new(
            agent_runtime_host.clone(),
        ));
        let lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort> = Arc::new(
            LifecycleReadModelQueryAdapter::new(repos.lifecycle_read_model_repos()),
        );
        let resource_surface_query = Arc::new(BusinessResourceSurfaceQuery::new(
            BusinessResourceSurfaceQueryDeps {
                binding_repo: repos.agent_run_runtime_binding_repo.clone(),
                surface_query: runtime_surface_query_port.clone(),
                lifecycle_surface_projection: Arc::new(
                    AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(
                        repos.skill_asset_repo.clone(),
                    ),
                ),
            },
        ));
        let resource_surface_query_port: Arc<dyn AgentRunResourceSurfaceQueryPort> =
            resource_surface_query.clone();
        let vfs_surface_resolver = VfsSurfaceResolver::new(VfsSurfaceResolverDeps {
            repos: repos.clone(),
            vfs_service: vfs_service.clone(),
            resource_surface_query: resource_surface_query_port,
        });
        let project_agent_run_list_query =
            ProjectAgentRunListQuery::new(ProjectAgentRunListQueryDeps {
                run_repo: repos.lifecycle_run_repo.clone(),
                agent_repo: repos.lifecycle_agent_repo.clone(),
                lineage_repo: repos.agent_lineage_repo.clone(),
                subject_repo: repos.lifecycle_subject_association_repo.clone(),
                project_agent_repo: repos.project_agent_repo.clone(),
                runtime: agent_run_runtime.clone(),
            });
        let session_mcp_access = Arc::new(CurrentSurfaceRuntimeMcpAccess::new(
            runtime_surface_query.clone(),
            mcp_tool_discovery,
        ));
        let runtime_gateway = crate::bootstrap::runtime_gateway::build_runtime_gateway(
            mcp_probe_relay,
            repos.clone(),
            backend_registry.clone(),
            setup_action_transport,
            session_mcp_access,
            repos.project_extension_installation_repo.clone(),
            backend_registry.clone(),
        );
        workspace_module_runtime_gateway
            .set(runtime_gateway.clone())
            .await;
        let extension_runtime_channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            repos.project_extension_installation_repo.clone(),
            backend_registry.clone(),
        ));

        let auth_mode = crate::bootstrap::auth::validate_auth_provider_registered(
            crate::bootstrap::auth::resolve_configured_auth_mode()?,
            integration_registration.auth_provider.is_some(),
        )?;

        let vfs_registry = crate::bootstrap::vfs::build_vfs_discovery_registry(
            integration_registration.vfs_providers,
        );

        let routine_executor = Arc::new(RoutineExecutor::new(
            repos.clone(),
            backend_registry.clone(),
            agent_run_product_delivery.clone(),
        ));

        let state = Self {
            repos,
            services: ServiceSet {
                agent_run_runtime,
                agent_run_journal,
                agent_run_product_delivery,
                terminal_application_effect_worker,
                project_agent_run_list_query,
                agent_runtime_host,
                agent_runtime_inventory,
                runtime_surface_query: runtime_surface_query_port,
                lifecycle_read_model_query,
                resource_surface_query,
                vfs_surface_resolver,
                vfs_service,
                vfs_mutation_dispatcher,
                extra_skill_dirs,
                skill_discovery_providers,
                memory_discovery_providers,
                marketplace_source_providers: integration_registration.marketplace_source_providers,
                backend_registry,
                backend_runtime_events,
                project_control_plane_events,
                shell_output_registry,
                terminal_registry,
                vfs_registry,
                mount_provider_registry,
                hook_provider,
                auth_session_service,
                cron_scheduler: CronSchedulerHandle::new(),
                routine_executor: Some(routine_executor),
                audit_bus,
                runtime_gateway,
                extension_runtime_channel_invoker,
                extension_package_artifact_storage,
                function_runner,
            },
            config: AppConfig {
                platform_config,
                auth_mode,
            },
            secrets: SecretSet {
                llm_provider_secret,
            },
            auth_provider: integration_registration.auth_provider,
            identity_directory_provider: integration_registration.identity_directory_provider,
            diagnostics,
        };

        let mut state = Arc::new(state);

        crate::bootstrap::background_workers::start_post_app_state_workers(&mut state).await;

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_platform_mcp_base_url;

    #[test]
    fn platform_mcp_base_url_missing_env_keeps_platform_mcp_disabled() {
        assert_eq!(resolve_platform_mcp_base_url(None), None);
    }

    #[test]
    fn platform_mcp_base_url_blank_env_keeps_platform_mcp_disabled() {
        assert_eq!(resolve_platform_mcp_base_url(Some("   ".to_string())), None);
    }

    #[test]
    fn platform_mcp_base_url_uses_explicit_env_value() {
        assert_eq!(
            resolve_platform_mcp_base_url(Some("  http://127.0.0.1:3001/  ".to_string())),
            Some("http://127.0.0.1:3001/".to_string())
        );
    }
}
