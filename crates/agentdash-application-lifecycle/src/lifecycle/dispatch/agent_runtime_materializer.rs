use uuid::Uuid;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::lifecycle_materialization::{
    WorkflowAgentNodeMaterializationRequest, WorkflowAgentNodeMaterializationResult,
};
use agentdash_application_ports::runtime_session_delivery as runtime_session_delivery_port;
use agentdash_application_ports::workflow_agent_frame_materialization as workflow_node_frame_port;
use agentdash_domain::workflow::{
    AgentPolicy, AgentProcedureContract, AgentRunDeliveryBinding,
    AgentRunDeliveryBindingRepository, AgentRuntimeRefs, AgentSource, DeliveryBindingStatus,
    ExecutionSource, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    OrchestrationBindingRefs, RuntimePolicy, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository,
};

use crate::lifecycle::WorkflowApplicationError;

use super::plan::{
    DispatchPlan, MaterializedAgentRuntime, WorkflowAgentNodeRuntimeContext,
    workflow_error_from_agent_frame_materialization_error,
    workflow_error_from_runtime_session_delivery_error,
};

pub(crate) struct AgentRuntimeMaterializer<'a> {
    agent_repo: &'a dyn LifecycleAgentRepository,
    anchor_repo: Option<&'a dyn RuntimeSessionExecutionAnchorRepository>,
    delivery_binding_repo: Option<&'a dyn AgentRunDeliveryBindingRepository>,
    runtime_session_creator:
        Option<&'a dyn runtime_session_delivery_port::RuntimeSessionCreationPort>,
    frame_construction:
        Option<&'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort>,
    workflow_agent_frame_materialization:
        Option<&'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort>,
}

impl<'a> AgentRuntimeMaterializer<'a> {
    pub(crate) fn new(
        agent_repo: &'a dyn LifecycleAgentRepository,
        anchor_repo: Option<&'a dyn RuntimeSessionExecutionAnchorRepository>,
        delivery_binding_repo: Option<&'a dyn AgentRunDeliveryBindingRepository>,
        runtime_session_creator: Option<
            &'a dyn runtime_session_delivery_port::RuntimeSessionCreationPort,
        >,
        frame_construction: Option<
            &'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort,
        >,
        workflow_agent_frame_materialization: Option<
            &'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort,
        >,
    ) -> Self {
        Self {
            agent_repo,
            anchor_repo,
            delivery_binding_repo,
            runtime_session_creator,
            frame_construction,
            workflow_agent_frame_materialization,
        }
    }

    pub(crate) async fn materialize_dispatch_runtime(
        &self,
        run: &LifecycleRun,
        plan: &DispatchPlan,
        orchestration_binding: Option<OrchestrationBindingRefs>,
    ) -> Result<MaterializedAgentRuntime, WorkflowApplicationError> {
        let agent = self.resolve_or_create_agent(run, plan).await?;
        let runtime_session_ref = self
            .resolve_or_create_runtime_session(plan, run, &agent)
            .await?;
        let frame_id = self
            .construct_launch_anchor_frame(&agent, runtime_session_ref)
            .await?;

        if let (Some(anchor_repo), Some(session_id)) = (self.anchor_repo, runtime_session_ref) {
            let anchor = match &orchestration_binding {
                Some(binding) => RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
                    session_id.to_string(),
                    run.id,
                    frame_id,
                    agent.id,
                    binding.orchestration_ref,
                    binding.node_path.clone(),
                    binding.attempt,
                ),
                None => RuntimeSessionExecutionAnchor::new_dispatch(
                    session_id.to_string(),
                    run.id,
                    frame_id,
                    agent.id,
                ),
            };
            anchor_repo.create_once(&anchor).await?;
            let binding_repo = self.delivery_binding_repo.ok_or_else(|| {
                WorkflowApplicationError::Internal(
                    "RuntimeSession current delivery binding 缺少 AgentRunDeliveryBindingRepository"
                        .to_string(),
                )
            })?;
            binding_repo
                .upsert(&AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Ready,
                    chrono::Utc::now(),
                ))
                .await?;
        }

        let runtime_refs = AgentRuntimeRefs::new(run.id, agent.id, frame_id, orchestration_binding);
        Ok(MaterializedAgentRuntime {
            agent,
            frame_id,
            runtime_session_ref,
            runtime_refs,
        })
    }

    pub(crate) async fn materialize_workflow_agent_node(
        &self,
        context: WorkflowAgentNodeRuntimeContext,
        request: WorkflowAgentNodeMaterializationRequest,
    ) -> Result<WorkflowAgentNodeMaterializationResult, WorkflowApplicationError> {
        let run = context.run;
        let agent = LifecycleAgent::new_root_for_user(
            run.id,
            run.project_id,
            AgentSource::WorkflowAgent,
            &run.created_by_user_id,
        )
        .with_bootstrap_status(agentdash_domain::workflow::bootstrap_status::NOT_APPLICABLE);
        self.agent_repo.create(&agent).await?;

        let runtime_session_ref = self
            .resolve_workflow_node_runtime_session(&run, &agent, request.runtime_policy)
            .await?;
        let frame_id = self
            .materialize_workflow_agent_node_frame(
                &run,
                &agent,
                runtime_session_ref,
                request.frame_created_by_id,
                request.orchestration_binding.clone(),
                context.lifecycle_key,
                context.activity,
                request.workflow_contract,
            )
            .await?;

        let anchor_repo = self.anchor_repo.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "Workflow AgentCall materialization 缺少 RuntimeSessionExecutionAnchorRepository"
                    .to_string(),
            )
        })?;
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            runtime_session_ref.to_string(),
            run.id,
            frame_id,
            agent.id,
            request.orchestration_binding.orchestration_ref,
            request.orchestration_binding.node_path.clone(),
            request.orchestration_binding.attempt,
        );
        anchor_repo.create_once(&anchor).await?;
        let binding_repo = self.delivery_binding_repo.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "Workflow AgentCall materialization 缺少 AgentRunDeliveryBindingRepository"
                    .to_string(),
            )
        })?;
        binding_repo
            .upsert(&AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Ready,
                chrono::Utc::now(),
            ))
            .await?;

        Ok(WorkflowAgentNodeMaterializationResult {
            runtime_refs: AgentRuntimeRefs::new(
                run.id,
                agent.id,
                frame_id,
                Some(request.orchestration_binding),
            ),
            delivery_runtime_ref: runtime_session_ref,
        })
    }

    async fn resolve_or_create_agent(
        &self,
        run: &LifecycleRun,
        plan: &DispatchPlan,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        match plan.agent_policy {
            AgentPolicy::Reuse | AgentPolicy::Resume => {
                if let Some(agent_id) = plan.parent_agent_id {
                    return self.resolve_explicit_reuse_agent(run, plan, agent_id).await;
                }
                let agents = self.agent_repo.list_by_run(run.id).await?;
                if let Some(existing) = agents.into_iter().find(|a| a.status == "active") {
                    return Ok(existing);
                }
                self.create_agent(run, plan).await
            }
            AgentPolicy::Create | AgentPolicy::SpawnChild => self.create_agent(run, plan).await,
        }
    }

    async fn resolve_explicit_reuse_agent(
        &self,
        run: &LifecycleRun,
        plan: &DispatchPlan,
        agent_id: Uuid,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        let agent = self.agent_repo.get(agent_id).await?.ok_or_else(|| {
            WorkflowApplicationError::BadRequest(format!("parent_agent_id {agent_id} 不存在"))
        })?;
        if agent.run_id != run.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "parent_agent_id {} 属于 run {}，不能复用到 run {}",
                agent.id, agent.run_id, run.id
            )));
        }
        if agent.project_id != plan.project_id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "parent_agent_id {} 属于 project {}，不能复用到 project {}",
                agent.id, agent.project_id, plan.project_id
            )));
        }
        if agent.status != "active" {
            return Err(WorkflowApplicationError::Conflict(format!(
                "parent_agent_id {} 当前不是 active",
                agent.id
            )));
        }
        Ok(agent)
    }

    async fn create_agent(
        &self,
        run: &LifecycleRun,
        plan: &DispatchPlan,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        let source = agent_source_from_execution_source(&plan.source);
        let agent = LifecycleAgent::new_root_for_user(
            run.id,
            plan.project_id,
            source,
            plan.created_by_user_id
                .as_deref()
                .unwrap_or(run.created_by_user_id.as_str()),
        );
        self.agent_repo.create(&agent).await?;
        Ok(agent)
    }

    async fn resolve_or_create_runtime_session(
        &self,
        plan: &DispatchPlan,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
    ) -> Result<Option<Uuid>, WorkflowApplicationError> {
        match plan.runtime_policy {
            RuntimePolicy::AttachExisting(id) | RuntimePolicy::ContinueCurrent(id) => Ok(Some(id)),
            RuntimePolicy::CreateRuntimeSession => {
                let creator = self.runtime_session_creator.ok_or_else(|| {
                    WorkflowApplicationError::Internal(
                        "RuntimePolicy::CreateRuntimeSession 缺少 RuntimeSessionCreationPort"
                            .to_string(),
                    )
                })?;
                let request = runtime_session_delivery_port::RuntimeSessionCreationRequest {
                    project_id: plan.project_id,
                    run_id: run.id,
                    agent_id: agent.id,
                    source: plan.source.clone(),
                };
                let result = creator
                    .create_runtime_session(request)
                    .await
                    .map_err(workflow_error_from_runtime_session_delivery_error)?;
                Ok(Some(result.runtime_session_id))
            }
        }
    }

    async fn resolve_workflow_node_runtime_session(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        runtime_policy: RuntimePolicy,
    ) -> Result<Uuid, WorkflowApplicationError> {
        match runtime_policy {
            RuntimePolicy::CreateRuntimeSession => {
                let creator = self.runtime_session_creator.ok_or_else(|| {
                    WorkflowApplicationError::Internal(
                        "Workflow AgentCall materialization 缺少 RuntimeSessionCreationPort"
                            .to_string(),
                    )
                })?;
                creator
                    .create_runtime_session(
                        runtime_session_delivery_port::RuntimeSessionCreationRequest {
                            project_id: run.project_id,
                            run_id: run.id,
                            agent_id: agent.id,
                            source: ExecutionSource::ParentAgent,
                        },
                    )
                    .await
                    .map_err(workflow_error_from_runtime_session_delivery_error)
                    .map(|result| result.runtime_session_id)
            }
            RuntimePolicy::AttachExisting(id) | RuntimePolicy::ContinueCurrent(id) => Ok(id),
        }
    }

    async fn construct_launch_anchor_frame(
        &self,
        agent: &LifecycleAgent,
        runtime_session_ref: Option<Uuid>,
    ) -> Result<Uuid, WorkflowApplicationError> {
        self.construct_launch_anchor_frame_with_created_by(
            agent,
            runtime_session_ref,
            runtime_session_ref.map(|value| value.to_string()),
        )
        .await
    }

    async fn construct_launch_anchor_frame_with_created_by(
        &self,
        agent: &LifecycleAgent,
        runtime_session_ref: Option<Uuid>,
        created_by_id: Option<String>,
    ) -> Result<Uuid, WorkflowApplicationError> {
        let frame_construction = self.frame_construction.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "Lifecycle dispatch 缺少 AgentRunFrameConstructionPort".to_string(),
            )
        })?;
        let runtime_session_ref = runtime_session_ref.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "Lifecycle launch anchor frame construction 缺少 RuntimeSession ref".to_string(),
            )
        })?;
        let outcome = frame_construction
            .execute_frame_construction_command(
                agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
                    run_id: agent.run_id,
                    agent_id: agent.id,
                    runtime_session_id: runtime_session_ref.to_string(),
                    created_by_id,
                },
            )
            .await
            .map_err(workflow_error_from_agent_frame_materialization_error)?;
        outcome.frame_id.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "AgentRunFrameConstructionPort 未返回 frame_id".to_string(),
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn materialize_workflow_agent_node_frame(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        runtime_session_ref: Uuid,
        created_by_id: Option<String>,
        orchestration_binding: OrchestrationBindingRefs,
        lifecycle_key: String,
        activity: agentdash_domain::workflow::ActivityDefinition,
        workflow_contract: Option<AgentProcedureContract>,
    ) -> Result<Uuid, WorkflowApplicationError> {
        let frame_materialization = self.workflow_agent_frame_materialization.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "Workflow AgentCall materialization 缺少 WorkflowAgentNodeFrameMaterializationPort"
                    .to_string(),
            )
        })?;
        let outcome = frame_materialization
            .materialize_workflow_agent_node_frame(
                workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationInput {
                    run_id: run.id,
                    project_id: run.project_id,
                    agent_id: agent.id,
                    runtime_session_id: runtime_session_ref.to_string(),
                    created_by_id,
                    orchestration_id: orchestration_binding.orchestration_ref,
                    node_path: orchestration_binding.node_path,
                    attempt: orchestration_binding.attempt,
                    lifecycle_key,
                    activity,
                    workflow_contract,
                    base_vfs: None,
                    inherited_executor_config: None,
                    ready_port_keys: Default::default(),
                },
            )
            .await
            .map_err(workflow_error_from_agent_frame_materialization_error)?;
        outcome.frame_id.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "WorkflowAgentNodeFrameMaterializationPort 未返回 frame_id".to_string(),
            )
        })
    }
}

fn agent_source_from_execution_source(source: &ExecutionSource) -> AgentSource {
    match source {
        ExecutionSource::User | ExecutionSource::ProjectAgent | ExecutionSource::Api => {
            AgentSource::ProjectAgent
        }
        ExecutionSource::Routine => AgentSource::Routine,
        ExecutionSource::ParentAgent => AgentSource::Subagent,
    }
}
