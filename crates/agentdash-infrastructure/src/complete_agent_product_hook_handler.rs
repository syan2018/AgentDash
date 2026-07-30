use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    AgentHookAction, AgentHookDecision, AgentHookOutcome, AgentHookPoint, AgentHookTiming,
    AgentHostCallbackError, AgentHostCallbackErrorCode, AgentInputContent,
};
use agentdash_agent_runtime_host::{CompleteAgentHookHandler, ResolvedCompleteAgentHookCallback};
use agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository;
use agentdash_application_hooks::AppExecutionHookProvider;
use agentdash_application_ports::agent_frame_hook_plan::AgentFrameHookPlan;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxRepository, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
    MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity, NewAgentRunMailboxMessage,
    SteeringStopEffect,
};
use agentdash_platform_spi::{
    AgentFrameHookEvaluationQuery, HookControlTarget, HookResolution, HookTrigger,
    RuntimeAdapterProvenance,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Bridges an admitted Complete Agent callback to the exact Product hook rule pinned in its frame.
pub struct ProductCompleteAgentHookHandler {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    hooks: Arc<AppExecutionHookProvider>,
    pool: PgPool,
    mailbox: Arc<dyn AgentRunMailboxRepository>,
}

impl ProductCompleteAgentHookHandler {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        hooks: Arc<AppExecutionHookProvider>,
        pool: PgPool,
        mailbox: Arc<dyn AgentRunMailboxRepository>,
    ) -> Self {
        Self {
            bindings,
            hooks,
            pool,
            mailbox,
        }
    }
}

#[async_trait]
impl CompleteAgentHookHandler for ProductCompleteAgentHookHandler {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentHookCallback,
    ) -> Result<AgentHookOutcome, AgentHostCallbackError> {
        let binding = self
            .bindings
            .load_product_binding_by_runtime_thread(&callback.context.runtime_thread_id)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| {
                unsupported("Complete Agent callback has no active Product Runtime binding")
            })?;
        let trigger = hook_trigger(callback.invocation.point, callback.invocation.timing)?;
        let plan_digest = self.adopt_hook_plan(&binding, &callback).await?;
        let hook_run_id =
            product_hook_run_id(&binding, callback.invocation.meta.idempotency_key.as_str());
        if let Some(outcome) = self
            .load_succeeded_outcome(&hook_run_id, &binding, &plan_digest)
            .await?
        {
            return Ok(outcome);
        }
        self.accept_hook_run(&hook_run_id, &binding, &plan_digest, trigger, &callback)
            .await?;
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
        let outcome =
            outcome_from_resolution(&callback.invocation.allowed_actions, trigger, resolution)?;
        let continuation_message_ids = self
            .materialize_continuation(&hook_run_id, &binding, trigger, &callback, &outcome)
            .await?;
        self.commit_hook_outcome(&hook_run_id, &outcome, &continuation_message_ids)
            .await?;
        Ok(outcome)
    }
}

impl ProductCompleteAgentHookHandler {
    async fn adopt_hook_plan(
        &self,
        binding: &agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
        callback: &ResolvedCompleteAgentHookCallback,
    ) -> Result<String, AgentHostCallbackError> {
        let adopted: Option<(String, Value)> = sqlx::query_as(
            "SELECT frame_id,plan_json FROM agent_run_hook_plans \
             WHERE run_id=$1 AND agent_id=$2",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        if let Some((frame_id, plan_json)) = adopted
            && frame_id == binding.launch_frame.frame_id.to_string()
        {
            return validate_adopted_hook_plan(&plan_json, callback);
        }

        let frame: Value = sqlx::query_scalar(
            "SELECT frame FROM lifecycle_agents AS agent \
             CROSS JOIN LATERAL jsonb_array_elements(agent.frames) AS frame \
             WHERE agent.id=$1 AND frame ->> 'id'=$2",
        )
        .bind(binding.target.agent_id.to_string())
        .bind(binding.launch_frame.frame_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        let plan_json = frame
            .pointer("/surface/hook_plan")
            .cloned()
            .filter(|value| !value.is_null())
            .ok_or_else(|| internal("bound Product AgentFrame has no immutable HookPlan"))?;
        let plan_digest = validate_adopted_hook_plan(&plan_json, callback)?;
        let plan: AgentFrameHookPlan = serde_json::from_value(plan_json.clone())
            .map_err(|error| internal(format!("bound Product HookPlan is invalid: {error}")))?;
        let revision = i64::try_from(plan.revision.0)
            .map_err(|_| internal("Product HookPlan revision exceeds PostgreSQL range"))?;
        let now = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO agent_run_hook_plans \
             (run_id,agent_id,frame_id,surface_coordinate,plan_digest,plan_json,revision,\
              created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               frame_id=EXCLUDED.frame_id,\
               surface_coordinate=EXCLUDED.surface_coordinate,\
               plan_digest=EXCLUDED.plan_digest,\
               plan_json=EXCLUDED.plan_json,\
               revision=EXCLUDED.revision,\
               updated_at=EXCLUDED.updated_at",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(binding.launch_frame.frame_id.to_string())
        .bind(callback.context.source.as_str())
        .bind(&plan_digest)
        .bind(plan_json)
        .bind(revision)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        Ok(plan_digest)
    }

    async fn load_succeeded_outcome(
        &self,
        hook_run_id: &str,
        binding: &agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
        plan_digest: &str,
    ) -> Result<Option<AgentHookOutcome>, AgentHostCallbackError> {
        let row: Option<(String, String, String, String, Option<Value>)> = sqlx::query_as(
            "SELECT run_id,agent_id,plan_digest,status,outcome_json \
             FROM agent_run_hook_runs WHERE id=$1",
        )
        .bind(hook_run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        let Some((run_id, agent_id, stored_plan_digest, status, outcome)) = row else {
            return Ok(None);
        };
        if run_id != binding.target.run_id.to_string()
            || agent_id != binding.target.agent_id.to_string()
            || stored_plan_digest != plan_digest
        {
            return Err(AgentHostCallbackError::new(
                AgentHostCallbackErrorCode::DuplicateConflict,
                "Product Hook idempotency identity belongs to another AgentRun owner",
                false,
            ));
        }
        if status != "succeeded" {
            return Ok(None);
        }
        let outcome = outcome.ok_or_else(|| {
            internal("succeeded Product HookRun is missing its canonical outcome")
        })?;
        serde_json::from_value(outcome)
            .map(Some)
            .map_err(|error| internal(error.to_string()))
    }

    async fn accept_hook_run(
        &self,
        hook_run_id: &str,
        binding: &agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
        plan_digest: &str,
        trigger: HookTrigger,
        callback: &ResolvedCompleteAgentHookCallback,
    ) -> Result<(), AgentHostCallbackError> {
        let now = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO agent_run_hook_runs \
             (id,run_id,agent_id,hook_kind,hook_definition_id,plan_digest,runtime_thread_id,\
              source_coordinate,binding_generation,source_turn_id,source_item_id,\
              source_interaction_id,source_sequence,status,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,'accepted',$13,$13) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(hook_run_id)
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(trigger.as_key())
        .bind(callback.invocation.definition_id.as_str())
        .bind(plan_digest)
        .bind(callback.context.runtime_thread_id.as_str())
        .bind(callback.context.source.as_str())
        .bind(
            i64::try_from(callback.context.binding_generation.0).map_err(|_| {
                internal("Complete Agent binding generation exceeds Product Hook ledger range")
            })?,
        )
        .bind(callback.invocation.meta.turn_id.as_str())
        .bind(
            callback
                .invocation
                .meta
                .item_id
                .as_ref()
                .map(|item| item.as_str()),
        )
        .bind(
            callback
                .invocation
                .meta
                .interaction_id
                .as_ref()
                .map(|interaction| interaction.as_str()),
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        let identity: (String, String, String, i64, Option<String>) = sqlx::query_as(
            "SELECT hook_definition_id,runtime_thread_id,source_coordinate,\
             binding_generation,source_turn_id FROM agent_run_hook_runs WHERE id=$1",
        )
        .bind(hook_run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        if identity.0 != callback.invocation.definition_id.as_str()
            || identity.1 != callback.context.runtime_thread_id.as_str()
            || identity.2 != callback.context.source.as_str()
            || identity.3
                != i64::try_from(callback.context.binding_generation.0).map_err(|_| {
                    internal("Complete Agent binding generation exceeds Product Hook ledger range")
                })?
            || identity.4.as_deref() != Some(callback.invocation.meta.turn_id.as_str())
        {
            return Err(AgentHostCallbackError::new(
                AgentHostCallbackErrorCode::DuplicateConflict,
                "Product Hook idempotency identity was reused with different callback evidence",
                false,
            ));
        }
        sqlx::query(
            "UPDATE agent_run_hook_runs SET status='running',updated_at=$2 \
             WHERE id=$1 AND status <> 'succeeded'",
        )
        .bind(hook_run_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        Ok(())
    }

    async fn materialize_continuation(
        &self,
        hook_run_id: &str,
        binding: &agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
        trigger: HookTrigger,
        callback: &ResolvedCompleteAgentHookCallback,
        outcome: &AgentHookOutcome,
    ) -> Result<Vec<Uuid>, AgentHostCallbackError> {
        if outcome.continue_turn.is_empty() {
            return Ok(Vec::new());
        }
        let (barrier, stop_effect, source) = match trigger {
            HookTrigger::AfterTurn => (
                ConsumptionBarrier::AgentLoopTurnBoundary,
                SteeringStopEffect::None,
                MailboxSourceIdentity::hook_after_turn(),
            ),
            HookTrigger::BeforeStop => (
                ConsumptionBarrier::AgentRunTurnBoundary,
                SteeringStopEffect::ContinueOnStop,
                MailboxSourceIdentity::hook_before_stop(),
            ),
            _ => {
                return Err(internal(
                    "Product Hook continuation was emitted outside a supported Agent boundary",
                ));
            }
        };
        let source = source
            .with_source_ref(callback.invocation.definition_id.as_str())
            .with_correlation_ref(hook_run_id)
            .with_metadata(json!({
                "hook_run_id": hook_run_id,
                "turn_id": callback.invocation.meta.turn_id.as_str(),
            }));
        let payload = serde_json::to_value(&outcome.continue_turn)
            .map_err(|error| internal(error.to_string()))?;
        let message = self
            .mailbox
            .create_message_idempotent(NewAgentRunMailboxMessage {
                run_id: binding.target.run_id,
                agent_id: binding.target.agent_id,
                delivery_runtime_thread_id: Some(
                    callback.context.runtime_thread_id.as_str().to_owned(),
                ),
                delivery_source_coordinate: Some(callback.context.source.as_str().to_owned()),
                delivery_binding_generation: Some(
                    i64::try_from(callback.context.binding_generation.0)
                        .map_err(|_| internal("binding generation exceeds mailbox range"))?,
                ),
                delivery_snapshot_revision: None,
                origin: MailboxMessageOrigin::Hook,
                source,
                delivery: MailboxDelivery::SteerActiveTurn { stop_effect },
                barrier,
                drain_mode: MailboxDrainMode::All,
                priority: 20_000,
                source_dedup_key: Some(format!("product-hook:{hook_run_id}:continuation")),
                queued_agent_run_turn_id: Some(
                    callback.invocation.meta.turn_id.as_str().to_owned(),
                ),
                expected_active_agent_run_turn_id: Some(
                    callback.invocation.meta.turn_id.as_str().to_owned(),
                ),
                command_receipt_id: None,
                payload_json: Some(payload),
                executor_config_json: None,
                launch_planning_input: None,
                preview: hook_continuation_preview(&outcome.continue_turn),
                has_images: outcome
                    .continue_turn
                    .iter()
                    .any(|item| matches!(item, AgentInputContent::Image { .. })),
                retain_payload: true,
            })
            .await
            .map_err(|error| unavailable(error.to_string()))?
            .message;
        self.mailbox
            .mark_message_status(
                message.id,
                None,
                MailboxMessageStatus::Steered,
                Some(callback.invocation.meta.turn_id.as_str().to_owned()),
                None,
                None,
            )
            .await
            .map_err(|error| unavailable(error.to_string()))?;
        Ok(vec![message.id])
    }

    async fn commit_hook_outcome(
        &self,
        hook_run_id: &str,
        outcome: &AgentHookOutcome,
        continuation_message_ids: &[Uuid],
    ) -> Result<(), AgentHostCallbackError> {
        let outcome_json =
            serde_json::to_value(outcome).map_err(|error| internal(error.to_string()))?;
        let effect_set_digest = format!("{:x}", Sha256::digest(outcome_json.to_string()));
        let mut effects = Vec::new();
        for decision in &outcome.decisions {
            if let AgentHookDecision::EmitEffect { effect } = decision {
                effects.push(("emit", effect.clone(), None));
            }
        }
        if outcome.refresh_surface {
            effects.push(("refresh_surface", json!({}), None));
        }
        for message_id in continuation_message_ids {
            effects.push((
                "continue_turn",
                json!({"mailbox_message_id": message_id}),
                Some(message_id.to_string()),
            ));
        }

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| unavailable(error.to_string()))?;
        for (index, (kind, payload, mailbox_message_id)) in effects.iter().enumerate() {
            let payload_digest = format!("{:x}", Sha256::digest(payload.to_string()));
            let idempotency_key = format!("{hook_run_id}:effect:{index}");
            sqlx::query(
                "INSERT INTO agent_run_hook_effects \
                 (id,hook_run_id,effect_kind,payload_digest,payload_json,idempotency_key,\
                  retry_policy_json,status,mailbox_message_id,attempt_count,created_at,updated_at,\
                  terminal_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,'applied',$8,1,$9,$9,$9) \
                 ON CONFLICT (idempotency_key) DO NOTHING",
            )
            .bind(format!("hook-effect:{idempotency_key}"))
            .bind(hook_run_id)
            .bind(kind)
            .bind(payload_digest)
            .bind(payload)
            .bind(idempotency_key)
            .bind(json!({"max_attempts": 1}))
            .bind(mailbox_message_id)
            .bind(chrono::Utc::now())
            .execute(&mut *transaction)
            .await
            .map_err(|error| unavailable(error.to_string()))?;
        }
        sqlx::query(
            "UPDATE agent_run_hook_runs SET status='succeeded',outcome_json=$2,\
             effect_set_digest=$3,last_error=NULL,updated_at=$4,terminal_at=$4 WHERE id=$1",
        )
        .bind(hook_run_id)
        .bind(outcome_json)
        .bind(effect_set_digest)
        .bind(chrono::Utc::now())
        .execute(&mut *transaction)
        .await
        .map_err(|error| unavailable(error.to_string()))?;
        transaction
            .commit()
            .await
            .map_err(|error| unavailable(error.to_string()))
    }
}

fn hook_continuation_preview(content: &[AgentInputContent]) -> String {
    content
        .iter()
        .find_map(|item| match item {
            AgentInputContent::Text { text } => Some(text.as_str()),
            AgentInputContent::Structured { value, .. } => value
                .get("completion")
                .and_then(|completion| completion.get("reason"))
                .and_then(Value::as_str),
            _ => None,
        })
        .unwrap_or("Hook requested a continuation")
        .chars()
        .take(240)
        .collect()
}

fn validate_adopted_hook_plan(
    plan_json: &Value,
    callback: &ResolvedCompleteAgentHookCallback,
) -> Result<String, AgentHostCallbackError> {
    let plan: AgentFrameHookPlan = serde_json::from_value(plan_json.clone())
        .map_err(|error| internal(format!("bound Product HookPlan is invalid: {error}")))?;
    plan.validate()
        .map_err(|error| internal(format!("bound Product HookPlan is invalid: {error}")))?;
    if !plan.requirements.iter().any(|requirement| {
        requirement.definition_id.as_str() == callback.invocation.definition_id.as_str()
    }) {
        return Err(internal(
            "Complete Agent callback definition is absent from the bound Product HookPlan",
        ));
    }
    Ok(plan.digest.as_str().to_owned())
}

fn product_hook_run_id(
    binding: &agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
    idempotency_key: &str,
) -> String {
    format!(
        "product-hook-run:{:x}",
        Sha256::digest(
            format!(
                "agentdash.product-hook-run/v1:{}:{}:{}",
                binding.target.run_id, binding.target.agent_id, idempotency_key
            )
            .as_bytes()
        )
    )
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

fn outcome_from_resolution(
    allowed: &std::collections::BTreeSet<AgentHookAction>,
    trigger: HookTrigger,
    resolution: HookResolution,
) -> Result<AgentHookOutcome, AgentHostCallbackError> {
    if resolution
        .diagnostics
        .iter()
        .any(|entry| entry.code == "hook_script_error")
    {
        return Err(internal("Product hook rule evaluation failed"));
    }
    if resolution.approval_request.is_some()
        || resolution.pending_advance.is_some()
        || !resolution.pending_execution_log.is_empty()
        || resolution.compaction.is_some()
    {
        return Err(unsupported(
            "Product hook emitted semantics outside the Complete Agent callback decision contract",
        ));
    }

    let mut outcome = AgentHookOutcome {
        diagnostics: resolution
            .diagnostics
            .iter()
            .map(|entry| format!("{}: {}", entry.code, entry.message))
            .collect(),
        ..AgentHookOutcome::allow()
    };
    if let Some(reason) = resolution.block_reason {
        require_action(allowed, AgentHookAction::AllowOrDeny)?;
        outcome.decisions.push(AgentHookDecision::Deny { reason });
    }
    if let Some(value) = resolution.rewritten_tool_input {
        if allowed.contains(&AgentHookAction::RewriteInput) {
            outcome
                .decisions
                .push(AgentHookDecision::ReplaceInput { input: value });
        } else {
            require_action(allowed, AgentHookAction::RewriteResult)?;
            outcome
                .decisions
                .push(AgentHookDecision::ReplaceResult { result: value });
        }
    }
    let continue_at_boundary = matches!(trigger, HookTrigger::AfterTurn | HookTrigger::BeforeStop)
        && (resolution
            .completion
            .as_ref()
            .is_some_and(|completion| !completion.satisfied)
            || !resolution.injections.is_empty());
    if continue_at_boundary {
        require_action(allowed, AgentHookAction::ContinueTurn)?;
        outcome.continue_turn.push(AgentInputContent::Structured {
            schema: "agentdash.hook/continuation.v1".to_owned(),
            value: serde_json::json!({
                "injections": resolution.injections,
                "completion": resolution.completion,
            }),
        });
    } else if !resolution.injections.is_empty() {
        require_action(allowed, AgentHookAction::AddContext)?;
        outcome.decisions.push(AgentHookDecision::AddContext {
            context: serde_json::to_value(resolution.injections)
                .map_err(|error| internal(error.to_string()))?,
        });
    }
    if !resolution.effects.is_empty() {
        require_action(allowed, AgentHookAction::EmitEffect)?;
        outcome.decisions.push(AgentHookDecision::EmitEffect {
            effect: serde_json::to_value(resolution.effects)
                .map_err(|error| internal(error.to_string()))?,
        });
    }
    if resolution.refresh_snapshot {
        require_action(allowed, AgentHookAction::RefreshSurface)?;
        outcome.refresh_surface = true;
    }
    Ok(outcome)
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
            outcome_from_resolution(
                &BTreeSet::from([AgentHookAction::EmitEffect]),
                HookTrigger::BeforeTool,
                HookResolution::default(),
            )
            .unwrap(),
            AgentHookOutcome::allow()
        );
    }

    #[test]
    fn exact_block_resolution_maps_to_deny() {
        assert_eq!(
            outcome_from_resolution(
                &BTreeSet::from([AgentHookAction::AllowOrDeny]),
                HookTrigger::BeforeTool,
                HookResolution {
                    block_reason: Some("policy denied".to_owned()),
                    ..HookResolution::default()
                },
            )
            .unwrap(),
            AgentHookOutcome {
                decisions: vec![AgentHookDecision::Deny {
                    reason: "policy denied".to_owned()
                }],
                ..AgentHookOutcome::allow()
            }
        );
    }

    #[test]
    fn simultaneous_product_decisions_are_preserved() {
        let outcome = outcome_from_resolution(
            &BTreeSet::from([AgentHookAction::AddContext, AgentHookAction::EmitEffect]),
            HookTrigger::BeforeTool,
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
        .expect("the callback outcome carries every Product result");

        assert_eq!(outcome.decisions.len(), 2);
        assert!(matches!(
            outcome.decisions[0],
            AgentHookDecision::AddContext { .. }
        ));
        assert!(matches!(
            outcome.decisions[1],
            AgentHookDecision::EmitEffect { .. }
        ));
    }

    #[test]
    fn unsatisfied_stop_completion_requests_a_follow_up_turn() {
        let outcome = outcome_from_resolution(
            &BTreeSet::from([AgentHookAction::ContinueTurn]),
            HookTrigger::BeforeStop,
            HookResolution {
                completion: Some(agentdash_platform_spi::HookCompletionStatus {
                    mode: "explicit".to_owned(),
                    satisfied: false,
                    advanced: false,
                    reason: "work remains".to_owned(),
                }),
                ..HookResolution::default()
            },
        )
        .unwrap();

        assert_eq!(outcome.continue_turn.len(), 1);
    }
}
