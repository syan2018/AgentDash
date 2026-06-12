use uuid::Uuid;

use agentdash_contracts::{
    vfs::ResolvedVfsSurface,
    workflow::{
        AgentConversationIdentity, AgentConversationLifecycleContext, AgentConversationSnapshot,
        AgentFrameRefDto, AgentRunRefDto, ConversationCommandKind, ConversationCommandPlacement,
        ConversationCommandSetView, ConversationCommandStaleGuardView, ConversationCommandView,
        ConversationDiagnosticView, ConversationEffectiveExecutorConfigView,
        ConversationExecutionStatus, ConversationExecutionView, ConversationKeyboardMapView,
        ConversationModelConfigSource, ConversationModelConfigStatus, ConversationModelConfigView,
        ConversationPendingSnapshotView, LifecycleRunRefDto, LifecycleSubjectAssociationDto,
        RuntimeSessionRefDto, ValidationSeverity,
    },
};
use agentdash_domain::agent::ProjectAgent;
use agentdash_spi::{AgentConfig, ThinkingLevel};

use crate::session::SessionExecutionState;
use crate::workflow::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct ConversationModelConfigResolution {
    pub config: AgentConfig,
    pub view: ConversationModelConfigView,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationModelConfigInput<'a> {
    pub project_agent_preset: Option<&'a AgentConfig>,
    pub frame_execution_profile: Option<&'a AgentConfig>,
    pub user_override: Option<&'a AgentConfig>,
    pub executor_discovery_default: Option<&'a AgentConfig>,
}

pub struct ConversationModelConfigResolver;

impl ConversationModelConfigResolver {
    pub fn resolve(input: ConversationModelConfigInput<'_>) -> ConversationModelConfigResolution {
        let mut source = ConversationModelConfigSource::Unspecified;
        let mut config = input
            .project_agent_preset
            .cloned()
            .map(|config| {
                source = ConversationModelConfigSource::ProjectAgentPreset;
                config
            })
            .unwrap_or_default();

        if let Some(frame_config) = input.frame_execution_profile {
            config = merge_executor_config_fields(config, frame_config);
            source = ConversationModelConfigSource::FrameExecutionProfile;
        }
        if let Some(user_config) = input.user_override {
            config = merge_executor_config_fields(config, user_config);
            source = ConversationModelConfigSource::UserOverride;
        }
        if let Some(discovery_config) = input.executor_discovery_default {
            let before = config.clone();
            config = fill_executor_config_missing_fields(config, discovery_config);
            if before.model_id != config.model_id || before.provider_id != config.provider_id {
                source = ConversationModelConfigSource::ExecutorDiscoveryDefault;
            }
        }

        let missing_fields = missing_required_model_fields(&config);
        let status = if missing_fields.is_empty() {
            ConversationModelConfigStatus::Resolved
        } else {
            ConversationModelConfigStatus::ModelRequired
        };
        let message = if status == ConversationModelConfigStatus::ModelRequired {
            Some(model_required_message(&config, &missing_fields))
        } else {
            None
        };
        let effective_executor_config = Some(effective_executor_config_view(&config, source));

        ConversationModelConfigResolution {
            config,
            view: ConversationModelConfigView {
                status,
                effective_executor_config,
                missing_fields,
                message,
            },
        }
    }

    pub fn resolve_project_agent_start(
        project_agent: &ProjectAgent,
        user_override: Option<&AgentConfig>,
    ) -> Result<ConversationModelConfigResolution, WorkflowApplicationError> {
        let preset = project_agent.preset_config()?;
        let preset_config = preset.to_agent_config(&project_agent.agent_type);
        let resolution = Self::resolve(ConversationModelConfigInput {
            project_agent_preset: Some(&preset_config),
            user_override,
            ..Default::default()
        });
        if resolution.view.status == ConversationModelConfigStatus::ModelRequired {
            return Err(WorkflowApplicationError::ModelRequired(
                resolution
                    .view
                    .message
                    .clone()
                    .unwrap_or_else(|| "当前 ProjectAgent 缺少模型选择。".to_string()),
            ));
        }
        Ok(resolution)
    }

    pub fn view_for_config(
        config: &AgentConfig,
        source: ConversationModelConfigSource,
    ) -> ConversationEffectiveExecutorConfigView {
        effective_executor_config_view(config, source)
    }
}

pub fn merge_executor_config_fields(
    mut base: AgentConfig,
    override_config: &AgentConfig,
) -> AgentConfig {
    base.executor = override_config.executor.clone();
    if override_config.provider_id.is_some() {
        base.provider_id = normalize_option_string(override_config.provider_id.clone());
    }
    if override_config.model_id.is_some() {
        base.model_id = normalize_option_string(override_config.model_id.clone());
    }
    if override_config.agent_id.is_some() {
        base.agent_id = normalize_option_string(override_config.agent_id.clone());
    }
    if override_config.thinking_level.is_some() {
        base.thinking_level = override_config.thinking_level;
    }
    if override_config.permission_policy.is_some() {
        base.permission_policy = normalize_option_string(override_config.permission_policy.clone());
    }
    if override_config.system_prompt.is_some() {
        base.system_prompt = normalize_option_string(override_config.system_prompt.clone());
    }
    if override_config.system_prompt_mode.is_some() {
        base.system_prompt_mode = override_config.system_prompt_mode;
    }
    base
}

fn fill_executor_config_missing_fields(
    mut base: AgentConfig,
    default_config: &AgentConfig,
) -> AgentConfig {
    if base
        .provider_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        base.provider_id = normalize_option_string(default_config.provider_id.clone());
    }
    if base
        .model_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        base.model_id = normalize_option_string(default_config.model_id.clone());
    }
    if base
        .agent_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        base.agent_id = normalize_option_string(default_config.agent_id.clone());
    }
    base
}

fn missing_required_model_fields(config: &AgentConfig) -> Vec<String> {
    if !config.is_cloud_native() {
        return Vec::new();
    }
    let mut missing = Vec::new();
    if config
        .provider_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        missing.push("provider_id".to_string());
    }
    if config
        .model_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        missing.push("model_id".to_string());
    }
    missing
}

fn model_required_message(config: &AgentConfig, missing_fields: &[String]) -> String {
    let fields = missing_fields.join(", ");
    format!(
        "执行器 {} 缺少必需模型配置: {fields}。请先选择 provider 和 model。",
        config.executor
    )
}

fn normalize_option_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn effective_executor_config_view(
    config: &AgentConfig,
    source: ConversationModelConfigSource,
) -> ConversationEffectiveExecutorConfigView {
    ConversationEffectiveExecutorConfigView {
        executor: config.executor.clone(),
        provider_id: normalize_option_string(config.provider_id.clone()),
        model_id: normalize_option_string(config.model_id.clone()),
        agent_id: normalize_option_string(config.agent_id.clone()),
        thinking_level: config.thinking_level.map(thinking_level_string),
        permission_policy: normalize_option_string(config.permission_policy.clone()),
        source,
    }
}

fn thinking_level_string(level: ThinkingLevel) -> String {
    match level {
        ThinkingLevel::Off => "off",
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
        ThinkingLevel::Xhigh => "xhigh",
    }
    .to_string()
}

pub struct AgentConversationSnapshotInput {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_ref: Option<(Uuid, i32)>,
    pub delivery_runtime_session_id: Option<String>,
    pub subject_associations: Vec<LifecycleSubjectAssociationDto>,
    pub execution_state: SessionExecutionState,
    pub terminal_agent: bool,
    pub supports_steering: bool,
    pub pending_paused: bool,
    pub pending_visible_message_count: usize,
    pub resource_surface: Option<ResolvedVfsSurface>,
    pub resource_diagnostics: Vec<ConversationDiagnosticView>,
    pub model_config: ConversationModelConfigView,
}

pub struct AgentConversationSnapshotResolver;

impl AgentConversationSnapshotResolver {
    pub fn resolve(input: AgentConversationSnapshotInput) -> AgentConversationSnapshot {
        let active_turn_id = active_turn_id(&input.execution_state);
        let execution = conversation_execution_view(&input, active_turn_id.clone());
        let commands = conversation_commands(&input, execution.status, active_turn_id.as_deref());
        let resume_command = commands
            .commands
            .iter()
            .find(|command| command.kind == ConversationCommandKind::ResumePendingQueue)
            .cloned()
            .filter(|command| command.enabled);
        let diagnostics = conversation_diagnostics(&input.model_config, input.resource_diagnostics);

        AgentConversationSnapshot {
            identity: AgentConversationIdentity {
                run_ref: LifecycleRunRefDto {
                    run_id: input.run_id.to_string(),
                },
                agent_ref: AgentRunRefDto {
                    run_id: input.run_id.to_string(),
                    agent_id: input.agent_id.to_string(),
                },
                project_id: input.project_id.to_string(),
            },
            lifecycle_context: AgentConversationLifecycleContext {
                frame_ref: input
                    .frame_ref
                    .map(|(frame_id, revision)| AgentFrameRefDto {
                        agent_id: input.agent_id.to_string(),
                        frame_id: frame_id.to_string(),
                        revision: Some(revision),
                    }),
                delivery_runtime_ref: input
                    .delivery_runtime_session_id
                    .clone()
                    .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
                subject_associations: input.subject_associations,
            },
            execution,
            model_config: input.model_config,
            commands,
            pending: ConversationPendingSnapshotView {
                visible_message_count: input.pending_visible_message_count,
                paused: input.pending_paused,
                user_attention: input.pending_visible_message_count > 0 && input.pending_paused,
                resume_command,
            },
            resource_surface: input.resource_surface,
            diagnostics,
        }
    }
}

fn conversation_execution_view(
    input: &AgentConversationSnapshotInput,
    active_turn_id: Option<String>,
) -> ConversationExecutionView {
    let (status, reason) = if input.terminal_agent {
        (
            ConversationExecutionStatus::Terminal,
            Some("当前 AgentRun 已结束。".to_string()),
        )
    } else if input.delivery_runtime_session_id.is_none() {
        (
            ConversationExecutionStatus::DeliveryMissing,
            Some("当前 AgentRun 缺少可投递的 runtime 通道。".to_string()),
        )
    } else if input.frame_ref.is_none() {
        (
            ConversationExecutionStatus::FrameMissing,
            Some("当前 AgentRun 没有可投递的 runtime frame。".to_string()),
        )
    } else if input.model_config.status == ConversationModelConfigStatus::ModelRequired {
        (
            ConversationExecutionStatus::ModelRequired,
            input.model_config.message.clone(),
        )
    } else {
        match input.execution_state {
            SessionExecutionState::Running { turn_id: None } => (
                ConversationExecutionStatus::StartingClaimed,
                Some("当前 AgentRun 正在启动中，等待 active turn 建立。".to_string()),
            ),
            SessionExecutionState::Running { turn_id: Some(_) } => (
                ConversationExecutionStatus::RunningActive,
                Some("当前 AgentRun 正在执行中。".to_string()),
            ),
            SessionExecutionState::Cancelling { .. } => (
                ConversationExecutionStatus::Cancelling,
                Some("当前 AgentRun 正在取消中，等待执行器收口。".to_string()),
            ),
            _ => (ConversationExecutionStatus::Ready, None),
        }
    };
    ConversationExecutionView {
        status,
        runtime_session_ref: input
            .delivery_runtime_session_id
            .clone()
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        active_turn_id,
        reason,
    }
}

fn conversation_commands(
    input: &AgentConversationSnapshotInput,
    status: ConversationExecutionStatus,
    active_turn_id: Option<&str>,
) -> ConversationCommandSetView {
    let model_ready = input.model_config.status == ConversationModelConfigStatus::Resolved;
    let send_next = status == ConversationExecutionStatus::Ready && model_ready;
    let running_active =
        status == ConversationExecutionStatus::RunningActive && active_turn_id.is_some();
    let steer = running_active && input.supports_steering;
    let cancel = matches!(
        status,
        ConversationExecutionStatus::StartingClaimed
            | ConversationExecutionStatus::RunningActive
            | ConversationExecutionStatus::Cancelling
    );

    let commands = vec![
        command_view(
            input,
            ConversationCommandKind::StartDraft,
            status == ConversationExecutionStatus::Draft && model_ready,
            "当前 workspace 不是 draft start 状态。",
            Some("command_unavailable"),
            Some("enter"),
            true,
            "required",
            vec![ConversationCommandPlacement::ComposerPrimary],
        ),
        command_view(
            input,
            ConversationCommandKind::SendNext,
            send_next,
            unavailable_reason_for_ready(status, model_ready),
            Some(disabled_code_for_status(status)),
            Some("enter"),
            true,
            "allowed",
            vec![ConversationCommandPlacement::ComposerPrimary],
        ),
        command_view(
            input,
            ConversationCommandKind::Enqueue,
            running_active,
            "当前 AgentRun 不在可排队新消息的运行状态。",
            Some(if status == ConversationExecutionStatus::StartingClaimed {
                "starting_claimed"
            } else {
                "command_unavailable"
            }),
            Some("enter"),
            true,
            "allowed",
            vec![ConversationCommandPlacement::ComposerPrimary],
        ),
        command_view(
            input,
            ConversationCommandKind::Steer,
            steer,
            "当前 AgentRun 没有可用 steer intent。",
            Some(if running_active {
                "connector_steer_unsupported"
            } else if status == ConversationExecutionStatus::StartingClaimed {
                "starting_claimed"
            } else {
                "command_unavailable"
            }),
            Some("mod+enter"),
            true,
            "ignored",
            vec![ConversationCommandPlacement::ComposerSecondary],
        ),
        command_view(
            input,
            ConversationCommandKind::PromotePending,
            running_active && input.supports_steering,
            "当前 AgentRun 不在可投递 pending 消息的运行状态。",
            Some(if status == ConversationExecutionStatus::StartingClaimed {
                "starting_claimed"
            } else if running_active {
                "connector_steer_unsupported"
            } else {
                "command_unavailable"
            }),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacement::PendingRow],
        ),
        command_view(
            input,
            ConversationCommandKind::ResumePendingQueue,
            input.pending_paused && input.pending_visible_message_count > 0,
            "当前没有需要用户恢复的 pending 队列。",
            Some("command_unavailable"),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacement::PendingBanner],
        ),
        command_view(
            input,
            ConversationCommandKind::Cancel,
            cancel,
            "当前 AgentRun 没有正在执行的 turn。",
            Some("command_unavailable"),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacement::Header],
        ),
    ];

    ConversationCommandSetView {
        keyboard: ConversationKeyboardMapView {
            enter: if send_next {
                Some(command_id_for(ConversationCommandKind::SendNext))
            } else if running_active {
                Some(command_id_for(ConversationCommandKind::Enqueue))
            } else {
                None
            },
            ctrl_enter: if steer {
                Some(command_id_for(ConversationCommandKind::Steer))
            } else if send_next {
                Some(command_id_for(ConversationCommandKind::SendNext))
            } else if running_active {
                Some(command_id_for(ConversationCommandKind::Enqueue))
            } else {
                None
            },
        },
        primary: if send_next {
            Some(ConversationCommandKind::SendNext)
        } else if running_active {
            Some(ConversationCommandKind::Enqueue)
        } else {
            None
        },
        secondary: if steer {
            Some(ConversationCommandKind::Steer)
        } else {
            None
        },
        commands,
    }
}

fn command_view(
    input: &AgentConversationSnapshotInput,
    kind: ConversationCommandKind,
    enabled: bool,
    unavailable_reason: impl Into<String>,
    disabled_code: Option<&str>,
    shortcut: Option<&str>,
    requires_input: bool,
    executor_config_policy: impl Into<String>,
    placement: Vec<ConversationCommandPlacement>,
) -> ConversationCommandView {
    ConversationCommandView {
        kind,
        command_id: command_id_for(kind),
        enabled,
        unavailable_reason: if enabled {
            None
        } else {
            Some(unavailable_reason.into())
        },
        disabled_code: if enabled {
            None
        } else {
            disabled_code.map(str::to_string)
        },
        shortcut: shortcut.map(str::to_string),
        requires_input,
        executor_config_policy: executor_config_policy.into(),
        placement,
        stale_guard: ConversationCommandStaleGuardView {
            run_id: input.run_id.to_string(),
            agent_id: input.agent_id.to_string(),
            frame_id: input.frame_ref.map(|(frame_id, _)| frame_id.to_string()),
            runtime_session_id: input.delivery_runtime_session_id.clone(),
            active_turn_id: active_turn_id(&input.execution_state),
        },
    }
}

fn command_id_for(kind: ConversationCommandKind) -> String {
    match kind {
        ConversationCommandKind::StartDraft => "start_draft",
        ConversationCommandKind::SendNext => "send_next",
        ConversationCommandKind::Enqueue => "enqueue",
        ConversationCommandKind::Steer => "steer",
        ConversationCommandKind::PromotePending => "promote_pending",
        ConversationCommandKind::ResumePendingQueue => "resume_pending_queue",
        ConversationCommandKind::Cancel => "cancel",
    }
    .to_string()
}

fn disabled_code_for_status(status: ConversationExecutionStatus) -> &'static str {
    match status {
        ConversationExecutionStatus::Draft => "draft",
        ConversationExecutionStatus::ModelRequired => "model_required",
        ConversationExecutionStatus::Ready => "command_unavailable",
        ConversationExecutionStatus::StartingClaimed => "starting_claimed",
        ConversationExecutionStatus::RunningActive => "running_active",
        ConversationExecutionStatus::Cancelling => "cancelling",
        ConversationExecutionStatus::Terminal => "terminal",
        ConversationExecutionStatus::FrameMissing => "missing_frame",
        ConversationExecutionStatus::DeliveryMissing => "missing_delivery_runtime",
    }
}

fn unavailable_reason_for_ready(
    status: ConversationExecutionStatus,
    model_ready: bool,
) -> &'static str {
    if !model_ready {
        return "当前 AgentRun 缺少模型选择。";
    }
    match status {
        ConversationExecutionStatus::StartingClaimed => {
            "当前 AgentRun 正在启动中，等待 active turn 建立。"
        }
        ConversationExecutionStatus::RunningActive => {
            "当前 AgentRun 正在执行中，不能并发发送下一轮消息。"
        }
        ConversationExecutionStatus::Cancelling => {
            "当前 AgentRun 正在取消中，等待执行器收口后再发送下一轮消息。"
        }
        ConversationExecutionStatus::Terminal => "当前 AgentRun 已结束，不能继续发送消息。",
        ConversationExecutionStatus::FrameMissing => "当前 AgentRun 没有可投递的 runtime frame。",
        ConversationExecutionStatus::DeliveryMissing => "当前 AgentRun 缺少可投递的 runtime 通道。",
        ConversationExecutionStatus::ModelRequired => "当前 AgentRun 缺少模型选择。",
        ConversationExecutionStatus::Draft | ConversationExecutionStatus::Ready => {
            "当前 AgentRun 暂不可发送下一轮消息。"
        }
    }
}

fn active_turn_id(execution_state: &SessionExecutionState) -> Option<String> {
    match execution_state {
        SessionExecutionState::Running { turn_id }
        | SessionExecutionState::Cancelling { turn_id }
        | SessionExecutionState::Interrupted { turn_id, .. } => turn_id.clone(),
        SessionExecutionState::Completed { turn_id }
        | SessionExecutionState::Failed { turn_id, .. } => Some(turn_id.clone()),
        SessionExecutionState::Idle => None,
    }
}

fn conversation_diagnostics(
    model_config: &ConversationModelConfigView,
    mut resource_diagnostics: Vec<ConversationDiagnosticView>,
) -> Vec<ConversationDiagnosticView> {
    if model_config.status == ConversationModelConfigStatus::ModelRequired {
        resource_diagnostics.push(ConversationDiagnosticView {
            code: "model_required".to_string(),
            severity: ValidationSeverity::Error,
            message: model_config
                .message
                .clone()
                .unwrap_or_else(|| "当前 AgentRun 缺少模型选择。".to_string()),
            detail: Some(serde_json::json!({
                "missing_fields": model_config.missing_fields,
            })),
        });
    }
    resource_diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_contracts::vfs::{
        ResolvedMountEditCapabilities, ResolvedMountPurpose, ResolvedMountSummary,
        ResolvedVfsSurfaceSource,
    };

    fn resolved_model_config() -> ConversationModelConfigView {
        ConversationModelConfigView {
            status: ConversationModelConfigStatus::Resolved,
            effective_executor_config: None,
            missing_fields: Vec::new(),
            message: None,
        }
    }

    fn snapshot_input(execution_state: SessionExecutionState) -> AgentConversationSnapshotInput {
        AgentConversationSnapshotInput {
            project_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_ref: Some((Uuid::new_v4(), 1)),
            delivery_runtime_session_id: Some("runtime-1".to_string()),
            subject_associations: Vec::new(),
            execution_state,
            terminal_agent: false,
            supports_steering: true,
            pending_paused: false,
            pending_visible_message_count: 0,
            resource_surface: None,
            resource_diagnostics: Vec::new(),
            model_config: resolved_model_config(),
        }
    }

    fn lifecycle_surface() -> ResolvedVfsSurface {
        ResolvedVfsSurface {
            surface_ref: "agent-run:run-1:agent-1".to_string(),
            source: ResolvedVfsSurfaceSource::AgentRun {
                run_id: "run-1".to_string(),
                agent_id: "agent-1".to_string(),
            },
            mounts: vec![ResolvedMountSummary {
                id: "lifecycle".to_string(),
                display_name: "Lifecycle".to_string(),
                provider: "lifecycle_vfs".to_string(),
                backend_id: "lifecycle".to_string(),
                capabilities: vec!["read".to_string(), "list".to_string()],
                default_write: false,
                purpose: ResolvedMountPurpose::Lifecycle,
                backend_online: None,
                file_count: None,
                edit_capabilities: ResolvedMountEditCapabilities::default(),
            }],
            default_mount_id: Some("lifecycle".to_string()),
        }
    }

    #[test]
    fn executor_only_override_keeps_preset_provider_and_model() {
        let preset = AgentConfig {
            executor: "PI_AGENT".to_string(),
            provider_id: Some("openai".to_string()),
            model_id: Some("gpt-5".to_string()),
            agent_id: None,
            thinking_level: None,
            permission_policy: None,
            system_prompt: Some("preset prompt".to_string()),
            system_prompt_mode: None,
        };
        let user = AgentConfig::new("PI_AGENT");

        let resolved = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: Some(&preset),
            user_override: Some(&user),
            ..Default::default()
        });

        assert_eq!(resolved.config.provider_id.as_deref(), Some("openai"));
        assert_eq!(resolved.config.model_id.as_deref(), Some("gpt-5"));
        assert_eq!(
            resolved.config.system_prompt.as_deref(),
            Some("preset prompt")
        );
        assert_eq!(
            resolved.view.status,
            ConversationModelConfigStatus::Resolved
        );
    }

    #[test]
    fn cloud_native_without_model_is_model_required() {
        let preset = AgentConfig::new("PI_AGENT");

        let resolved = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: Some(&preset),
            ..Default::default()
        });

        assert_eq!(
            resolved.view.status,
            ConversationModelConfigStatus::ModelRequired
        );
        assert_eq!(
            resolved.view.missing_fields,
            vec!["provider_id".to_string(), "model_id".to_string()]
        );
        assert!(
            resolved
                .view
                .message
                .as_deref()
                .unwrap()
                .contains("PI_AGENT")
        );
    }

    #[test]
    fn discovery_default_fills_missing_model_fields() {
        let preset = AgentConfig::new("PI_AGENT");
        let discovery = AgentConfig {
            executor: "PI_AGENT".to_string(),
            provider_id: Some("openai".to_string()),
            model_id: Some("gpt-5".to_string()),
            agent_id: None,
            thinking_level: None,
            permission_policy: None,
            system_prompt: None,
            system_prompt_mode: None,
        };

        let resolved = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: Some(&preset),
            executor_discovery_default: Some(&discovery),
            ..Default::default()
        });

        assert_eq!(
            resolved.view.status,
            ConversationModelConfigStatus::Resolved
        );
        assert_eq!(resolved.config.provider_id.as_deref(), Some("openai"));
        assert_eq!(resolved.config.model_id.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn starting_claimed_exposes_no_active_turn_commands() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input(
            SessionExecutionState::Running { turn_id: None },
        ));

        assert_eq!(
            snapshot.execution.status,
            ConversationExecutionStatus::StartingClaimed
        );
        assert_eq!(snapshot.commands.keyboard.enter, None);
        assert_eq!(snapshot.commands.keyboard.ctrl_enter, None);
        for kind in [
            ConversationCommandKind::Enqueue,
            ConversationCommandKind::Steer,
            ConversationCommandKind::PromotePending,
        ] {
            let command = snapshot
                .commands
                .commands
                .iter()
                .find(|command| command.kind == kind)
                .expect("command exists");
            assert!(!command.enabled);
            assert_eq!(command.disabled_code.as_deref(), Some("starting_claimed"));
        }
    }

    #[test]
    fn running_active_exposes_enqueue_and_supported_steer() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input(
            SessionExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            },
        ));

        assert_eq!(
            snapshot.execution.status,
            ConversationExecutionStatus::RunningActive
        );
        assert_eq!(snapshot.commands.keyboard.enter.as_deref(), Some("enqueue"));
        assert_eq!(
            snapshot.commands.keyboard.ctrl_enter.as_deref(),
            Some("steer")
        );
        assert!(
            snapshot
                .commands
                .commands
                .iter()
                .any(|command| command.kind == ConversationCommandKind::Enqueue
                    && command.enabled
                    && command.stale_guard.active_turn_id.as_deref() == Some("turn-1"))
        );
        assert!(
            snapshot
                .commands
                .commands
                .iter()
                .any(|command| command.kind == ConversationCommandKind::Steer && command.enabled)
        );
    }

    #[test]
    fn running_active_without_steer_support_maps_ctrl_enter_to_enqueue() {
        let mut input = snapshot_input(SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        });
        input.supports_steering = false;

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        assert_eq!(snapshot.commands.keyboard.enter.as_deref(), Some("enqueue"));
        assert_eq!(
            snapshot.commands.keyboard.ctrl_enter.as_deref(),
            Some("enqueue")
        );
        let steer = snapshot
            .commands
            .commands
            .iter()
            .find(|command| command.kind == ConversationCommandKind::Steer)
            .expect("steer command exists");
        assert!(!steer.enabled);
        assert_eq!(
            steer.disabled_code.as_deref(),
            Some("connector_steer_unsupported")
        );
    }

    #[test]
    fn ready_keyboard_maps_enter_and_ctrl_enter_to_send_next() {
        let snapshot =
            AgentConversationSnapshotResolver::resolve(snapshot_input(SessionExecutionState::Idle));

        assert_eq!(
            snapshot.execution.status,
            ConversationExecutionStatus::Ready
        );
        assert_eq!(
            snapshot.commands.keyboard.enter.as_deref(),
            Some("send_next")
        );
        assert_eq!(
            snapshot.commands.keyboard.ctrl_enter.as_deref(),
            Some("send_next")
        );
    }

    #[test]
    fn paused_empty_pending_queue_does_not_need_user_attention() {
        let mut input = snapshot_input(SessionExecutionState::Idle);
        input.pending_paused = true;
        input.pending_visible_message_count = 0;

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        assert!(snapshot.pending.paused);
        assert!(!snapshot.pending.user_attention);
        assert!(snapshot.pending.resume_command.is_none());
    }

    #[test]
    fn snapshot_preserves_typed_resource_surface() {
        let mut input = snapshot_input(SessionExecutionState::Idle);
        input.resource_surface = Some(lifecycle_surface());

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        let surface = snapshot.resource_surface.expect("resource surface");
        assert!(matches!(
            surface.source,
            ResolvedVfsSurfaceSource::AgentRun { .. }
        ));
        assert!(
            surface
                .mounts
                .iter()
                .any(|mount| mount.id == "lifecycle" && mount.provider == "lifecycle_vfs")
        );
    }

    #[test]
    fn snapshot_includes_resource_diagnostics() {
        let mut input = snapshot_input(SessionExecutionState::Idle);
        input.resource_diagnostics = vec![ConversationDiagnosticView {
            code: "resource_surface_lifecycle_mount_missing".to_string(),
            severity: ValidationSeverity::Error,
            message: "missing lifecycle mount".to_string(),
            detail: None,
        }];

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        assert!(snapshot.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "resource_surface_lifecycle_mount_missing"
                && diagnostic.severity == ValidationSeverity::Error
        }));
    }
}
