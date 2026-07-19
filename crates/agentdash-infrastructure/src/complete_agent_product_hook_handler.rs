use std::sync::Arc;

use agentdash_agent_runtime_host::{CompleteAgentHookHandler, ResolvedCompleteAgentHookCallback};
use agentdash_agent_service_api::{
    AgentHookAction, AgentHookDecision, AgentHookPoint, AgentHookTiming, AgentHostCallbackError,
    AgentHostCallbackErrorCode,
};
use agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository;
use agentdash_application_hooks::AppExecutionHookProvider;
use agentdash_platform_spi::{
    AgentFrameHookEvaluationQuery, HookControlTarget, HookResolution, HookTrigger,
    RuntimeAdapterProvenance,
};
use async_trait::async_trait;

/// Bridges an admitted Complete Agent callback to the exact Product hook rule pinned in its frame.
pub struct ProductCompleteAgentHookHandler {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    hooks: Arc<AppExecutionHookProvider>,
}

impl ProductCompleteAgentHookHandler {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        hooks: Arc<AppExecutionHookProvider>,
    ) -> Self {
        Self { bindings, hooks }
    }
}

#[async_trait]
impl CompleteAgentHookHandler for ProductCompleteAgentHookHandler {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentHookCallback,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        let binding = self
            .bindings
            .load_product_binding_by_runtime_thread(&callback.context.runtime_thread_id)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| {
                unsupported("Complete Agent callback has no active Product Runtime binding")
            })?;
        let trigger = hook_trigger(callback.invocation.point, callback.invocation.timing)?;
        let tool_name = callback
            .invocation
            .input
            .get("tool")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let definition_id = callback.invocation.definition_id.to_string();
        let resolution = self
            .hooks
            .evaluate_complete_agent_hook(
                &definition_id,
                AgentFrameHookEvaluationQuery {
                    target: HookControlTarget {
                        run_id: binding.target.run_id,
                        agent_id: binding.target.agent_id,
                        frame_id: binding.launch_frame.frame_id,
                    },
                    provenance: RuntimeAdapterProvenance::runtime_thread(
                        callback.context.runtime_thread_id.to_string(),
                        Some(callback.invocation.meta.turn_id.to_string()),
                        format!(
                            "complete_agent_hook:{}",
                            callback.context.service_instance_id
                        ),
                    ),
                    trigger,
                    tool_name,
                    tool_call_id: callback
                        .invocation
                        .meta
                        .item_id
                        .as_ref()
                        .map(ToString::to_string),
                    subagent_type: None,
                    snapshot: None,
                    payload: Some(callback.invocation.input.clone()),
                    token_stats: None,
                },
            )
            .await
            .map_err(|error| internal(error.to_string()))?;
        decision_from_resolution(&callback.invocation.allowed_actions, resolution)
    }
}

fn hook_trigger(
    point: AgentHookPoint,
    timing: AgentHookTiming,
) -> Result<HookTrigger, AgentHostCallbackError> {
    match (point, timing) {
        (AgentHookPoint::BeforeTurn, AgentHookTiming::Before) => Ok(HookTrigger::UserPromptSubmit),
        (AgentHookPoint::AfterTurn, AgentHookTiming::After) => Ok(HookTrigger::AfterTurn),
        (AgentHookPoint::BeforeProviderRequest, AgentHookTiming::Before) => {
            Ok(HookTrigger::BeforeProviderRequest)
        }
        (AgentHookPoint::BeforeTool, AgentHookTiming::Before) => Ok(HookTrigger::BeforeTool),
        (AgentHookPoint::AfterTool, AgentHookTiming::After) => Ok(HookTrigger::AfterTool),
        (AgentHookPoint::BeforeCompaction, AgentHookTiming::Before) => {
            Ok(HookTrigger::BeforeCompact)
        }
        (AgentHookPoint::AfterCompaction, AgentHookTiming::After) => Ok(HookTrigger::AfterCompact),
        (AgentHookPoint::BeforeStop, AgentHookTiming::Before) => Ok(HookTrigger::BeforeStop),
        (AgentHookPoint::AfterItem, AgentHookTiming::After) => Ok(HookTrigger::SessionTerminal),
        _ => Err(unsupported(
            "Complete Agent hook point/timing is not a Product hook boundary",
        )),
    }
}

fn decision_from_resolution(
    allowed: &std::collections::BTreeSet<AgentHookAction>,
    resolution: HookResolution,
) -> Result<AgentHookDecision, AgentHostCallbackError> {
    if resolution
        .diagnostics
        .iter()
        .any(|entry| entry.code == "hook_script_error")
    {
        return Err(internal("Product hook rule evaluation failed"));
    }
    if resolution.approval_request.is_some()
        || resolution.completion.is_some()
        || resolution.refresh_snapshot
        || resolution.pending_advance.is_some()
        || !resolution.pending_execution_log.is_empty()
        || resolution.compaction.is_some()
    {
        return Err(unsupported(
            "Product hook emitted semantics outside the Complete Agent callback decision contract",
        ));
    }

    let mut decisions = Vec::new();
    if let Some(reason) = resolution.block_reason {
        require_action(allowed, AgentHookAction::AllowOrDeny)?;
        decisions.push(AgentHookDecision::Deny { reason });
    }
    if let Some(value) = resolution.rewritten_tool_input {
        if allowed.contains(&AgentHookAction::RewriteInput) {
            decisions.push(AgentHookDecision::ReplaceInput { input: value });
        } else {
            require_action(allowed, AgentHookAction::RewriteResult)?;
            decisions.push(AgentHookDecision::ReplaceResult { result: value });
        }
    }
    if !resolution.injections.is_empty() {
        require_action(allowed, AgentHookAction::AddContext)?;
        decisions.push(AgentHookDecision::AddContext {
            context: serde_json::to_value(resolution.injections)
                .map_err(|error| internal(error.to_string()))?,
        });
    }
    if !resolution.effects.is_empty() {
        require_action(allowed, AgentHookAction::EmitEffect)?;
        decisions.push(AgentHookDecision::EmitEffect {
            effect: serde_json::to_value(resolution.effects)
                .map_err(|error| internal(error.to_string()))?,
        });
    }
    match decisions.len() {
        0 => Ok(AgentHookDecision::Allow),
        1 => Ok(decisions.pop().expect("one Product hook decision")),
        _ => Err(unsupported(
            "Product hook produced multiple simultaneous decisions that cannot be represented exactly",
        )),
    }
}

fn require_action(
    allowed: &std::collections::BTreeSet<AgentHookAction>,
    action: AgentHookAction,
) -> Result<(), AgentHostCallbackError> {
    if allowed.contains(&action) {
        Ok(())
    } else {
        Err(internal(format!(
            "Product hook emitted an action outside its immutable surface: {action:?}"
        )))
    }
}

fn unsupported(message: impl Into<String>) -> AgentHostCallbackError {
    AgentHostCallbackError::new(AgentHostCallbackErrorCode::Unsupported, message, false)
}

fn internal(message: impl Into<String>) -> AgentHostCallbackError {
    AgentHostCallbackError::new(AgentHostCallbackErrorCode::Internal, message, false)
}

fn unavailable(message: impl Into<String>) -> AgentHostCallbackError {
    AgentHostCallbackError::new(AgentHostCallbackErrorCode::Unavailable, message, true)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_platform_spi::{HookEffect, HookInjection};

    use super::*;

    #[test]
    fn empty_non_blocking_resolution_continues_without_fabricating_an_action() {
        assert_eq!(
            decision_from_resolution(
                &BTreeSet::from([AgentHookAction::EmitEffect]),
                HookResolution::default(),
            )
            .unwrap(),
            AgentHookDecision::Allow
        );
    }

    #[test]
    fn exact_block_resolution_maps_to_deny() {
        assert_eq!(
            decision_from_resolution(
                &BTreeSet::from([AgentHookAction::AllowOrDeny]),
                HookResolution {
                    block_reason: Some("policy denied".to_owned()),
                    ..HookResolution::default()
                },
            )
            .unwrap(),
            AgentHookDecision::Deny {
                reason: "policy denied".to_owned()
            }
        );
    }

    #[test]
    fn simultaneous_product_decisions_fail_instead_of_losing_semantics() {
        let error = decision_from_resolution(
            &BTreeSet::from([AgentHookAction::AddContext, AgentHookAction::EmitEffect]),
            HookResolution {
                injections: vec![HookInjection {
                    slot: "constraint".to_owned(),
                    content: "keep exact".to_owned(),
                    source: "test".to_owned(),
                }],
                effects: vec![HookEffect {
                    kind: "test:effect".to_owned(),
                    payload: serde_json::json!({}),
                    presentation: None,
                }],
                ..HookResolution::default()
            },
        )
        .expect_err("one callback decision cannot erase either Product result");

        assert_eq!(error.code, AgentHostCallbackErrorCode::Unsupported);
    }
}
