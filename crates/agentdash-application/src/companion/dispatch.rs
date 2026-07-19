use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, GatePolicy,
    GateWaitPolicyTemplate, InteractionDispatchIntent, LifecycleTaskPlanItemPatch, RunPolicy,
    RuntimePolicy, WaitExpectedResult, WaitTerminalOutcome, WaitTerminalPolicy, WaitWakeTarget,
};
use agentdash_platform_spi::AgentConfig;
use agentdash_platform_spi::action_type as at;

use super::tools::CompanionAdoptionMode;
use crate::lifecycle::LifecycleDispatchService;
use crate::repository_set::RepositorySet;
use crate::task::plan::update_run_task;
use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentRunForkChildProductSelection, AgentRunForkFacade, AgentRunForkParent,
    AgentRunForkRequestId, AgentRunForkSagaPhase, AgentRunProductProtocolPorts,
    AgentRunProductRuntimeProvisioningRequest, CompanionDispatchCoordinator,
    CompanionDispatchTargetPlan, CompanionFreshPhase, CompanionFreshRequestId,
    CompanionFreshSagaWorker, CompanionFreshStableIdentities, CompanionFullForkRequest,
    CompanionRuntimePreparation, PreallocatedAgentRunChild, ProductAgentFrameRef,
    ProductAgentSurfaceFacts, ProductExecutionProfileRef,
};
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_application_ports::launch::CompanionLaunchSource;
use agentdash_application_workflow::gate::{LifecycleGateResolver, OpenCompanionGateCommand};
use agentdash_domain::agent_run_target::AgentRunTarget;

#[derive(Debug, Clone)]
pub(crate) struct CompanionChildDispatchRequest {
    pub project_id: Uuid,
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_frame_id: Uuid,
    pub wait: bool,
    pub slice_mode: agentdash_platform_spi::CompanionSliceMode,
    pub adoption_mode: CompanionAdoptionMode,
    pub dispatch_id: String,
    pub companion_label: String,
    pub task_id: Option<Uuid>,
    pub selected_project_agent_id: Uuid,
    pub selected_agent_key: String,
    pub companion_executor_config: AgentConfig,
    pub parent_session_id: String,
    pub dispatch_prompt: String,
    pub protocol_plan: CompanionDispatchTargetPlan,
}

#[derive(Clone)]
pub(crate) struct CompanionChildDispatchOutcome {
    pub run_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub gate_ref: Option<Uuid>,
    pub runtime_thread_id: RuntimeThreadId,
    pub launch_source: CompanionLaunchSource,
}

pub(crate) struct CompanionChildDispatchService<'a> {
    repos: &'a RepositorySet,
    product_protocols: &'a AgentRunProductProtocolPorts,
    frame_construction: &'a dyn AgentRunFrameConstructionPort,
}

impl<'a> CompanionChildDispatchService<'a> {
    pub(crate) fn new(
        repos: &'a RepositorySet,
        product_protocols: &'a AgentRunProductProtocolPorts,
        frame_construction: &'a dyn AgentRunFrameConstructionPort,
    ) -> Self {
        Self {
            repos,
            product_protocols,
            frame_construction,
        }
    }

    pub(crate) async fn dispatch_child(
        &self,
        request: CompanionChildDispatchRequest,
    ) -> Result<CompanionChildDispatchOutcome, agentdash_platform_spi::AgentToolError> {
        let context_policy = match request.slice_mode {
            agentdash_platform_spi::CompanionSliceMode::Full => ContextPolicy::Inherit,
            _ => ContextPolicy::Slice,
        };
        let outcome = if matches!(
            request.protocol_plan.preparation,
            CompanionRuntimePreparation::ForkParentHistory { .. }
        ) {
            self.dispatch_full_fork(&request).await?
        } else {
            let dispatch_service = self.lifecycle_dispatch_service();
            let outcome = if request.wait {
                let result = dispatch_service
                    .open_interaction_gate(&InteractionDispatchIntent {
                        project_id: request.project_id,
                        source: ExecutionSource::ParentAgent,
                        parent_run_id: request.parent_run_id,
                        parent_agent_id: request.parent_agent_id,
                        workflow_graph_ref: None,
                        context_policy,
                        capability_policy: CapabilityPolicy::InheritedSlice,
                        runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
                        gate_policy: GatePolicy {
                            gate_kind: gate_kind(request.adoption_mode).to_string(),
                            correlation_id: Some(request.dispatch_id.clone()),
                            payload: Some(serde_json::json!({
                                "parent_agent_id": request.parent_agent_id,
                                "parent_frame_id": request.parent_frame_id,
                                "companion_label": request.companion_label.clone(),
                                "adoption_mode": adoption_mode_key(request.adoption_mode),
                                "dispatch_id": request.dispatch_id.clone(),
                                "task_id": request.task_id.map(|id| id.to_string()),
                            })),
                            wait_policy: Some(companion_agent_run_delivery_wait_policy_template(
                                request.dispatch_id.clone(),
                                request.parent_run_id,
                                request.parent_agent_id,
                            )),
                        },
                    })
                    .await
                    .map_err(|error| {
                        agentdash_platform_spi::AgentToolError::ExecutionFailed(format!(
                            "dispatch 失败: {error}"
                        ))
                    })?;
                let runtime_thread_id =
                    RuntimeThreadId::new(result.delivery_runtime_ref.to_string())
                        .expect("delivery runtime ref is non-empty");
                CompanionChildDispatchOutcome {
                    run_ref: result.runtime_refs.run_ref,
                    agent_ref: result.runtime_refs.agent_ref,
                    frame_ref: result.runtime_refs.frame_ref,
                    gate_ref: Some(result.gate_ref),
                    runtime_thread_id,
                    launch_source: build_launch_source(&request),
                }
            } else {
                let result = dispatch_service
                    .launch_agent(&AgentLaunchIntent {
                        project_id: request.project_id,
                        project_agent_id: None,
                        execution_profile_override: None,
                        source: ExecutionSource::ParentAgent,
                        created_by_user_id: None,
                        subject_ref: None,
                        parent_run_id: Some(request.parent_run_id),
                        parent_agent_id: Some(request.parent_agent_id),
                        workflow_graph_ref: None,
                        run_policy: RunPolicy::AppendGraph,
                        agent_policy: AgentPolicy::SpawnChild,
                        context_policy,
                        capability_policy: CapabilityPolicy::InheritedSlice,
                        runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
                    })
                    .await
                    .map_err(|error| {
                        agentdash_platform_spi::AgentToolError::ExecutionFailed(format!(
                            "dispatch 失败: {error}"
                        ))
                    })?;
                CompanionChildDispatchOutcome {
                    run_ref: result.runtime_refs.run_ref,
                    agent_ref: result.runtime_refs.agent_ref,
                    frame_ref: result.runtime_refs.frame_ref,
                    gate_ref: None,
                    runtime_thread_id: RuntimeThreadId::new(
                        result.delivery_runtime_ref.to_string(),
                    )
                    .expect("delivery runtime ref is non-empty"),
                    launch_source: build_launch_source(&request),
                }
            };
            self.advance_fresh_protocol(&request, &outcome).await?;
            outcome
        };

        if let Some(task_id) = request.task_id {
            update_run_task(
                self.repos.lifecycle_run_repo.as_ref(),
                request.parent_run_id,
                task_id,
                LifecycleTaskPlanItemPatch {
                    assigned_agent_id: Some(Some(outcome.agent_ref)),
                    ..LifecycleTaskPlanItemPatch::default()
                },
            )
            .await
            .map_err(|error| {
                agentdash_platform_spi::AgentToolError::ExecutionFailed(format!(
                    "Companion 已创建但 Task 指派关系写回失败: {error}"
                ))
            })?;
        }

        self.bind_selected_project_agent(outcome.agent_ref, request.selected_project_agent_id)
            .await?;

        Ok(outcome)
    }

    fn lifecycle_dispatch_service(&self) -> LifecycleDispatchService<'_> {
        LifecycleDispatchService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            self.repos.lifecycle_gate_repo.as_ref(),
            self.repos.agent_lineage_repo.as_ref(),
        )
        .with_frame_construction_port(self.frame_construction)
    }

    async fn bind_selected_project_agent(
        &self,
        lifecycle_agent_id: Uuid,
        project_agent_id: Uuid,
    ) -> Result<(), agentdash_platform_spi::AgentToolError> {
        let Some(mut lifecycle_agent) = self
            .repos
            .lifecycle_agent_repo
            .get(lifecycle_agent_id)
            .await
            .map_err(|error| {
                agentdash_platform_spi::AgentToolError::ExecutionFailed(error.to_string())
            })?
        else {
            return Err(agentdash_platform_spi::AgentToolError::ExecutionFailed(
                format!(
                    "LifecycleAgent {lifecycle_agent_id} 不存在，无法绑定 selected companion ProjectAgent"
                ),
            ));
        };
        lifecycle_agent.project_agent_id = Some(project_agent_id);
        self.repos
            .lifecycle_agent_repo
            .update(&lifecycle_agent)
            .await
            .map_err(|error| {
                agentdash_platform_spi::AgentToolError::ExecutionFailed(error.to_string())
            })?;
        Ok(())
    }

    async fn advance_fresh_protocol(
        &self,
        request: &CompanionChildDispatchRequest,
        outcome: &CompanionChildDispatchOutcome,
    ) -> Result<(), agentdash_platform_spi::AgentToolError> {
        let frame = self
            .repos
            .agent_frame_repo
            .get(outcome.frame_ref)
            .await
            .map_err(|error| {
                agentdash_platform_spi::AgentToolError::ExecutionFailed(error.to_string())
            })?
            .ok_or_else(|| {
                agentdash_platform_spi::AgentToolError::ExecutionFailed(format!(
                    "Companion launch frame {} 不存在",
                    outcome.frame_ref
                ))
            })?;
        let mut execution_profile = ProductExecutionProfileRef {
            profile_key: request.companion_executor_config.executor.clone(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::to_value(&request.companion_executor_config).map_err(
                |error| agentdash_platform_spi::AgentToolError::ExecutionFailed(error.to_string()),
            )?,
            credential_scope: None,
        };
        execution_profile.refresh_digest();
        let provisioning = AgentRunProductRuntimeProvisioningRequest {
            target: AgentRunTarget {
                run_id: outcome.run_ref,
                agent_id: outcome.agent_ref,
            },
            runtime_thread_id: outcome.runtime_thread_id.clone(),
            idempotency_key: format!("companion:{}:runtime", request.dispatch_id),
            frame: ProductAgentFrameRef {
                frame_id: frame.id,
                agent_id: frame.agent_id,
                revision: u64::try_from(frame.revision).map_err(|_| {
                    agentdash_platform_spi::AgentToolError::ExecutionFailed(
                        "Companion launch frame revision 无效".to_string(),
                    )
                })?,
            },
            execution_profile,
            surface_facts: ProductAgentSurfaceFacts::from_frame(&frame),
        };
        let request_uuid = stable_uuid(&request.dispatch_id, "fresh-request");
        let identities = CompanionFreshStableIdentities {
            request_id: CompanionFreshRequestId(request_uuid),
            runtime_thread_id: outcome.runtime_thread_id.clone(),
            create_effect_id: stable_uuid(&request.dispatch_id, "fresh-create"),
            activation_effect_id: stable_uuid(&request.dispatch_id, "fresh-activate"),
            first_input_effect_id: stable_uuid(&request.dispatch_id, "fresh-first-input"),
        };
        let coordinator = CompanionDispatchCoordinator::new(
            self.product_protocols.fork_sagas.as_ref(),
            self.product_protocols.companion_fresh_sagas.as_ref(),
        );
        coordinator
            .materialize_fresh(
                identities.clone(),
                request.protocol_plan.clone(),
                provisioning,
            )
            .await
            .map_err(protocol_error)?;
        let worker = CompanionFreshSagaWorker::new(
            self.product_protocols.companion_fresh_sagas.as_ref(),
            self.product_protocols.companion_fresh_runtime.as_ref(),
        );
        for _ in 0..16 {
            let saga = worker
                .advance(&identities.request_id)
                .await
                .map_err(protocol_error)?;
            if saga.phase() == CompanionFreshPhase::Succeeded {
                return Ok(());
            }
            if saga.failure().is_some() {
                return Err(agentdash_platform_spi::AgentToolError::ExecutionFailed(
                    format!(
                        "fresh Companion protocol terminalized before success: {:?}",
                        saga.failure()
                    ),
                ));
            }
            tokio::task::yield_now().await;
        }
        Err(agentdash_platform_spi::AgentToolError::ExecutionFailed(
            "fresh Companion protocol 未在同步派发窗口内收敛；后台 recovery 将继续同一 saga"
                .to_string(),
        ))
    }

    async fn dispatch_full_fork(
        &self,
        request: &CompanionChildDispatchRequest,
    ) -> Result<CompanionChildDispatchOutcome, agentdash_platform_spi::AgentToolError> {
        let CompanionRuntimePreparation::ForkParentHistory {
            parent_runtime_thread_id,
            through_turn_id,
        } = &request.protocol_plan.preparation
        else {
            return Err(agentdash_platform_spi::AgentToolError::ExecutionFailed(
                "Full Companion 缺少 exact fork preparation".to_string(),
            ));
        };
        let child = PreallocatedAgentRunChild {
            agent_run_id: stable_uuid(&request.dispatch_id, "full-agent-run"),
            run_id: stable_uuid(&request.dispatch_id, "full-run"),
            agent_id: stable_uuid(&request.dispatch_id, "full-agent"),
            frame_id: stable_uuid(&request.dispatch_id, "full-frame"),
            runtime_thread_id: RuntimeThreadId::new(format!(
                "companion-full:{}",
                stable_uuid(&request.dispatch_id, "full-runtime")
            ))
            .expect("stable full RuntimeThread id"),
        };
        let request_id = AgentRunForkRequestId(stable_uuid(&request.dispatch_id, "full-request"));
        let mut execution_profile = ProductExecutionProfileRef {
            profile_key: request.companion_executor_config.executor.clone(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::to_value(&request.companion_executor_config)
                .map_err(protocol_error)?,
            credential_scope: None,
        };
        execution_profile.refresh_digest();
        let parent = AgentRunForkParent {
            run_id: request.parent_run_id,
            agent_id: request.parent_agent_id,
            runtime_thread_id: parent_runtime_thread_id.clone(),
            through_turn_id: through_turn_id.clone(),
        };
        let coordinator = CompanionDispatchCoordinator::new(
            self.product_protocols.fork_sagas.as_ref(),
            self.product_protocols.companion_fresh_sagas.as_ref(),
        );
        coordinator
            .materialize_full_fork(
                &request.protocol_plan,
                CompanionFullForkRequest {
                    request_id: request_id.clone(),
                    parent,
                    child: child.clone(),
                    child_product_selection: AgentRunForkChildProductSelection {
                        project_agent_id: request.selected_project_agent_id,
                        execution_profile,
                        idempotency_key: format!("companion:{}:full-runtime", request.dispatch_id),
                    },
                },
            )
            .await
            .map_err(protocol_error)?;
        let facade = AgentRunForkFacade::new(
            self.product_protocols.fork_sagas.as_ref(),
            self.product_protocols.fork_runtime.as_ref(),
            self.product_protocols.fork_product_graph.as_ref(),
        );
        for _ in 0..16 {
            let saga = facade.advance(&request_id).await.map_err(protocol_error)?;
            if saga.phase() == AgentRunForkSagaPhase::Succeeded {
                let selected_frame_id = saga
                    .materialized_child_product_selection()
                    .map(|provisioning| provisioning.frame.frame_id)
                    .unwrap_or(child.frame_id);
                let gate_ref = if request.wait {
                    Some(
                        LifecycleGateResolver::new(self.repos.lifecycle_gate_repo.clone())
                            .open_companion_gate(OpenCompanionGateCommand {
                                run_id: child.run_id,
                                agent_id: child.agent_id,
                                frame_id: Some(selected_frame_id),
                                gate_kind: gate_kind(request.adoption_mode).to_string(),
                                correlation_id: request.dispatch_id.clone(),
                                payload: Some(companion_gate_payload(request)),
                                wait_policy: Some(
                                    companion_agent_run_delivery_wait_policy_template(
                                        request.dispatch_id.clone(),
                                        request.parent_run_id,
                                        request.parent_agent_id,
                                    ),
                                ),
                            })
                            .await
                            .map_err(protocol_error)?
                            .gate
                            .id,
                    )
                } else {
                    None
                };
                return Ok(CompanionChildDispatchOutcome {
                    run_ref: child.run_id,
                    agent_ref: child.agent_id,
                    frame_ref: selected_frame_id,
                    gate_ref,
                    runtime_thread_id: child.runtime_thread_id,
                    launch_source: build_launch_source(request),
                });
            }
            tokio::task::yield_now().await;
        }
        Err(agentdash_platform_spi::AgentToolError::ExecutionFailed(
            "Full Companion fork 未在同步派发窗口内收敛；后台 recovery 将继续同一 saga".to_string(),
        ))
    }
}

fn protocol_error(error: impl std::fmt::Display) -> agentdash_platform_spi::AgentToolError {
    agentdash_platform_spi::AgentToolError::ExecutionFailed(error.to_string())
}

fn stable_uuid(seed: &str, purpose: &str) -> Uuid {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("agentdash.companion/v1:{purpose}:{seed}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn companion_gate_payload(request: &CompanionChildDispatchRequest) -> serde_json::Value {
    serde_json::json!({
        "parent_agent_id": request.parent_agent_id,
        "parent_frame_id": request.parent_frame_id,
        "companion_label": request.companion_label,
        "adoption_mode": adoption_mode_key(request.adoption_mode),
        "dispatch_id": request.dispatch_id,
        "task_id": request.task_id.map(|id| id.to_string()),
    })
}

fn companion_agent_run_delivery_wait_policy_template(
    correlation_ref: String,
    target_run_id: Uuid,
    target_agent_id: Uuid,
) -> GateWaitPolicyTemplate {
    GateWaitPolicyTemplate {
        expected_result: WaitExpectedResult {
            kind: "companion_result".to_string(),
            correlation_ref: Some(correlation_ref),
        },
        terminal_policy: WaitTerminalPolicy {
            failed: WaitTerminalOutcome {
                status: "failed".to_string(),
                failure_kind: "runtime_terminal_failed".to_string(),
            },
            interrupted: WaitTerminalOutcome {
                status: "cancelled".to_string(),
                failure_kind: "runtime_terminal_cancelled".to_string(),
            },
            completed: WaitTerminalOutcome {
                status: "failed".to_string(),
                failure_kind: "missing_companion_respond".to_string(),
            },
        },
        wake_target: WaitWakeTarget {
            namespace: "companion".to_string(),
            target_run_id,
            target_agent_id,
            client_command_id: "companion-result:{gate_id}".to_string(),
        },
    }
}

fn build_launch_source(request: &CompanionChildDispatchRequest) -> CompanionLaunchSource {
    CompanionLaunchSource {
        parent_session_id: request.parent_session_id.clone(),
        selected_project_agent_id: Some(request.selected_project_agent_id),
        selected_agent_key: Some(request.selected_agent_key.clone()),
        slice_mode: request.slice_mode,
        companion_executor_config: request.companion_executor_config.clone(),
        dispatch_prompt: request.dispatch_prompt.clone(),
        workflow: None,
    }
}

fn gate_kind(mode: CompanionAdoptionMode) -> &'static str {
    match mode {
        CompanionAdoptionMode::BlockingReview => "companion_wait_blocking",
        CompanionAdoptionMode::FollowUpRequired => "companion_wait_follow_up",
        CompanionAdoptionMode::Suggestion => "companion_wait",
    }
}

fn adoption_mode_key(mode: CompanionAdoptionMode) -> &'static str {
    match mode {
        CompanionAdoptionMode::Suggestion => at::SUGGESTION,
        CompanionAdoptionMode::FollowUpRequired => at::FOLLOW_UP_REQUIRED,
        CompanionAdoptionMode::BlockingReview => at::BLOCKING_REVIEW,
    }
}
