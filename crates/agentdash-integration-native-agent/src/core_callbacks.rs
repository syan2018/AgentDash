use std::sync::Arc;

use agentdash_agent::dash::{
    DashBeforeToolDecision, DashCoreError, DashToolCall, DashToolCallbacks, DashToolResult,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentEffectIdentity, AgentHookAction,
    AgentHookDecision, AgentHookDefinitionId, AgentHookInvocation, AgentHookPoint, AgentHookTiming,
    AgentHostCallbackMeta, AgentHostCallbacks, AgentIdempotencyKey, AgentItemId,
    AgentSourceCoordinate, AgentSurfaceContributionPayload, AgentToolInvocation, AgentToolName,
    AgentToolResult, AgentTurnId, BoundAgentSurface,
};
use async_trait::async_trait;
use serde_json::json;

#[derive(Clone)]
struct DashHookBinding {
    definition_id: AgentHookDefinitionId,
    point: AgentHookPoint,
    timing: AgentHookTiming,
    actions: std::collections::BTreeSet<AgentHookAction>,
    deadline_ms: u64,
}

pub struct DashAgentCoreToolCallbacks {
    callbacks: Arc<dyn AgentHostCallbacks>,
    route_id: AgentCallbackRouteId,
    binding_generation: AgentBindingGeneration,
    source: AgentSourceCoordinate,
    deadline: DashCallbackDeadline,
    hooks: Vec<DashHookBinding>,
}

enum DashCallbackDeadline {
    Absolute(u64),
    FromInvocation(u64),
}

impl DashAgentCoreToolCallbacks {
    pub fn new(
        callbacks: Arc<dyn AgentHostCallbacks>,
        route_id: AgentCallbackRouteId,
        binding_generation: AgentBindingGeneration,
        source: AgentSourceCoordinate,
        deadline_at_ms: u64,
    ) -> Self {
        Self {
            callbacks,
            route_id,
            binding_generation,
            source,
            deadline: DashCallbackDeadline::Absolute(deadline_at_ms),
            hooks: Vec::new(),
        }
    }

    pub fn from_bound_surface(
        callbacks: Arc<dyn AgentHostCallbacks>,
        route_id: AgentCallbackRouteId,
        binding_generation: AgentBindingGeneration,
        source: AgentSourceCoordinate,
        default_deadline_ms: u64,
        surface: &BoundAgentSurface,
    ) -> Self {
        let hooks = surface
            .contributions
            .iter()
            .filter_map(|contribution| match &contribution.payload {
                AgentSurfaceContributionPayload::Hook {
                    definition_id,
                    point,
                    timing,
                    actions,
                    deadline_ms,
                } => Some(DashHookBinding {
                    definition_id: definition_id.clone(),
                    point: *point,
                    timing: *timing,
                    actions: actions.clone(),
                    deadline_ms: *deadline_ms,
                }),
                _ => None,
            })
            .collect();
        Self {
            callbacks,
            route_id,
            binding_generation,
            source,
            deadline: DashCallbackDeadline::FromInvocation(default_deadline_ms),
            hooks,
        }
    }

    fn deadline_at_ms(&self, hook_deadline_ms: Option<u64>) -> u64 {
        match self.deadline {
            DashCallbackDeadline::Absolute(deadline_at_ms) => deadline_at_ms,
            DashCallbackDeadline::FromInvocation(duration_ms) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                now.saturating_add(hook_deadline_ms.unwrap_or(duration_ms).min(duration_ms))
            }
        }
    }

    fn callback_meta(
        &self,
        turn_id: &agentdash_agent::dash::AgentTurnId,
        call_id: &str,
        effect_id: String,
        deadline_ms: Option<u64>,
    ) -> Result<AgentHostCallbackMeta, DashCoreError> {
        Ok(AgentHostCallbackMeta {
            route_id: self.route_id.clone(),
            binding_generation: self.binding_generation,
            source: self.source.clone(),
            turn_id: AgentTurnId::new(turn_id.0.clone()).map_err(callback_error)?,
            item_id: Some(AgentItemId::new(call_id).map_err(callback_error)?),
            interaction_id: None,
            effect_id: AgentEffectIdentity::new(effect_id.clone()).map_err(callback_error)?,
            idempotency_key: AgentIdempotencyKey::new(effect_id).map_err(callback_error)?,
            deadline_at_ms: self.deadline_at_ms(deadline_ms),
        })
    }

    async fn invoke_hook(
        &self,
        turn_id: &agentdash_agent::dash::AgentTurnId,
        call: &DashToolCall,
        binding: &DashHookBinding,
        input: serde_json::Value,
    ) -> Result<AgentHookDecision, DashCoreError> {
        let point = match binding.point {
            AgentHookPoint::BeforeTool => "before",
            AgentHookPoint::AfterTool => "after",
            _ => "hook",
        };
        self.callbacks
            .invoke_hook(AgentHookInvocation {
                meta: self.callback_meta(
                    turn_id,
                    &call.call_id,
                    format!(
                        "hook:{point}:{}:{}",
                        binding.definition_id.as_str(),
                        call.call_id
                    ),
                    Some(binding.deadline_ms),
                )?,
                definition_id: binding.definition_id.clone(),
                point: binding.point,
                timing: binding.timing,
                allowed_actions: binding.actions.clone(),
                input,
            })
            .await
            .map_err(callback_error)
    }
}

#[async_trait]
impl DashToolCallbacks for DashAgentCoreToolCallbacks {
    async fn before_tool(
        &self,
        turn_id: &agentdash_agent::dash::AgentTurnId,
        mut call: DashToolCall,
    ) -> Result<DashBeforeToolDecision, DashCoreError> {
        for hook in self.hooks.iter().filter(|hook| {
            hook.point == AgentHookPoint::BeforeTool && hook.timing == AgentHookTiming::Before
        }) {
            let decision = self
                .invoke_hook(
                    turn_id,
                    &call,
                    hook,
                    json!({"tool": call.name, "arguments": call.arguments}),
                )
                .await?;
            match decision {
                AgentHookDecision::Allow => {}
                AgentHookDecision::Deny { reason } => {
                    return Ok(DashBeforeToolDecision::Deny {
                        result: DashToolResult {
                            call_id: call.call_id,
                            content: json!({"code": "hook_denied", "message": reason}).to_string(),
                            is_error: true,
                        },
                    });
                }
                AgentHookDecision::ReplaceInput { input }
                    if hook.actions.contains(&AgentHookAction::RewriteInput) =>
                {
                    call.arguments = input.get("arguments").cloned().unwrap_or(input);
                }
                other => return Err(unsupported_hook_decision(other)),
            }
        }
        Ok(DashBeforeToolDecision::Invoke { call })
    }

    async fn invoke(
        &self,
        turn_id: &agentdash_agent::dash::AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        let item_id =
            AgentItemId::new(call.call_id.clone()).map_err(|error| DashCoreError::Tool {
                message: error.to_string(),
                retryable: false,
            })?;
        let invocation = AgentToolInvocation {
            meta: AgentHostCallbackMeta {
                route_id: self.route_id.clone(),
                binding_generation: self.binding_generation,
                source: self.source.clone(),
                turn_id: AgentTurnId::new(turn_id.0.clone()).map_err(|error| {
                    DashCoreError::Tool {
                        message: error.to_string(),
                        retryable: false,
                    }
                })?,
                item_id: Some(item_id),
                interaction_id: None,
                effect_id: AgentEffectIdentity::new(format!("tool:{}", call.call_id)).map_err(
                    |error| DashCoreError::Tool {
                        message: error.to_string(),
                        retryable: false,
                    },
                )?,
                idempotency_key: AgentIdempotencyKey::new(format!("tool:{}", call.call_id))
                    .map_err(|error| DashCoreError::Tool {
                        message: error.to_string(),
                        retryable: false,
                    })?,
                deadline_at_ms: self.deadline_at_ms(None),
            },
            tool: AgentToolName::new(call.name).map_err(|error| DashCoreError::Tool {
                message: error.to_string(),
                retryable: false,
            })?,
            arguments: call.arguments,
        };
        let result = self
            .callbacks
            .invoke_tool(invocation)
            .await
            .map_err(|error| DashCoreError::Tool {
                message: error.to_string(),
                retryable: false,
            })?;
        match result {
            AgentToolResult::Completed { output } => Ok(DashToolResult {
                call_id: call.call_id,
                content: output.to_string(),
                is_error: false,
            }),
            AgentToolResult::Rejected { code, message }
            | AgentToolResult::Failed { code, message } => Ok(DashToolResult {
                call_id: call.call_id,
                content: serde_json::json!({"code": code, "message": message}).to_string(),
                is_error: true,
            }),
        }
    }

    async fn after_tool(
        &self,
        turn_id: &agentdash_agent::dash::AgentTurnId,
        call: &DashToolCall,
        mut result: DashToolResult,
    ) -> Result<DashToolResult, DashCoreError> {
        for hook in self.hooks.iter().filter(|hook| {
            hook.point == AgentHookPoint::AfterTool && hook.timing == AgentHookTiming::After
        }) {
            let decision = self
                .invoke_hook(
                    turn_id,
                    call,
                    hook,
                    json!({
                        "tool": call.name,
                        "arguments": call.arguments,
                        "result": {
                            "content": result.content,
                            "is_error": result.is_error,
                        }
                    }),
                )
                .await?;
            match decision {
                AgentHookDecision::Allow => {}
                AgentHookDecision::ReplaceResult {
                    result: replacement,
                } if hook.actions.contains(&AgentHookAction::RewriteResult) => {
                    result.content = replacement
                        .get("content")
                        .map(|value| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string())
                        })
                        .unwrap_or_else(|| replacement.to_string());
                    result.is_error = replacement
                        .get("is_error")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(result.is_error);
                }
                other => return Err(unsupported_hook_decision(other)),
            }
        }
        Ok(result)
    }
}

fn callback_error(error: impl std::fmt::Display) -> DashCoreError {
    DashCoreError::Tool {
        message: error.to_string(),
        retryable: false,
    }
}

fn unsupported_hook_decision(decision: AgentHookDecision) -> DashCoreError {
    DashCoreError::Tool {
        message: format!("host returned unsupported hook decision: {decision:?}"),
        retryable: false,
    }
}
