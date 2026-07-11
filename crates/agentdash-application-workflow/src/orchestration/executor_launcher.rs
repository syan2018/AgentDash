use std::sync::Arc;

use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisioner;
use agentdash_application_ports::lifecycle_materialization::WorkflowAgentNodeMaterializationPort;
use agentdash_domain::workflow::{
    AgentProcedureRepository, ArtifactAliasPolicy, ExecutorRunRef, LifecycleGateRepository,
    LifecycleRun, LifecycleRunRepository, PlanNode, PlanNodeKind, RuntimeNodeError,
};
use agentdash_spi::FunctionRunner;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{WorkflowApplicationError, WorkflowRepositorySet};

use super::agent_node_launcher::{AgentNodeLaunchOutcome, AgentNodeLauncher};
use super::function_node_runner::FunctionNodeRunner;
use super::human_gate_launcher::{HumanGateLauncher, HumanGateOpenOutcome};
use super::ready_node::{ReadyNodeView, RuntimeNodeCoordinate};
use super::runtime::{OrchestrationRuntimeEvent, apply_orchestration_event_to_run};

const MAX_DRAIN_STEPS: usize = 128;

#[derive(Debug, Clone, Default)]
pub struct OrchestrationExecutorDrainResult {
    pub launched_agent_nodes: Vec<LaunchedAgentNode>,
    pub opened_human_gates: Vec<OpenedHumanGate>,
    pub completed_effect_nodes: Vec<String>,
    pub failed_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchedAgentNode {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub runtime_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedHumanGate {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub gate_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct SubmitHumanGateDecisionInput {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub decision: Value,
    pub resolved_by: String,
}

#[derive(Debug, Clone)]
pub struct SubmitHumanGateDecisionResult {
    pub run: LifecycleRun,
    pub gate_id: Uuid,
    pub drain_result: OrchestrationExecutorDrainResult,
}

#[derive(Clone)]
pub struct OrchestrationExecutorLauncher {
    repos: OrchestrationExecutorRepositories,
    agent_node_launcher: AgentNodeLauncher,
    function_node_runner: FunctionNodeRunner,
    human_gate_launcher: HumanGateLauncher,
}

#[derive(Clone)]
struct OrchestrationExecutorRepositories {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    workflow_agent_node_materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
    agent_run_runtime_provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
}

impl From<WorkflowRepositorySet> for OrchestrationExecutorRepositories {
    fn from(repos: WorkflowRepositorySet) -> Self {
        Self {
            lifecycle_run_repo: repos.lifecycle_run_repo,
            agent_procedure_repo: repos.agent_procedure_repo,
            lifecycle_gate_repo: repos.lifecycle_gate_repo,
            workflow_agent_node_materialization: repos.workflow_agent_node_materialization,
            agent_run_runtime_provisioner: repos.agent_run_runtime_provisioner,
        }
    }
}

impl OrchestrationExecutorLauncher {
    pub fn new(repos: WorkflowRepositorySet) -> Self {
        Self::from_executor_repositories(repos.into())
    }

    fn from_executor_repositories(repos: OrchestrationExecutorRepositories) -> Self {
        let agent_node_launcher = AgentNodeLauncher::new(
            repos.agent_procedure_repo.clone(),
            repos.workflow_agent_node_materialization.clone(),
            repos.agent_run_runtime_provisioner.clone(),
        );
        let human_gate_launcher = HumanGateLauncher::new(repos.lifecycle_gate_repo.clone());
        Self {
            repos,
            agent_node_launcher,
            function_node_runner: FunctionNodeRunner::new(),
            human_gate_launcher,
        }
    }

    pub fn with_function_runner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.function_node_runner = self.function_node_runner.with_runner(runner);
        self
    }

    pub fn with_operation_script_caller(
        mut self,
        caller: crate::SharedWorkflowOperationScriptCaller,
    ) -> Self {
        self.function_node_runner = self
            .function_node_runner
            .with_operation_script_caller(caller);
        self
    }

    pub async fn drain_ready_nodes(
        &self,
        run_id: Uuid,
    ) -> Result<OrchestrationExecutorDrainResult, WorkflowApplicationError> {
        self.drain_ready_nodes_with_cancel(run_id, CancellationToken::new())
            .await
    }

    pub async fn drain_ready_nodes_with_cancel(
        &self,
        run_id: Uuid,
        cancel: CancellationToken,
    ) -> Result<OrchestrationExecutorDrainResult, WorkflowApplicationError> {
        let mut result = OrchestrationExecutorDrainResult::default();
        for _ in 0..MAX_DRAIN_STEPS {
            if cancel.is_cancelled() {
                return Err(WorkflowApplicationError::Conflict(
                    "orchestration executor drain 已取消".to_string(),
                ));
            }
            let run = self.load_run(run_id).await?;
            let Some((coordinate, kind, attempt_policy)) = ReadyNodeView::next(&run).map(|view| {
                (
                    view.coordinate.clone(),
                    view.plan_node.kind,
                    unsupported_attempt_policy(view.plan_node),
                )
            }) else {
                return Ok(result);
            };
            if let Some((code, message)) = attempt_policy {
                let node_path = coordinate.node_path.clone();
                self.block_ready_node(run, coordinate, code, &message, false)
                    .await?;
                result.failed_nodes.push(node_path);
                continue;
            }
            match kind {
                PlanNodeKind::AgentCall => {
                    if let Some(launched) = self.launch_agent_node(run, coordinate).await? {
                        result.launched_agent_nodes.push(launched);
                    }
                }
                PlanNodeKind::Function | PlanNodeKind::LocalEffect => {
                    let node_path = coordinate.node_path.clone();
                    match self
                        .launch_function_node(run, coordinate, cancel.clone())
                        .await?
                    {
                        FunctionLaunchTerminal::Completed => {
                            result.completed_effect_nodes.push(node_path);
                        }
                        FunctionLaunchTerminal::Failed => {
                            result.failed_nodes.push(node_path);
                        }
                    }
                }
                PlanNodeKind::HumanGate => {
                    if let Some(opened) = self.open_human_gate(run, coordinate).await? {
                        result.opened_human_gates.push(opened);
                    }
                }
                _ => {
                    let node_path = coordinate.node_path.clone();
                    self.block_ready_node(
                        run,
                        coordinate,
                        "unsupported_plan_node_kind",
                        "当前 orchestration executor 不支持该 plan node kind",
                        false,
                    )
                    .await?;
                    result.failed_nodes.push(node_path);
                }
            }
        }
        Err(WorkflowApplicationError::Internal(format!(
            "orchestration executor drain 超过 {MAX_DRAIN_STEPS} 步，疑似循环未受限"
        )))
    }

    pub async fn submit_human_gate_decision(
        &self,
        input: SubmitHumanGateDecisionInput,
    ) -> Result<SubmitHumanGateDecisionResult, WorkflowApplicationError> {
        let run = self.load_run(input.run_id).await?;
        let coordinate = RuntimeNodeCoordinate::new(
            input.run_id,
            input.orchestration_id,
            input.node_path.clone(),
            input.attempt,
        );
        let decision = self
            .human_gate_launcher
            .resolve_decision(&run, &input, &coordinate)
            .await?;

        let run = self
            .apply_event(
                run,
                coordinate.orchestration_id,
                OrchestrationRuntimeEvent::NodeCompleted {
                    node_path: coordinate.node_path.clone(),
                    attempt: coordinate.attempt,
                    outputs: decision.outputs,
                    timestamp: chrono::Utc::now(),
                },
            )
            .await?;
        let drain_result = self.drain_ready_nodes(run.id).await?;
        let final_run = self.load_run(run.id).await?;
        Ok(SubmitHumanGateDecisionResult {
            run: final_run,
            gate_id: decision.gate_id,
            drain_result,
        })
    }

    async fn launch_agent_node(
        &self,
        run: LifecycleRun,
        coordinate: RuntimeNodeCoordinate,
    ) -> Result<Option<LaunchedAgentNode>, WorkflowApplicationError> {
        match self.agent_node_launcher.launch(&run, &coordinate).await? {
            AgentNodeLaunchOutcome::Launched { launched, event } => {
                self.apply_event(run, coordinate.orchestration_id, *event)
                    .await?;
                Ok(Some(launched))
            }
            AgentNodeLaunchOutcome::Blocked {
                code,
                message,
                retryable,
            } => {
                self.block_ready_node(run, coordinate, &code, &message, retryable)
                    .await?;
                Ok(None)
            }
        }
    }

    async fn launch_function_node(
        &self,
        run: LifecycleRun,
        coordinate: RuntimeNodeCoordinate,
        cancel: CancellationToken,
    ) -> Result<FunctionLaunchTerminal, WorkflowApplicationError> {
        let function_run_id = Uuid::new_v4().to_string();
        let run = self
            .apply_event(
                run,
                coordinate.orchestration_id,
                OrchestrationRuntimeEvent::NodeStarted {
                    node_path: coordinate.node_path.clone(),
                    attempt: coordinate.attempt,
                    executor_run_ref: Some(ExecutorRunRef::FunctionRun {
                        run_id: function_run_id.clone(),
                    }),
                    timestamp: chrono::Utc::now(),
                },
            )
            .await?;

        let terminal = match self
            .function_node_runner
            .execute(&run, &coordinate, cancel)
            .await
        {
            Ok(outputs) => OrchestrationRuntimeEvent::NodeCompleted {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                outputs,
                timestamp: chrono::Utc::now(),
            },
            Err(error) => OrchestrationRuntimeEvent::NodeFailed {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                error,
                timestamp: chrono::Utc::now(),
            },
        };
        let completed = matches!(terminal, OrchestrationRuntimeEvent::NodeCompleted { .. });
        let run_id = run.id;
        if let Err(error) = self
            .apply_event(run, coordinate.orchestration_id, terminal)
            .await
        {
            let latest_run = self.load_run(run_id).await?;
            self.apply_event(
                latest_run,
                coordinate.orchestration_id,
                OrchestrationRuntimeEvent::NodeFailed {
                    node_path: coordinate.node_path.clone(),
                    attempt: coordinate.attempt,
                    error: RuntimeNodeError {
                        code: "terminal_materialization_failed".to_string(),
                        message: error.to_string(),
                        retryable: false,
                        detail: Some(coordinate.detail()),
                    },
                    timestamp: chrono::Utc::now(),
                },
            )
            .await?;
            return Ok(FunctionLaunchTerminal::Failed);
        }
        Ok(if completed {
            FunctionLaunchTerminal::Completed
        } else {
            FunctionLaunchTerminal::Failed
        })
    }

    async fn open_human_gate(
        &self,
        run: LifecycleRun,
        coordinate: RuntimeNodeCoordinate,
    ) -> Result<Option<OpenedHumanGate>, WorkflowApplicationError> {
        match self.human_gate_launcher.open(&run, &coordinate).await? {
            HumanGateOpenOutcome::Opened { opened, event } => {
                self.apply_event(run, coordinate.orchestration_id, *event)
                    .await?;
                Ok(Some(opened))
            }
            HumanGateOpenOutcome::Blocked {
                code,
                message,
                retryable,
            } => {
                self.block_ready_node(run, coordinate, &code, &message, retryable)
                    .await?;
                Ok(None)
            }
        }
    }

    async fn block_ready_node(
        &self,
        run: LifecycleRun,
        coordinate: RuntimeNodeCoordinate,
        code: &str,
        message: &str,
        retryable: bool,
    ) -> Result<(), WorkflowApplicationError> {
        self.apply_event(
            run,
            coordinate.orchestration_id,
            OrchestrationRuntimeEvent::NodeBlocked {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                error: RuntimeNodeError {
                    code: code.to_string(),
                    message: message.to_string(),
                    retryable,
                    detail: Some(coordinate.detail()),
                },
                timestamp: chrono::Utc::now(),
            },
        )
        .await?;
        Ok(())
    }

    async fn apply_event(
        &self,
        run: LifecycleRun,
        orchestration_id: Uuid,
        event: OrchestrationRuntimeEvent,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let (run, _) = apply_orchestration_event_to_run(run, orchestration_id, event)
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        self.repos.lifecycle_run_repo.update(&run).await?;
        Ok(run)
    }

    async fn load_run(&self, run_id: Uuid) -> Result<LifecycleRun, WorkflowApplicationError> {
        self.repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("LifecycleRun 不存在: {run_id}"))
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionLaunchTerminal {
    Completed,
    Failed,
}

fn unsupported_attempt_policy(plan_node: &PlanNode) -> Option<(&'static str, String)> {
    let policy = plan_node.iteration_policy.as_ref()?;
    match policy.max_attempts {
        Some(1) => {}
        Some(max_attempts) => {
            return Some((
                "attempt_policy_not_supported",
                format!(
                    "node `{}` 声明 max_attempts={max_attempts}，当前 orchestration executor 只支持单次 attempt",
                    plan_node.node_id
                ),
            ));
        }
        None => {
            return Some((
                "unbounded_attempt_policy_not_supported",
                format!(
                    "node `{}` 声明 unbounded attempt policy，当前 orchestration executor 不支持无界重试",
                    plan_node.node_id
                ),
            ));
        }
    }
    if policy.artifact_alias != ArtifactAliasPolicy::Latest {
        return Some((
            "artifact_alias_policy_not_supported",
            format!(
                "node `{}` 声明 {:?} artifact alias policy，当前 orchestration executor 只支持 latest",
                plan_node.node_id, policy.artifact_alias
            ),
        ));
    }
    None
}
