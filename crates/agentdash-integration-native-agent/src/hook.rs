use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverThreadId, DriverTurnId, HookPoint, RuntimeBindingId,
    RuntimeDriverGeneration, RuntimeItemId, RuntimeThreadId, RuntimeTurnId,
};
use agentdash_agent_types::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage,
    AgentRuntimeDelegateSet, AgentRuntimeError, BeforeProviderRequestInput, BeforeStopInput,
    BeforeToolCallInput, ContentPart, RuntimeProviderObserverDelegate, RuntimeToolPolicyDelegate,
    RuntimeTurnBoundaryDelegate, StopDecision, ToolCallDecision, TurnControlDecision,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_integration_api::{
    AgentRuntimeHookCallback, AuthIdentity, DriverHookBinding, DriverHookDecision,
    DriverHookInvocation, DriverHookSurface,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::context::NativeBindingContext;

pub(crate) struct NativeHookDelegate {
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    source_thread_id: DriverThreadId,
    runtime_thread_id: RuntimeThreadId,
    authorization_identity: Option<AuthIdentity>,
    active_turn: Arc<RwLock<Option<DriverTurnId>>>,
    active_runtime_turn: Arc<RwLock<Option<RuntimeTurnId>>>,
    surface: DriverHookSurface,
    callback: Arc<dyn AgentRuntimeHookCallback>,
}

impl NativeHookDelegate {
    pub(crate) fn delegates(
        binding: NativeBindingContext,
        active_turn: Arc<RwLock<Option<DriverTurnId>>>,
        active_runtime_turn: Arc<RwLock<Option<RuntimeTurnId>>>,
        surface: DriverHookSurface,
        callback: Arc<dyn AgentRuntimeHookCallback>,
    ) -> AgentRuntimeDelegateSet {
        let delegate = Arc::new(Self {
            binding_id: binding.binding_id,
            generation: binding.generation,
            source_thread_id: binding.source_thread_id,
            runtime_thread_id: binding.runtime_thread_id,
            authorization_identity: binding.authorization_identity,
            active_turn,
            active_runtime_turn,
            surface,
            callback,
        });
        AgentRuntimeDelegateSet::new()
            .with_tool_policy(Some(delegate.clone()))
            .with_turn_boundary(Some(delegate.clone()))
            .with_provider_observer(Some(delegate))
    }

    async fn execute(
        &self,
        point: HookPoint,
        source_item_id: Option<DriverItemId>,
        mut payload: serde_json::Value,
    ) -> Result<DriverHookDecision, AgentRuntimeError> {
        for binding in self
            .surface
            .bindings
            .iter()
            .filter(|binding| binding.point == point && supported_hook(binding))
        {
            let source_turn_id = self.active_turn.read().await.clone();
            let turn_id = self.active_runtime_turn.read().await.clone();
            let item_id = source_item_id
                .as_ref()
                .and_then(|value| RuntimeItemId::new(value.to_string()).ok());
            let decision = match self
                .callback
                .execute(DriverHookInvocation {
                    thread_id: self.runtime_thread_id.clone(),
                    turn_id,
                    item_id,
                    binding_id: self.binding_id.clone(),
                    generation: self.generation,
                    hook_plan_revision: self.surface.revision,
                    hook_plan_digest: self.surface.digest.clone(),
                    source_thread_id: self.source_thread_id.clone(),
                    source_turn_id,
                    source_item_id: source_item_id.clone(),
                    definition_id: binding.definition_id.clone(),
                    point,
                    payload: payload.clone(),
                    authorization_identity: self.authorization_identity.clone(),
                })
                .await
            {
                Ok(decision) => decision,
                Err(error)
                    if matches!(
                        binding.failure_policy,
                        agentdash_agent_runtime_contract::HookFailurePolicy::FailOpenWithDiagnostic
                            | agentdash_agent_runtime_contract::HookFailurePolicy::ObserveOnly
                    ) =>
                {
                    diag!(
                        Warn,
                        Subsystem::Hooks,
                        hook_definition_id = %binding.definition_id,
                        hook_point = ?binding.point,
                        error = %error,
                        "native hook callback failed open according to the bound HookPlan"
                    );
                    continue;
                }
                Err(error) => return Err(AgentRuntimeError::Runtime(error.to_string())),
            };
            match decision {
                DriverHookDecision::Continue { payload: next } => payload = next,
                terminal => return Ok(terminal),
            }
        }
        Ok(DriverHookDecision::Continue { payload })
    }
}

#[async_trait]
impl RuntimeProviderObserverDelegate for NativeHookDelegate {
    async fn on_before_provider_request(
        &self,
        input: BeforeProviderRequestInput,
        _cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        match self
            .execute(
                HookPoint::BeforeProviderRequest,
                None,
                json!({
                    "system_prompt_len": input.system_prompt_len,
                    "message_count": input.message_count,
                    "tool_count": input.tool_count,
                    "estimated_input_tokens": input.estimated_input_tokens,
                    "context_window": input.context_window,
                    "reserve_tokens": input.reserve_tokens,
                }),
            )
            .await?
        {
            DriverHookDecision::Continue { .. } => Ok(()),
            DriverHookDecision::Block { reason }
            | DriverHookDecision::InteractionRequired { reason, .. } => {
                Err(AgentRuntimeError::Runtime(reason))
            }
        }
    }
}

#[async_trait]
impl RuntimeToolPolicyDelegate for NativeHookDelegate {
    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        let item_id = input
            .tool_call
            .id
            .parse::<DriverItemId>()
            .map_err(|error| {
                AgentRuntimeError::Runtime(format!("invalid tool item identity: {error}"))
            })?;
        match self
            .execute(
                HookPoint::BeforeTool,
                Some(item_id),
                json!({
                    "tool_name": input.tool_call.name,
                    "arguments": input.args,
                }),
            )
            .await?
        {
            DriverHookDecision::Continue { payload } => {
                if let Some(arguments) = payload.get("arguments") {
                    Ok(ToolCallDecision::Rewrite {
                        args: arguments.clone(),
                        note: Some("rewritten by managed Runtime HookPlan".to_string()),
                    })
                } else {
                    Ok(ToolCallDecision::Allow)
                }
            }
            DriverHookDecision::Block { reason } => Ok(ToolCallDecision::Deny { reason }),
            DriverHookDecision::InteractionRequired { reason, .. } => Ok(ToolCallDecision::Ask {
                reason,
                args: None,
                details: None,
            }),
        }
    }

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
        let item_id = input
            .tool_call
            .id
            .parse::<DriverItemId>()
            .map_err(|error| {
                AgentRuntimeError::Runtime(format!("invalid tool item identity: {error}"))
            })?;
        match self
            .execute(
                HookPoint::AfterTool,
                Some(item_id),
                json!({
                    "tool_name": input.tool_call.name,
                    "arguments": input.args,
                    "result": input.result.details,
                    "is_error": input.is_error,
                }),
            )
            .await?
        {
            DriverHookDecision::Continue { payload } => {
                let output = payload.get("result").cloned();
                Ok(AfterToolCallEffects {
                    content: output
                        .as_ref()
                        .map(|value| vec![ContentPart::text(value.to_string())]),
                    details: output,
                    is_error: payload.get("is_error").and_then(|value| value.as_bool()),
                    ..Default::default()
                })
            }
            DriverHookDecision::Block { reason }
            | DriverHookDecision::InteractionRequired { reason, .. } => {
                Err(AgentRuntimeError::Runtime(reason))
            }
        }
    }
}

#[async_trait]
impl RuntimeTurnBoundaryDelegate for NativeHookDelegate {
    async fn after_turn(
        &self,
        input: AfterTurnInput,
        _cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError> {
        match self
            .execute(
                HookPoint::AfterTurn,
                None,
                json!({
                    "message": input.message.first_text(),
                    "tool_result_count": input.tool_results.len(),
                }),
            )
            .await?
        {
            DriverHookDecision::Continue { payload } => Ok(TurnControlDecision {
                steering: messages_from_payload(&payload, "steering"),
                follow_up: messages_from_payload(&payload, "follow_up"),
                ..Default::default()
            }),
            DriverHookDecision::Block { reason }
            | DriverHookDecision::InteractionRequired { reason, .. } => {
                Err(AgentRuntimeError::Runtime(reason))
            }
        }
    }

    async fn before_stop(
        &self,
        _input: BeforeStopInput,
        _cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError> {
        match self.execute(HookPoint::BeforeStop, None, json!({})).await? {
            DriverHookDecision::Continue { payload } => {
                let steering = messages_from_payload(&payload, "steering");
                let follow_up = messages_from_payload(&payload, "follow_up");
                if steering.is_empty() && follow_up.is_empty() {
                    Ok(StopDecision::Stop)
                } else {
                    Ok(StopDecision::Continue {
                        steering,
                        follow_up,
                        reason: payload
                            .get("reason")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        allow_empty: false,
                    })
                }
            }
            DriverHookDecision::Block { reason }
            | DriverHookDecision::InteractionRequired { reason, .. } => {
                Err(AgentRuntimeError::Runtime(reason))
            }
        }
    }
}

pub(crate) fn supported_hook(binding: &DriverHookBinding) -> bool {
    crate::driver::native_hook_capabilities()
        .into_iter()
        .any(|capability| {
            capability.point == binding.point
                && capability.strength.satisfies(binding.strength)
                && binding
                    .actions
                    .iter()
                    .all(|action| capability.actions.contains(action))
                && capability
                    .failure_policies
                    .contains(&binding.failure_policy)
                && capability.acknowledged
        })
}

fn messages_from_payload(payload: &serde_json::Value, key: &str) -> Vec<AgentMessage> {
    payload
        .get(key)
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(AgentMessage::user)
        .collect()
}
