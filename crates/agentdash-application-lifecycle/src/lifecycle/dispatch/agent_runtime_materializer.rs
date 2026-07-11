use uuid::Uuid;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::lifecycle_materialization::{
    WorkflowAgentNodeMaterializationRequest, WorkflowAgentNodeMaterializationResult,
};
use agentdash_application_ports::workflow_agent_frame_materialization as workflow_node_frame_port;
use agentdash_domain::workflow::{
    AgentPolicy, AgentProcedureContract, AgentRuntimeRefs, AgentSource, ExecutionSource,
    LifecycleAgent, LifecycleAgentRepository, LifecycleRun, OrchestrationBindingRefs,
};

use crate::lifecycle::WorkflowApplicationError;

use super::plan::{
    DispatchPlan, MaterializedAgentRuntime, WorkflowAgentNodeRuntimeContext,
    workflow_error_from_agent_frame_materialization_error,
};

pub(crate) struct AgentRuntimeMaterializer<'a> {
    agent_repo: &'a dyn LifecycleAgentRepository,
    frame_construction:
        Option<&'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort>,
    workflow_agent_frame_materialization:
        Option<&'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort>,
}

impl<'a> AgentRuntimeMaterializer<'a> {
    pub(crate) fn new(
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_construction: Option<
            &'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort,
        >,
        workflow_agent_frame_materialization: Option<
            &'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort,
        >,
    ) -> Self {
        Self {
            agent_repo,
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
        let frame_id = self.construct_launch_anchor_frame(&agent, plan).await?;

        let runtime_refs = AgentRuntimeRefs::new(run.id, agent.id, frame_id, orchestration_binding);
        Ok(MaterializedAgentRuntime {
            agent,
            frame_id,
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

        let frame_id = self
            .materialize_workflow_agent_node_frame(
                &run,
                &agent,
                request.frame_created_by_id,
                request.orchestration_binding.clone(),
                context.lifecycle_key,
                context.activity,
                request.workflow_contract,
            )
            .await?;

        Ok(WorkflowAgentNodeMaterializationResult {
            runtime_refs: AgentRuntimeRefs::new(
                run.id,
                agent.id,
                frame_id,
                Some(request.orchestration_binding),
            ),
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
        let mut agent = LifecycleAgent::new_root_for_user(
            run.id,
            plan.project_id,
            source,
            plan.created_by_user_id
                .as_deref()
                .unwrap_or(run.created_by_user_id.as_str()),
        );
        if let Some(project_agent_id) = plan.project_agent_id {
            agent = agent.with_project_agent(project_agent_id);
        }
        self.agent_repo.create(&agent).await?;
        Ok(agent)
    }

    async fn construct_launch_anchor_frame(
        &self,
        agent: &LifecycleAgent,
        plan: &DispatchPlan,
    ) -> Result<Uuid, WorkflowApplicationError> {
        let frame_construction = self.frame_construction.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "Lifecycle dispatch 缺少 AgentRunFrameConstructionPort".to_string(),
            )
        })?;
        let outcome = frame_construction
            .execute_frame_construction_command(
                agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
                    run_id: agent.run_id,
                    agent_id: agent.id,
                    runtime_session_id: None,
                    created_by_id: None,
                    execution_profile: plan.execution_profile_override.clone(),
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
                    runtime_session_id: None,
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
