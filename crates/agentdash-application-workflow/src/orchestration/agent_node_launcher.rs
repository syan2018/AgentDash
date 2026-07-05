use std::sync::Arc;

use agentdash_application_ports::lifecycle_materialization::{
    WorkflowAgentNodeMaterializationPort, WorkflowAgentNodeMaterializationRequest,
};
use agentdash_domain::workflow::{
    AgentProcedureExecutionSpec, AgentProcedureRepository, AgentReusePolicy, ExecutorSpec,
    LifecycleRun, OrchestrationBindingRefs, RuntimePolicy, RuntimeSessionPolicy,
};

use crate::WorkflowApplicationError;

use super::executor_launcher::LaunchedAgentNode;
use super::ready_node::{ReadyNodeView, RuntimeNodeCoordinate};
use super::runtime::OrchestrationRuntimeEvent;

#[derive(Clone)]
pub(super) struct AgentNodeLauncher {
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    workflow_agent_node_materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
}

impl AgentNodeLauncher {
    pub(super) fn new(
        agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
        workflow_agent_node_materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
    ) -> Self {
        Self {
            agent_procedure_repo,
            workflow_agent_node_materialization,
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
        let materialized = self
            .workflow_agent_node_materialization
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
            event: Box::new(OrchestrationRuntimeEvent::NodeClaimed {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
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
