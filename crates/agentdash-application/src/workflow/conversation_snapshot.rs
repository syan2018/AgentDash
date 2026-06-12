use uuid::Uuid;

use agentdash_contracts::workflow::{
    AgentConversationIdentity, AgentConversationLifecycleContext, AgentConversationSnapshot,
    AgentFrameRefDto, AgentRunRefDto, ConversationCommandKind, ConversationCommandSetView,
    ConversationCommandView, ConversationDiagnosticView, ConversationEffectiveExecutorConfigView,
    ConversationExecutionStatus, ConversationExecutionView, ConversationModelConfigSource,
    ConversationModelConfigStatus, ConversationModelConfigView, ConversationPendingSnapshotView,
    LifecycleRunRefDto, LifecycleSubjectAssociationDto, RuntimeSessionRefDto, ValidationSeverity,
};
use agentdash_domain::agent::ProjectAgent;
use agentdash_spi::{AgentConfig, ThinkingLevel};
use serde_json::Value;

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
    pub resource_surface: Option<Value>,
    pub model_config: ConversationModelConfigView,
}

pub struct AgentConversationSnapshotResolver;

impl AgentConversationSnapshotResolver {
    pub fn resolve(input: AgentConversationSnapshotInput) -> AgentConversationSnapshot {
        let active_turn_id = active_turn_id(&input.execution_state);
        let execution = conversation_execution_view(&input, active_turn_id.clone());
        let commands = conversation_commands(&input, execution.status, active_turn_id.as_deref());
        let diagnostics = conversation_diagnostics(&input.model_config);

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
            SessionExecutionState::Running { .. } => (
                ConversationExecutionStatus::Running,
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
    let running_active = status == ConversationExecutionStatus::Running && active_turn_id.is_some();
    let steer = running_active && input.supports_steering;
    let cancel = matches!(
        status,
        ConversationExecutionStatus::Running | ConversationExecutionStatus::Cancelling
    );

    let commands = vec![
        command_view(
            ConversationCommandKind::StartDraft,
            status == ConversationExecutionStatus::Draft && model_ready,
            "当前 workspace 不是 draft start 状态。",
            Some("enter"),
            true,
            "required",
        ),
        command_view(
            ConversationCommandKind::SendNext,
            send_next,
            unavailable_reason_for_ready(status, model_ready),
            Some("enter"),
            true,
            "allowed",
        ),
        command_view(
            ConversationCommandKind::Enqueue,
            running_active,
            "当前 AgentRun 没有 active turn，不能排队新消息。",
            Some("enter"),
            true,
            "allowed",
        ),
        command_view(
            ConversationCommandKind::Steer,
            steer,
            "当前 AgentRun 没有可用 steer intent。",
            Some("mod+enter"),
            true,
            "ignored",
        ),
        command_view(
            ConversationCommandKind::PromotePending,
            running_active,
            "当前 AgentRun 没有 active turn，不能投递 pending 消息。",
            None,
            false,
            "ignored",
        ),
        command_view(
            ConversationCommandKind::ResumePendingQueue,
            input.pending_paused && input.pending_visible_message_count > 0,
            "当前没有需要用户恢复的 pending 队列。",
            None,
            false,
            "ignored",
        ),
        command_view(
            ConversationCommandKind::Cancel,
            cancel,
            "当前 AgentRun 没有正在执行的 turn。",
            None,
            false,
            "ignored",
        ),
    ];

    ConversationCommandSetView {
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
    kind: ConversationCommandKind,
    enabled: bool,
    unavailable_reason: impl Into<String>,
    shortcut: Option<&str>,
    requires_input: bool,
    executor_config_policy: impl Into<String>,
) -> ConversationCommandView {
    ConversationCommandView {
        kind,
        enabled,
        unavailable_reason: if enabled {
            None
        } else {
            Some(unavailable_reason.into())
        },
        shortcut: shortcut.map(str::to_string),
        requires_input,
        executor_config_policy: executor_config_policy.into(),
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
        ConversationExecutionStatus::Running => {
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
) -> Vec<ConversationDiagnosticView> {
    if model_config.status != ConversationModelConfigStatus::ModelRequired {
        return Vec::new();
    }
    vec![ConversationDiagnosticView {
        code: "model_required".to_string(),
        severity: ValidationSeverity::Error,
        message: model_config
            .message
            .clone()
            .unwrap_or_else(|| "当前 AgentRun 缺少模型选择。".to_string()),
        detail: Some(serde_json::json!({
            "missing_fields": model_config.missing_fields,
        })),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
