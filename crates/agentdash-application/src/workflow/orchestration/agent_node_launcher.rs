use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentProcedureExecutionSpec, AgentProcedureRepository,
    AgentReusePolicy, ExecutorRunRef, ExecutorSpec, LifecycleAgent, LifecycleAgentRepository,
    LifecycleRun, RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    RuntimeSessionPolicy,
};

use crate::workflow::frame_builder::AgentFrameBuilder;
use crate::workflow::{
    RuntimeSessionCreationRequest, RuntimeSessionCreator, WorkflowApplicationError,
};

use super::executor_launcher::LaunchedAgentNode;
use super::ready_node::{ReadyNodeView, RuntimeNodeCoordinate};
use super::runtime::OrchestrationRuntimeEvent;

#[derive(Clone)]
pub(super) struct AgentNodeLauncher {
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    runtime_session_creator: Arc<dyn RuntimeSessionCreator>,
}

impl AgentNodeLauncher {
    pub(super) fn new(
        agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        runtime_session_creator: Arc<dyn RuntimeSessionCreator>,
    ) -> Self {
        Self {
            agent_procedure_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            runtime_session_creator,
        }
    }

    pub(super) async fn launch(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<AgentNodeLaunchOutcome, WorkflowApplicationError> {
        let executor = ReadyNodeView::for_coordinate(run, coordinate)?
            .plan_node
            .executor
            .clone();
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

        if let AgentProcedureExecutionSpec::ByKey { procedure_key } = &procedure {
            match self
                .agent_procedure_repo
                .get_by_project_and_key(run.project_id, procedure_key)
                .await?
            {
                Some(_) => {}
                None => {
                    return Ok(AgentNodeLaunchOutcome::blocked(
                        "agent_procedure_not_found",
                        format!("AgentProcedure `{procedure_key}` 不存在"),
                        false,
                    ));
                }
            };
        }

        let (mut agent, session_id) = match (agent_reuse_policy, runtime_session_policy) {
            (AgentReusePolicy::CreateActivityAgent, RuntimeSessionPolicy::CreateNew) => {
                let agent = LifecycleAgent::new_root(run.id, run.project_id, "workflow_agent")
                    .with_bootstrap_status(
                        agentdash_domain::workflow::bootstrap_status::NOT_APPLICABLE,
                    );
                self.lifecycle_agent_repo.create(&agent).await?;
                let session_id = self
                    .runtime_session_creator
                    .create_runtime_session(RuntimeSessionCreationRequest {
                        project_id: run.project_id,
                        run_id: run.id,
                        agent_id: agent.id,
                        source: agentdash_domain::workflow::ExecutionSource::ParentAgent,
                    })
                    .await?
                    .to_string();
                (agent, session_id)
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

        let frame = self
            .create_frame(&agent, coordinate, Some(session_id.clone()))
            .await?;
        agent.set_current_frame(frame.id);
        self.lifecycle_agent_repo.update(&agent).await?;
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            session_id.clone(),
            run.id,
            frame.id,
            agent.id,
            coordinate.orchestration_id,
            coordinate.node_path.clone(),
            coordinate.attempt,
        );
        self.execution_anchor_repo.upsert(&anchor).await?;

        Ok(AgentNodeLaunchOutcome::Launched {
            launched: LaunchedAgentNode {
                run_id: anchor.run_id,
                orchestration_id: coordinate.orchestration_id,
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                runtime_session_id: session_id.clone(),
            },
            event: OrchestrationRuntimeEvent::NodeStarted {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                executor_run_ref: Some(ExecutorRunRef::RuntimeSession { session_id }),
                timestamp: chrono::Utc::now(),
            },
        })
    }

    async fn create_frame(
        &self,
        agent: &LifecycleAgent,
        coordinate: &RuntimeNodeCoordinate,
        runtime_session_ref: Option<String>,
    ) -> Result<AgentFrame, WorkflowApplicationError> {
        let mut builder = AgentFrameBuilder::new(agent.id).with_created_by(
            "orchestration_executor",
            Some(format!(
                "{}:{}#{}",
                coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
            )),
        );
        if let Some(session_id) = runtime_session_ref {
            builder = builder.with_runtime_session(session_id);
        }
        Ok(builder.build(self.agent_frame_repo.as_ref()).await?)
    }
}

pub(super) enum AgentNodeLaunchOutcome {
    Launched {
        launched: LaunchedAgentNode,
        event: OrchestrationRuntimeEvent,
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
