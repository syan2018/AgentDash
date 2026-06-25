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
mod owner_bootstrap;
mod request_assembler;
mod subject_assignment;
mod workflow_projection;

use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application_ports::frame_launch_envelope::{
    FrameLaunchCommand, FrameLaunchEnvelopeRequest, FrameLaunchModifier, FrameLaunchSource,
    RuntimeTraceLaunchStateRef,
};
use agentdash_application_ports::lifecycle_surface_projection::LifecycleSurfaceProjectionPort;
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::{
    AgentConfig, AgentConnector, ConnectorError, MemoryDiscoveryProvider, SkillDiscoveryProvider,
};

use crate::agent_run::frame::builder::AgentFrameBuilder;
use crate::agent_run::frame::launch_envelope_provider::FrameLaunchEnvelopeConstructionInput;
use crate::agent_run::frame::runtime_launch::{
    FrameLaunchEnvelope, FrameLaunchIntent, FrameLaunchSurface, FrameRuntimeSurface,
    LaunchResolutionTrace,
};
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::frame::surface::FrameSurfaceDraft;
use crate::agent_run::merge_executor_config_fields;
use crate::agent_run::runtime_capability::replay_runtime_capability_transitions;
use crate::agent_run_repository_set::RepositorySet;
use crate::context::SharedContextAuditBus;
use crate::platform_config::PlatformConfig;
use crate::agent_run::runtime_session_boundary::RuntimeCommandRecord;
use crate::agent_run::runtime_session_boundary::{
    PromptLaunchPath, RuntimeTraceLaunchState, SessionRepositoryRehydrateMode, UserPromptInput,
};
use crate::agent_run::runtime_session_boundary::{LaunchCommand, TerminalHookEffectBinding};
use crate::workspace::resolution::BackendAvailability;
use agentdash_application_vfs::VfsService;

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
    pub(crate) connector: Arc<dyn AgentConnector>,
    pub(crate) extra_skill_dirs: Vec<PathBuf>,
    pub(crate) skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub(crate) memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
}

pub struct FrameConstructionDeps {
    pub repos: RepositorySet,
    pub vfs_service: Arc<VfsService>,
    pub availability: Arc<dyn BackendAvailability>,
    pub platform_config: Arc<PlatformConfig>,
    pub audit_bus: SharedContextAuditBus,
    pub companion_facts: Arc<dyn CompanionParentFactsProvider>,
    pub lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
    pub connector: Arc<dyn AgentConnector>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
}

pub(crate) use assembly::FrameAssemblyLaunchExtras;
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
            connector: deps.connector,
            extra_skill_dirs: deps.extra_skill_dirs,
            skill_discovery_providers: deps.skill_discovery_providers,
            memory_discovery_providers: deps.memory_discovery_providers,
        }
    }

    /// 统一 frame construction 入口：分类 → compose → 持久化 → envelope。
    pub(crate) async fn construct_launch_envelope(
        &self,
        input: FrameLaunchEnvelopeConstructionInput,
    ) -> Result<FrameLaunchEnvelope, ConnectorError> {
        let session_id = input.session_id.clone();
        let anchor = self
            .repos
            .execution_anchor_repo
            .find_by_session(&session_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "RuntimeSession {session_id} 缺少 RuntimeSessionExecutionAnchor，拒绝 launch"
                ))
            })?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(anchor.agent_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "RuntimeSessionExecutionAnchor 指向的 LifecycleAgent {} 不存在",
                    anchor.agent_id
                ))
            })?;
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(anchor.run_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "LifecycleAgent {} 指向的 LifecycleRun {} 不存在",
                    agent.id, agent.run_id
                ))
            })?;
        if agent.run_id != run.id || agent.project_id != run.project_id {
            return Err(ConnectorError::InvalidConfig(format!(
                "RuntimeSession {session_id} 的 anchor agent/run 不一致"
            )));
        }
        let frame = self
            .repos
            .agent_frame_repo
            .get_current(agent.id)
            .await
            .map_err(connector_internal)?
            .or(self
                .repos
                .agent_frame_repo
                .get(anchor.launch_frame_id)
                .await
                .map_err(connector_internal)?)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "LifecycleAgent {} 没有可用 AgentFrame，拒绝 launch",
                    agent.id
                ))
            })?;

        classify::route_and_compose(self, frame, agent, run, input).await
    }

    pub async fn construct_launch_envelope_from_request(
        &self,
        request: FrameLaunchEnvelopeRequest,
    ) -> Result<FrameLaunchEnvelope, ConnectorError> {
        self.construct_launch_envelope(frame_launch_provider_input_from_request(request)?)
            .await
    }

    // ─── 内部 helpers ───

    pub(crate) fn assembler(&self) -> FrameRequestAssembler<'_> {
        FrameRequestAssembler::new(
            self.vfs_service.as_ref(),
            &self.repos,
            self.platform_config.as_ref(),
            self.lifecycle_surface_projection.as_ref(),
        )
        .with_audit_bus(self.audit_bus.clone())
        .with_companion_parent_facts_provider(self.companion_facts.as_ref())
        .with_skill_discovery(&self.extra_skill_dirs, &self.skill_discovery_providers)
    }

    pub(crate) fn owner_bootstrap_composer(&self) -> OwnerBootstrapComposer<'_> {
        OwnerBootstrapComposer::new(
            self.vfs_service.as_ref(),
            self.repos.canvas_repo.as_ref(),
            self.availability.as_ref(),
            &self.repos,
            self.platform_config.as_ref(),
            self.lifecycle_surface_projection.as_ref(),
        )
        .with_audit_bus(self.audit_bus.clone())
        .with_skill_discovery(&self.extra_skill_dirs, &self.skill_discovery_providers)
        .with_memory_discovery(&self.memory_discovery_providers)
    }

    pub(crate) fn prompt_launch_path(
        &self,
        executor_config: Option<&AgentConfig>,
        input: &FrameLaunchEnvelopeConstructionInput,
    ) -> PromptLaunchPath {
        let supports_repository_restore = executor_config
            .map(|config| {
                self.connector
                    .supports_repository_restore(config.executor.as_str())
            })
            .unwrap_or(false);
        crate::agent_run::runtime_session_boundary::resolve_prompt_launch_path(
            &input.runtime_trace_state,
            input.had_existing_runtime,
            supports_repository_restore,
            input.agent_needs_bootstrap,
        )
    }

    /// 构造 compose 后的 pending frame revision，并从该 frame 构造 FrameLaunchEnvelope。
    pub(crate) async fn compose_pending_frame(
        &self,
        builder: AgentFrameBuilder,
        extras: FrameAssemblyLaunchExtras,
        command: &LaunchCommand,
        runtime_session_id: &str,
        hook_binding: Option<TerminalHookEffectBinding>,
        requested_runtime_commands: &[RuntimeCommandRecord],
    ) -> Result<FrameLaunchEnvelope, ConnectorError> {
        let frame = builder
            .build_uncommitted(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(connector_internal)?;
        let mut envelope = build_envelope_from_frame(
            &frame,
            Some(extras),
            command,
            hook_binding,
            runtime_session_id,
            requested_runtime_commands,
        )?;
        envelope.pending_frame = Some(frame);
        Ok(envelope)
    }
}

fn frame_launch_provider_input_from_request(
    request: FrameLaunchEnvelopeRequest,
) -> Result<FrameLaunchEnvelopeConstructionInput, ConnectorError> {
    Ok(FrameLaunchEnvelopeConstructionInput {
        session_id: request.runtime_session_id,
        command: launch_command_from_frame_launch(request.command)?,
        runtime_trace_state: runtime_trace_launch_state_from_ref(request.runtime_trace_state),
        had_existing_runtime: request.had_existing_runtime,
        requested_runtime_commands: request.requested_runtime_commands,
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

fn launch_command_from_frame_launch(
    command: FrameLaunchCommand,
) -> Result<LaunchCommand, ConnectorError> {
    let mut companion = None;
    let mut routine = None;
    let mut local_relay = None;
    for modifier in command.modifiers {
        match modifier {
            FrameLaunchModifier::Companion(source) => {
                companion = Some(*source);
            }
            FrameLaunchModifier::Routine(source) => {
                routine = Some(source);
            }
            FrameLaunchModifier::LocalRelay(payload) => {
                local_relay = Some(payload);
            }
            FrameLaunchModifier::HookAutoResume => {}
        }
    }
    let user_input = UserPromptInput {
        input: command.user_input.input,
        env: command.user_input.environment_variables,
        executor_config: command.user_input.executor_config,
        backend_selection: None,
    };
    let launch = match command.source {
        FrameLaunchSource::HttpPrompt => {
            LaunchCommand::http_prompt_input(user_input, command.identity)
        }
        FrameLaunchSource::LifecycleAgentUserMessage => {
            LaunchCommand::lifecycle_agent_user_message_input(user_input, command.identity)
        }
        FrameLaunchSource::HookAutoResume => LaunchCommand::hook_auto_resume_input(user_input),
        FrameLaunchSource::CompanionDispatch => LaunchCommand::companion_dispatch_input(
            user_input,
            command.identity,
            companion.ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "companion dispatch launch request 缺少 companion source".to_string(),
                )
            })?,
        ),
        FrameLaunchSource::CompanionParentResume => {
            LaunchCommand::companion_parent_resume_input(user_input)
        }
        FrameLaunchSource::WorkflowOrchestrator => {
            LaunchCommand::workflow_orchestrator_input(user_input)
        }
        FrameLaunchSource::RoutineExecutor => LaunchCommand::routine_executor_input(
            user_input,
            command.identity,
            routine.ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "routine executor launch request 缺少 routine source".to_string(),
                )
            })?,
        ),
        FrameLaunchSource::LocalRelayPrompt => {
            let payload = local_relay.ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "local relay launch request 缺少 local relay payload".to_string(),
                )
            })?;
            LaunchCommand::local_relay_prompt_input(
                user_input,
                payload.mcp_servers,
                payload.workspace_root,
            )
        }
    };
    Ok(launch.with_follow_up(command.follow_up_session_id))
}

// ─── Free-standing helpers ───

pub(crate) fn connector_internal(error: impl std::fmt::Display) -> ConnectorError {
    ConnectorError::Runtime(error.to_string())
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
    input: &UserPromptInput,
) -> Result<Vec<agentdash_agent_protocol::UserInputBlock>, ConnectorError> {
    input
        .input
        .clone()
        .ok_or_else(|| ConnectorError::InvalidConfig("必须提供 input".to_string()))
}

pub(crate) fn frame_builder_from_existing(
    frame: &AgentFrame,
    runtime_session_id: &str,
    created_by_id: &str,
) -> Result<AgentFrameBuilder, ConnectorError> {
    let mut builder = AgentFrameBuilder::new(frame.agent_id)
        .with_runtime_session(runtime_session_id.to_string())
        .with_created_by("session_launch", Some(created_by_id.to_string()));
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
    runtime_session_id: &str,
    requested_runtime_commands: &[RuntimeCommandRecord],
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let surface = FrameRuntimeSurface::from_frame(frame, Some(runtime_session_id.to_string()));

    let mut surface_draft = FrameSurfaceDraft::from_frame(frame);
    let mut vfs = surface_draft.vfs.clone();
    let mut executor_config = surface_draft.execution_profile.clone();
    let mut capability_state = surface_draft.capability_state.clone();
    let mut mcp_servers = surface_draft.mcp_servers.clone();
    let mut context_bundle = None;
    let mut memory_inventory = agentdash_spi::MemoryDiscoveryOutput::default();

    if let Some(config) = command.user_input().executor_config.clone() {
        executor_config = Some(match executor_config {
            Some(base) => merge_executor_config_fields(base, &config),
            None => config,
        });
    }

    let mut input = command.user_input().input.clone();
    let mut environment_variables = command.user_input().env.clone();

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
        memory_inventory = extras.memory_inventory;
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
        ConnectorError::InvalidConfig(
            "FrameLaunchEnvelope: executor_config 未在 frame construction 阶段解析".into(),
        )
    })?;
    let capability_state = capability_state.ok_or_else(|| {
        ConnectorError::InvalidConfig(
            "FrameLaunchEnvelope: capability_state 未在 frame construction 阶段解析".into(),
        )
    })?;
    surface_draft.capability_state = Some(capability_state.clone());
    surface_draft.vfs = vfs.clone();
    surface_draft.mcp_servers = mcp_servers.clone();
    surface_draft.execution_profile = Some(executor_config.clone());
    let closed_surface =
        close_frame_launch_surface(&mut surface_draft, requested_runtime_commands)?;
    let working_directory = closed_surface
        .launch_surface
        .vfs
        .default_mount()
        .map(|m| PathBuf::from(m.root_ref.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(
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
        .map_err(|error| ConnectorError::InvalidConfig(error.to_string()))?;

    Ok(FrameLaunchEnvelope {
        surface,
        surface_draft,
        launch_surface: closed_surface.launch_surface,
        pending_frame: None,
        intent: FrameLaunchIntent {
            input,
            environment_variables,
            identity: command.identity(),
            terminal_hook_effect_binding: hook_binding,
            discovered_guidelines: Vec::new(),
            discovered_memory: memory_inventory,
        },
        working_directory,
        context_bundle,
        continuation_context_frame: None,
        base_capability_state: closed_surface.base_capability_state,
        runtime_backend_anchor,
        resolution_trace: closed_surface.resolution_trace,
    })
}

pub(crate) struct ClosedFrameLaunchSurface {
    pub launch_surface: FrameLaunchSurface,
    pub base_capability_state: Option<agentdash_spi::CapabilityState>,
    pub resolution_trace: LaunchResolutionTrace,
}

pub(crate) fn close_frame_launch_surface(
    surface_draft: &mut FrameSurfaceDraft,
    requested_runtime_commands: &[RuntimeCommandRecord],
) -> Result<ClosedFrameLaunchSurface, ConnectorError> {
    let base_launch_surface = FrameLaunchSurface::from_surface_draft(surface_draft)
        .map_err(|error| ConnectorError::InvalidConfig(format!("FrameLaunchEnvelope: {error}")))?;

    if requested_runtime_commands.is_empty() {
        return Ok(ClosedFrameLaunchSurface {
            launch_surface: base_launch_surface,
            base_capability_state: None,
            resolution_trace: LaunchResolutionTrace::default(),
        });
    }

    let base_capability_state = base_launch_surface.capability_state.clone();
    let requested_transitions = requested_runtime_commands
        .iter()
        .map(|command| command.pending_capability_state_transition())
        .collect::<Vec<_>>();
    let replay =
        replay_runtime_capability_transitions(&base_capability_state, &requested_transitions)
            .map_err(|error| {
                ConnectorError::InvalidConfig(format!(
                    "FrameLaunchEnvelope: pending runtime command closure 失败: {error}"
                ))
            })?;

    let mut final_capability_state = replay.capability_state;
    let effective_vfs = replay
        .effective_vfs
        .unwrap_or_else(|| base_launch_surface.vfs.clone());
    final_capability_state.vfs.active = Some(effective_vfs.clone());
    let effective_mcp_servers = replay
        .effective_mcp_servers
        .unwrap_or_else(|| final_capability_state.tool.mcp_servers.clone());
    final_capability_state.tool.mcp_servers = effective_mcp_servers.clone();
    let execution_profile = base_launch_surface.execution_profile;
    let launch_surface = FrameLaunchSurface::new(
        final_capability_state,
        effective_vfs,
        effective_mcp_servers,
        execution_profile,
    )
    .map_err(|error| ConnectorError::InvalidConfig(format!("FrameLaunchEnvelope: {error}")))?;
    launch_surface.write_back_to_surface_draft(surface_draft);

    Ok(ClosedFrameLaunchSurface {
        launch_surface,
        base_capability_state: Some(base_capability_state),
        resolution_trace: LaunchResolutionTrace {
            vfs_source: Some("pending_runtime_command".to_string()),
            mcp_source: Some("pending_runtime_command".to_string()),
            capability_source: Some("pending_runtime_command".to_string()),
            pending_overlay_applied: true,
        },
    })
}
