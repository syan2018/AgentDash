use agentdash_domain::agent::ProjectAgent;
use agentdash_spi::{AgentConfig, ThinkingLevel};

use crate::error::WorkflowApplicationError;

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
pub struct ConversationModelConfigModel {
    pub status: ConversationModelConfigStatusModel,
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigModel>,
    pub missing_fields: Vec<String>,
    pub message: Option<String>,
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
