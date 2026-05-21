use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::mount_providers::RelayFsMountProvider;
use crate::plugins::{
    builtin_plugins, collect_plugin_registration, validate_connector_executor_ids,
};
use crate::relay::registry::BackendRegistry;
use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::context::{
    InMemoryContextAuditBus, SharedContextAuditBus, VfsDiscoveryRegistry, builtin_vfs_registry,
};
use agentdash_application::hooks::AppExecutionHookProvider;
use agentdash_application::platform_config::{PlatformConfig, SharedPlatformConfig};
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application::routine::RoutineExecutor;
use agentdash_application::runtime_gateway::{
    McpCallToolProvider, McpListToolsProvider, McpProbeTransportProvider, RuntimeGateway,
    RuntimeSessionMcpAccess, WorkspaceBrowseDirectoryProvider, WorkspaceDetectGitProvider,
    WorkspaceDetectProvider,
};
use agentdash_application::scheduling::CronSchedulerHandle;
use agentdash_application::session::{
    SessionCapabilityService, SessionControlService, SessionCoreService, SessionEffectsService,
    SessionEventingService, SessionHookService, SessionLaunchService, SessionRuntimeBuilder,
    SessionRuntimeService, SessionTitleService,
};
use agentdash_application::shared_library::SharedLibraryService;
use agentdash_application::task::service::StoryStepActivationService;
use agentdash_application::task_lock::TaskLockMap;
use agentdash_application::vfs::RelayVfsService;
use agentdash_application::vfs::tools::provider::{
    RelayRuntimeToolProvider, SessionToolServices, SharedSessionToolServicesHandle,
};
use agentdash_application::vfs::{MountProviderRegistry, MountProviderRegistryBuilder};
use agentdash_domain::llm_provider::LlmProviderRepository;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_executor::AgentConnector;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_infrastructure::{
    PostgresAuthSessionRepository, PostgresBackendRepository, PostgresCanvasRepository,
    PostgresInlineFileRepository, PostgresLlmProviderRepository, PostgresMcpPresetRepository,
    PostgresProjectAgentRepository, PostgresProjectBackendAccessRepository,
    PostgresProjectExtensionInstallationRepository, PostgresProjectVfsMountRepository,
    PostgresProjectRepository, PostgresRoutineExecutionRepository, PostgresRoutineRepository,
    PostgresRuntimeHealthRepository, PostgresSessionBindingRepository, PostgresSessionRepository,
    PostgresSettingsRepository, PostgresSharedLibraryRepository, PostgresSkillAssetRepository,
    PostgresStateChangeRepository, PostgresStoryRepository, PostgresUserDirectoryRepository,
    PostgresWorkflowRepository, PostgresWorkspaceRepository,
};
use agentdash_plugin_api::AgentDashPlugin;
use agentdash_plugin_api::AuthMode;

const BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY: usize = 256;

/// 应用服务集合 — 执行引擎、连接器与各类注册表
pub struct ServiceSet {
    pub session_core: SessionCoreService,
    pub session_eventing: SessionEventingService,
    pub session_runtime: SessionRuntimeService,
    pub session_control: SessionControlService,
    pub session_launch: SessionLaunchService,
    pub session_hooks: SessionHookService,
    pub session_capability: SessionCapabilityService,
    pub session_effects: SessionEffectsService,
    pub session_title: SessionTitleService,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
    /// 统一 VFS 访问服务 — 供 declared sources、runtime tools、workspace browse 共享
    pub vfs_service: Arc<RelayVfsService>,
    /// 插件额外 skill 目录 — construction 阶段统一 discovery 后进入 session capabilities。
    pub extra_skill_dirs: Vec<std::path::PathBuf>,
    /// WebSocket 中继后端注册表 — 跟踪在线的本机后端
    pub backend_registry: Arc<BackendRegistry>,
    /// Backend runtime 在线/离线/能力变化事件 — 供全局事件流驱动前端刷新
    pub backend_runtime_events: broadcast::Sender<String>,
    /// 串行 Shell 流式输出路由 — ShellExecTool 注册，ws_handler 投递
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    /// 交互式终端运行时状态缓存
    pub terminal_cache: Arc<agentdash_application::session::terminal_cache::SessionTerminalCache>,
    /// 寻址空间注册表 — 持有可用的资源引用能力提供者
    pub vfs_registry: VfsDiscoveryRegistry,
    /// Mount 级 I/O 提供者注册表（`inline_fs` / `relay_fs` 等）
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    /// Story step activation 服务 — task route 仅作为用户入口转发到这里
    pub story_step_activation_service: Arc<StoryStepActivationService>,
    /// Hook 提供者 — 供 API 层验证脚本等管理接口使用
    pub hook_provider: Arc<AppExecutionHookProvider>,
    /// 统一认证会话服务（application 层）
    pub auth_session_service: Arc<AuthSessionService>,
    /// 业务终态 → session cancel 指令通道 — Story/Task 状态变更时联动 session 取消
    pub terminal_cancel_coordinator:
        Arc<agentdash_application::reconcile::terminal_cancel::TerminalCancelCoordinator>,
    /// Cron 调度器句柄 — 配置变更时调用 `notify_config_changed()` 触发热重载
    pub cron_scheduler: CronSchedulerHandle,
    /// Routine 执行器 — 统一处理定时/Webhook/插件触发
    pub routine_executor: Option<Arc<RoutineExecutor>>,
    /// Session 上下文审计总线 — Bundle / Fragment 产出与消费的可观测轨迹
    pub audit_bus: SharedContextAuditBus,
    /// 统一运行时能力网关 — Session/Setup runtime action 的共享入口
    pub runtime_gateway: Arc<RuntimeGateway>,
}

/// 应用级配置
pub struct AppConfig {
    /// 进程级平台配置（MCP base URL 等不变量，`Arc` 共享避免逐层透传）
    pub platform_config: SharedPlatformConfig,
    /// 当前宿主配置的认证模式
    pub auth_mode: AuthMode,
}

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
/// 按职责分为 3 个子集：repos / services / config。
pub struct AppState {
    pub repos: RepositorySet,
    pub services: ServiceSet,
    pub config: AppConfig,
    /// 认证/授权提供者（由插件注入，None 表示无认证）
    pub auth_provider: Option<Arc<dyn agentdash_plugin_api::AuthProvider>>,
}

impl AppState {
    pub async fn new(pool: PgPool) -> Result<Arc<Self>> {
        Self::new_with_plugins(pool, builtin_plugins()).await
    }

    /// 携带插件列表构建 AppState
    ///
    /// 返回 `Arc<Self>` 以支持内部 `DeferredTurnDispatcher` 的延迟绑定。
    pub async fn new_with_plugins(
        pool: PgPool,
        plugins: Vec<Box<dyn AgentDashPlugin>>,
    ) -> Result<Arc<Self>> {
        let plugin_registration = collect_plugin_registration(plugins)
            .map_err(|err| anyhow::anyhow!("插件注册失败: {err}"))?;

        // 按依赖顺序初始化：projects → workspaces → stories → tasks
        let project_repo = Arc::new(PostgresProjectRepository::new(pool.clone()));

        let canvas_repo = Arc::new(PostgresCanvasRepository::new(pool.clone()));

        let workspace_repo = Arc::new(PostgresWorkspaceRepository::new(pool.clone()));
        workspace_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("workspaces 表初始化失败: {e}"))?;

        let story_repo = Arc::new(PostgresStoryRepository::new(pool.clone()));
        let state_change_repo = Arc::new(PostgresStateChangeRepository::new(pool.clone()));

        let session_binding_repo = Arc::new(PostgresSessionBindingRepository::new(pool.clone()));
        let session_repo = Arc::new(PostgresSessionRepository::new(pool.clone()));

        let backend_repo = Arc::new(PostgresBackendRepository::new(pool.clone()));
        let runtime_health_repo = Arc::new(PostgresRuntimeHealthRepository::new(pool.clone()));
        runtime_health_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("runtime_health 表初始化失败: {e}"))?;
        let project_backend_access_repo =
            Arc::new(PostgresProjectBackendAccessRepository::new(pool.clone()));
        project_backend_access_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("project_backend_access 表初始化失败: {e}"))?;

        let user_directory_repo = Arc::new(PostgresUserDirectoryRepository::new(pool.clone()));

        let settings_repo = Arc::new(PostgresSettingsRepository::new(pool.clone()));

        let shared_library_repo = Arc::new(PostgresSharedLibraryRepository::new(pool.clone()));
        shared_library_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("library_assets 表初始化失败: {e}"))?;
        {
            let service = SharedLibraryService::new(shared_library_repo.as_ref());
            let seeded = service
                .seed_builtin_assets(Default::default())
                .await
                .map_err(|e| anyhow::anyhow!("builtin Shared Library assets 初始化失败: {e}"))?;
            tracing::info!(
                seeded = seeded.len(),
                "已同步 builtin Shared Library assets"
            );
        }

        let project_extension_installation_repo = Arc::new(
            PostgresProjectExtensionInstallationRepository::new(pool.clone()),
        );
        project_extension_installation_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("project_extension_installations 表初始化失败: {e}"))?;

        let project_agent_repo = Arc::new(PostgresProjectAgentRepository::new(pool.clone()));
        project_agent_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("project_agents 表初始化失败: {e}"))?;

        let project_vfs_mount_repo =
            Arc::new(PostgresProjectVfsMountRepository::new(pool.clone()));
        project_vfs_mount_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("project_vfs_mounts 表初始化失败: {e}"))?;

        let routine_repo = Arc::new(PostgresRoutineRepository::new(pool.clone()));
        routine_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("routines 表初始化失败: {e}"))?;
        let routine_execution_repo =
            Arc::new(PostgresRoutineExecutionRepository::new(pool.clone()));
        routine_execution_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("routine_executions 表初始化失败: {e}"))?;

        let llm_provider_repo = Arc::new(PostgresLlmProviderRepository::new(pool.clone()));
        llm_provider_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("llm_providers 表初始化失败: {e}"))?;

        let auth_session_repo = Arc::new(PostgresAuthSessionRepository::new(pool.clone()));
        let auth_session_service = Arc::new(AuthSessionService::new(auth_session_repo.clone()));

        let workflow_repo = Arc::new(PostgresWorkflowRepository::new(pool.clone()));

        let mcp_preset_repo = Arc::new(PostgresMcpPresetRepository::new(pool.clone()));
        mcp_preset_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("mcp_presets 表初始化失败: {e}"))?;

        let skill_asset_repo = Arc::new(PostgresSkillAssetRepository::new(pool.clone()));
        skill_asset_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("skill_assets 表初始化失败: {e}"))?;

        let inline_file_repo = Arc::new(PostgresInlineFileRepository::new(pool));
        inline_file_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("inline_fs_files 表初始化失败: {e}"))?;

        // RepositorySet —— 提前构造,供 build_pi_agent_connector / RoutineExecutor / AppState 共用
        let repos = RepositorySet {
            project_repo: project_repo.clone(),
            canvas_repo: canvas_repo.clone(),
            workspace_repo: workspace_repo.clone(),
            story_repo: story_repo.clone(),
            state_change_repo: state_change_repo.clone(),
            session_binding_repo: session_binding_repo.clone(),
            backend_repo: backend_repo.clone(),
            runtime_health_repo: runtime_health_repo.clone(),
            project_backend_access_repo: project_backend_access_repo.clone(),
            backend_workspace_inventory_repo: project_backend_access_repo.clone(),
            auth_session_repo: auth_session_repo.clone(),
            user_directory_repo: user_directory_repo.clone(),
            settings_repo: settings_repo.clone(),
            shared_library_repo: shared_library_repo.clone(),
            project_extension_installation_repo: project_extension_installation_repo.clone(),
            llm_provider_repo: llm_provider_repo.clone(),
            mcp_preset_repo: mcp_preset_repo.clone(),
            skill_asset_repo: skill_asset_repo.clone(),
            project_agent_repo: project_agent_repo.clone(),
            project_vfs_mount_repo: project_vfs_mount_repo.clone(),
            workflow_definition_repo: workflow_repo.clone(),
            workflow_template_install_repo: workflow_repo.clone(),
            lifecycle_definition_repo: workflow_repo.clone(),
            activity_lifecycle_definition_repo: workflow_repo.clone(),
            activity_execution_claim_repo: workflow_repo.clone(),
            lifecycle_run_repo: workflow_repo.clone(),
            routine_repo: routine_repo.clone(),
            routine_execution_repo: routine_execution_repo.clone(),
            inline_file_repo: inline_file_repo.clone(),
        };

        let plugin_asset_count = plugin_registration.library_asset_seeds.len();
        if plugin_asset_count > 0 {
            let service = SharedLibraryService::new(shared_library_repo.as_ref());
            let seeded = service
                .seed_plugin_embedded_assets(plugin_registration.library_asset_seeds.clone())
                .await
                .map_err(|e| anyhow::anyhow!("plugin embedded library assets 初始化失败: {e}"))?;
            tracing::info!(
                declared = plugin_asset_count,
                seeded = seeded.len(),
                "已同步 plugin embedded Shared Library assets"
            );
        }

        let backend_registry = BackendRegistry::new();
        let (backend_runtime_events, _) =
            broadcast::channel(BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY);
        let mcp_probe_relay: Arc<dyn agentdash_spi::McpRelayProvider> = backend_registry.clone();
        let setup_action_transport: Arc<
            dyn agentdash_application::backend_transport::BackendTransport,
        > = backend_registry.clone();
        let shell_output_registry = agentdash_relay::ShellOutputRegistry::new();
        let terminal_cache =
            agentdash_application::session::terminal_cache::SessionTerminalCache::new();

        let mut mount_registry_builder = MountProviderRegistryBuilder::new()
            .with_builtins(
                workflow_repo.clone(),
                canvas_repo.clone(),
                inline_file_repo.clone(),
                skill_asset_repo.clone(),
                session_repo.clone(),
            )
            .register(Arc::new(RelayFsMountProvider::new(
                backend_registry.clone(),
            )));

        for provider in plugin_registration.mount_providers {
            tracing::info!("注册插件 MountProvider: {}", provider.provider_id());
            mount_registry_builder = mount_registry_builder.register(provider);
        }

        let mount_provider_registry = Arc::new(mount_registry_builder.build());

        let vfs_service = Arc::new(RelayVfsService::new(mount_provider_registry.clone()));
        let session_services_handle = SharedSessionToolServicesHandle::default();

        let inline_persister: Arc<
            dyn agentdash_application::vfs::inline_persistence::InlineContentPersister,
        > = Arc::new(
            agentdash_application::vfs::inline_persistence::DbInlineContentPersister::new(
                inline_file_repo.clone(),
            ),
        );

        let platform_config: SharedPlatformConfig = Arc::new(PlatformConfig {
            mcp_base_url: std::env::var("AGENTDASH_MCP_BASE_URL").ok().or_else(|| {
                let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
                Some(format!("http://127.0.0.1:{port}"))
            }),
        });

        let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();
        let mut title_bridge: Option<Arc<dyn agentdash_agent::LlmBridge>> = None;
        let mut prompt_config: Option<(String, Vec<String>)> = None;
        let materialization_transport = Arc::new(
            crate::vfs_materialization::RelayVfsMaterializationTransport::new(
                backend_registry.clone(),
            ),
        );
        let materialization_service =
            Arc::new(agentdash_application::vfs::VfsMaterializationService::new(
                vfs_service.clone(),
                materialization_transport.clone(),
            ));

        let runtime_tool_provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider> =
            Arc::new(
                RelayRuntimeToolProvider::new(
                    vfs_service.clone(),
                    repos.clone(),
                    session_services_handle.clone(),
                    Some(inline_persister),
                    platform_config.clone(),
                )
                .with_materialization_service(materialization_service.clone())
                .with_shell_output_registry(shell_output_registry.clone()),
            );
        let mcp_relay_provider: Arc<dyn agentdash_spi::McpRelayProvider> = Arc::new(
            crate::vfs_materialization::MaterializingMcpRelayProvider::new(
                backend_registry.clone(),
                materialization_service,
            ),
        );

        if let Some(result) = build_pi_agent_connector(PiAgentConnectorDeps {
            settings_repo: settings_repo.clone(),
            llm_provider_repo: llm_provider_repo.clone(),
        })
        .await
        {
            title_bridge = Some(result.connector.default_bridge());
            prompt_config = Some((
                result.connector.base_system_prompt().to_string(),
                result.connector.user_preferences().to_vec(),
            ));
            sub_connectors.push(Arc::new(result.connector));
        }
        // relay connector — 将远程后端上报的执行器纳入统一路由
        {
            let relay_transport: Arc<
                dyn agentdash_application::backend_transport::RelayPromptTransport,
            > = backend_registry.clone();
            sub_connectors.push(Arc::new(
                agentdash_application::relay_connector::RelayAgentConnector::new(relay_transport),
            ));
        }

        sub_connectors.extend(plugin_registration.connectors);
        validate_connector_executor_ids(&sub_connectors)
            .map_err(|err| anyhow::anyhow!("连接器注册失败: {err}"))?;

        let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
        let hook_provider = Arc::new(AppExecutionHookProvider::new(
            project_repo.clone(),
            story_repo.clone(),
            session_binding_repo.clone(),
            workflow_repo.clone(),
            workflow_repo.clone(),
            workflow_repo.clone(),
            inline_file_repo.clone(),
        ));
        let extra_skill_dirs = plugin_registration.extra_skill_dirs.clone();
        let mut session_runtime_builder = SessionRuntimeBuilder::new_with_hooks_and_persistence(
            connector.clone(),
            Some(hook_provider.clone()),
            session_repo,
        )
        .with_vfs_service(vfs_service.clone())
        .with_extra_skill_dirs(plugin_registration.extra_skill_dirs)
        .with_runtime_tool_provider(runtime_tool_provider)
        .with_mcp_relay_provider(mcp_relay_provider);
        if let Some((base_sp, user_prefs)) = prompt_config {
            session_runtime_builder =
                session_runtime_builder.with_system_prompt_config(base_sp, user_prefs);
        }
        if let Some(bridge) = title_bridge {
            session_runtime_builder = session_runtime_builder.with_title_generator(Arc::new(
                crate::title_generator::LlmTitleGenerator::new(bridge),
            ));
        }
        let session_core = session_runtime_builder.core_service();
        let session_eventing = session_runtime_builder.eventing_service();
        let session_runtime = session_runtime_builder.runtime_service();
        let session_control = session_runtime_builder.control_service();
        let session_launch = session_runtime_builder.launch_service();
        let session_hooks = session_runtime_builder.hook_service();
        let session_capability = session_runtime_builder.capability_service();
        let session_effects = session_runtime_builder.effects_service();
        let session_title = session_runtime_builder.title_service();

        // Lifecycle DAG Orchestrator — 在 session 终态后评估后继 node 并启动新 session
        {
            let orchestrator =
                Arc::new(agentdash_application::workflow::LifecycleOrchestrator::new(
                    session_core.clone(),
                    session_launch.clone(),
                    session_hooks.clone(),
                    session_capability.clone(),
                    repos.clone(),
                    platform_config.clone(),
                ));
            session_runtime_builder
                .set_terminal_callback(orchestrator)
                .await;
        }

        session_services_handle
            .set(SessionToolServices {
                core: session_core.clone(),
                eventing: session_eventing.clone(),
                control: session_control.clone(),
                launch: session_launch.clone(),
                hooks: session_hooks.clone(),
                capability: session_capability.clone(),
                companion_wait_registry: session_runtime_builder.companion_wait_registry(),
            })
            .await;

        let session_mcp_access: Arc<dyn RuntimeSessionMcpAccess> =
            Arc::new(session_capability.clone());
        let runtime_gateway = Arc::new(
            RuntimeGateway::new()
                .with_provider(Arc::new(McpProbeTransportProvider::new(Some(
                    mcp_probe_relay,
                ))))
                .with_provider(Arc::new(WorkspaceDetectProvider::new(
                    setup_action_transport.clone(),
                )))
                .with_provider(Arc::new(WorkspaceDetectGitProvider::new(
                    setup_action_transport.clone(),
                )))
                .with_provider(Arc::new(WorkspaceBrowseDirectoryProvider::new(
                    setup_action_transport,
                )))
                .with_provider(Arc::new(McpListToolsProvider::new(
                    session_mcp_access.clone(),
                )))
                .with_provider(Arc::new(McpCallToolProvider::new(session_mcp_access))),
        );

        let lock_map = Arc::new(TaskLockMap::new());

        let project_repo_port: Arc<dyn ProjectRepository> = project_repo.clone();
        let state_change_repo_port: Arc<dyn StateChangeRepository> = state_change_repo.clone();
        let story_repo_port: Arc<dyn StoryRepository> = story_repo.clone();

        // 启动对账管线：Session → Task → Infrastructure（有序不可跳过）
        //
        // M2-c：Task 对账改为从 LifecycleRun/step state 反投影，不再需要 session 状态读取器。
        {
            let deps = agentdash_application::reconcile::boot::BootReconcileDeps {
                session_runtime: session_runtime.clone(),
                project_repo: project_repo_port.clone(),
                state_change_repo: state_change_repo_port.clone(),
                story_repo: story_repo_port.clone(),
                session_binding_repo: session_binding_repo.clone(),
                workflow_definition_repo: workflow_repo.clone(),
                activity_lifecycle_definition_repo: workflow_repo.clone(),
                lifecycle_run_repo: workflow_repo.clone(),
            };
            let report = agentdash_application::reconcile::boot::run_boot_reconcile(&deps).await;
            if report.has_errors() {
                for phase in &report.phases {
                    for err in &phase.errors {
                        tracing::warn!(phase = phase.phase, error = %err, "启动对账阶段出错");
                    }
                }
            }
        }

        let auth_mode = resolve_configured_auth_mode()?;

        if plugin_registration.auth_provider.is_none() {
            anyhow::bail!("认证模式 `{auth_mode}` 未注册 AuthProvider，无法启动服务");
        }

        tracing::info!(auth_mode = %auth_mode, "认证模式已加载");

        let mut vfs_registry = builtin_vfs_registry();
        for provider in plugin_registration.vfs_providers {
            vfs_registry.register(provider);
        }

        let terminal_cancel_coordinator = Arc::new(
            agentdash_application::reconcile::terminal_cancel::TerminalCancelCoordinator::new(
                session_runtime.clone(),
                story_repo_port.clone(),
                repos.session_binding_repo.clone(),
            ),
        );

        let dispatcher =
            crate::bootstrap::turn_dispatcher::AppStateTurnDispatcher::new(session_runtime.clone());

        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(2000));
        session_runtime_builder
            .set_context_audit_bus(audit_bus.clone())
            .await;

        let story_step_activation_service = Arc::new(StoryStepActivationService {
            repos: repos.clone(),
            session_core: session_core.clone(),
            session_eventing: session_eventing.clone(),
            session_launch: session_launch.clone(),
            backend_availability: backend_registry.clone(),
            dispatcher: dispatcher.clone(),
            lock_map: lock_map.clone(),
        });

        let state = Self {
            repos,
            services: ServiceSet {
                session_core,
                session_eventing,
                session_runtime,
                session_control,
                session_launch,
                session_hooks,
                session_capability,
                session_effects,
                session_title,
                connector,
                vfs_service,
                extra_skill_dirs,
                backend_registry,
                backend_runtime_events,
                shell_output_registry,
                terminal_cache,
                vfs_registry,
                mount_provider_registry,
                story_step_activation_service,
                hook_provider,
                auth_session_service,
                terminal_cancel_coordinator,
                cron_scheduler: CronSchedulerHandle::new(),
                routine_executor: None,
                audit_bus,
                runtime_gateway,
            },
            config: AppConfig {
                platform_config,
                auth_mode,
            },
            auth_provider: plugin_registration.auth_provider,
        };

        let mut state = Arc::new(state);

        // 注入 SessionConstructionProvider：让 session 内部 auto-resume 等 prompt
        // 路径与 HTTP 主通道共享同一条 `build_session_construction_for_launch`，
        // 避免 owner / MCP / capability_state / context_bundle 漂移。
        {
            let provider = Arc::new(
                crate::bootstrap::session_construction_provider::AppStateSessionConstructionProvider::new(
                    state.clone(),
                ),
            );
            session_runtime_builder
                .set_session_construction_provider(provider)
                .await;
        }

        {
            let registry = Arc::new(
                agentdash_application::task::gateway::effect_executor::TaskHookEffectHandlerRegistry {
                    repos: state.repos.clone(),
                },
            );
            session_runtime_builder
                .set_hook_effect_handler_registry(registry)
                .await;
        }

        session_runtime_builder
            .assert_ready_for_app_state()
            .await
            .map_err(|err| anyhow::anyhow!("AppState session 依赖未 ready: {err}"))?;

        match state
            .services
            .session_effects
            .replay_terminal_effect_outbox(100)
            .await
        {
            Ok(count) if count > 0 => {
                tracing::info!(count, "已调度 terminal effect outbox 恢复执行");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(error = %error, "terminal effect outbox 恢复执行失败");
            }
        }

        // 后台 session stall 检测：定期扫描 running session，超时自动取消
        agentdash_application::session::stall_detector::spawn_stall_detector(
            state.services.session_runtime.clone(),
            agentdash_application::session::stall_detector::DEFAULT_STALL_TIMEOUT_MS,
        );

        // 后台 cron 调度器：从 Routine 表加载 scheduled 类型条目，按 cron 表达式触发
        {
            let routine_executor = Arc::new(
                RoutineExecutor::new(
                    state.repos.clone(),
                    state.services.session_core.clone(),
                    state.services.session_launch.clone(),
                    state.services.vfs_service.clone(),
                    state.services.connector.clone(),
                    state.config.platform_config.clone(),
                    state.services.backend_registry.clone(),
                )
                .with_audit_bus(state.services.audit_bus.clone()),
            );
            // 将 executor 注入 ServiceSet（通过 Arc::get_mut 安全修改）
            // SAFETY: 此时 state 的 Arc 引用计数为 1，get_mut 保证成功
            if let Some(s) = Arc::get_mut(&mut state) {
                s.services.routine_executor = Some(routine_executor.clone());
            }
            let cron_repos = state.repos.clone();
            let cron_handle = state.services.cron_scheduler.clone();
            tokio::spawn(async move {
                agentdash_application::scheduling::cron_scheduler::spawn_cron_scheduler(
                    cron_repos,
                    routine_executor,
                    &cron_handle,
                )
                .await;
            });
        }

        // 后台定时清理过期认证会话，避免 auth_sessions 无限增长。
        {
            let auth_session_service = state.services.auth_session_service.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10 * 60));
                loop {
                    interval.tick().await;
                    match auth_session_service.cleanup_expired_sessions().await {
                        Ok(count) if count > 0 => {
                            tracing::info!(deleted = count, "已清理过期认证会话")
                        }
                        Ok(_) => {}
                        Err(err) => {
                            tracing::warn!(error = %err, "清理过期认证会话失败")
                        }
                    }
                }
            });
        }

        Ok(state)
    }
}

fn resolve_configured_auth_mode() -> Result<AuthMode> {
    match std::env::var("AGENTDASH_AUTH_MODE") {
        Ok(raw) => raw
            .parse::<AuthMode>()
            .map_err(|err| anyhow::anyhow!("AGENTDASH_AUTH_MODE 配置无效: {err}")),
        Err(std::env::VarError::NotPresent) => Ok(AuthMode::Personal),
        Err(err) => Err(anyhow::anyhow!("读取 AGENTDASH_AUTH_MODE 失败: {err}")),
    }
}

struct PiAgentConnectorDeps {
    settings_repo: Arc<dyn SettingsRepository>,
    llm_provider_repo: Arc<dyn LlmProviderRepository>,
}

/// 构建结果：connector 本体 + 需要注入到 session runtime 的 provider
struct PiAgentConnectorBuildResult {
    connector: agentdash_executor::connectors::pi_agent::PiAgentConnector,
}

async fn build_pi_agent_connector(
    deps: PiAgentConnectorDeps,
) -> Option<PiAgentConnectorBuildResult> {
    let mut connector = agentdash_executor::connectors::pi_agent::build_pi_agent_connector(
        deps.settings_repo.as_ref(),
        deps.llm_provider_repo.as_ref(),
    )
    .await?;
    connector.set_settings_repository(deps.settings_repo);
    connector.set_llm_provider_repository(deps.llm_provider_repo);
    Some(PiAgentConnectorBuildResult { connector })
}
