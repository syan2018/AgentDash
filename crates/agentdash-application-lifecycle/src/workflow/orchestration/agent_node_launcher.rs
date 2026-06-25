use std::sync::Arc;

use agentdash_application_ports::runtime_session_delivery::RuntimeSessionCreationPort;
use agentdash_application_ports::workflow_agent_frame_materialization::WorkflowAgentNodeFrameMaterializationPort;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentProcedureExecutionSpec, AgentProcedureRepository, AgentReusePolicy,
    ExecutorRunRef, ExecutorSpec, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, OrchestrationBindingRefs, RuntimePolicy,
    RuntimeSessionExecutionAnchorRepository, RuntimeSessionPolicy, WorkflowGraphRepository,
};

use crate::lifecycle::{
    LifecycleDispatchService, WorkflowAgentNodeMaterializationRequest, WorkflowApplicationError,
};

use super::executor_launcher::LaunchedAgentNode;
use super::ready_node::{ReadyNodeView, RuntimeNodeCoordinate};
use super::runtime::OrchestrationRuntimeEvent;

#[derive(Clone)]
pub(super) struct AgentNodeLauncher {
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    lifecycle_gate_repo: Arc<dyn agentdash_domain::workflow::LifecycleGateRepository>,
    agent_lineage_repo: Arc<dyn agentdash_domain::workflow::AgentLineageRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    runtime_session_creator: Arc<dyn RuntimeSessionCreationPort>,
    workflow_agent_frame_materialization: Arc<dyn WorkflowAgentNodeFrameMaterializationPort>,
}

impl AgentNodeLauncher {
    pub(super) fn new(
        agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
        lifecycle_gate_repo: Arc<dyn agentdash_domain::workflow::LifecycleGateRepository>,
        agent_lineage_repo: Arc<dyn agentdash_domain::workflow::AgentLineageRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        runtime_session_creator: Arc<dyn RuntimeSessionCreationPort>,
        workflow_agent_frame_materialization: Arc<dyn WorkflowAgentNodeFrameMaterializationPort>,
    ) -> Self {
        Self {
            agent_procedure_repo,
            lifecycle_run_repo,
            workflow_graph_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            lifecycle_subject_association_repo,
            lifecycle_gate_repo,
            agent_lineage_repo,
            execution_anchor_repo,
            runtime_session_creator,
            workflow_agent_frame_materialization,
        }
    }

    pub(super) async fn launch(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<AgentNodeLaunchOutcome, WorkflowApplicationError> {
        let ready_node = ReadyNodeView::for_coordinate(run, coordinate)?;
        let plan_node = ready_node.plan_node;
        let executor = plan_node.executor.clone();
        let Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_session_policy,
        }) = executor
        else {
            return Ok(AgentNodeLaunchOutcome::blocked(
                "agent_executor_missing",
                "AgentCall node 缺少 AgentProcedure executor spec",
                false,
            ));
        };

        let loaded_workflow =
            if let AgentProcedureExecutionSpec::ByKey { procedure_key } = &procedure {
                match self
                    .agent_procedure_repo
                    .get_by_project_and_key(run.project_id, procedure_key)
                    .await?
                {
                    Some(workflow) => Some(workflow),
                    None => {
                        return Ok(AgentNodeLaunchOutcome::blocked(
                            "agent_procedure_not_found",
                            format!("AgentProcedure `{procedure_key}` 不存在"),
                            false,
                        ));
                    }
                }
            } else {
                None
            };
        let workflow_contract = procedure.snapshot_contract().cloned().or_else(|| {
            loaded_workflow
                .as_ref()
                .map(|workflow| workflow.contract.clone())
        });

        let runtime_policy = match (agent_reuse_policy, runtime_session_policy) {
            (AgentReusePolicy::CreateActivityAgent, RuntimeSessionPolicy::CreateNew) => {
                RuntimePolicy::CreateRuntimeSession
            }
            (
                AgentReusePolicy::ContinueCurrentAgent,
                RuntimeSessionPolicy::DeliverToCurrentTrace,
            ) => {
                return Ok(AgentNodeLaunchOutcome::blocked(
                    "agent_executor_policy_not_supported",
                    "ContinueCurrentAgent + DeliverToCurrentTrace 需要 connector delivery surface，当前 orchestration executor 不伪造已投递状态",
                    false,
                ));
            }
            _ => {
                return Ok(AgentNodeLaunchOutcome::blocked(
                    "agent_executor_policy_not_supported",
                    "AgentCall executor policy 当前 scheduler 不支持",
                    false,
                ));
            }
        };

        let orchestration_binding = OrchestrationBindingRefs::new(
            coordinate.orchestration_id,
            coordinate.node_path.clone(),
            coordinate.attempt,
        );
        let lifecycle_dispatch = LifecycleDispatchService::new(
            self.lifecycle_run_repo.as_ref(),
            self.workflow_graph_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
            self.lifecycle_subject_association_repo.as_ref(),
            self.lifecycle_gate_repo.as_ref(),
            self.agent_lineage_repo.as_ref(),
        )
        .with_anchor_repo(self.execution_anchor_repo.as_ref())
        .with_runtime_session_creator(self.runtime_session_creator.as_ref())
        .with_workflow_agent_frame_materialization_port(
            self.workflow_agent_frame_materialization.as_ref(),
        );
        let materialized = lifecycle_dispatch
            .materialize_workflow_agent_node(WorkflowAgentNodeMaterializationRequest {
                run_id: run.id,
                orchestration_binding,
                runtime_policy,
                frame_created_by_id: Some(format!(
                    "{}:{}#{}",
                    coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
                )),
                workflow_contract,
            })
            .await?;
        let session_id = materialized.delivery_runtime_ref.to_string();

        Ok(AgentNodeLaunchOutcome::Launched {
            launched: LaunchedAgentNode {
                run_id: materialized.runtime_refs.run_ref,
                orchestration_id: coordinate.orchestration_id,
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                runtime_session_id: session_id.clone(),
            },
            event: Box::new(OrchestrationRuntimeEvent::NodeStarted {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                executor_run_ref: Some(ExecutorRunRef::RuntimeSession { session_id }),
                timestamp: chrono::Utc::now(),
            }),
        })
    }
}

pub(super) enum AgentNodeLaunchOutcome {
    Launched {
        launched: LaunchedAgentNode,
        event: Box<OrchestrationRuntimeEvent>,
    },
    Blocked {
        code: String,
        message: String,
        retryable: bool,
    },
}

impl AgentNodeLaunchOutcome {
    fn blocked(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self::Blocked {
            code: code.to_string(),
            message: message.into(),
            retryable,
        }
    }
}
