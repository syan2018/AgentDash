use agentdash_domain::{
    routine::{DispatchStrategy, Routine, RoutineExecution, RoutineExecutionStatus},
    workflow::{AgentRuntimeRefs, OrchestrationBindingRefs},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RoutineCreationResponse {
    #[serde(flatten)]
    pub routine: RoutineResponse,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub webhook_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RegenerateTokenResponse {
    pub endpoint_id: String,
    pub webhook_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RoutineResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub prompt_template: String,
    pub project_agent_id: String,
    pub trigger_config: RoutineTriggerConfigResponse,
    pub dispatch_strategy: RoutineDispatchStrategyDto,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_fired_at: Option<String>,
}

impl From<Routine> for RoutineResponse {
    fn from(routine: Routine) -> Self {
        Self {
            id: routine.id.to_string(),
            project_id: routine.project_id.to_string(),
            name: routine.name,
            prompt_template: routine.prompt_template,
            project_agent_id: routine.project_agent_id.to_string(),
            trigger_config: routine.trigger_config.into(),
            dispatch_strategy: routine.dispatch_strategy.into(),
            enabled: routine.enabled,
            created_at: routine.created_at.to_rfc3339(),
            updated_at: routine.updated_at.to_rfc3339(),
            last_fired_at: routine.last_fired_at.map(|time| time.to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RoutineExecutionResponse {
    pub id: String,
    pub routine_id: String,
    pub trigger_source: String,
    pub trigger_payload: Option<Value>,
    pub resolved_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_refs: Option<RoutineAgentRuntimeRefsDto>,
    pub status: RoutineExecutionStatusDto,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub entity_key: Option<String>,
}

impl From<RoutineExecution> for RoutineExecutionResponse {
    fn from(execution: RoutineExecution) -> Self {
        Self {
            id: execution.id.to_string(),
            routine_id: execution.routine_id.to_string(),
            trigger_source: execution.trigger_source,
            trigger_payload: execution.trigger_payload,
            resolved_prompt: execution.resolved_prompt,
            runtime_refs: execution.dispatch_refs.map(|refs| refs.runtime_refs.into()),
            status: execution.status.into(),
            started_at: execution.started_at.to_rfc3339(),
            completed_at: execution.completed_at.map(|time| time.to_rfc3339()),
            error: execution.error,
            entity_key: execution.entity_key,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateRoutineRequest {
    pub name: String,
    pub prompt_template: String,
    pub project_agent_id: String,
    pub trigger_config: RoutineTriggerConfigRequest,
    #[serde(default)]
    #[ts(optional)]
    pub dispatch_strategy: Option<RoutineDispatchStrategyDto>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct UpdateRoutineRequest {
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub prompt_template: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub project_agent_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub trigger_config: Option<RoutineTriggerConfigRequest>,
    #[serde(default)]
    #[ts(optional)]
    pub dispatch_strategy: Option<RoutineDispatchStrategyDto>,
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct EnableRoutineRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct FireWebhookRequest {
    #[serde(default)]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct ListExecutionsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineTriggerConfigRequest {
    Scheduled {
        cron_expression: String,
        #[serde(default)]
        #[ts(optional)]
        timezone: Option<String>,
    },
    Webhook {},
    Plugin {
        provider_key: String,
        #[serde(default)]
        provider_config: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineTriggerConfigResponse {
    Scheduled {
        cron_expression: String,
        #[serde(default)]
        #[ts(optional)]
        timezone: Option<String>,
    },
    Webhook {
        endpoint_id: String,
    },
    Plugin {
        provider_key: String,
        #[serde(default)]
        provider_config: Value,
    },
}

impl From<agentdash_domain::routine::RoutineTriggerConfig> for RoutineTriggerConfigResponse {
    fn from(config: agentdash_domain::routine::RoutineTriggerConfig) -> Self {
        match config {
            agentdash_domain::routine::RoutineTriggerConfig::Scheduled {
                cron_expression,
                timezone,
            } => Self::Scheduled {
                cron_expression,
                timezone,
            },
            agentdash_domain::routine::RoutineTriggerConfig::Webhook { endpoint_id, .. } => {
                Self::Webhook { endpoint_id }
            }
            agentdash_domain::routine::RoutineTriggerConfig::Plugin {
                provider_key,
                provider_config,
            } => Self::Plugin {
                provider_key,
                provider_config,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RoutineDispatchStrategyDto {
    Fresh,
    Reuse,
    PerEntity { entity_key_path: String },
}

impl From<DispatchStrategy> for RoutineDispatchStrategyDto {
    fn from(strategy: DispatchStrategy) -> Self {
        match strategy {
            DispatchStrategy::Fresh => Self::Fresh,
            DispatchStrategy::Reuse => Self::Reuse,
            DispatchStrategy::PerEntity { entity_key_path } => Self::PerEntity { entity_key_path },
        }
    }
}

impl From<RoutineDispatchStrategyDto> for DispatchStrategy {
    fn from(strategy: RoutineDispatchStrategyDto) -> Self {
        match strategy {
            RoutineDispatchStrategyDto::Fresh => Self::Fresh,
            RoutineDispatchStrategyDto::Reuse => Self::Reuse,
            RoutineDispatchStrategyDto::PerEntity { entity_key_path } => {
                Self::PerEntity { entity_key_path }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutineExecutionStatusDto {
    Pending,
    Dispatched,
    Failed,
    Skipped,
}

impl From<RoutineExecutionStatus> for RoutineExecutionStatusDto {
    fn from(status: RoutineExecutionStatus) -> Self {
        match status {
            RoutineExecutionStatus::Pending => Self::Pending,
            RoutineExecutionStatus::Dispatched => Self::Dispatched,
            RoutineExecutionStatus::Failed => Self::Failed,
            RoutineExecutionStatus::Skipped => Self::Skipped,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct RoutineAgentRuntimeRefsDto {
    pub run_ref: String,
    pub agent_ref: String,
    pub frame_ref: String,
    pub orchestration_binding: Option<RoutineOrchestrationBindingRefsDto>,
}

impl From<AgentRuntimeRefs> for RoutineAgentRuntimeRefsDto {
    fn from(refs: AgentRuntimeRefs) -> Self {
        Self {
            run_ref: refs.run_ref.to_string(),
            agent_ref: refs.agent_ref.to_string(),
            frame_ref: refs.frame_ref.to_string(),
            orchestration_binding: refs.orchestration_binding.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct RoutineOrchestrationBindingRefsDto {
    pub orchestration_ref: String,
    pub node_path: String,
    pub attempt: u32,
}

impl From<OrchestrationBindingRefs> for RoutineOrchestrationBindingRefsDto {
    fn from(refs: OrchestrationBindingRefs) -> Self {
        Self {
            orchestration_ref: refs.orchestration_ref.to_string(),
            node_path: refs.node_path,
            attempt: refs.attempt,
        }
    }
}
