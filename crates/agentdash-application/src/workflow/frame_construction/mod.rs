//! FrameConstructionService — 将 compose 路由 + 持久化 + launch request 投影统一为
//! 一次 `construct_launch_envelope` 调用。
//!
//! 各 composer 子模块负责具体路径的 bootstrap spec 组装，
//! 本模块负责路径分类 (classify) 和最终 frame 持久化。

mod classify;
mod composer_companion;
mod composer_lifecycle_node;
mod composer_project_agent;
mod composer_story;
mod composer_task;

use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentFrame, AgentProcedureRef, LifecycleAgent, RuntimeSessionSelectionPolicy,
};
use agentdash_spi::{AgentConfig, AgentConnector, ConnectorError};

use crate::context::SharedContextAuditBus;
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::assembler::CompanionParentFactsProvider;
use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::types::{
    SessionPromptLifecycle, SessionRepositoryRehydrateMode, UserPromptInput,
};
use crate::session::{
    AssemblyLaunchExtras, LaunchCommand, OwnerPromptLifecycle,
    SessionRequestAssembler, TerminalHookEffectBinding,
};
use crate::vfs::VfsService;
use crate::workflow::frame_builder::AgentFrameBuilder;
use crate::workflow::runtime_launch::RuntimeLaunchRequest;
use crate::workspace::resolution::BackendAvailability;

pub use crate::workflow::runtime_launch::FrameLaunchEnvelope;

// ─── FrameConstructionService ───

/// Session frame compose 的唯一入口。
///
/// 替代此前散落在 API 层 `AppStateSessionConstructionProvider` 中的 5 个 compose 方法，
/// 将"路径分类 → compose → 持久化 → RuntimeLaunchRequest 投影"收束为一次调用。
pub struct FrameConstructionService {
    pub(crate) repos: RepositorySet,
    pub(crate) vfs_service: Arc<VfsService>,
    pub(crate) availability: Arc<dyn BackendAvailability>,
    pub(crate) platform_config: Arc<PlatformConfig>,
    pub(crate) audit_bus: SharedContextAuditBus,
    pub(crate) companion_facts: Arc<dyn CompanionParentFactsProvider>,
    pub(crate) connector: Arc<dyn AgentConnector>,
}

impl FrameConstructionService {
    pub fn new(
        repos: RepositorySet,
        vfs_service: Arc<VfsService>,
        availability: Arc<dyn BackendAvailability>,
        platform_config: Arc<PlatformConfig>,
        audit_bus: SharedContextAuditBus,
        companion_facts: Arc<dyn CompanionParentFactsProvider>,
        connector: Arc<dyn AgentConnector>,
    ) -> Self {
        Self {
            repos,
            vfs_service,
            availability,
            platform_config,
            audit_bus,
            companion_facts,
            connector,
        }
    }

    /// 统一 frame construction 入口：分类 → compose → 持久化 → envelope。
    pub async fn construct_launch_envelope(
        &self,
        input: SessionConstructionProviderInput,
    ) -> Result<FrameLaunchEnvelope, ConnectorError> {
        let request = self.build_launch_request(input).await?;
        FrameLaunchEnvelope::try_from_launch_request(request).map_err(|msg| {
            ConnectorError::InvalidConfig(format!("FrameLaunchEnvelope 构造失败: {msg}"))
        })
    }

    /// 内部路由：直接命中 → 分类 → compose。
    async fn build_launch_request(
        &self,
        input: SessionConstructionProviderInput,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let session_id = input.session_id.clone();
        let frame = self
            .repos
            .agent_frame_repo
            .find_by_runtime_session(&session_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "RuntimeSession {session_id} 没有关联 AgentFrame，拒绝 launch"
                ))
            })?;

        let direct_request = RuntimeLaunchRequest::from_frame(
            &frame,
            runtime_session_policy(input.session_id.as_str()),
        );
        let direct_lifecycle =
            self.prompt_lifecycle(direct_request.executor_config.as_ref(), &input);
        if matches!(direct_lifecycle, SessionPromptLifecycle::Plain)
            && launch_request_ready(&direct_request)
        {
            return Ok(apply_command_and_extras(
                direct_request,
                None,
                &input.command,
                None,
            ));
        }

        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(frame.agent_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "AgentFrame {} 指向的 LifecycleAgent {} 不存在",
                    frame.id, frame.agent_id
                ))
            })?;
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(agent.run_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "LifecycleAgent {} 指向的 LifecycleRun {} 不存在",
                    agent.id, agent.run_id
                ))
            })?;

        classify::route_and_compose(self, frame, agent, run, input).await
    }

    // ─── 内部 helpers ───

    pub(crate) fn assembler(&self) -> SessionRequestAssembler<'_> {
        SessionRequestAssembler::new(
            self.vfs_service.as_ref(),
            self.repos.canvas_repo.as_ref(),
            self.availability.as_ref(),
            &self.repos,
            self.platform_config.as_ref(),
        )
        .with_audit_bus(self.audit_bus.clone())
        .with_companion_parent_facts_provider(self.companion_facts.as_ref())
    }

    pub(crate) fn prompt_lifecycle(
        &self,
        executor_config: Option<&AgentConfig>,
        input: &SessionConstructionProviderInput,
    ) -> SessionPromptLifecycle {
        let supports_repository_restore = executor_config
            .map(|config| {
                self.connector
                    .supports_repository_restore(config.executor.as_str())
            })
            .unwrap_or(false);
        crate::session::types::resolve_session_prompt_lifecycle(
            &input.session_meta,
            input.had_existing_runtime,
            supports_repository_restore,
            input.agent_needs_bootstrap,
        )
    }

    pub(crate) async fn persist_composed_frame(
        &self,
        builder: AgentFrameBuilder,
        agent: &mut LifecycleAgent,
        extras: AssemblyLaunchExtras,
        command: &LaunchCommand,
        runtime_session_id: &str,
        hook_binding: Option<TerminalHookEffectBinding>,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let frame = builder
            .build(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(connector_internal)?;
        agent.set_current_frame(frame.id);
        self.repos
            .lifecycle_agent_repo
            .update(agent)
            .await
            .map_err(connector_internal)?;
        let request =
            RuntimeLaunchRequest::from_frame(&frame, runtime_session_policy(runtime_session_id));
        Ok(apply_command_and_extras(
            request,
            Some(extras),
            command,
            hook_binding,
        ))
    }
}

// ─── Free-standing helpers ───

pub(crate) fn connector_internal(error: impl std::fmt::Display) -> ConnectorError {
    ConnectorError::Runtime(error.to_string())
}

pub(crate) fn launch_request_ready(request: &RuntimeLaunchRequest) -> bool {
    request.executor_config.is_some()
        && request.working_directory.is_some()
        && request.typed_capability_state.is_some()
}

pub(crate) fn owner_prompt_lifecycle(lifecycle: SessionPromptLifecycle) -> OwnerPromptLifecycle {
    match lifecycle {
        SessionPromptLifecycle::OwnerBootstrap => OwnerPromptLifecycle::OwnerBootstrap,
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::SystemContext,
        ) => OwnerPromptLifecycle::RepositoryRehydrate {
            prebuilt_continuation_bundle: None,
            include_owner_bundle: false,
        },
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ) => OwnerPromptLifecycle::RepositoryRehydrate {
            prebuilt_continuation_bundle: None,
            include_owner_bundle: true,
        },
        SessionPromptLifecycle::Plain => OwnerPromptLifecycle::Plain,
    }
}

pub(crate) fn merge_user_executor_config(
    user_config: Option<AgentConfig>,
    preset_config: &AgentConfig,
) -> AgentConfig {
    match user_config {
        Some(mut user_ec) => {
            if user_ec.system_prompt.is_none() {
                user_ec.system_prompt = preset_config.system_prompt.clone();
            }
            if user_ec.system_prompt_mode.is_none() {
                user_ec.system_prompt_mode = preset_config.system_prompt_mode;
            }
            user_ec
        }
        None => preset_config.clone(),
    }
}

pub(crate) fn required_prompt_blocks(
    input: &UserPromptInput,
) -> Result<Vec<serde_json::Value>, ConnectorError> {
    input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ConnectorError::InvalidConfig("必须提供 promptBlocks".to_string()))
}

pub(crate) fn frame_builder_from_existing(
    frame: &AgentFrame,
    runtime_session_id: &str,
    created_by_id: &str,
) -> Result<AgentFrameBuilder, ConnectorError> {
    let runtime_session_id = frame
        .select_runtime_session_id(runtime_session_policy(runtime_session_id))
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "AgentFrame {} 缺少 runtime_session ref",
                frame.id
            ))
        })?;
    let mut builder = AgentFrameBuilder::new(frame.agent_id)
        .with_runtime_session(runtime_session_id)
        .with_created_by("session_launch", Some(created_by_id.to_string()));
    if let Some(procedure_id) = frame.procedure_id {
        builder = builder.with_procedure(AgentProcedureRef::ById(procedure_id));
    }
    if let (Some(graph_instance_id), Some(activity_key)) =
        (frame.graph_instance_id, frame.activity_key.clone())
    {
        builder = builder.with_graph_instance(graph_instance_id, activity_key);
    }
    if let Some(profile) = frame.execution_profile_json.clone() {
        builder = builder.with_execution_profile_raw(profile);
    }
    Ok(builder)
}

pub(crate) fn runtime_session_policy(runtime_session_id: &str) -> RuntimeSessionSelectionPolicy {
    RuntimeSessionSelectionPolicy::Specific {
        runtime_session_id: runtime_session_id.to_string(),
    }
}

pub(crate) fn apply_command_and_extras(
    mut request: RuntimeLaunchRequest,
    extras: Option<AssemblyLaunchExtras>,
    command: &LaunchCommand,
    hook_binding: Option<TerminalHookEffectBinding>,
) -> RuntimeLaunchRequest {
    let mut prompt_blocks = command.user_input().prompt_blocks.clone();
    let mut environment_variables = command.user_input().env.clone();
    if let Some(config) = command.user_input().executor_config.clone() {
        request.executor_config = Some(config);
    }
    if let Some(extras) = extras {
        if extras.prompt_blocks.is_some() {
            prompt_blocks = extras.prompt_blocks;
        }
        if !extras.environment_variables.is_empty() {
            environment_variables = extras.environment_variables;
        }
        if let Some(config) = extras.executor_config {
            request.executor_config = Some(config);
        }
        if let Some(bundle) = extras.context_bundle {
            request.context_bundle = Some(bundle);
        }
        if let Some(capability_state) = extras.capability_state {
            request.typed_capability_state = Some(capability_state);
        }
        if let Some(vfs) = extras.vfs {
            request.working_directory = vfs
                .default_mount()
                .map(|mount| std::path::PathBuf::from(mount.root_ref.trim()))
                .filter(|path| !path.as_os_str().is_empty())
                .or(request.working_directory);
            request.typed_vfs = Some(vfs);
        }
        if !extras.mcp_servers.is_empty() {
            request.typed_mcp_servers = extras.mcp_servers;
        }
    }
    request.prompt_blocks = prompt_blocks;
    request.environment_variables = environment_variables;
    request.identity = command.identity();
    if let Some(binding) = hook_binding {
        request.terminal_hook_effect_binding = Some(binding);
    }
    request
}
