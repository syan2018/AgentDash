use agentdash_agent_service_api::{
    AgentHookAction, AgentHookDecision, AgentHostCallbackError, AgentHostCallbackErrorCode,
};
use async_trait::async_trait;

use crate::{CompleteAgentHookHandler, ResolvedCompleteAgentHookCallback};

/// Final platform Hook handler for policy gates.
///
/// Hook definitions remain immutable Complete-Agent surface contributions. This handler executes
/// the final allow/deny semantic route without reintroducing the legacy Hook runtime/session SPI.
/// Rewrite, context and effect actions require their own Product-owned handlers and therefore
/// never receive a fabricated success here.
#[derive(Debug, Default)]
pub struct RuntimePlatformHookHandler;

impl RuntimePlatformHookHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompleteAgentHookHandler for RuntimePlatformHookHandler {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentHookCallback,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        if callback
            .invocation
            .allowed_actions
            .contains(&AgentHookAction::AllowOrDeny)
        {
            return Ok(AgentHookDecision::Allow);
        }
        Err(AgentHostCallbackError::new(
            AgentHostCallbackErrorCode::Unsupported,
            "bound Hook has no executable allow/deny action; a Product semantic handler is required",
            false,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentEffectIdentity, AgentHookDefinitionId, AgentHookInvocation,
        AgentHookPoint, AgentHookTiming, AgentHostCallbackMeta, AgentIdempotencyKey,
        AgentProfileDigest, AgentServiceInstanceId, AgentSourceCoordinate, AgentSurfaceDigest,
        AgentSurfaceRevision, AgentTurnId,
    };

    use super::*;
    use crate::{CompleteAgentBindingId, ResolvedCompleteAgentCallbackContext};

    #[tokio::test]
    async fn allow_or_deny_hook_uses_final_handler() {
        let decision = RuntimePlatformHookHandler::new()
            .invoke(callback(BTreeSet::from([AgentHookAction::AllowOrDeny])))
            .await
            .expect("allow/deny hook");
        assert_eq!(decision, AgentHookDecision::Allow);
    }

    #[tokio::test]
    async fn mutation_only_hook_is_not_fabricated() {
        let error = RuntimePlatformHookHandler::new()
            .invoke(callback(BTreeSet::from([AgentHookAction::EmitEffect])))
            .await
            .expect_err("effect hook requires Product handler");
        assert_eq!(error.code, AgentHostCallbackErrorCode::Unsupported);
    }

    fn callback(allowed_actions: BTreeSet<AgentHookAction>) -> ResolvedCompleteAgentHookCallback {
        ResolvedCompleteAgentHookCallback {
            context: ResolvedCompleteAgentCallbackContext {
                runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
                binding_id: CompleteAgentBindingId::new("binding-test").unwrap(),
                binding_generation: AgentBindingGeneration(1),
                source: AgentSourceCoordinate::new("source-test").unwrap(),
                service_instance_id: AgentServiceInstanceId::new("service-test").unwrap(),
                profile_digest: AgentProfileDigest::new("profile-test").unwrap(),
                bound_surface_revision: AgentSurfaceRevision(1),
                bound_surface_digest: AgentSurfaceDigest::new("bound-test").unwrap(),
                bound_surface_offer_profile_digest: AgentProfileDigest::new("profile-test")
                    .unwrap(),
                applied_surface_revision: AgentSurfaceRevision(1),
                applied_surface_digest: AgentSurfaceDigest::new("applied-test").unwrap(),
            },
            invocation: AgentHookInvocation {
                meta: AgentHostCallbackMeta {
                    route_id: "route-test".parse().unwrap(),
                    binding_generation: AgentBindingGeneration(1),
                    source: AgentSourceCoordinate::new("source-test").unwrap(),
                    turn_id: AgentTurnId::new("turn-test").unwrap(),
                    item_id: None,
                    interaction_id: None,
                    effect_id: AgentEffectIdentity::new("effect-test").unwrap(),
                    idempotency_key: AgentIdempotencyKey::new("hook-test").unwrap(),
                    deadline_at_ms: u64::MAX,
                },
                definition_id: AgentHookDefinitionId::new("hook-test").unwrap(),
                point: AgentHookPoint::BeforeTool,
                timing: AgentHookTiming::Before,
                allowed_actions,
                input: serde_json::json!({}),
            },
        }
    }
}
