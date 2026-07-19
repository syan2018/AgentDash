use uuid::Uuid;

use agentdash_application_ports::workflow_graph_planning as workflow_graph_planning_port;
use agentdash_application_ports::workflow_graph_planning::WorkflowGraphPlanningPort;
use agentdash_application_workflow::ApplicationWorkflowGraphPlanner;
use agentdash_application_workflow::orchestration::{
    ROOT_ORCHESTRATION_ROLE, activate_orchestration,
};
use agentdash_domain::workflow::LifecycleRunRepository;
use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, BashExecExecutorSpec,
    ExecutionSource, ExecutorSpec, FunctionActivityExecutorSpec, LifecycleRun,
    LifecycleRunStartDispatchResult, LifecycleRunStartIntent, OrchestrationBindingRefs,
    OrchestrationInstance, OrchestrationPlanSnapshot, OrchestrationSourceRef, RunPolicy,
    WorkflowGraph, WorkflowGraphRef, WorkflowGraphRepository,
};

use crate::lifecycle::WorkflowApplicationError;

use super::plan::{
    DispatchPlan, PreparedGraphDispatch, WorkflowAgentNodeRuntimeContext,
    workflow_error_from_workflow_graph_planning_error,
};

pub(crate) struct RunOrchestrationStarter<'a> {
    run_repo: &'a dyn LifecycleRunRepository,
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    workflow_graph_planner: Option<&'a dyn workflow_graph_planning_port::WorkflowGraphPlanningPort>,
}

impl<'a> RunOrchestrationStarter<'a> {
    pub(crate) fn new(
        run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        workflow_graph_planner: Option<
            &'a dyn workflow_graph_planning_port::WorkflowGraphPlanningPort,
        >,
    ) -> Self {
        Self {
            run_repo,
            workflow_graph_repo,
            workflow_graph_planner,
        }
    }

    pub(crate) async fn start_lifecycle_run(
        &self,
        intent: &LifecycleRunStartIntent,
    ) -> Result<LifecycleRunStartDispatchResult, WorkflowApplicationError> {
        let planned_graph = self
            .plan_workflow_graph(intent.project_id, &intent.workflow_graph_ref)
            .await?;
        let mut run = LifecycleRun::new_control_for_user(
            intent.project_id,
            LifecycleRun::SYSTEM_CREATED_BY_USER_ID,
        );
        let orchestration_binding = ensure_workflow_graph_orchestration(
            &mut run,
            &planned_graph.graph,
            ROOT_ORCHESTRATION_ROLE,
            planned_graph.plan_snapshot,
        )?;
        self.run_repo.create(&run).await?;

        Ok(LifecycleRunStartDispatchResult {
            run_ref: run.id,
            orchestration_ref: orchestration_binding.orchestration_ref,
        })
    }

    pub(crate) async fn prepare_graph_dispatch(
        &self,
        plan: &DispatchPlan,
        workflow_graph_ref: &WorkflowGraphRef,
    ) -> Result<PreparedGraphDispatch, WorkflowApplicationError> {
        let planned_graph = self
            .plan_workflow_graph(plan.project_id, workflow_graph_ref)
            .await?;
        let mut run = self.resolve_or_create_run(plan).await?;
        let orchestration_binding = ensure_workflow_graph_orchestration(
            &mut run,
            &planned_graph.graph,
            orchestration_role_for_dispatch(plan),
            planned_graph.plan_snapshot,
        )?;
        self.run_repo.update(&run).await?;

        Ok(PreparedGraphDispatch {
            run,
            orchestration_binding,
        })
    }

    pub(crate) async fn resolve_or_create_plain_run(
        &self,
        plan: &DispatchPlan,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        match (&plan.run_policy, plan.parent_run_id) {
            (RunPolicy::ReuseExisting | RunPolicy::AppendGraph, Some(run_id)) => {
                let run = self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
                    WorkflowApplicationError::BadRequest(format!("parent_run_id {run_id} 不存在"))
                })?;
                Ok(run)
            }
            (RunPolicy::ReuseExisting, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::ReuseExisting 需要 parent_run_id".to_string(),
            )),
            (RunPolicy::AppendGraph, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::AppendGraph 需要 parent_run_id".to_string(),
            )),
            _ => {
                let run = LifecycleRun::new_plain_for_user(
                    plan.project_id,
                    plan.created_by_user_id
                        .as_deref()
                        .unwrap_or(LifecycleRun::SYSTEM_CREATED_BY_USER_ID),
                );
                self.run_repo.create(&run).await?;
                Ok(run)
            }
        }
    }

    pub(crate) async fn workflow_agent_node_context(
        &self,
        run_id: Uuid,
        binding: &OrchestrationBindingRefs,
    ) -> Result<WorkflowAgentNodeRuntimeContext, WorkflowApplicationError> {
        let run = self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
            WorkflowApplicationError::BadRequest(format!("LifecycleRun {} 不存在", run_id))
        })?;
        ensure_orchestration_node_binding(&run, binding)?;
        let orchestration = orchestration_for_binding(&run, binding)?;
        let plan_node = orchestration
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_path == binding.node_path)
            .ok_or_else(|| {
                WorkflowApplicationError::Internal(format!(
                    "orchestration {} 中不存在 node_path {}",
                    binding.orchestration_ref, binding.node_path
                ))
            })?;
        let lifecycle_key = lifecycle_key_from_orchestration(orchestration);
        let activity = activity_definition_from_plan_node(plan_node);

        Ok(WorkflowAgentNodeRuntimeContext {
            run,
            lifecycle_key,
            activity,
        })
    }

    async fn plan_workflow_graph(
        &self,
        project_id: Uuid,
        workflow_graph_ref: &WorkflowGraphRef,
    ) -> Result<workflow_graph_planning_port::PlannedWorkflowGraph, WorkflowApplicationError> {
        let request = workflow_graph_planning_port::WorkflowGraphPlanningRequest {
            project_id,
            workflow_graph_ref: workflow_graph_ref.clone(),
        };
        match self.workflow_graph_planner {
            Some(planner) => planner.plan_workflow_graph(request).await,
            None => {
                ApplicationWorkflowGraphPlanner::new(self.workflow_graph_repo)
                    .plan_workflow_graph(request)
                    .await
            }
        }
        .map_err(workflow_error_from_workflow_graph_planning_error)
    }

    async fn resolve_or_create_run(
        &self,
        plan: &DispatchPlan,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        match (&plan.run_policy, plan.parent_run_id) {
            (RunPolicy::ReuseExisting | RunPolicy::AppendGraph, Some(run_id)) => {
                let run = self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
                    WorkflowApplicationError::BadRequest(format!("parent_run_id {run_id} 不存在"))
                })?;
                Ok(run)
            }
            (RunPolicy::ReuseExisting, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::ReuseExisting 需要 parent_run_id".to_string(),
            )),
            (RunPolicy::AppendGraph, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::AppendGraph 需要 parent_run_id".to_string(),
            )),
            _ => {
                let run = create_lifecycle_run(plan);
                self.run_repo.create(&run).await?;
                Ok(run)
            }
        }
    }
}

fn activity_definition_from_plan_node(
    plan_node: &agentdash_domain::workflow::PlanNode,
) -> ActivityDefinition {
    let executor = match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_thread_policy,
        }) => ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
            procedure_key: procedure
                .procedure_key()
                .unwrap_or("__inline_agent_procedure")
                .to_string(),
            agent_reuse_policy: *agent_reuse_policy,
            runtime_thread_policy: *runtime_thread_policy,
        }),
        Some(ExecutorSpec::Function { spec }) => ActivityExecutorSpec::Function(spec.clone()),
        Some(ExecutorSpec::Human { spec }) => ActivityExecutorSpec::Human(spec.clone()),
        Some(ExecutorSpec::LocalEffect { .. })
        | Some(ExecutorSpec::ExtensionAction { .. })
        | None => ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(
            BashExecExecutorSpec {
                command: "true".to_string(),
                args: Vec::new(),
                working_directory: None,
            },
        )),
    };
    ActivityDefinition {
        key: plan_node.node_path.clone(),
        description: plan_node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("description"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| plan_node.label.clone())
            .unwrap_or_default(),
        executor,
        input_ports: plan_node.input_ports.clone(),
        output_ports: plan_node.output_ports.clone(),
        completion_policy: plan_node.completion_policy.clone().unwrap_or_default(),
        iteration_policy: plan_node.iteration_policy.clone().unwrap_or_default(),
        join_policy: plan_node.join_policy.unwrap_or_default(),
    }
}

fn lifecycle_key_from_orchestration(orchestration: &OrchestrationInstance) -> String {
    orchestration
        .plan_snapshot
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("source"))
        .and_then(|source| source.get("key"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| match &orchestration.source_ref {
            OrchestrationSourceRef::WorkflowGraph { graph_id, .. } => {
                format!("workflow_graph:{graph_id}")
            }
            OrchestrationSourceRef::RunScriptArtifact { artifact_id, .. } => {
                format!("run_script:{artifact_id}")
            }
            OrchestrationSourceRef::WorkflowScript { script_id, .. } => {
                format!("workflow_script:{script_id}")
            }
            OrchestrationSourceRef::Inline { source_digest } => {
                let digest = source_digest
                    .strip_prefix("sha256:")
                    .unwrap_or(source_digest);
                format!("inline:{}", digest.get(..12).unwrap_or(digest))
            }
        })
}

fn create_lifecycle_run(plan: &DispatchPlan) -> LifecycleRun {
    LifecycleRun::new_control_for_user(
        plan.project_id,
        plan.created_by_user_id
            .as_deref()
            .unwrap_or(LifecycleRun::SYSTEM_CREATED_BY_USER_ID),
    )
}

fn ensure_workflow_graph_orchestration(
    run: &mut LifecycleRun,
    workflow_graph: &WorkflowGraph,
    role: &str,
    plan_snapshot: OrchestrationPlanSnapshot,
) -> Result<OrchestrationBindingRefs, WorkflowApplicationError> {
    if let Some(existing) = run.orchestrations.iter().find(|orchestration| {
        orchestration.role == role
            && orchestration.plan_snapshot.plan_digest == plan_snapshot.plan_digest
    }) {
        return orchestration_entry_binding(existing);
    }

    let source_ref = OrchestrationSourceRef::WorkflowGraph {
        graph_id: workflow_graph.id,
        graph_version: Some(workflow_graph.version),
    };
    let orchestration = activate_orchestration(role.to_string(), source_ref, plan_snapshot);
    let binding = orchestration_entry_binding(&orchestration)?;
    run.add_orchestration(orchestration);
    Ok(binding)
}

fn orchestration_entry_binding(
    orchestration: &OrchestrationInstance,
) -> Result<OrchestrationBindingRefs, WorkflowApplicationError> {
    let entry_node_id = orchestration
        .activation
        .ready_node_ids
        .first()
        .or_else(|| orchestration.plan_snapshot.entry_node_ids.first())
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "OrchestrationInstance {} 缺少 entry runtime node",
                orchestration.orchestration_id
            ))
        })?;
    let node = orchestration
        .node_tree
        .iter()
        .find(|node| node.node_id == *entry_node_id)
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "OrchestrationInstance {} entry node {} 尚未 materialize",
                orchestration.orchestration_id, entry_node_id
            ))
        })?;
    Ok(OrchestrationBindingRefs::new(
        orchestration.orchestration_id,
        node.node_path.clone(),
        node.attempt,
    ))
}

fn ensure_orchestration_node_binding(
    run: &LifecycleRun,
    binding: &OrchestrationBindingRefs,
) -> Result<(), WorkflowApplicationError> {
    let orchestration = orchestration_for_binding(run, binding)?;
    let exists = orchestration
        .node_tree
        .iter()
        .any(|node| node.node_path == binding.node_path && node.attempt == binding.attempt);
    if exists {
        Ok(())
    } else {
        Err(WorkflowApplicationError::Internal(format!(
            "Orchestration {} 中不存在节点 {}#{}",
            binding.orchestration_ref, binding.node_path, binding.attempt
        )))
    }
}

fn orchestration_for_binding<'a>(
    run: &'a LifecycleRun,
    binding: &OrchestrationBindingRefs,
) -> Result<&'a OrchestrationInstance, WorkflowApplicationError> {
    run.orchestrations
        .iter()
        .find(|item| item.orchestration_id == binding.orchestration_ref)
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "LifecycleRun {} 中不存在 orchestration {}",
                run.id, binding.orchestration_ref
            ))
        })
}

fn orchestration_role_for_dispatch(plan: &DispatchPlan) -> &str {
    if matches!(plan.run_policy, RunPolicy::AppendGraph) {
        append_orchestration_role_from_source(&plan.source)
    } else {
        ROOT_ORCHESTRATION_ROLE
    }
}

fn append_orchestration_role_from_source(source: &ExecutionSource) -> &'static str {
    match source {
        ExecutionSource::ParentAgent => "task_execution",
        ExecutionSource::Routine => "routine_execution",
        _ => "append",
    }
}
