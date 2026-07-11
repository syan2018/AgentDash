use uuid::Uuid;

use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::workflow::LifecycleGate;
use agentdash_spi::{AgentConfig, ThinkingLevel};

use crate::agent_run::AgentRunExecutionState;
use crate::agent_run::lifecycle_read_model_facade::LifecycleSubjectAssociationView;
use crate::agent_run::workspace::types::AgentRunResourceSurfaceCoordinateModel;
use crate::error::WorkflowApplicationError;
use agentdash_application_vfs::ResolvedVfsSurface;

#[derive(Debug, Clone)]
pub struct AgentConversationSnapshotModel {
    pub snapshot_id: String,
    pub identity: AgentConversationIdentityModel,
    pub lifecycle_context: AgentConversationLifecycleContextModel,
    pub execution: ConversationExecutionModel,
    pub model_config: ConversationModelConfigModel,
    pub commands: ConversationCommandSetModel,
    pub mailbox: ConversationMailboxSnapshotModel,
    pub resource_surface: Option<ResolvedVfsSurface>,
    pub resource_surface_coordinate: Option<AgentRunResourceSurfaceCoordinateModel>,
    pub diagnostics: Vec<ConversationDiagnosticModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConversationIdentityModel {
    pub run_id: String,
    pub agent_id: String,
    pub project_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentConversationLifecycleContextModel {
    pub frame_ref: Option<AgentConversationFrameRefModel>,
    pub runtime_thread_id: Option<String>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConversationFrameRefModel {
    pub agent_id: String,
    pub frame_id: String,
    pub revision: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationExecutionStatusModel {
    Draft,
    ModelRequired,
    Ready,
    StartingClaimed,
    RunningActive,
    Cancelling,
    Terminal,
    FrameMissing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationModelConfigStatusModel {
    Resolved,
    ModelRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationModelConfigSourceModel {
    ProjectAgentPreset,
    FrameExecutionProfile,
    UserOverride,
    ExecutorDiscoveryDefault,
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEffectiveExecutorConfigModel {
    pub executor: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<String>,
    pub permission_policy: Option<String>,
    pub source: ConversationModelConfigSourceModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunOwnershipModel {
    pub run_created_by_user_id: String,
    pub agent_created_by_user_id: String,
    pub current_user_controls_run: bool,
}

impl AgentRunOwnershipModel {
    pub fn from_owner_fields(
        run_created_by_user_id: impl Into<String>,
        agent_created_by_user_id: impl Into<String>,
        viewer_user_id: Option<&str>,
    ) -> Self {
        let run_created_by_user_id = run_created_by_user_id.into();
        let agent_created_by_user_id = agent_created_by_user_id.into();
        let current_user_controls_run =
            viewer_user_id.is_some_and(|viewer| viewer == run_created_by_user_id);
        Self {
            run_created_by_user_id,
            agent_created_by_user_id,
            current_user_controls_run,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationModelConfigModel {
    pub status: ConversationModelConfigStatusModel,
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigModel>,
    pub missing_fields: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationCommandKindModel {
    SubmitMessage,
    PromoteMailboxMessage,
    DeleteMailboxMessage,
    MoveMailboxMessage,
    ResumeMailbox,
    Cancel,
    CompactContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationCommandPlacementModel {
    ComposerPrimary,
    ComposerSecondary,
    MailboxRow,
    MailboxBanner,
    Header,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConversationCommandStaleGuardModel {
    pub snapshot_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub frame_id: Option<String>,
    pub active_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunCommandPreconditionModel {
    pub command_id: String,
    pub command_kind: ConversationCommandKindModel,
    pub stale_guard: ConversationCommandStaleGuardModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCommandModel {
    pub kind: ConversationCommandKindModel,
    pub command_id: String,
    pub enabled: bool,
    pub unavailable_reason: Option<String>,
    pub disabled_code: Option<String>,
    pub shortcut: Option<String>,
    pub requires_input: bool,
    pub executor_config_policy: String,
    pub placement: Vec<ConversationCommandPlacementModel>,
    pub stale_guard: ConversationCommandStaleGuardModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationKeyboardMapModel {
    pub enter: Option<String>,
    pub ctrl_enter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCommandSetModel {
    pub ownership: AgentRunOwnershipModel,
    pub commands: Vec<ConversationCommandModel>,
    pub keyboard: ConversationKeyboardMapModel,
}

#[derive(Debug, Clone)]
pub struct ConversationExecutionModel {
    pub status: ConversationExecutionStatusModel,
    pub runtime_session_id: Option<String>,
    pub active_turn_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConversationMailboxSnapshotModel {
    pub visible_message_count: usize,
    pub paused: bool,
    pub user_attention: bool,
    pub resume_command: Option<ConversationCommandModel>,
    pub waiting_items: Vec<ConversationWaitingItemModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationWaitingItemModel {
    pub wait_id: String,
    pub gate_id: String,
    pub kind: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub status: String,
    pub source_label: Option<String>,
    pub preview: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

impl ConversationWaitingItemModel {
    pub fn from_lifecycle_gate(gate: &LifecycleGate) -> Self {
        let projection = gate.waiting_projection();
        Self {
            wait_id: gate.id.to_string(),
            gate_id: gate.id.to_string(),
            kind: projection.kind,
            source_ref: Some(gate.id.to_string()),
            correlation_ref: non_empty_string(Some(&gate.correlation_id)),
            status: gate
                .resolved_payload_status()
                .unwrap_or_else(|| gate.status.clone()),
            source_label: projection.source_label,
            preview: projection.preview,
            created_at: gate.created_at.to_rfc3339(),
            resolved_at: gate.resolved_at.map(|at| at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverityModel {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConversationDiagnosticModel {
    pub code: String,
    pub severity: ValidationSeverityModel,
    pub message: String,
    pub detail: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ConversationModelConfigResolution {
    pub config: AgentConfig,
    pub view: ConversationModelConfigModel,
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
        let mut source = ConversationModelConfigSourceModel::Unspecified;
        let mut config = input
            .project_agent_preset
            .cloned()
            .inspect(|_| {
                source = ConversationModelConfigSourceModel::ProjectAgentPreset;
            })
            .unwrap_or_default();

        if let Some(frame_config) = input.frame_execution_profile {
            config = merge_executor_config_fields(config, frame_config);
            source = ConversationModelConfigSourceModel::FrameExecutionProfile;
        }
        if let Some(user_config) = input.user_override {
            config = merge_executor_config_fields(config, user_config);
            source = ConversationModelConfigSourceModel::UserOverride;
        }
        if let Some(discovery_config) = input.executor_discovery_default {
            let before = config.clone();
            config = fill_executor_config_missing_fields(config, discovery_config);
            if before.model_id != config.model_id || before.provider_id != config.provider_id {
                source = ConversationModelConfigSourceModel::ExecutorDiscoveryDefault;
            }
        }

        let missing_fields = missing_required_model_fields(&config);
        let status = if missing_fields.is_empty() {
            ConversationModelConfigStatusModel::Resolved
        } else {
            ConversationModelConfigStatusModel::ModelRequired
        };
        let message = if status == ConversationModelConfigStatusModel::ModelRequired {
            Some(model_required_message(&config, &missing_fields))
        } else {
            None
        };
        let effective_executor_config = Some(effective_executor_config_view(&config, source));

        ConversationModelConfigResolution {
            config,
            view: ConversationModelConfigModel {
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
        if resolution.view.status == ConversationModelConfigStatusModel::ModelRequired {
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
        source: ConversationModelConfigSourceModel,
    ) -> ConversationEffectiveExecutorConfigModel {
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
    source: ConversationModelConfigSourceModel,
) -> ConversationEffectiveExecutorConfigModel {
    ConversationEffectiveExecutorConfigModel {
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

#[derive(Debug, Clone)]
pub struct ConversationCommandAvailabilityInput {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_ref: Option<(Uuid, i32)>,
    pub runtime_thread_id: Option<String>,
    pub execution_state: AgentRunExecutionState,
    pub terminal_agent: bool,
    pub supports_steering: bool,
    pub mailbox_paused: bool,
    pub mailbox_visible_message_count: usize,
    pub model_config_status: ConversationModelConfigStatusModel,
    pub ownership: AgentRunOwnershipModel,
}

impl ConversationCommandAvailabilityInput {
    fn from_snapshot_input(input: &AgentConversationSnapshotInput) -> Self {
        Self {
            run_id: input.run_id,
            agent_id: input.agent_id,
            frame_ref: input.frame_ref,
            runtime_thread_id: input.runtime_thread_id.clone(),
            execution_state: input.execution_state.clone(),
            terminal_agent: input.terminal_agent,
            supports_steering: input.supports_steering,
            mailbox_paused: input.mailbox_paused,
            mailbox_visible_message_count: input.mailbox_visible_message_count,
            model_config_status: input.model_config.status,
            ownership: input.ownership.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConversationCommandAvailability {
    pub snapshot_id: String,
    pub execution_status: ConversationExecutionStatusModel,
    pub frame_id: Option<String>,
    pub runtime_session_id: Option<String>,
    pub active_turn_id: Option<String>,
    pub terminal_agent: bool,
    pub commands: ConversationCommandSetModel,
}

pub struct ConversationCommandAvailabilityResolver;

impl ConversationCommandAvailabilityResolver {
    pub fn resolve(input: ConversationCommandAvailabilityInput) -> ConversationCommandAvailability {
        let active_turn_id = active_turn_id(&input.execution_state);
        let execution_status = conversation_execution_status(&input);
        let snapshot_id = conversation_snapshot_id(
            input.run_id,
            input.agent_id,
            input.frame_ref,
            &input.execution_state,
            input.terminal_agent,
        );
        let commands = conversation_commands(
            &input,
            execution_status,
            active_turn_id.as_deref(),
            &snapshot_id,
        );

        ConversationCommandAvailability {
            snapshot_id,
            execution_status,
            frame_id: input.frame_ref.map(|(frame_id, _)| frame_id.to_string()),
            runtime_session_id: input.runtime_thread_id,
            active_turn_id,
            terminal_agent: input.terminal_agent,
            commands,
        }
    }
}

pub struct AgentConversationSnapshotInput {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_ref: Option<(Uuid, i32)>,
    pub runtime_thread_id: Option<String>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub execution_state: AgentRunExecutionState,
    pub terminal_agent: bool,
    pub supports_steering: bool,
    pub mailbox_paused: bool,
    pub mailbox_visible_message_count: usize,
    pub open_wait_items: Vec<ConversationWaitingItemModel>,
    pub resource_surface: Option<ResolvedVfsSurface>,
    pub resource_surface_coordinate: Option<AgentRunResourceSurfaceCoordinateModel>,
    pub resource_diagnostics: Vec<ConversationDiagnosticModel>,
    pub model_config: ConversationModelConfigModel,
    pub ownership: AgentRunOwnershipModel,
}

pub struct AgentConversationSnapshotResolver;

impl AgentConversationSnapshotResolver {
    pub fn resolve(input: AgentConversationSnapshotInput) -> AgentConversationSnapshotModel {
        let availability = ConversationCommandAvailabilityResolver::resolve(
            ConversationCommandAvailabilityInput::from_snapshot_input(&input),
        );
        let execution = conversation_execution_view(
            &input,
            availability.execution_status,
            availability.active_turn_id.clone(),
        );
        let commands = availability.commands;
        let resume_command = commands
            .commands
            .iter()
            .find(|command| command.kind == ConversationCommandKindModel::ResumeMailbox)
            .cloned()
            .filter(|command| command.enabled);
        let diagnostics = conversation_diagnostics(&input.model_config, input.resource_diagnostics);

        AgentConversationSnapshotModel {
            snapshot_id: availability.snapshot_id.clone(),
            identity: AgentConversationIdentityModel {
                run_id: input.run_id.to_string(),
                agent_id: input.agent_id.to_string(),
                project_id: input.project_id.to_string(),
            },
            lifecycle_context: AgentConversationLifecycleContextModel {
                frame_ref: input.frame_ref.map(|(frame_id, revision)| {
                    AgentConversationFrameRefModel {
                        agent_id: input.agent_id.to_string(),
                        frame_id: frame_id.to_string(),
                        revision: Some(revision),
                    }
                }),
                runtime_thread_id: input.runtime_thread_id.clone(),
                subject_associations: input.subject_associations,
            },
            execution,
            model_config: input.model_config,
            commands,
            mailbox: ConversationMailboxSnapshotModel {
                visible_message_count: input.mailbox_visible_message_count,
                paused: input.mailbox_paused,
                user_attention: input.mailbox_visible_message_count > 0 && input.mailbox_paused,
                resume_command,
                waiting_items: input.open_wait_items,
            },
            resource_surface: input.resource_surface,
            resource_surface_coordinate: input.resource_surface_coordinate,
            diagnostics,
        }
    }
}

fn conversation_execution_view(
    input: &AgentConversationSnapshotInput,
    status: ConversationExecutionStatusModel,
    active_turn_id: Option<String>,
) -> ConversationExecutionModel {
    let reason = match status {
        ConversationExecutionStatusModel::Terminal => Some("当前 AgentRun 已结束。".to_string()),
        ConversationExecutionStatusModel::FrameMissing => {
            Some("当前 AgentRun 没有可投递的 runtime frame。".to_string())
        }
        ConversationExecutionStatusModel::ModelRequired => input.model_config.message.clone(),
        ConversationExecutionStatusModel::StartingClaimed => {
            Some("当前 AgentRun 正在启动中，等待 active turn 建立。".to_string())
        }
        ConversationExecutionStatusModel::RunningActive => {
            Some("当前 AgentRun 正在执行中。".to_string())
        }
        ConversationExecutionStatusModel::Cancelling => {
            Some("当前 AgentRun 正在取消中，等待执行器收口。".to_string())
        }
        ConversationExecutionStatusModel::Draft | ConversationExecutionStatusModel::Ready => None,
    };
    ConversationExecutionModel {
        status,
        runtime_session_id: input.runtime_thread_id.clone(),
        active_turn_id,
        reason,
    }
}

fn conversation_execution_status(
    input: &ConversationCommandAvailabilityInput,
) -> ConversationExecutionStatusModel {
    if input.terminal_agent {
        ConversationExecutionStatusModel::Terminal
    } else if input.frame_ref.is_none() {
        ConversationExecutionStatusModel::FrameMissing
    } else if input.model_config_status == ConversationModelConfigStatusModel::ModelRequired {
        ConversationExecutionStatusModel::ModelRequired
    } else {
        match input.execution_state {
            AgentRunExecutionState::Running { turn_id: None } => {
                ConversationExecutionStatusModel::StartingClaimed
            }
            AgentRunExecutionState::Running { turn_id: Some(_) } => {
                ConversationExecutionStatusModel::RunningActive
            }
            AgentRunExecutionState::Cancelling { .. } => {
                ConversationExecutionStatusModel::Cancelling
            }
            _ => ConversationExecutionStatusModel::Ready,
        }
    }
}

fn conversation_commands(
    input: &ConversationCommandAvailabilityInput,
    status: ConversationExecutionStatusModel,
    active_turn_id: Option<&str>,
    snapshot_id: &str,
) -> ConversationCommandSetModel {
    let model_ready = input.model_config_status == ConversationModelConfigStatusModel::Resolved;
    let submit_message = !matches!(
        status,
        ConversationExecutionStatusModel::Draft
            | ConversationExecutionStatusModel::Terminal
            | ConversationExecutionStatusModel::FrameMissing
            | ConversationExecutionStatusModel::ModelRequired
    ) && model_ready;
    let running_active =
        status == ConversationExecutionStatusModel::RunningActive && active_turn_id.is_some();
    let compact_context = model_ready
        && input.frame_ref.is_some()
        && input.runtime_thread_id.is_some()
        && (status == ConversationExecutionStatusModel::Ready || running_active);
    let cancel = matches!(
        status,
        ConversationExecutionStatusModel::StartingClaimed
            | ConversationExecutionStatusModel::RunningActive
            | ConversationExecutionStatusModel::Cancelling
    );
    let mailbox_can_resume = !input.terminal_agent
        && input.frame_ref.is_some()
        && input.mailbox_paused
        && input.mailbox_visible_message_count > 0;

    let commands = vec![
        command_view(
            input,
            ConversationCommandKindModel::SubmitMessage,
            snapshot_id,
            submit_message,
            unavailable_reason_for_submit(status, model_ready),
            Some(disabled_code_for_status(status)),
            Some("enter"),
            true,
            "allowed",
            vec![ConversationCommandPlacementModel::ComposerPrimary],
        ),
        command_view(
            input,
            ConversationCommandKindModel::PromoteMailboxMessage,
            snapshot_id,
            running_active && input.supports_steering,
            "当前 AgentRun 不在可投递 mailbox 消息的运行状态。",
            Some(
                if status == ConversationExecutionStatusModel::StartingClaimed {
                    "starting_claimed"
                } else if running_active {
                    "connector_steer_unsupported"
                } else {
                    "command_unavailable"
                },
            ),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacementModel::MailboxRow],
        ),
        command_view(
            input,
            ConversationCommandKindModel::DeleteMailboxMessage,
            snapshot_id,
            input.mailbox_visible_message_count > 0,
            "当前没有可删除的 mailbox message。",
            Some("command_unavailable"),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacementModel::MailboxRow],
        ),
        command_view(
            input,
            ConversationCommandKindModel::MoveMailboxMessage,
            snapshot_id,
            input.mailbox_visible_message_count > 0,
            "当前没有可移动的 mailbox message。",
            Some("command_unavailable"),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacementModel::MailboxRow],
        ),
        command_view(
            input,
            ConversationCommandKindModel::ResumeMailbox,
            snapshot_id,
            mailbox_can_resume,
            "当前没有需要用户恢复的 mailbox。",
            Some("command_unavailable"),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacementModel::MailboxBanner],
        ),
        command_view(
            input,
            ConversationCommandKindModel::Cancel,
            snapshot_id,
            cancel,
            "当前 AgentRun 没有正在执行的 turn。",
            Some("command_unavailable"),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacementModel::Header],
        ),
        command_view(
            input,
            ConversationCommandKindModel::CompactContext,
            snapshot_id,
            compact_context,
            unavailable_reason_for_compact_context(
                status,
                model_ready,
                input.frame_ref.is_some(),
                input.runtime_thread_id.is_some(),
            ),
            Some(disabled_code_for_compact_context(
                status,
                model_ready,
                input.frame_ref.is_some(),
                input.runtime_thread_id.is_some(),
            )),
            None,
            false,
            "ignored",
            vec![ConversationCommandPlacementModel::Header],
        ),
    ];

    ConversationCommandSetModel {
        ownership: input.ownership.clone(),
        keyboard: ConversationKeyboardMapModel {
            enter: if submit_message {
                Some(command_id_for(ConversationCommandKindModel::SubmitMessage))
            } else {
                None
            },
            ctrl_enter: if submit_message {
                Some(command_id_for(ConversationCommandKindModel::SubmitMessage))
            } else {
                None
            },
        },
        commands,
    }
}

#[allow(clippy::too_many_arguments)]
fn command_view(
    input: &ConversationCommandAvailabilityInput,
    kind: ConversationCommandKindModel,
    snapshot_id: &str,
    enabled: bool,
    unavailable_reason: impl Into<String>,
    disabled_code: Option<&str>,
    shortcut: Option<&str>,
    requires_input: bool,
    executor_config_policy: impl Into<String>,
    placement: Vec<ConversationCommandPlacementModel>,
) -> ConversationCommandModel {
    ConversationCommandModel {
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
        stale_guard: ConversationCommandStaleGuardModel {
            snapshot_id: snapshot_id.to_string(),
            run_id: input.run_id.to_string(),
            agent_id: input.agent_id.to_string(),
            frame_id: input.frame_ref.map(|(frame_id, _)| frame_id.to_string()),
            active_turn_id: active_turn_id(&input.execution_state),
        },
    }
}

pub fn conversation_snapshot_id(
    run_id: Uuid,
    agent_id: Uuid,
    frame_ref: Option<(Uuid, i32)>,
    execution_state: &AgentRunExecutionState,
    terminal_agent: bool,
) -> String {
    let frame = frame_ref
        .map(|(frame_id, revision)| format!("{frame_id}:{revision}"))
        .unwrap_or_else(|| "none".to_string());
    let turn = active_turn_id(execution_state).unwrap_or_else(|| "none".to_string());
    format!(
        "agentrun:{run_id}:{agent_id}:frame:{frame}:state:{}:turn:{turn}:terminal:{terminal_agent}",
        conversation_execution_state_code(execution_state)
    )
}

pub fn conversation_execution_state_code(execution_state: &AgentRunExecutionState) -> &'static str {
    match execution_state {
        AgentRunExecutionState::Idle => "idle",
        AgentRunExecutionState::Running { turn_id: None } => "starting_claimed",
        AgentRunExecutionState::Running { turn_id: Some(_) } => "running_active",
        AgentRunExecutionState::Cancelling { .. } => "cancelling",
        AgentRunExecutionState::Completed { .. } => "completed",
        AgentRunExecutionState::Failed { .. } => "failed",
        AgentRunExecutionState::Interrupted { .. } => "interrupted",
        AgentRunExecutionState::Lost { .. } => "lost",
    }
}

pub fn conversation_command_id_for(kind: ConversationCommandKindModel) -> &'static str {
    match kind {
        ConversationCommandKindModel::SubmitMessage => "submit_message",
        ConversationCommandKindModel::PromoteMailboxMessage => "promote_mailbox_message",
        ConversationCommandKindModel::DeleteMailboxMessage => "delete_mailbox_message",
        ConversationCommandKindModel::MoveMailboxMessage => "move_mailbox_message",
        ConversationCommandKindModel::ResumeMailbox => "resume_mailbox",
        ConversationCommandKindModel::Cancel => "cancel",
        ConversationCommandKindModel::CompactContext => "compact_context",
    }
}

fn command_id_for(kind: ConversationCommandKindModel) -> String {
    conversation_command_id_for(kind).to_string()
}

fn disabled_code_for_status(status: ConversationExecutionStatusModel) -> &'static str {
    match status {
        ConversationExecutionStatusModel::Draft => "draft",
        ConversationExecutionStatusModel::ModelRequired => "model_required",
        ConversationExecutionStatusModel::Ready => "command_unavailable",
        ConversationExecutionStatusModel::StartingClaimed => "starting_claimed",
        ConversationExecutionStatusModel::RunningActive => "running_active",
        ConversationExecutionStatusModel::Cancelling => "cancelling",
        ConversationExecutionStatusModel::Terminal => "terminal",
        ConversationExecutionStatusModel::FrameMissing => "missing_frame",
    }
}

fn unavailable_reason_for_submit(
    status: ConversationExecutionStatusModel,
    model_ready: bool,
) -> &'static str {
    if !model_ready {
        return "当前 AgentRun 缺少模型选择。";
    }
    match status {
        ConversationExecutionStatusModel::StartingClaimed => {
            "当前 AgentRun 正在启动中，等待 active turn 建立。"
        }
        ConversationExecutionStatusModel::RunningActive => {
            "当前 AgentRun 正在执行中，新消息将进入 mailbox。"
        }
        ConversationExecutionStatusModel::Cancelling => {
            "当前 AgentRun 正在取消中，新消息将由 mailbox 等待可消费边界。"
        }
        ConversationExecutionStatusModel::Terminal => "当前 AgentRun 已结束，不能继续发送消息。",
        ConversationExecutionStatusModel::FrameMissing => {
            "当前 AgentRun 没有可投递的 runtime frame。"
        }
        ConversationExecutionStatusModel::ModelRequired => "当前 AgentRun 缺少模型选择。",
        ConversationExecutionStatusModel::Draft | ConversationExecutionStatusModel::Ready => {
            "当前 AgentRun 暂不可提交消息。"
        }
    }
}

fn disabled_code_for_compact_context(
    status: ConversationExecutionStatusModel,
    model_ready: bool,
    has_frame: bool,
    has_runtime_session: bool,
) -> &'static str {
    if !has_frame {
        return "frame_missing";
    }
    if !model_ready {
        return "model_required";
    }
    if !has_runtime_session {
        return "runtime_session_missing";
    }
    match status {
        ConversationExecutionStatusModel::Ready
        | ConversationExecutionStatusModel::RunningActive => "command_unavailable",
        ConversationExecutionStatusModel::Draft => "draft",
        ConversationExecutionStatusModel::StartingClaimed => "starting_claimed",
        ConversationExecutionStatusModel::Cancelling => "cancelling",
        ConversationExecutionStatusModel::Terminal => "terminal",
        ConversationExecutionStatusModel::FrameMissing => "frame_missing",
        ConversationExecutionStatusModel::ModelRequired => "model_required",
    }
}

fn unavailable_reason_for_compact_context(
    status: ConversationExecutionStatusModel,
    model_ready: bool,
    has_frame: bool,
    has_runtime_session: bool,
) -> &'static str {
    if !has_frame {
        return "当前 AgentRun 缺少可用 frame。";
    }
    if !model_ready {
        return "当前 AgentRun 缺少模型选择。";
    }
    if !has_runtime_session {
        return "当前 AgentRun 缺少可压缩的 runtime session。";
    }
    match status {
        ConversationExecutionStatusModel::StartingClaimed => {
            "当前 AgentRun 正在启动中，等待 active turn 建立。"
        }
        ConversationExecutionStatusModel::Cancelling => "当前 AgentRun 正在取消中。",
        ConversationExecutionStatusModel::Terminal => "当前 AgentRun 已结束。",
        ConversationExecutionStatusModel::FrameMissing => "当前 AgentRun 缺少可用 frame。",
        ConversationExecutionStatusModel::ModelRequired => "当前 AgentRun 缺少模型选择。",
        ConversationExecutionStatusModel::Draft => "当前 AgentRun 尚未启动。",
        ConversationExecutionStatusModel::Ready
        | ConversationExecutionStatusModel::RunningActive => "当前 AgentRun 不可压缩上下文。",
    }
}

fn active_turn_id(execution_state: &AgentRunExecutionState) -> Option<String> {
    match execution_state {
        AgentRunExecutionState::Running { turn_id }
        | AgentRunExecutionState::Cancelling { turn_id } => turn_id.clone(),
        AgentRunExecutionState::Idle
        | AgentRunExecutionState::Completed { .. }
        | AgentRunExecutionState::Failed { .. }
        | AgentRunExecutionState::Interrupted { .. }
        | AgentRunExecutionState::Lost { .. } => None,
    }
}

fn non_empty_string(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn conversation_diagnostics(
    model_config: &ConversationModelConfigModel,
    mut resource_diagnostics: Vec<ConversationDiagnosticModel>,
) -> Vec<ConversationDiagnosticModel> {
    if model_config.status == ConversationModelConfigStatusModel::ModelRequired {
        resource_diagnostics.push(ConversationDiagnosticModel {
            code: "model_required".to_string(),
            severity: ValidationSeverityModel::Error,
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
    use crate::agent_run::workspace::AgentRunResourceSurfaceSourceAnchorModel;
    use agentdash_application_vfs::{
        ResolvedMountEditCapabilities, ResolvedMountPurpose, ResolvedMountSummary,
        ResolvedVfsSurfaceSource,
    };

    fn resolved_model_config() -> ConversationModelConfigModel {
        ConversationModelConfigModel {
            status: ConversationModelConfigStatusModel::Resolved,
            effective_executor_config: None,
            missing_fields: Vec::new(),
            message: None,
        }
    }

    fn snapshot_input(execution_state: AgentRunExecutionState) -> AgentConversationSnapshotInput {
        AgentConversationSnapshotInput {
            project_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_ref: Some((Uuid::new_v4(), 1)),
            runtime_thread_id: Some("runtime-1".to_string()),
            subject_associations: Vec::new(),
            execution_state,
            terminal_agent: false,
            supports_steering: true,
            mailbox_paused: false,
            mailbox_visible_message_count: 0,
            open_wait_items: Vec::new(),
            resource_surface: None,
            resource_surface_coordinate: None,
            resource_diagnostics: Vec::new(),
            model_config: resolved_model_config(),
            ownership: AgentRunOwnershipModel::from_owner_fields(
                "owner-user",
                "owner-user",
                Some("owner-user"),
            ),
        }
    }

    fn command(
        snapshot: &AgentConversationSnapshotModel,
        kind: ConversationCommandKindModel,
    ) -> &ConversationCommandModel {
        snapshot
            .commands
            .commands
            .iter()
            .find(|command| command.kind == kind)
            .expect("command exists")
    }

    #[test]
    fn ownership_model_marks_only_run_owner_as_controller() {
        let owner = AgentRunOwnershipModel::from_owner_fields(
            "owner-user",
            "agent-owner",
            Some("owner-user"),
        );
        assert!(owner.current_user_controls_run);
        assert_eq!(owner.run_created_by_user_id, "owner-user");
        assert_eq!(owner.agent_created_by_user_id, "agent-owner");

        let collaborator = AgentRunOwnershipModel::from_owner_fields(
            "owner-user",
            "agent-owner",
            Some("collaborator"),
        );
        assert!(!collaborator.current_user_controls_run);
    }

    fn lifecycle_surface() -> ResolvedVfsSurface {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        ResolvedVfsSurface {
            surface_ref: format!("agent-run:{run_id}:{agent_id}"),
            source: ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id },
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
            ConversationModelConfigStatusModel::Resolved
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
            ConversationModelConfigStatusModel::ModelRequired
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
        };

        let resolved = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: Some(&preset),
            executor_discovery_default: Some(&discovery),
            ..Default::default()
        });

        assert_eq!(
            resolved.view.status,
            ConversationModelConfigStatusModel::Resolved
        );
        assert_eq!(resolved.config.provider_id.as_deref(), Some("openai"));
        assert_eq!(resolved.config.model_id.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn starting_claimed_exposes_no_active_turn_commands() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Running { turn_id: None },
        ));

        assert_eq!(
            snapshot.execution.status,
            ConversationExecutionStatusModel::StartingClaimed
        );
        assert_eq!(
            snapshot.commands.keyboard.enter.as_deref(),
            Some("submit_message")
        );
        let promote = snapshot
            .commands
            .commands
            .iter()
            .find(|command| command.kind == ConversationCommandKindModel::PromoteMailboxMessage)
            .expect("promote command exists");
        assert!(!promote.enabled);
        assert_eq!(promote.disabled_code.as_deref(), Some("starting_claimed"));
    }

    #[test]
    fn running_active_exposes_submit_and_supported_promote() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            },
        ));

        assert_eq!(
            snapshot.execution.status,
            ConversationExecutionStatusModel::RunningActive
        );
        assert_eq!(
            snapshot.commands.keyboard.enter.as_deref(),
            Some("submit_message")
        );
        assert!(snapshot.commands.commands.iter().any(|command| command.kind
            == ConversationCommandKindModel::SubmitMessage
            && command.enabled
            && command.stale_guard.active_turn_id.as_deref() == Some("turn-1")));
        assert!(snapshot.commands.commands.iter().any(|command| command.kind
            == ConversationCommandKindModel::PromoteMailboxMessage
            && command.enabled));
    }

    #[test]
    fn compact_context_is_available_when_ready_or_running_active() {
        let ready = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Idle,
        ));
        let running = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            },
        ));

        let ready_compact = command(&ready, ConversationCommandKindModel::CompactContext);
        assert!(ready_compact.enabled);
        assert_eq!(ready_compact.command_id, "compact_context");
        assert_eq!(ready_compact.stale_guard.active_turn_id, None);

        let running_compact = command(&running, ConversationCommandKindModel::CompactContext);
        assert!(running_compact.enabled);
        assert_eq!(
            running_compact.stale_guard.active_turn_id.as_deref(),
            Some("turn-1")
        );
    }

    #[test]
    fn compact_context_disabled_states_explain_current_blocker() {
        let starting = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Running { turn_id: None },
        ));
        assert_eq!(
            command(&starting, ConversationCommandKindModel::CompactContext)
                .disabled_code
                .as_deref(),
            Some("starting_claimed")
        );

        let cancelling = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Cancelling {
                turn_id: Some("turn-1".to_string()),
            },
        ));
        assert_eq!(
            command(&cancelling, ConversationCommandKindModel::CompactContext)
                .disabled_code
                .as_deref(),
            Some("cancelling")
        );

        let mut model_required = snapshot_input(AgentRunExecutionState::Idle);
        model_required.model_config.status = ConversationModelConfigStatusModel::ModelRequired;
        let model_required = AgentConversationSnapshotResolver::resolve(model_required);
        assert_eq!(
            command(
                &model_required,
                ConversationCommandKindModel::CompactContext
            )
            .disabled_code
            .as_deref(),
            Some("model_required")
        );

        let mut frame_missing = snapshot_input(AgentRunExecutionState::Idle);
        frame_missing.frame_ref = None;
        let frame_missing = AgentConversationSnapshotResolver::resolve(frame_missing);
        assert_eq!(
            command(&frame_missing, ConversationCommandKindModel::CompactContext)
                .disabled_code
                .as_deref(),
            Some("frame_missing")
        );

        let mut runtime_missing = snapshot_input(AgentRunExecutionState::Idle);
        runtime_missing.runtime_thread_id = None;
        let runtime_missing = AgentConversationSnapshotResolver::resolve(runtime_missing);
        assert_eq!(
            command(
                &runtime_missing,
                ConversationCommandKindModel::CompactContext
            )
            .disabled_code
            .as_deref(),
            Some("runtime_session_missing")
        );
    }

    #[test]
    fn running_active_without_steer_support_keeps_submit_and_disables_promote() {
        let mut input = snapshot_input(AgentRunExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        });
        input.supports_steering = false;

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        assert_eq!(
            snapshot.commands.keyboard.enter.as_deref(),
            Some("submit_message")
        );
        let promote = snapshot
            .commands
            .commands
            .iter()
            .find(|command| command.kind == ConversationCommandKindModel::PromoteMailboxMessage)
            .expect("promote command exists");
        assert!(!promote.enabled);
        assert_eq!(
            promote.disabled_code.as_deref(),
            Some("connector_steer_unsupported")
        );
    }

    #[test]
    fn ready_keyboard_maps_enter_and_ctrl_enter_to_submit_message() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Idle,
        ));

        assert_eq!(
            snapshot.execution.status,
            ConversationExecutionStatusModel::Ready
        );
        assert_eq!(
            snapshot.commands.keyboard.enter.as_deref(),
            Some("submit_message")
        );
        assert_eq!(
            snapshot.commands.keyboard.ctrl_enter.as_deref(),
            Some("submit_message")
        );
    }

    #[test]
    fn runtime_snapshot_does_not_emit_draft_start_command() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Idle,
        ));

        assert!(
            snapshot
                .commands
                .commands
                .iter()
                .all(|command| command.command_id != "start_draft")
        );
    }

    #[test]
    fn command_guards_share_snapshot_id() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            },
        ));

        assert!(!snapshot.snapshot_id.is_empty());
        assert!(
            snapshot
                .commands
                .commands
                .iter()
                .all(|command| { command.stale_guard.snapshot_id == snapshot.snapshot_id })
        );
    }

    #[test]
    fn snapshot_id_ignores_delivery_runtime_session() {
        let input = ConversationCommandAvailabilityInput::from_snapshot_input(&snapshot_input(
            AgentRunExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            },
        ));
        let mut rotated_runtime = input.clone();
        rotated_runtime.runtime_thread_id = Some("runtime-2".to_string());

        let first = ConversationCommandAvailabilityResolver::resolve(input);
        let second = ConversationCommandAvailabilityResolver::resolve(rotated_runtime);

        assert_eq!(first.snapshot_id, second.snapshot_id);
        assert!(!first.snapshot_id.contains(":runtime:"));
    }

    #[test]
    fn completed_turn_changes_snapshot_and_keyboard_stays_submit_message() {
        let running = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            },
        ));
        let completed = AgentConversationSnapshotResolver::resolve(snapshot_input(
            AgentRunExecutionState::Completed {
                turn_id: "turn-1".to_string(),
            },
        ));

        assert_ne!(running.snapshot_id, completed.snapshot_id);
        assert_eq!(
            running.commands.keyboard.enter.as_deref(),
            Some("submit_message")
        );
        assert_eq!(
            completed.commands.keyboard.enter.as_deref(),
            Some("submit_message")
        );
        assert_eq!(
            completed.commands.keyboard.ctrl_enter.as_deref(),
            Some("submit_message")
        );
        assert_eq!(completed.execution.active_turn_id, None);
        assert!(
            completed
                .commands
                .commands
                .iter()
                .all(|command| command.stale_guard.active_turn_id.is_none())
        );
    }

    #[test]
    fn paused_empty_mailbox_does_not_need_user_attention() {
        let mut input = snapshot_input(AgentRunExecutionState::Idle);
        input.mailbox_paused = true;
        input.mailbox_visible_message_count = 0;

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        assert!(snapshot.mailbox.paused);
        assert!(!snapshot.mailbox.user_attention);
        assert!(snapshot.mailbox.resume_command.is_none());
    }

    #[test]
    fn snapshot_preserves_typed_resource_surface() {
        let mut input = snapshot_input(AgentRunExecutionState::Idle);
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
    fn snapshot_preserves_resource_surface_coordinate() {
        let mut input = snapshot_input(AgentRunExecutionState::Idle);
        let frame_id = input.frame_ref.expect("frame ref").0;
        input.resource_surface = Some(lifecycle_surface());
        input.resource_surface_coordinate = Some(AgentRunResourceSurfaceCoordinateModel {
            surface_frame_ref: crate::agent_run::workspace::types::AgentRunWorkspaceFrameRefModel {
                agent_id: input.agent_id.to_string(),
                frame_id: frame_id.to_string(),
                revision: Some(1),
            },
            source_anchor: Some(AgentRunResourceSurfaceSourceAnchorModel {
                runtime_session_id: "runtime-1".to_string(),
                launch_frame_id: "launch-frame-1".to_string(),
                orchestration_id: Some("orchestration-1".to_string()),
                node_path: Some("root.review".to_string()),
                node_attempt: Some(2),
                delivery_status: "running".to_string(),
                observed_at: "2026-06-21T00:00:00+00:00".to_string(),
            }),
        });

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        let coordinate = snapshot
            .resource_surface_coordinate
            .expect("resource surface coordinate");
        assert_eq!(coordinate.surface_frame_ref.frame_id, frame_id.to_string());
        let source_anchor = coordinate.source_anchor.expect("source anchor");
        assert_eq!(source_anchor.runtime_session_id, "runtime-1");
        assert_eq!(source_anchor.launch_frame_id, "launch-frame-1");
        assert_eq!(source_anchor.node_attempt, Some(2));
    }

    #[test]
    fn snapshot_includes_resource_diagnostics() {
        let mut input = snapshot_input(AgentRunExecutionState::Idle);
        input.resource_diagnostics = vec![ConversationDiagnosticModel {
            code: "resource_surface_lifecycle_mount_missing".to_string(),
            severity: ValidationSeverityModel::Error,
            message: "missing lifecycle mount".to_string(),
            detail: None,
        }];

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        assert!(snapshot.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "resource_surface_lifecycle_mount_missing"
                && diagnostic.severity == ValidationSeverityModel::Error
        }));
    }

    #[test]
    fn open_companion_and_human_gates_are_projected_as_waiting_items() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let child_gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            Some(Uuid::new_v4()),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(serde_json::json!({
                "companion_label": "reviewer",
                "summary": "Review the implementation",
                "dispatch_id": "dispatch-1"
            })),
        );
        let human_gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            None,
            "companion_human_request",
            "human-request",
            Some(serde_json::json!({
                "request_type": "approval",
                "payload": {
                    "message": "Approve the release?"
                }
            })),
        );
        let blocking_human_gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            None,
            "companion_wait",
            "human-wait-request",
            Some(serde_json::json!({
                "request_type": "approval",
                "summary": "Waiting for approval"
            })),
        );

        let mut input = snapshot_input(AgentRunExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        });
        input.open_wait_items = vec![
            ConversationWaitingItemModel::from_lifecycle_gate(&child_gate),
            ConversationWaitingItemModel::from_lifecycle_gate(&human_gate),
            ConversationWaitingItemModel::from_lifecycle_gate(&blocking_human_gate),
        ];

        let snapshot = AgentConversationSnapshotResolver::resolve(input);

        assert_eq!(snapshot.mailbox.waiting_items.len(), 3);
        let child_wait = &snapshot.mailbox.waiting_items[0];
        assert_eq!(child_wait.wait_id, child_gate.id.to_string());
        assert_eq!(child_wait.gate_id, child_gate.id.to_string());
        assert_eq!(child_wait.kind, "subagent");
        assert_eq!(child_wait.status, "open");
        assert_eq!(child_wait.correlation_ref.as_deref(), Some("dispatch-1"));
        assert_eq!(child_wait.source_label.as_deref(), Some("reviewer"));
        assert_eq!(
            child_wait.preview.as_deref(),
            Some("Review the implementation")
        );

        let human_wait = &snapshot.mailbox.waiting_items[1];
        assert_eq!(human_wait.kind, "human");
        assert_eq!(human_wait.source_label.as_deref(), Some("approval"));
        assert_eq!(human_wait.preview.as_deref(), Some("Approve the release?"));
        assert!(human_wait.resolved_at.is_none());

        let blocking_human_wait = &snapshot.mailbox.waiting_items[2];
        assert_eq!(blocking_human_wait.kind, "human");
        assert_eq!(
            blocking_human_wait.preview.as_deref(),
            Some("Waiting for approval")
        );
    }

    #[test]
    fn resolved_gate_waiting_item_uses_payload_status() {
        let mut gate = LifecycleGate::open(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            Some(Uuid::new_v4()),
            "companion_wait_follow_up",
            "dispatch-failed",
            Some(serde_json::json!({
                "status": "failed",
                "summary": "provider model unsupported",
                "companion_label": "reviewer"
            })),
        );
        gate.resolve("runtime_terminal");

        let item = ConversationWaitingItemModel::from_lifecycle_gate(&gate);
        let projection = gate.waiting_projection();

        assert_eq!(item.kind, projection.kind);
        assert_eq!(item.status, "failed");
        assert_eq!(item.source_label, projection.source_label);
        assert_eq!(item.preview, projection.preview);
        assert_eq!(projection.kind, "subagent");
        assert_eq!(projection.source_label.as_deref(), Some("reviewer"));
        assert_eq!(
            projection.preview.as_deref(),
            Some("provider model unsupported")
        );
        assert!(item.resolved_at.is_some());
    }
}
