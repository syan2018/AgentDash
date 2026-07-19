use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunForkRequestId, AgentRunForkSagaPhase,
    AgentRunProductInputDeliveryPort, AgentRunProductInputPreparation,
    AgentRunProductProtocolPorts, CompanionAfterDispatchHookEvidence, CompanionChannelEvidence,
    CompanionContinuationEffectIdentity, CompanionContinuationEffectPort,
    CompanionContinuationRuntimeProtocol, CompanionContinuationSaga, CompanionEffectProgress,
    CompanionFirstInputEvidence, CompanionFreshPhase, CompanionFreshRequestId,
    CompanionGateEvidence, CompanionPreparedFirstInputEvidence, CompanionRuntimeReadiness,
    CompanionRuntimeReadyEvidence, CompanionTaskEvidence, DeliverAgentRunProductInput,
};
use agentdash_application_hooks::AppExecutionHookProvider;
use agentdash_application_ports::agent_frame_hook_plan::{HookExecutionSite, HookPoint};
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_application_workflow::gate::{LifecycleGateResolver, OpenCompanionGateCommand};
use agentdash_domain::agent_run_mailbox::{MailboxMessageOrigin, MailboxSourceIdentity};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::channel::{
    ChannelDeliveryState, ChannelDeliveryStatus, ChannelOwner, ChannelParticipantRef,
    MaterializedDeliveryRef,
};
use agentdash_domain::workflow::LifecycleTaskPlanItemPatch;
use agentdash_platform_spi::{
    AgentFrameHookEvaluationQuery, HookControlTarget, HookTrigger, RuntimeAdapterProvenance,
};
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::dispatch::{
    CompanionChildDispatchRequest, CompanionChildDispatchService,
    companion_agent_run_delivery_wait_policy_template,
};
use super::tools::{
    companion_channel_delivery_intent, companion_channel_service, ensure_companion_agent_channel,
};
use crate::repository_set::RepositorySet;
use crate::task::plan::update_run_task;

pub struct ApplicationCompanionContinuationEffects {
    repos: RepositorySet,
    protocols: Arc<AgentRunProductProtocolPorts>,
    product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort>,
    frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    hook_provider: Arc<AppExecutionHookProvider>,
}

impl ApplicationCompanionContinuationEffects {
    pub fn new(
        repos: RepositorySet,
        protocols: Arc<AgentRunProductProtocolPorts>,
        product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort>,
        frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
        hook_provider: Arc<AppExecutionHookProvider>,
    ) -> Self {
        Self {
            repos,
            protocols,
            product_input_delivery,
            frame_construction,
            hook_provider,
        }
    }
}

#[async_trait]
impl CompanionContinuationEffectPort for ApplicationCompanionContinuationEffects {
    async fn converge_runtime(
        &self,
        saga: &CompanionContinuationSaga,
    ) -> Result<CompanionRuntimeReadiness, String> {
        let request = saga.request();
        let runtime_exists = match request.runtime_protocol {
            CompanionContinuationRuntimeProtocol::FullFork => self
                .protocols
                .fork_sagas
                .load(&AgentRunForkRequestId(request.runtime_protocol_request_id))
                .await
                .map_err(|error| error.to_string())?
                .is_some(),
            CompanionContinuationRuntimeProtocol::FreshCreate => self
                .protocols
                .companion_fresh_sagas
                .load(&CompanionFreshRequestId(
                    request.runtime_protocol_request_id,
                ))
                .await
                .map_err(|error| error.to_string())?
                .is_some(),
        };
        if !runtime_exists {
            CompanionChildDispatchService::new(
                &self.repos,
                self.protocols.as_ref(),
                self.frame_construction.as_ref(),
            )
            .dispatch_child(CompanionChildDispatchRequest {
                project_id: request.project_id,
                parent_run_id: request.parent_run_id,
                parent_agent_id: request.parent_agent_id,
                child_agent_id: request.child_agent_id,
                child_runtime_thread_id: request.child_runtime_thread_id.clone(),
                slice_mode: match request.runtime_protocol {
                    CompanionContinuationRuntimeProtocol::FullFork => {
                        agentdash_platform_spi::CompanionSliceMode::Full
                    }
                    CompanionContinuationRuntimeProtocol::FreshCreate => {
                        agentdash_platform_spi::CompanionSliceMode::Compact
                    }
                },
                dispatch_id: request.dispatch_id.clone(),
                selected_project_agent_id: request.selected_project_agent_id,
                companion_executor_config: request.companion_executor_config.clone(),
                protocol_plan: request.protocol_plan.clone(),
            })
            .await
            .map_err(|error| error.to_string())?;
        }
        let ready = match request.runtime_protocol {
            CompanionContinuationRuntimeProtocol::FullFork => {
                let inner = self
                    .protocols
                    .fork_sagas
                    .load(&AgentRunForkRequestId(request.runtime_protocol_request_id))
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Full Companion fork saga is missing".to_owned())?;
                if let Some(failure) = inner.failure() {
                    return Ok(CompanionRuntimeReadiness::Failed(failure.reason.clone()));
                }
                if let Some(lost) = inner.lost() {
                    return Ok(CompanionRuntimeReadiness::Failed(lost.reason.clone()));
                }
                if inner.phase() != AgentRunForkSagaPhase::Succeeded {
                    return Ok(CompanionRuntimeReadiness::Pending);
                }
                CompanionRuntimeReadyEvidence {
                    child_run_id: inner.child().run_id,
                    child_agent_id: inner.child().agent_id,
                    child_frame_id: inner
                        .materialized_child_product_selection()
                        .map(|selection| selection.frame.frame_id)
                        .unwrap_or(inner.child().frame_id),
                    child_runtime_thread_id: inner.child().runtime_thread_id.clone(),
                }
            }
            CompanionContinuationRuntimeProtocol::FreshCreate => {
                let inner = self
                    .protocols
                    .companion_fresh_sagas
                    .load(&CompanionFreshRequestId(
                        request.runtime_protocol_request_id,
                    ))
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Fresh Companion saga is missing".to_owned())?;
                if let Some(failure) = inner.failure() {
                    return Ok(CompanionRuntimeReadiness::Failed(failure.reason.clone()));
                }
                if inner.phase() != CompanionFreshPhase::Succeeded {
                    return Ok(CompanionRuntimeReadiness::Pending);
                }
                CompanionRuntimeReadyEvidence {
                    child_run_id: inner.provisioning().target.run_id,
                    child_agent_id: inner.provisioning().target.agent_id,
                    child_frame_id: inner.provisioning().frame.frame_id,
                    child_runtime_thread_id: inner.provisioning().runtime_thread_id.clone(),
                }
            }
        };
        Ok(CompanionRuntimeReadiness::Ready(ready))
    }

    async fn converge_first_input(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionFirstInputEvidence>, String> {
        let request = saga.request();
        match request.runtime_protocol {
            CompanionContinuationRuntimeProtocol::FullFork => {
                let CompanionPreparedFirstInputEvidence::ProductDelivery { envelope } = saga
                    .evidence()
                    .prepared_first_input
                    .as_ref()
                    .ok_or_else(|| "Companion prepared first input is missing".to_owned())?
                else {
                    return Err("Full Companion prepared first input has wrong kind".to_owned());
                };
                let outcome = self
                    .product_input_delivery
                    .dispatch_prepared(envelope.clone())
                    .await
                    .map_err(|error| error.to_string())?;
                if outcome.queued {
                    return Ok(CompanionEffectProgress::Pending);
                }
                Ok(CompanionEffectProgress::Applied(
                    CompanionFirstInputEvidence {
                        mailbox_message_id: outcome.mailbox_message_id,
                        runtime_operation_id: outcome
                            .operation_receipt
                            .map(|receipt| receipt.operation_id.to_string()),
                        submitted_by_runtime_protocol: false,
                    },
                ))
            }
            CompanionContinuationRuntimeProtocol::FreshCreate => {
                let command = first_input_command(saga, identity);
                let mailbox_message_id = self
                    .product_input_delivery
                    .record_dispatched(command)
                    .await
                    .map_err(|error| error.to_string())?;
                let inner = self
                    .protocols
                    .companion_fresh_sagas
                    .load(&CompanionFreshRequestId(
                        request.runtime_protocol_request_id,
                    ))
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Fresh Companion saga is missing".to_owned())?;
                let operation_id = inner
                    .receipts()
                    .first_input
                    .as_ref()
                    .map(|receipt| receipt.operation_id.to_string());
                Ok(CompanionEffectProgress::Applied(
                    CompanionFirstInputEvidence {
                        mailbox_message_id,
                        runtime_operation_id: operation_id,
                        submitted_by_runtime_protocol: true,
                    },
                ))
            }
        }
    }

    async fn prepare_first_input(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionPreparedFirstInputEvidence>, String> {
        match saga.request().runtime_protocol {
            CompanionContinuationRuntimeProtocol::FullFork => {
                match self
                    .product_input_delivery
                    .prepare_delivery(first_input_command(saga, identity))
                    .await
                    .map_err(|error| error.to_string())?
                {
                    AgentRunProductInputPreparation::Pending { .. } => {
                        Ok(CompanionEffectProgress::Pending)
                    }
                    AgentRunProductInputPreparation::Prepared(envelope) => {
                        Ok(CompanionEffectProgress::Applied(
                            CompanionPreparedFirstInputEvidence::ProductDelivery { envelope },
                        ))
                    }
                }
            }
            CompanionContinuationRuntimeProtocol::FreshCreate => {
                Ok(CompanionEffectProgress::Applied(
                    CompanionPreparedFirstInputEvidence::FreshRuntimeProtocol,
                ))
            }
        }
    }

    async fn converge_gate(
        &self,
        saga: &CompanionContinuationSaga,
        _identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionGateEvidence, String> {
        let request = saga.request();
        let runtime = saga
            .evidence()
            .runtime
            .as_ref()
            .ok_or_else(|| "Companion Runtime-ready evidence is missing".to_owned())?;
        if !request.wait {
            return Ok(CompanionGateEvidence { gate_id: None });
        }
        if let Some(existing) = self
            .repos
            .lifecycle_gate_repo
            .find_by_agent_and_correlation(request.child_agent_id, &request.dispatch_id)
            .await
            .map_err(|error| error.to_string())?
        {
            if existing.run_id != request.child_run_id
                || existing.frame_id != Some(runtime.child_frame_id)
            {
                return Err("Companion gate reclaim evidence drifted".to_owned());
            }
            return Ok(CompanionGateEvidence {
                gate_id: Some(existing.id),
            });
        }
        let outcome = LifecycleGateResolver::new(self.repos.lifecycle_gate_repo.clone())
            .open_companion_gate(OpenCompanionGateCommand {
                run_id: request.child_run_id,
                agent_id: request.child_agent_id,
                frame_id: Some(runtime.child_frame_id),
                gate_kind: gate_kind(&request.adoption_mode).to_owned(),
                correlation_id: request.dispatch_id.clone(),
                payload: Some(companion_gate_payload(request)),
                wait_policy: Some(companion_agent_run_delivery_wait_policy_template(
                    request.dispatch_id.clone(),
                    request.parent_run_id,
                    request.parent_agent_id,
                )),
            })
            .await
            .map_err(|error| error.to_string())?;
        Ok(CompanionGateEvidence {
            gate_id: Some(outcome.gate.id),
        })
    }

    async fn converge_channel(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionChannelEvidence, String> {
        let request = saga.request();
        let first_input = saga
            .evidence()
            .first_input
            .as_ref()
            .ok_or_else(|| "Companion first input evidence is missing".to_owned())?;
        let source = mailbox_source(request);
        let input_text = request.first_input_text.clone();
        let channel_id = ensure_companion_agent_channel(
            &self.repos,
            request.parent_run_id,
            request.parent_agent_id,
            request.child_agent_id,
            &request.companion_label,
        )
        .await
        .map_err(|error| error.to_string())?;
        let mut intent = companion_channel_delivery_intent(
            channel_id,
            request.child_run_id,
            request.child_agent_id,
            ChannelParticipantRef::LifecycleAgent {
                run_id: request.parent_run_id,
                agent_id: request.parent_agent_id,
            },
            &source,
            "companion_dispatch",
            &input_text,
        );
        intent.id = identity.effect_id;
        intent.message.id = stable_message_id(identity.effect_id);
        companion_channel_service(&self.repos)
            .materialize_delivery_to_mailbox(&intent)
            .map_err(|error| error.to_string())?;
        companion_channel_service(&self.repos)
            .record_delivery_state(
                &ChannelOwner::LifecycleRun {
                    run_id: request.parent_run_id,
                },
                channel_id,
                ChannelDeliveryState {
                    delivery_id: intent.id,
                    message_id: intent.message.id,
                    target: intent.target,
                    status: ChannelDeliveryStatus::Materialized,
                    materialized_ref: Some(MaterializedDeliveryRef::MailboxMessage {
                        message_id: first_input.mailbox_message_id,
                    }),
                    updated_at: Utc::now(),
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(CompanionChannelEvidence {
            channel_id,
            delivery_id: intent.id,
            mailbox_message_id: first_input.mailbox_message_id,
        })
    }

    async fn converge_task(
        &self,
        saga: &CompanionContinuationSaga,
        _identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionTaskEvidence, String> {
        let request = saga.request();
        if let Some(task_id) = request.task_id {
            update_run_task(
                self.repos.lifecycle_run_repo.as_ref(),
                request.parent_run_id,
                task_id,
                LifecycleTaskPlanItemPatch {
                    assigned_agent_id: Some(Some(request.child_agent_id)),
                    ..LifecycleTaskPlanItemPatch::default()
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        Ok(CompanionTaskEvidence {
            task_id: request.task_id,
            assigned_agent_id: request.task_id.map(|_| request.child_agent_id),
        })
    }

    async fn converge_after_dispatch_hook(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionAfterDispatchHookEvidence>, String> {
        let request = saga.request();
        if identity != &request.after_dispatch_hook_effect {
            return Err("Companion After hook effect identity drifted".to_owned());
        }
        let runtime = saga
            .evidence()
            .runtime
            .as_ref()
            .ok_or_else(|| "Companion Runtime-ready evidence is missing".to_owned())?;
        let first_input = saga
            .evidence()
            .first_input
            .as_ref()
            .ok_or_else(|| "Companion first input evidence is missing".to_owned())?;
        let gate = saga
            .evidence()
            .gate
            .as_ref()
            .ok_or_else(|| "Companion gate evidence is missing".to_owned())?;
        let provenance = RuntimeAdapterProvenance::runtime_thread(
            request.parent_runtime_thread_id.clone(),
            Some(request.parent_turn_id.clone()),
            format!(
                "companion_continuation_after_dispatch:{}",
                identity.effect_id
            ),
        );
        let target = HookControlTarget {
            run_id: request.parent_run_id,
            agent_id: request.parent_agent_id,
            frame_id: request.parent_frame_id,
        };
        let parent_frame = self
            .repos
            .agent_frame_repo
            .get(request.parent_frame_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Companion parent AgentFrame is missing".to_owned())?;
        if parent_frame.agent_id != request.parent_agent_id {
            return Err("Companion parent AgentFrame owner drifted".to_owned());
        }
        let hook_plan = parent_frame.validated_hook_plan()?;
        let requirements = hook_plan
            .requirements
            .into_iter()
            .filter(|requirement| {
                requirement.site == HookExecutionSite::ToolBroker
                    && requirement.requirement.point == HookPoint::AfterTool
            })
            .collect::<Vec<_>>();
        let resolution = self
            .hook_provider
            .evaluate_product_hook_event(
                &requirements,
                AgentFrameHookEvaluationQuery {
                    target: target.clone(),
                    provenance: provenance.clone(),
                    trigger: HookTrigger::AfterSubagentDispatch,
                    tool_name: None,
                    tool_call_id: None,
                    subagent_type: Some(request.companion_label.clone()),
                    snapshot: None,
                    payload: Some(serde_json::json!({
                        "effect_id": identity.effect_id,
                        "idempotency_key": identity.idempotency_key,
                        "dispatch_id": request.dispatch_id,
                        "agent_ref": request.child_agent_id,
                        "frame_ref": runtime.child_frame_id,
                        "gate_ref": gate.gate_id,
                        "runtime_thread_id": runtime.child_runtime_thread_id,
                        "runtime_operation_id": first_input.runtime_operation_id,
                        "mailbox_message_id": first_input.mailbox_message_id,
                        "mailbox_outcome": "dispatched",
                        "slice_mode": request.slice_mode,
                        "adoption_mode": request.adoption_mode,
                        "task_id": request.task_id,
                    })),
                    token_stats: None,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(CompanionEffectProgress::Applied(
            CompanionAfterDispatchHookEvidence {
                effect: identity.clone(),
                parent_frame_id: request.parent_frame_id,
                child_frame_id: runtime.child_frame_id,
                child_runtime_thread_id: runtime.child_runtime_thread_id.clone(),
                resolution,
            },
        ))
    }
}

fn gate_kind(adoption_mode: &str) -> &'static str {
    match adoption_mode {
        agentdash_platform_spi::action_type::BLOCKING_REVIEW => "companion_wait_blocking",
        agentdash_platform_spi::action_type::FOLLOW_UP_REQUIRED => "companion_wait_follow_up",
        _ => "companion_wait",
    }
}

fn companion_gate_payload(
    request: &agentdash_application_agentrun::agent_run::CompanionContinuationRequest,
) -> serde_json::Value {
    serde_json::json!({
        "parent_agent_id": request.parent_agent_id,
        "parent_frame_id": request.parent_frame_id,
        "companion_label": request.companion_label,
        "adoption_mode": request.adoption_mode,
        "dispatch_id": request.dispatch_id,
        "task_id": request.task_id.map(|id| id.to_string()),
    })
}

fn stable_message_id(effect_id: Uuid) -> Uuid {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("agentdash.companion-channel/v1:{effect_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn mailbox_source(
    request: &agentdash_application_agentrun::agent_run::CompanionContinuationRequest,
) -> MailboxSourceIdentity {
    MailboxSourceIdentity {
        namespace: request.first_input_source.namespace.clone(),
        kind: request.first_input_source.kind.clone(),
        source_ref: request.first_input_source.source_ref.clone(),
        correlation_ref: request.first_input_source.correlation_ref.clone(),
        actor: request.first_input_source.actor.clone(),
        route: request.first_input_source.route.clone(),
        display_label_key: request.first_input_source.display_label_key.clone(),
        metadata: request.first_input_source.metadata.clone(),
    }
}

fn first_input_command(
    saga: &CompanionContinuationSaga,
    identity: &CompanionContinuationEffectIdentity,
) -> DeliverAgentRunProductInput {
    let request = saga.request();
    DeliverAgentRunProductInput {
        target: AgentRunTarget {
            run_id: request.child_run_id,
            agent_id: request.child_agent_id,
        },
        origin: MailboxMessageOrigin::Companion,
        content: agentdash_agent_protocol::text_user_input_blocks(request.first_input_text.clone()),
        source: mailbox_source(request),
        client_command_id: identity.idempotency_key.clone(),
    }
}
