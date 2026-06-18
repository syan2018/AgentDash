use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentFrameRepository, AgentProcedureContract, AgentProcedureExecutionSpec,
    AgentProcedureRepository, AgentReusePolicy, ExecutorRunRef, ExecutorSpec,
    LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, OrchestrationBindingRefs, OrchestrationInstance,
    PlanNode, RuntimePolicy, RuntimeSessionExecutionAnchorRepository, RuntimeSessionPolicy,
    WorkflowGraphRepository,
};
use async_trait::async_trait;

use crate::agent_run::frame::builder::AgentFrameBuilder;
use crate::lifecycle::projection::{
    activity_definition_from_plan_node, lifecycle_identity_from_orchestration,
};
use crate::lifecycle::{
    LifecycleDispatchService, RuntimeSessionCreator, WorkflowAgentNodeFrameComposer,
    WorkflowAgentNodeMaterializationRequest, WorkflowApplicationError,
};
use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::{LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit};

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
    runtime_session_creator: Arc<dyn RuntimeSessionCreator>,
    frame_composer: Arc<dyn AgentNodeFrameComposer>,
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
        runtime_session_creator: Arc<dyn RuntimeSessionCreator>,
        frame_composer: Arc<dyn AgentNodeFrameComposer>,
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
            frame_composer,
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
        let snapshot_contract = procedure.snapshot_contract();
        let workflow_contract = snapshot_contract
            .or_else(|| loaded_workflow.as_ref().map(|workflow| &workflow.contract));
        let snapshot_label = snapshot_workflow_label(&procedure);
        let workflow_label = loaded_workflow
            .as_ref()
            .map(|workflow| format!("`{}` ({})", workflow.key, workflow.name))
            .or(snapshot_label);

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
        let frame_composer = AgentNodeMaterializationFrameComposer {
            inner: self.frame_composer.clone(),
            coordinate: coordinate.clone(),
            plan_node,
            workflow_contract,
            workflow_label: workflow_label.as_deref(),
        };
        let dispatch_service = LifecycleDispatchService::new(
            self.lifecycle_run_repo.as_ref(),
            self.workflow_graph_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
            self.lifecycle_subject_association_repo.as_ref(),
            self.lifecycle_gate_repo.as_ref(),
            self.agent_lineage_repo.as_ref(),
        )
        .with_anchor_repo(self.execution_anchor_repo.as_ref())
        .with_runtime_session_creator(self.runtime_session_creator.as_ref());
        let materialized = dispatch_service
            .materialize_workflow_agent_node(
                WorkflowAgentNodeMaterializationRequest {
                    run_id: run.id,
                    orchestration_binding,
                    runtime_policy,
                    frame_created_by_id: Some(format!(
                        "{}:{}#{}",
                        coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
                    )),
                },
                &frame_composer,
            )
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

struct AgentNodeMaterializationFrameComposer<'a> {
    inner: Arc<dyn AgentNodeFrameComposer>,
    coordinate: RuntimeNodeCoordinate,
    plan_node: &'a PlanNode,
    workflow_contract: Option<&'a AgentProcedureContract>,
    workflow_label: Option<&'a str>,
}

#[async_trait]
impl WorkflowAgentNodeFrameComposer for AgentNodeMaterializationFrameComposer<'_> {
    async fn compose_workflow_agent_node_frame(
        &self,
        builder: AgentFrameBuilder,
        run: &LifecycleRun,
        runtime_session_ref: uuid::Uuid,
    ) -> Result<AgentFrameBuilder, WorkflowApplicationError> {
        let runtime_session_id = runtime_session_ref.to_string();
        self.inner
            .compose_frame(
                builder,
                run,
                &self.coordinate,
                self.plan_node,
                self.workflow_contract,
                self.workflow_label,
                Some(runtime_session_id.as_str()),
            )
            .await
    }
}

#[async_trait]
pub(super) trait AgentNodeFrameComposer: Send + Sync {
    async fn compose_frame(
        &self,
        builder: AgentFrameBuilder,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
        plan_node: &PlanNode,
        workflow_contract: Option<&AgentProcedureContract>,
        workflow_label: Option<&str>,
        runtime_session_id: Option<&str>,
    ) -> Result<AgentFrameBuilder, WorkflowApplicationError>;
}

pub(super) struct RepositoryAgentNodeFrameComposer {
    repos: RepositorySet,
    platform_config: SharedPlatformConfig,
}

impl RepositoryAgentNodeFrameComposer {
    pub(super) fn new(repos: RepositorySet, platform_config: SharedPlatformConfig) -> Self {
        Self {
            repos,
            platform_config,
        }
    }
}

#[async_trait]
impl AgentNodeFrameComposer for RepositoryAgentNodeFrameComposer {
    async fn compose_frame(
        &self,
        builder: AgentFrameBuilder,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
        plan_node: &PlanNode,
        workflow_contract: Option<&AgentProcedureContract>,
        workflow_label: Option<&str>,
        runtime_session_id: Option<&str>,
    ) -> Result<AgentFrameBuilder, WorkflowApplicationError> {
        let orchestration = orchestration_for_coordinate(run, coordinate)?;
        let lifecycle_identity = lifecycle_identity_from_orchestration(orchestration);
        let activity = activity_definition_from_plan_node(plan_node);
        let (builder, _extras) = compose_lifecycle_node_to_frame_with_audit(
            builder,
            &self.repos,
            self.platform_config.as_ref(),
            LifecycleNodeSpec {
                run,
                orchestration_id: coordinate.orchestration_id,
                node_path: &coordinate.node_path,
                attempt: coordinate.attempt,
                lifecycle_key: &lifecycle_identity.key,
                activity: &activity,
                workflow_contract,
                base_vfs: None,
                workflow_label,
                inherited_executor_config: None,
            },
            None,
            runtime_session_id,
        )
        .await
        .map_err(WorkflowApplicationError::Internal)?;
        Ok(builder)
    }
}

fn orchestration_for_coordinate<'a>(
    run: &'a LifecycleRun,
    coordinate: &RuntimeNodeCoordinate,
) -> Result<&'a OrchestrationInstance, WorkflowApplicationError> {
    run.orchestrations
        .iter()
        .find(|item| item.orchestration_id == coordinate.orchestration_id)
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "LifecycleRun {} 中不存在 orchestration {}",
                run.id, coordinate.orchestration_id
            ))
        })
}

fn snapshot_workflow_label(procedure: &AgentProcedureExecutionSpec) -> Option<String> {
    match procedure {
        AgentProcedureExecutionSpec::Snapshot {
            procedure_key,
            name,
            ..
        } => Some(match (procedure_key.as_deref(), name.as_deref()) {
            (Some(key), Some(name)) => format!("`{key}` ({name})"),
            (Some(key), None) => format!("`{key}`"),
            (None, Some(name)) => name.to_string(),
            (None, None) => "inline workflow".to_string(),
        }),
        AgentProcedureExecutionSpec::ByKey { .. } => None,
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
