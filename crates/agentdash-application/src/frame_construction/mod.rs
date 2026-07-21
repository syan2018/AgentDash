//! FrameConstructionService — 将 compose 路由 + 持久化统一为
//! 一次 `construct_launch_envelope` 调用，直接产出 `FrameLaunchEnvelope`。
//!
//! 各 composer 子模块负责具体路径的 bootstrap spec 组装，
//! 本模块负责路径分类 (classify) 和最终 frame 持久化。

mod activity_activation;
mod assembly;
mod classify;
mod composer_companion;
mod composer_project_agent;
mod composer_workflow_node;
mod launch_anchor_materialization;
mod owner_bootstrap;
pub mod plan;
mod request_assembler;
mod subject_assignment;
mod workflow_node_materialization;
mod workflow_projection;

use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository;
use agentdash_application_ports::frame_launch_envelope::{
    FrameLaunchEnvelopeRequest, RuntimeTraceLaunchStateRef,
};
use agentdash_application_ports::launch::{LaunchCommand, LaunchPromptInput};
use agentdash_application_ports::lifecycle_surface_projection::LifecycleSurfaceProjectionPort;
use agentdash_domain::workflow::AgentFrame;
use agentdash_platform_spi::{
    AgentConfig, MemoryDiscoveryProvider, PlatformRuntimeError, SkillDiscoveryProvider,
};

use crate::repository_set::RepositorySet;
use agentdash_application_vfs::VfsService;

use crate::agent_run::TerminalHookEffectBinding;
use crate::agent_run::frame::{
    AgentFrameBuilder, AgentFrameSurfaceExt, FrameLaunchContextProjection, FrameLaunchDiagnostics,
    FrameLaunchEnvelope, FrameLaunchEnvelopeConstructionInput, FrameLaunchFrameRef,
    FrameLaunchIntent, FrameLaunchRuntimeSurface, FrameLaunchSurface, FrameRuntimeSurface,
    FrameSurfaceDraft, LaunchResolutionTrace,
};
use crate::agent_run::merge_executor_config_fields;
use crate::agent_run::{PromptLaunchPath, RuntimeTraceLaunchState, SessionRepositoryRehydrateMode};
use crate::context::SharedContextAuditBus;
use crate::platform_config::PlatformConfig;
use crate::workspace::resolution::BackendAvailability;

// ─── FrameConstructionService ───

/// Session frame compose 的唯一入口。
///
/// 将"路径分类 → compose → 持久化 → FrameLaunchEnvelope"收束为一次调用。
pub struct FrameConstructionService {
    pub(crate) repos: RepositorySet,
    pub(crate) vfs_service: Arc<VfsService>,
    pub(crate) availability: Arc<dyn BackendAvailability>,
    pub(crate) platform_config: Arc<PlatformConfig>,
    pub(crate) audit_bus: SharedContextAuditBus,
    pub(crate) companion_facts: Arc<dyn CompanionParentFactsProvider>,
    pub(crate) lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
    pub(crate) extra_skill_dirs: Vec<PathBuf>,
    pub(crate) skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub(crate) memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    pub(crate) product_runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
}

pub struct FrameConstructionDeps {
    pub repos: RepositorySet,
    pub vfs_service: Arc<VfsService>,
    pub availability: Arc<dyn BackendAvailability>,
    pub platform_config: Arc<PlatformConfig>,
    pub audit_bus: SharedContextAuditBus,
    pub companion_facts: Arc<dyn CompanionParentFactsProvider>,
    pub lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    pub product_runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
}

pub(crate) use assembly::FrameAssemblyLaunchExtras;
pub use launch_anchor_materialization::{
    AgentRunProjectOwnerFrameConstructionAdapter, AgentRunProjectOwnerFrameConstructionDeps,
};
pub(crate) use owner_bootstrap::{
    OwnerBootstrapComposer, OwnerBootstrapSpec, OwnerPromptLaunchPath, OwnerScope,
};
pub(crate) use request_assembler::{
    CompanionParentFactsProvider, CompanionParentSpec, CompanionParentWorkflowSpec,
    FrameRequestAssembler, LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit,
};

impl FrameConstructionService {
    pub fn new(deps: FrameConstructionDeps) -> Self {
        Self {
            repos: deps.repos,
            vfs_service: deps.vfs_service,
            availability: deps.availability,
            platform_config: deps.platform_config,
            audit_bus: deps.audit_bus,
            companion_facts: deps.companion_facts,
            lifecycle_surface_projection: deps.lifecycle_surface_projection,
            extra_skill_dirs: deps.extra_skill_dirs,
            skill_discovery_providers: deps.skill_discovery_providers,
            memory_discovery_providers: deps.memory_discovery_providers,
            product_runtime_bindings: deps.product_runtime_bindings,
        }
    }

    /// 统一 frame construction 入口：分类 → compose → 持久化 → envelope。
    pub(crate) async fn construct_launch_envelope(
        &self,
        input: FrameLaunchEnvelopeConstructionInput,
    ) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
        let session_id = input.runtime_thread_id.clone();
        let (_binding, agent, frame) =
            agentdash_application_lifecycle::resolve_current_frame_from_delivery_trace_ref(
                &session_id,
                self.product_runtime_bindings.as_ref(),
                self.repos.lifecycle_agent_repo.as_ref(),
                self.repos.agent_frame_repo.as_ref(),
            )
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                PlatformRuntimeError::InvalidConfig(format!(
                    "RuntimeThread {session_id} 缺少 AgentRunRuntimeBinding 或当前 AgentFrame，拒绝 launch"
                ))
            })?;
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(agent.run_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                PlatformRuntimeError::InvalidConfig(format!(
                    "LifecycleAgent {} 指向的 LifecycleRun {} 不存在",
                    agent.id, agent.run_id
                ))
            })?;
        if agent.run_id != run.id || agent.project_id != run.project_id {
            return Err(PlatformRuntimeError::InvalidConfig(format!(
                "RuntimeThread {session_id} 的 anchor agent/run 不一致"
            )));
        }
        classify::route_and_compose(self, frame, agent, run, input).await
    }

    pub async fn construct_launch_envelope_from_request(
        &self,
        request: FrameLaunchEnvelopeRequest,
    ) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
        self.construct_launch_envelope(frame_launch_provider_input_from_request(request)?)
            .await
    }

    // ─── 内部 helpers ───

    pub(crate) fn assembler(&self) -> FrameRequestAssembler<'_> {
        FrameRequestAssembler::new(
            &self.repos,
            self.platform_config.as_ref(),
            self.lifecycle_surface_projection.as_ref(),
            self.product_runtime_bindings.as_ref(),
        )
        .with_audit_bus(self.audit_bus.clone())
        .with_companion_parent_facts_provider(self.companion_facts.as_ref())
    }

    pub(crate) fn prompt_launch_path(
        &self,
        _executor_config: Option<&AgentConfig>,
        input: &FrameLaunchEnvelopeConstructionInput,
    ) -> PromptLaunchPath {
        crate::agent_run::resolve_prompt_launch_path(
            &input.runtime_trace_state,
            input.had_existing_runtime,
            false,
            input.agent_needs_bootstrap,
        )
    }

    /// 构造 compose 后的 pending frame revision，并从该 frame 构造 FrameLaunchEnvelope。
    pub(crate) async fn compose_pending_frame(
        &self,
        builder: AgentFrameBuilder,
        extras: FrameAssemblyLaunchExtras,
        command: &LaunchCommand,
        runtime_thread_id: &str,
        hook_binding: Option<TerminalHookEffectBinding>,
    ) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
        let frame = builder
            .build_uncommitted(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(connector_internal)?;
        let mut envelope = build_envelope_from_frame(
            &frame,
            Some(extras),
            command,
            hook_binding,
            runtime_thread_id,
        )?;
        self.apply_launch_context_discovery(&mut envelope, command.identity().as_ref())
            .await;
        envelope.frame.pending_frame = Some(frame);
        Ok(envelope)
    }

    /// Launch-time runtime context discovery 单入口。
    ///
    /// 在 runtime surface 闭包 (`build_envelope_from_frame` → `close_frame_launch_surface`)
    /// 之后，从最终 `FrameLaunchSurface.vfs` 一次性派生 guidelines / memory / skill baseline，
    /// 写入 envelope `context` projection 并把 skill baseline 合入 launch capability state。
    ///
    /// 所有 envelope 构造路径（ProjectAgent owner、LifecycleNode、ExistingSurface、
    /// companion modifier）都经此单入口，route 层不再各自 derive discovery。
    pub(crate) async fn apply_launch_context_discovery(
        &self,
        envelope: &mut FrameLaunchEnvelope,
        identity: Option<&agentdash_platform_spi::AuthIdentity>,
    ) {
        use crate::agent_run::runtime_capability_projection::{
            LaunchContextDiscoveryInput, derive_launch_context_discovery,
        };

        let launch_vfs = envelope.runtime.launch_surface.vfs.clone();
        let discovery = derive_launch_context_discovery(LaunchContextDiscoveryInput {
            vfs_service: self.vfs_service.as_ref(),
            launch_vfs: &launch_vfs,
            identity,
            extra_skill_dirs: &self.extra_skill_dirs,
            skill_discovery_providers: &self.skill_discovery_providers,
            memory_discovery_providers: &self.memory_discovery_providers,
            diagnostics_label: "launch_context_discovery",
        })
        .await;

        // Skill baseline 合入 launch capability state 与 surface draft，
        // 保持 capability delta 阶段消费统一发现结果。
        envelope
            .runtime
            .launch_surface
            .capability_state
            .skill
            .skills = discovery.session_capabilities.skills.clone();
        if let Some(state) = envelope.runtime.surface_draft.capability_state.as_mut() {
            state.skill.skills = discovery.session_capabilities.skills.clone();
        }

        // memory inventory 同步到 launch capability state 的 memory 维度，
        // 保持 bounded-index 行为与既有契约一致。
        envelope
            .runtime
            .launch_surface
            .capability_state
            .memory
            .inventory = discovery.discovered_memory.clone();
        if let Some(state) = envelope.runtime.surface_draft.capability_state.as_mut() {
            state.memory.inventory = discovery.discovered_memory.clone();
        }

        envelope.context.discovered_guidelines = discovery.discovered_guidelines;
        envelope.context.discovered_memory = discovery.discovered_memory;
    }
}

fn frame_launch_provider_input_from_request(
    request: FrameLaunchEnvelopeRequest,
) -> Result<FrameLaunchEnvelopeConstructionInput, PlatformRuntimeError> {
    Ok(FrameLaunchEnvelopeConstructionInput {
        runtime_thread_id: request.runtime_thread_id,
        command: request.command,
        runtime_trace_state: runtime_trace_launch_state_from_ref(request.runtime_trace_state),
        had_existing_runtime: request.had_existing_runtime,
        agent_needs_bootstrap: request.agent_needs_bootstrap,
    })
}

fn runtime_trace_launch_state_from_ref(
    input: RuntimeTraceLaunchStateRef,
) -> RuntimeTraceLaunchState {
    RuntimeTraceLaunchState {
        executor_session_id: input.executor_session_id,
        last_event_seq: input.last_event_seq,
    }
}

// ─── Free-standing helpers ───

pub(crate) fn connector_internal(error: impl std::fmt::Display) -> PlatformRuntimeError {
    PlatformRuntimeError::Runtime(error.to_string())
}

/// 检查 frame surface 是否已就绪（executor_config + capability_state + working_directory 齐全）。
pub(crate) fn frame_surface_ready(frame: &AgentFrame) -> bool {
    frame.typed_execution_profile().is_some()
        && frame.typed_capability_state().is_some()
        && frame
            .typed_vfs()
            .and_then(|v| v.default_mount().map(|m| !m.root_ref.trim().is_empty()))
            .unwrap_or(false)
}

pub(crate) fn owner_prompt_launch_path(launch_path: PromptLaunchPath) -> OwnerPromptLaunchPath {
    match launch_path {
        PromptLaunchPath::OwnerBootstrap => OwnerPromptLaunchPath::OwnerBootstrap,
        PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::SystemContext) => {
            OwnerPromptLaunchPath::RepositoryRehydrate {
                prebuilt_continuation_bundle: None,
                include_owner_bundle: false,
            }
        }
        PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::ExecutorState) => {
            OwnerPromptLaunchPath::RepositoryRehydrate {
                prebuilt_continuation_bundle: None,
                include_owner_bundle: true,
            }
        }
        PromptLaunchPath::Plain => OwnerPromptLaunchPath::Plain,
    }
}

pub(crate) fn merge_user_executor_config(
    user_config: Option<AgentConfig>,
    preset_config: &AgentConfig,
) -> AgentConfig {
    match user_config {
        Some(user_ec) => merge_executor_config_fields(preset_config.clone(), &user_ec),
        None => preset_config.clone(),
    }
}

pub(crate) fn required_user_input(
    input: &LaunchPromptInput,
) -> Result<Vec<agentdash_agent_protocol::UserInputBlock>, PlatformRuntimeError> {
    input
        .input
        .clone()
        .ok_or_else(|| PlatformRuntimeError::InvalidConfig("必须提供 input".to_string()))
}

pub(crate) fn frame_builder_from_existing(
    frame: &AgentFrame,
    created_by_id: &str,
) -> Result<AgentFrameBuilder, PlatformRuntimeError> {
    let mut builder = AgentFrameBuilder::new(frame.agent_id)
        .with_created_by("runtime_thread_launch", Some(created_by_id.to_string()));
    if let Some(profile) = frame.execution_profile_json.clone() {
        builder = builder.with_execution_profile_raw(profile);
    }
    Ok(builder)
}

/// 从已持久化的 AgentFrame 直接构造 FrameLaunchEnvelope，合并 extras 和 command 覆盖。
///
/// 替代此前从 frame 构建 launch request、应用 command/extras、再转换 envelope 的三步链路。
pub(crate) fn build_envelope_from_frame(
    frame: &AgentFrame,
    extras: Option<FrameAssemblyLaunchExtras>,
    command: &LaunchCommand,
    hook_binding: Option<TerminalHookEffectBinding>,
    runtime_thread_id: &str,
) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
    let surface = FrameRuntimeSurface::from_frame(frame, Some(runtime_thread_id.to_string()));

    let mut surface_draft = FrameSurfaceDraft::from_frame(frame);
    let mut vfs = surface_draft.vfs.clone();
    let mut executor_config = surface_draft.execution_profile.clone();
    let mut capability_state = surface_draft.capability_state.clone();
    let mut mcp_servers = surface_draft.mcp_servers.clone();
    let mut context_bundle = None;

    if let Some(config) = command.prompt().executor_config.clone() {
        executor_config = Some(match executor_config {
            Some(base) => merge_executor_config_fields(base, &config),
            None => config,
        });
    }

    let mut input = command.prompt().input.clone();
    let mut environment_variables = command.prompt().environment_variables.clone();

    if let Some(extras) = extras {
        surface_draft = extras.frame_surface_draft;
        if extras.input.is_some() {
            input = extras.input;
        }
        if !extras.environment_variables.is_empty() {
            environment_variables = extras.environment_variables;
        }
        if let Some(config) = surface_draft
            .execution_profile
            .clone()
            .or(extras.executor_config)
        {
            executor_config = Some(config);
        }
        if let Some(bundle) = extras.context_bundle {
            context_bundle = Some(bundle);
        }
        if let Some(cs) = surface_draft.capability_state.clone() {
            capability_state = Some(cs);
        }
        if let Some(v) = surface_draft.vfs.clone() {
            vfs = Some(v);
        }
        if !surface_draft.mcp_servers.is_empty() {
            mcp_servers = surface_draft.mcp_servers.clone();
        }
    }

    let executor_config = executor_config.ok_or_else(|| {
        PlatformRuntimeError::InvalidConfig(
            "FrameLaunchEnvelope: executor_config 未在 frame construction 阶段解析".into(),
        )
    })?;
    let capability_state = capability_state.ok_or_else(|| {
        PlatformRuntimeError::InvalidConfig(
            "FrameLaunchEnvelope: capability_state 未在 frame construction 阶段解析".into(),
        )
    })?;
    surface_draft.capability_state = Some(capability_state.clone());
    surface_draft.vfs = vfs.clone();
    surface_draft.mcp_servers = mcp_servers.clone();
    surface_draft.execution_profile = Some(executor_config.clone());
    let closed_surface = close_frame_launch_surface(&mut surface_draft)?;
    let working_directory = closed_surface
        .launch_surface
        .vfs
        .default_mount()
        .map(|m| PathBuf::from(m.root_ref.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            PlatformRuntimeError::InvalidConfig(
                "FrameLaunchEnvelope: working_directory 未在 frame construction 阶段解析".into(),
            )
        })?;
    let runtime_backend_anchor = closed_surface
        .launch_surface
        .runtime_backend_anchor(
            closed_surface
                .resolution_trace
                .vfs_source
                .clone()
                .or_else(|| Some("frame_launch_surface.default_mount".to_string())),
        )
        .map_err(|error| PlatformRuntimeError::InvalidConfig(error.to_string()))?;

    Ok(FrameLaunchEnvelope {
        frame: FrameLaunchFrameRef {
            surface,
            pending_frame: None,
        },
        command: FrameLaunchIntent {
            input,
            environment_variables,
            identity: command.identity(),
            terminal_hook_effect_binding: hook_binding,
        },
        runtime: FrameLaunchRuntimeSurface {
            surface_draft,
            launch_surface: closed_surface.launch_surface,
            working_directory,
            runtime_backend_anchor,
            base_capability_state: closed_surface.base_capability_state,
        },
        context: FrameLaunchContextProjection {
            context_bundle,
            // discovered_guidelines / discovered_memory 由 launch-time 单入口
            // (`apply_launch_context_discovery`) 在 runtime surface 闭包后统一派生，
            // route / extras 不再手填。
            discovered_guidelines: Vec::new(),
            discovered_memory: agentdash_platform_spi::MemoryDiscoveryOutput::default(),
        },
        diagnostics: FrameLaunchDiagnostics {
            resolution_trace: closed_surface.resolution_trace,
        },
    })
}

pub(crate) struct ClosedFrameLaunchSurface {
    pub launch_surface: FrameLaunchSurface,
    pub base_capability_state: Option<agentdash_platform_spi::CapabilityState>,
    pub resolution_trace: LaunchResolutionTrace,
}

pub(crate) fn close_frame_launch_surface(
    surface_draft: &mut FrameSurfaceDraft,
) -> Result<ClosedFrameLaunchSurface, PlatformRuntimeError> {
    let launch_surface =
        FrameLaunchSurface::from_surface_draft(surface_draft).map_err(|error| {
            PlatformRuntimeError::InvalidConfig(format!("FrameLaunchEnvelope: {error}"))
        })?;

    Ok(ClosedFrameLaunchSurface {
        launch_surface,
        base_capability_state: None,
        resolution_trace: LaunchResolutionTrace::default(),
    })
}

#[cfg(test)]
mod existing_surface_discovery_tests {
    //! ExistingSurface launch regression: 当 owner facts 缺失、只能凭已持久化的
    //! AgentFrame surface 启动时，`build_envelope_from_frame` 必须把 frame 上的 VFS
    //! 无损带进 launch surface，从而让 launch-time 单入口 discovery 能从同一份 VFS
    //! 发现 AGENTS.md 并产出 `system_guidelines` 所需的 `DiscoveredGuideline`。
    //!
    //! 这是 Phase 4 新堵的缺口：此前 ExistingSurface route 跳过 owner bootstrap，
    //! 若 launch surface 丢掉 frame VFS，`system_guidelines` 就会静默缺失。

    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::agent_run::frame::AgentFrameSurfaceExt;
    use agentdash_application_agentrun::agent_run::runtime_capability_projection::{
        LaunchContextDiscoveryInput, derive_launch_context_discovery,
    };
    use agentdash_application_ports::launch::{LaunchCommand, LaunchPromptInput};
    use agentdash_application_vfs::{
        ListOptions, ListResult, MountError, MountOperationContext, MountProvider,
        MountProviderRegistry, PROVIDER_INLINE_FS, ReadResult, RuntimeFileEntry, SearchQuery,
        SearchResult, VfsService,
    };
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::AgentFrame;
    use agentdash_platform_spi::{AgentConfig, CapabilityState, ToolCluster, Vfs};
    use uuid::Uuid;

    use super::build_envelope_from_frame;

    struct StaticFileProvider {
        files: HashMap<String, String>,
    }

    #[async_trait::async_trait]
    impl MountProvider for StaticFileProvider {
        fn provider_id(&self) -> &str {
            PROVIDER_INLINE_FS
        }

        fn supported_capabilities(&self) -> Vec<&str> {
            vec!["read", "list"]
        }

        async fn read_text(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            self.files
                .get(path)
                .cloned()
                .map(|content| ReadResult::new(path, content))
                .ok_or_else(|| MountError::NotFound(path.to_string()))
        }

        async fn write_text(
            &self,
            _mount: &Mount,
            _path: &str,
            _content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            Err(MountError::NotSupported("static provider".to_string()))
        }

        async fn list(
            &self,
            _mount: &Mount,
            options: &ListOptions,
            _ctx: &MountOperationContext,
        ) -> Result<ListResult, MountError> {
            let prefix = if options.path == "." {
                String::new()
            } else {
                format!("{}/", options.path.trim_end_matches('/'))
            };
            let mut entries = std::collections::BTreeMap::new();
            for path in self.files.keys() {
                let Some(rest) = path.strip_prefix(&prefix) else {
                    continue;
                };
                let entry = if let Some((child, _)) = rest.split_once('/') {
                    RuntimeFileEntry::dir(format!("{prefix}{child}"))
                } else {
                    RuntimeFileEntry::file(path.clone())
                };
                entries.entry(entry.path.clone()).or_insert(entry);
            }
            Ok(ListResult {
                entries: entries.into_values().collect(),
            })
        }

        async fn search_text(
            &self,
            _mount: &Mount,
            _query: &SearchQuery,
            _ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            Err(MountError::NotSupported("static provider".to_string()))
        }
    }

    fn persisted_frame_with_agents_md() -> AgentFrame {
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: PROVIDER_INLINE_FS.to_string(),
                backend_id: "backend".to_string(),
                root_ref: "inline://workspace".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(vfs.clone());

        let mut frame = AgentFrame::new_revision(Uuid::new_v4(), 3, "existing_surface");
        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = Some(serde_json::to_value(&vfs).unwrap());
        frame.execution_profile_json =
            Some(serde_json::to_value(AgentConfig::new("PI_AGENT")).unwrap());
        frame
    }

    /// ExistingSurface 路径把 frame VFS 带进 launch surface，单入口 discovery
    /// 从这份 VFS 发现 AGENTS.md，产出 `system_guidelines` 所需 guideline。
    #[tokio::test]
    async fn existing_surface_launch_discovers_agents_md_from_persisted_frame_vfs() {
        let frame = persisted_frame_with_agents_md();
        let command = LaunchCommand::http_prompt_input(LaunchPromptInput::from_text("hello"), None);

        // ExistingSurface route 的第一步：从已持久化 frame 构造 envelope，不走 owner bootstrap。
        let envelope = build_envelope_from_frame(&frame, None, &command, None, "sess-existing")
            .expect("build envelope from persisted frame");

        // 回归保护：frame 上的 workspace mount 必须无损进入 launch surface VFS。
        let launch_vfs = &envelope.runtime.launch_surface.vfs;
        assert_eq!(launch_vfs.mounts.len(), 1);
        assert_eq!(launch_vfs.mounts[0].id, "workspace");

        // ExistingSurface route 的第二步（apply_launch_context_discovery 内核）：
        // 从最终 launch surface VFS 派生 runtime context discovery。
        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(StaticFileProvider {
            files: HashMap::from([("AGENTS.md".to_string(), "使用中文交流".to_string())]),
        }));
        let vfs_service = VfsService::new(Arc::new(registry));

        let discovery = derive_launch_context_discovery(LaunchContextDiscoveryInput {
            vfs_service: &vfs_service,
            launch_vfs,
            identity: None,
            extra_skill_dirs: &[],
            skill_discovery_providers: &[],
            memory_discovery_providers: &[],
            diagnostics_label: "existing_surface_test",
        })
        .await;

        assert_eq!(discovery.discovered_guidelines.len(), 1);
        assert_eq!(discovery.discovered_guidelines[0].file_name, "AGENTS.md");
        assert_eq!(discovery.discovered_guidelines[0].mount_id, "workspace");
        assert_eq!(discovery.discovered_guidelines[0].content, "使用中文交流");
    }

    #[tokio::test]
    async fn product_frame_materialization_persists_discovered_skills_and_guidelines() {
        let mut frame = persisted_frame_with_agents_md();
        let mut vfs = frame.typed_vfs().expect("VFS");
        vfs.mounts[0].metadata = serde_json::json!({
            "skill_asset_project_id": Uuid::new_v4().to_string(),
            "skill_asset_keys": ["review"],
        });
        let mut capability_state = frame.typed_capability_state().expect("capability state");
        capability_state.vfs.active = Some(vfs.clone());
        frame.vfs_surface_json = Some(serde_json::to_value(vfs).unwrap());
        frame.effective_capability_json = Some(serde_json::to_value(capability_state).unwrap());
        let mut surface = frame.surface_document();
        surface.context_source_snapshot = Some(
            serde_json::to_value(crate::agent_run::frame::AgentContextSourceSnapshot {
                bundle_id: Uuid::new_v4(),
                session_id: "product-frame".to_owned(),
                phase_tag: "project_agent".to_owned(),
                created_at_ms: 1,
                fragments: Vec::new(),
            })
            .unwrap(),
        );
        frame.surface = Some(surface);
        frame.apply_surface_projection();

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(StaticFileProvider {
            files: HashMap::from([
                ("AGENTS.md".to_owned(), "使用中文交流".to_owned()),
                (
                    "skills/review/SKILL.md".to_owned(),
                    "---\nname: review\ndescription: Review changes carefully.\n---\n# Review"
                        .to_owned(),
                ),
            ]),
        }));
        let vfs_service = VfsService::new(Arc::new(registry));

        super::launch_anchor_materialization::materialize_frame_context_discovery(
            &mut frame,
            &vfs_service,
            &[],
            &[],
            &[],
        )
        .await
        .expect("materialize discovery into immutable Product frame");

        let capability = frame.typed_capability_state().expect("capability state");
        assert!(
            capability
                .skill
                .skills
                .iter()
                .any(|skill| skill.name == "review" && skill.file_path.ends_with("SKILL.md"))
        );
        let context = frame.context_source_snapshot().expect("context source");
        assert!(context.fragments.iter().any(|fragment| {
            fragment.source == "vfs_guideline:workspace://AGENTS.md"
                && fragment.content.contains("使用中文交流")
        }));
    }

    #[tokio::test]
    async fn product_frame_rejects_a_declared_skill_missing_from_final_vfs_discovery() {
        let mut frame = persisted_frame_with_agents_md();
        let mut vfs = frame.typed_vfs().expect("VFS");
        vfs.mounts[0].metadata = serde_json::json!({
            "skill_asset_project_id": Uuid::new_v4().to_string(),
            "skill_asset_keys": ["canvas-system"],
        });
        let mut capability_state = frame.typed_capability_state().expect("capability state");
        capability_state.vfs.active = Some(vfs.clone());
        frame.vfs_surface_json = Some(serde_json::to_value(vfs).unwrap());
        frame.effective_capability_json = Some(serde_json::to_value(capability_state).unwrap());

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(StaticFileProvider {
            files: HashMap::new(),
        }));
        let vfs_service = VfsService::new(Arc::new(registry));

        let error = super::launch_anchor_materialization::materialize_frame_context_discovery(
            &mut frame,
            &vfs_service,
            &[],
            &[],
            &[],
        )
        .await
        .expect_err("declared SkillAsset must be discovered from the final VFS");
        assert!(error.to_string().contains("canvas-system"));
    }

    /// 若持久化 frame 缺少 VFS surface，ExistingSurface 无法闭包 launch surface，
    /// 应在构造阶段暴露而不是产出空 discovery（避免静默丢 guidelines）。
    #[test]
    fn existing_surface_without_vfs_rejects_launch_surface_close() {
        let mut frame = persisted_frame_with_agents_md();
        frame.vfs_surface_json = None;
        let capability_without_vfs = CapabilityState::from_clusters([ToolCluster::Read]);
        frame.effective_capability_json =
            Some(serde_json::to_value(&capability_without_vfs).unwrap());
        let command = LaunchCommand::http_prompt_input(LaunchPromptInput::from_text("hi"), None);

        let result = build_envelope_from_frame(&frame, None, &command, None, "sess-existing");

        assert!(result.is_err());
    }
}
