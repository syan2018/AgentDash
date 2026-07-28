use uuid::Uuid;

use agentdash_application_vfs::ResolvedVfsSurface;
use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::workflow::LifecycleGate;
use agentdash_platform_spi::{AgentConfig, ThinkingLevel};

use crate::agent_run::lifecycle_read_model_facade::LifecycleSubjectAssociationView;
use crate::agent_run::workspace::types::AgentRunResourceSurfaceCoordinateModel;
use crate::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct AgentConversationSnapshotModel {
    pub identity: AgentConversationIdentityModel,
    pub lifecycle_context: AgentConversationLifecycleContextModel,
    pub model_config: ConversationModelConfigModel,
    pub commands: ConversationCommandSetModel,
    pub waiting_items: Vec<ConversationWaitingItemModel>,
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
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConversationFrameRefModel {
    pub agent_id: String,
    pub frame_id: String,
    pub revision: Option<i32>,
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
    Cancel,
    CompactContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationCommandPlacementModel {
    ComposerPrimary,
    ComposerSecondary,
    Header,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCommandModel {
    pub kind: ConversationCommandKindModel,
    pub command_id: String,
    pub shortcut: Option<String>,
    pub requires_input: bool,
    pub executor_config_policy: String,
    pub placement: Vec<ConversationCommandPlacementModel>,
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
        let message = (status == ConversationModelConfigStatusModel::ModelRequired)
            .then(|| model_required_message(&config, &missing_fields));
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
    format!(
        "执行器 {} 缺少必需模型配置: {}。请先选择 provider 和 model。",
        config.executor,
        missing_fields.join(", ")
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
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
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
        let commands = conversation_commands(input.ownership.clone());
        let diagnostics = conversation_diagnostics(&input.model_config, input.resource_diagnostics);

        AgentConversationSnapshotModel {
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
                subject_associations: input.subject_associations,
            },
            model_config: input.model_config,
            commands,
            waiting_items: input.open_wait_items,
            resource_surface: input.resource_surface,
            resource_surface_coordinate: input.resource_surface_coordinate,
            diagnostics,
        }
    }
}

fn conversation_commands(ownership: AgentRunOwnershipModel) -> ConversationCommandSetModel {
    let submit_id = conversation_command_id_for(ConversationCommandKindModel::SubmitMessage);
    ConversationCommandSetModel {
        ownership,
        commands: vec![
            command_binding(
                ConversationCommandKindModel::SubmitMessage,
                Some("enter"),
                true,
                "allowed",
                vec![ConversationCommandPlacementModel::ComposerPrimary],
            ),
            command_binding(
                ConversationCommandKindModel::Cancel,
                None,
                false,
                "ignored",
                vec![ConversationCommandPlacementModel::Header],
            ),
            command_binding(
                ConversationCommandKindModel::CompactContext,
                None,
                false,
                "ignored",
                vec![ConversationCommandPlacementModel::Header],
            ),
        ],
        keyboard: ConversationKeyboardMapModel {
            enter: Some(submit_id.to_string()),
            ctrl_enter: Some(submit_id.to_string()),
        },
    }
}

fn command_binding(
    kind: ConversationCommandKindModel,
    shortcut: Option<&str>,
    requires_input: bool,
    executor_config_policy: &str,
    placement: Vec<ConversationCommandPlacementModel>,
) -> ConversationCommandModel {
    ConversationCommandModel {
        kind,
        command_id: conversation_command_id_for(kind).to_string(),
        shortcut: shortcut.map(str::to_string),
        requires_input,
        executor_config_policy: executor_config_policy.to_string(),
        placement,
    }
}

pub fn conversation_command_id_for(kind: ConversationCommandKindModel) -> &'static str {
    match kind {
        ConversationCommandKindModel::SubmitMessage => "submit_message",
        ConversationCommandKindModel::Cancel => "cancel",
        ConversationCommandKindModel::CompactContext => "compact_context",
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

    fn resolved_model_config() -> ConversationModelConfigModel {
        ConversationModelConfigModel {
            status: ConversationModelConfigStatusModel::Resolved,
            effective_executor_config: None,
            missing_fields: Vec::new(),
            message: None,
        }
    }

    fn snapshot_input() -> AgentConversationSnapshotInput {
        AgentConversationSnapshotInput {
            project_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_ref: Some((Uuid::new_v4(), 2)),
            subject_associations: Vec::new(),
            open_wait_items: Vec::new(),
            resource_surface: None,
            resource_surface_coordinate: None,
            resource_diagnostics: Vec::new(),
            model_config: resolved_model_config(),
            ownership: AgentRunOwnershipModel::from_owner_fields(
                "owner",
                "agent-owner",
                Some("owner"),
            ),
        }
    }

    #[test]
    fn ownership_model_marks_only_run_owner_as_controller() {
        let owner = AgentRunOwnershipModel::from_owner_fields(
            "run-owner",
            "agent-owner",
            Some("run-owner"),
        );
        let viewer =
            AgentRunOwnershipModel::from_owner_fields("run-owner", "agent-owner", Some("viewer"));
        assert!(owner.current_user_controls_run);
        assert!(!viewer.current_user_controls_run);
    }

    #[test]
    fn cloud_native_without_model_is_model_required() {
        let config = AgentConfig::new("PI_AGENT");
        let resolution = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: Some(&config),
            ..Default::default()
        });
        assert_eq!(
            resolution.view.status,
            ConversationModelConfigStatusModel::ModelRequired
        );
        assert_eq!(
            resolution.view.missing_fields,
            vec!["provider_id", "model_id"]
        );
    }

    #[test]
    fn executor_override_preserves_unspecified_model_coordinates() {
        let base = AgentConfig {
            executor: "CODEX".to_string(),
            provider_id: Some("openai".to_string()),
            model_id: Some("gpt-5".to_string()),
            ..Default::default()
        };
        let override_config = AgentConfig {
            executor: "CLAUDE_CODE".to_string(),
            ..Default::default()
        };
        let merged = merge_executor_config_fields(base, &override_config);
        assert_eq!(merged.executor, "CLAUDE_CODE");
        assert_eq!(merged.provider_id.as_deref(), Some("openai"));
        assert_eq!(merged.model_id.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn workspace_conversation_exposes_only_static_runtime_command_bindings() {
        let snapshot = AgentConversationSnapshotResolver::resolve(snapshot_input());
        assert_eq!(
            snapshot
                .commands
                .commands
                .iter()
                .map(|command| command.kind)
                .collect::<Vec<_>>(),
            vec![
                ConversationCommandKindModel::SubmitMessage,
                ConversationCommandKindModel::Cancel,
                ConversationCommandKindModel::CompactContext,
            ]
        );
        assert_eq!(
            snapshot.commands.keyboard.enter.as_deref(),
            Some("submit_message")
        );
    }

    #[test]
    fn model_required_diagnostic_remains_a_product_fact() {
        let mut input = snapshot_input();
        input.model_config = ConversationModelConfigModel {
            status: ConversationModelConfigStatusModel::ModelRequired,
            effective_executor_config: None,
            missing_fields: vec!["model_id".to_string()],
            message: Some("请选择模型".to_string()),
        };
        let snapshot = AgentConversationSnapshotResolver::resolve(input);
        assert_eq!(snapshot.diagnostics.len(), 1);
        assert_eq!(snapshot.diagnostics[0].code, "model_required");
    }
}
