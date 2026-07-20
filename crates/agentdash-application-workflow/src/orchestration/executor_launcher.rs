use std::sync::Arc;

use agentdash_domain::workflow::{
    ArtifactAliasPolicy, ExecutorRunRef, LifecycleRun, LifecycleRunRepository,
    LifecycleRunWriteError, PlanNode, PlanNodeKind, RuntimeNodeError, RuntimeNodeState,
    RuntimeNodeStatus, WorkflowExecutorEffectIdentity, WorkflowExecutorEffectRepository,
    WorkflowFunctionTerminalResult,
};
use agentdash_platform_spi::FunctionRunner;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{WorkflowApplicationError, WorkflowRepositorySet};

use super::agent_call::{
    WorkflowAgentCallDispatchPort, WorkflowAgentCallLaunchOutcome, WorkflowAgentCallLauncher,
};
use super::function_node_runner::{FunctionDispatchOutcome, FunctionNodeRunner};
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
    pub agent_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub runtime_thread_id: String,
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
    function_node_runner: FunctionNodeRunner,
    human_gate_launcher: HumanGateLauncher,
    agent_call_launcher: WorkflowAgentCallLauncher,
}

#[derive(Clone)]
struct OrchestrationExecutorRepositories {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    executor_effect_repo: Arc<dyn WorkflowExecutorEffectRepository>,
    agent_procedure_repo: Arc<dyn agentdash_domain::workflow::AgentProcedureRepository>,
}

impl OrchestrationExecutorRepositories {
    fn new(
        repos: WorkflowRepositorySet,
        executor_effect_repo: Arc<dyn WorkflowExecutorEffectRepository>,
    ) -> Self {
        Self {
            lifecycle_run_repo: repos.lifecycle_run_repo,
            executor_effect_repo,
            agent_procedure_repo: repos.agent_procedure_repo,
        }
    }
}

impl OrchestrationExecutorLauncher {
    #[cfg(test)]
    pub fn new(repos: WorkflowRepositorySet) -> Self {
        Self::new_for_test(
            repos,
            Arc::new(RecordingWorkflowExecutorEffectRepository::default()),
        )
    }

    #[cfg(test)]
    fn new_for_test(
        repos: WorkflowRepositorySet,
        executor_effect_repo: Arc<dyn WorkflowExecutorEffectRepository>,
    ) -> Self {
        Self::from_executor_repositories(OrchestrationExecutorRepositories::new(
            repos,
            executor_effect_repo,
        ))
    }

    pub fn new_durable(
        repos: WorkflowRepositorySet,
        executor_effect_repo: Arc<dyn WorkflowExecutorEffectRepository>,
        function_runner: Arc<dyn FunctionRunner>,
    ) -> Self {
        Self::from_executor_repositories(OrchestrationExecutorRepositories::new(
            repos,
            executor_effect_repo,
        ))
        .with_function_runner_inner(function_runner)
    }

    fn from_executor_repositories(repos: OrchestrationExecutorRepositories) -> Self {
        let human_gate_launcher = HumanGateLauncher::new(repos.executor_effect_repo.clone());
        let agent_call_launcher =
            WorkflowAgentCallLauncher::new(repos.agent_procedure_repo.clone());
        Self {
            repos,
            function_node_runner: FunctionNodeRunner::new(),
            human_gate_launcher,
            agent_call_launcher,
        }
    }

    #[cfg(test)]
    pub fn with_function_runner(self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.with_function_runner_inner(runner)
    }

    fn with_function_runner_inner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.function_node_runner = self.function_node_runner.with_runner(runner);
        self
    }

    pub fn with_agent_call_dispatch(
        mut self,
        dispatch: Arc<dyn WorkflowAgentCallDispatchPort>,
    ) -> Self {
        self.agent_call_launcher = self.agent_call_launcher.with_dispatch(dispatch);
        self
    }

    pub async fn drain_ready_nodes(
        &self,
        run_id: Uuid,
    ) -> Result<OrchestrationExecutorDrainResult, WorkflowApplicationError> {
        let mut result = OrchestrationExecutorDrainResult::default();
        'drain: for _ in 0..MAX_DRAIN_STEPS {
            let run = self.load_run(run_id).await?;
            if let Some((coordinate, kind)) = next_recoverable_executor_node(&run) {
                match kind {
                    PlanNodeKind::Function | PlanNodeKind::LocalEffect => {
                        let node_path = coordinate.node_path.clone();
                        match self.resume_function_node(run, coordinate).await {
                            Ok(Some(FunctionLaunchTerminal::Completed)) => {
                                result.completed_effect_nodes.push(node_path);
                                continue;
                            }
                            Ok(Some(FunctionLaunchTerminal::Failed)) => {
                                result.failed_nodes.push(node_path);
                                continue;
                            }
                            Ok(Some(FunctionLaunchTerminal::Blocked)) => {
                                continue;
                            }
                            Ok(None) => return Ok(result),
                            Err(WorkflowApplicationError::Conflict(_)) => continue,
                            Err(error) => return Err(error),
                        }
                    }
                    _ => {}
                }
            }
            for coordinate in running_human_gate_coordinates(&run) {
                if let Some(decision) = self
                    .inspect_human_gate_resolution(&run, &coordinate)
                    .await?
                {
                    match self
                        .apply_human_gate_decision(run, &coordinate, &decision)
                        .await
                    {
                        Ok(_) | Err(WorkflowApplicationError::Conflict(_)) => continue 'drain,
                        Err(error) => return Err(error),
                    }
                }
            }
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
                PlanNodeKind::Function | PlanNodeKind::LocalEffect => {
                    if !self.function_node_runner.is_composed() {
                        let node_path = coordinate.node_path.clone();
                        self.block_ready_node(
                            run,
                            coordinate,
                            "function_effect_protocol_not_composed",
                            "Workflow Function durable effect protocol 未注入",
                            true,
                        )
                        .await?;
                        result.failed_nodes.push(node_path);
                        continue;
                    }
                    match self.start_function_node(run, coordinate).await {
                        Ok(_) | Err(WorkflowApplicationError::Conflict(_)) => continue,
                        Err(error) => return Err(error),
                    }
                }
                PlanNodeKind::HumanGate => match self.open_human_gate(run, coordinate).await {
                    Ok(Some(opened)) => result.opened_human_gates.push(opened),
                    Ok(None) => {}
                    Err(WorkflowApplicationError::Conflict(_)) => continue,
                    Err(error) => return Err(error),
                },
                PlanNodeKind::AgentCall => {
                    match self.agent_call_launcher.launch(&run, &coordinate).await? {
                        WorkflowAgentCallLaunchOutcome::Prepared { event } => {
                            match self
                                .apply_event(run, coordinate.orchestration_id, event)
                                .await
                            {
                                Ok(_) => {}
                                Err(WorkflowApplicationError::Conflict(_)) => continue,
                                Err(error) => return Err(error),
                            }
                        }
                        WorkflowAgentCallLaunchOutcome::Pending => return Ok(result),
                        WorkflowAgentCallLaunchOutcome::Accepted {
                            target,
                            runtime_thread_id,
                            dispatch_event,
                        } => {
                            match self
                                .apply_event(run, coordinate.orchestration_id, dispatch_event)
                                .await
                            {
                                Ok(_) => {}
                                Err(WorkflowApplicationError::Conflict(_)) => continue,
                                Err(error) => return Err(error),
                            }
                            result.launched_agent_nodes.push(LaunchedAgentNode {
                                run_id: target.run_id,
                                agent_id: target.agent_id,
                                orchestration_id: coordinate.orchestration_id,
                                node_path: coordinate.node_path,
                                attempt: coordinate.attempt,
                                runtime_thread_id,
                            });
                        }
                        WorkflowAgentCallLaunchOutcome::Blocked {
                            code,
                            message,
                            retryable,
                        } => {
                            let node_path = coordinate.node_path.clone();
                            self.block_ready_node(run, coordinate, &code, &message, retryable)
                                .await?;
                            result.failed_nodes.push(node_path);
                        }
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
        let coordinate = RuntimeNodeCoordinate::new(
            input.run_id,
            input.orchestration_id,
            input.node_path.clone(),
            input.attempt,
        );
        let (run, decision) = loop {
            let run = self.load_run(input.run_id).await?;
            let orchestration = run
                .orchestrations
                .iter()
                .find(|orchestration| orchestration.orchestration_id == coordinate.orchestration_id)
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "orchestration 不存在: {}",
                        coordinate.orchestration_id
                    ))
                })?;
            let runtime_node = find_runtime_node(
                &orchestration.node_tree,
                &coordinate.node_path,
                coordinate.attempt,
            )
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "HumanGate runtime node 不存在: {}#{}",
                    coordinate.node_path, coordinate.attempt
                ))
            })?;
            let terminal_gate_id = is_executor_terminal(runtime_node.status)
                .then(|| super::human_gate_launcher::human_gate_id_from_node(runtime_node))
                .transpose()?;
            let decision = if let Some(gate_id) = terminal_gate_id {
                self.human_gate_launcher
                    .inspect_resolution(gate_id)
                    .await?
                    .ok_or_else(|| {
                        WorkflowApplicationError::Conflict(
                            "HumanGate 已终结但缺少 durable resolution receipt".to_owned(),
                        )
                    })
                    .and_then(|decision| {
                        if decision.decision != input.decision
                            || decision.resolved_by != input.resolved_by
                        {
                            return Err(WorkflowApplicationError::Conflict(
                                "HumanGate decision replay payload conflicts with durable receipt"
                                    .to_owned(),
                            ));
                        }
                        Ok(decision)
                    })?
            } else {
                self.human_gate_launcher
                    .resolve_decision(&run, &input, &coordinate)
                    .await?
            };
            match self
                .apply_human_gate_decision(run, &coordinate, &decision)
                .await
            {
                Ok(run) => break (run, decision),
                Err(WorkflowApplicationError::Conflict(_)) => continue,
                Err(error) => return Err(error),
            }
        };
        let drain_result = self.drain_ready_nodes(run.id).await?;
        let final_run = self.load_run(run.id).await?;
        Ok(SubmitHumanGateDecisionResult {
            run: final_run,
            gate_id: decision.gate_id,
            drain_result,
        })
    }

    async fn start_function_node(
        &self,
        run: LifecycleRun,
        coordinate: RuntimeNodeCoordinate,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let identity = function_effect_identity(&coordinate);
        let prepared = self
            .function_node_runner
            .prepare_ready(&run, &coordinate, identity.clone())
            .map_err(|error| WorkflowApplicationError::Conflict(error.message))?;
        self.repos
            .executor_effect_repo
            .prepare_function(prepared.request)
            .await
            .map_err(executor_effect_error)?;
        self.apply_event(
            run,
            coordinate.orchestration_id,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                executor_run_ref: Some(ExecutorRunRef::FunctionRun {
                    run_id: identity.effect_id,
                }),
                timestamp: chrono::Utc::now(),
            },
        )
        .await
    }

    async fn resume_function_node(
        &self,
        run: LifecycleRun,
        coordinate: RuntimeNodeCoordinate,
    ) -> Result<Option<FunctionLaunchTerminal>, WorkflowApplicationError> {
        let identity = function_effect_identity(&coordinate);
        let record = self
            .repos
            .executor_effect_repo
            .get_function(&identity.effect_id)
            .await
            .map_err(executor_effect_error)?
            .ok_or_else(|| {
                WorkflowApplicationError::Conflict(format!(
                    "Running Function node 缺少 durable Prepared effect: {}",
                    identity.effect_id
                ))
            })?;
        let prepared = self
            .function_node_runner
            .prepare_recovery(&run, &coordinate, record.request.clone())
            .map_err(|error| WorkflowApplicationError::Conflict(error.message))?;
        let terminal = match record.terminal {
            Some(terminal) => terminal,
            None => {
                let terminal = match self
                    .function_node_runner
                    .dispatch(&prepared)
                    .await
                    .map_err(|error| WorkflowApplicationError::Internal(error.message))?
                {
                    FunctionDispatchOutcome::Pending => return Ok(None),
                    FunctionDispatchOutcome::Terminal(terminal) => terminal,
                    FunctionDispatchOutcome::Lost { reason, evidence } => {
                        self.apply_event(
                            run,
                            coordinate.orchestration_id,
                            function_lost_event(
                                &coordinate,
                                &prepared.request.payload_digest,
                                reason,
                                evidence,
                            ),
                        )
                        .await?;
                        return Ok(Some(FunctionLaunchTerminal::Blocked));
                    }
                };
                self.repos
                    .executor_effect_repo
                    .commit_function_terminal(prepared.request, terminal)
                    .await
                    .map_err(executor_effect_error)?
                    .terminal
                    .expect("committed Function terminal receipt")
            }
        };
        let completed = matches!(terminal, WorkflowFunctionTerminalResult::Completed { .. });
        self.apply_event(
            run,
            coordinate.orchestration_id,
            function_terminal_event(&coordinate, terminal),
        )
        .await?;
        Ok(Some(if completed {
            FunctionLaunchTerminal::Completed
        } else {
            FunctionLaunchTerminal::Failed
        }))
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

    async fn inspect_human_gate_resolution(
        &self,
        run: &LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
    ) -> Result<Option<super::human_gate_launcher::HumanGateDecision>, WorkflowApplicationError>
    {
        let node = run
            .orchestrations
            .iter()
            .find(|orchestration| orchestration.orchestration_id == coordinate.orchestration_id)
            .and_then(|orchestration| {
                find_runtime_node(
                    &orchestration.node_tree,
                    &coordinate.node_path,
                    coordinate.attempt,
                )
            })
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "HumanGate runtime node 不存在: {}",
                    coordinate.node_path
                ))
            })?;
        let gate_id = super::human_gate_launcher::human_gate_id_from_node(node)?;
        self.human_gate_launcher.inspect_resolution(gate_id).await
    }

    async fn apply_human_gate_decision(
        &self,
        run: LifecycleRun,
        coordinate: &RuntimeNodeCoordinate,
        decision: &super::human_gate_launcher::HumanGateDecision,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        self.apply_event(
            run,
            coordinate.orchestration_id,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                outputs: decision.outputs.clone(),
                timestamp: chrono::Utc::now(),
            },
        )
        .await
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
        let expected_revision = run.revision;
        let (run, outcome) = apply_orchestration_event_to_run(run, orchestration_id, event)
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        if outcome.idempotent_replay {
            return Ok(run);
        }
        self.repos
            .lifecycle_run_repo
            .compare_and_swap(expected_revision, &run)
            .await
            .map_err(|error| match error {
                LifecycleRunWriteError::RevisionConflict { .. } => {
                    WorkflowApplicationError::Conflict(error.to_string())
                }
                LifecycleRunWriteError::Persistence(error) => error.into(),
                LifecycleRunWriteError::CasNotImplemented => {
                    WorkflowApplicationError::Internal(error.to_string())
                }
            })?;
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
    Blocked,
}

fn function_effect_id(coordinate: &RuntimeNodeCoordinate) -> String {
    format!(
        "workflow-function:{}:{}#{}",
        coordinate.orchestration_id, coordinate.node_path, coordinate.attempt
    )
}

pub(super) fn function_effect_identity(
    coordinate: &RuntimeNodeCoordinate,
) -> WorkflowExecutorEffectIdentity {
    WorkflowExecutorEffectIdentity {
        effect_id: function_effect_id(coordinate),
        lifecycle_run_id: coordinate.run_id,
        orchestration_id: coordinate.orchestration_id,
        node_path: coordinate.node_path.clone(),
        attempt: coordinate.attempt,
    }
}

fn function_terminal_event(
    coordinate: &RuntimeNodeCoordinate,
    terminal: WorkflowFunctionTerminalResult,
) -> OrchestrationRuntimeEvent {
    match terminal {
        WorkflowFunctionTerminalResult::Completed { outputs } => {
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: coordinate.node_path.clone(),
                attempt: coordinate.attempt,
                outputs,
                timestamp: chrono::Utc::now(),
            }
        }
        WorkflowFunctionTerminalResult::Failed { error } => OrchestrationRuntimeEvent::NodeFailed {
            node_path: coordinate.node_path.clone(),
            attempt: coordinate.attempt,
            error,
            timestamp: chrono::Utc::now(),
        },
    }
}

fn function_lost_event(
    coordinate: &RuntimeNodeCoordinate,
    payload_digest: &str,
    reason: String,
    evidence: Value,
) -> OrchestrationRuntimeEvent {
    OrchestrationRuntimeEvent::NodeBlocked {
        node_path: coordinate.node_path.clone(),
        attempt: coordinate.attempt,
        error: RuntimeNodeError {
            code: "function_effect_outcome_lost".to_owned(),
            message: reason.clone(),
            retryable: false,
            detail: Some(coordinate.detail_with([
                ("effect_id", json!(function_effect_id(coordinate))),
                ("payload_digest", json!(payload_digest)),
                ("reason", json!(reason)),
                ("evidence", evidence),
            ])),
        },
        timestamp: chrono::Utc::now(),
    }
}

fn next_recoverable_executor_node(
    run: &LifecycleRun,
) -> Option<(RuntimeNodeCoordinate, PlanNodeKind)> {
    for orchestration in &run.orchestrations {
        if let Some(node) = find_recoverable_node(&orchestration.node_tree) {
            return Some((
                RuntimeNodeCoordinate::new(
                    run.id,
                    orchestration.orchestration_id,
                    node.node_path.clone(),
                    node.attempt,
                ),
                node.kind,
            ));
        }
    }
    None
}

fn find_recoverable_node(nodes: &[RuntimeNodeState]) -> Option<&RuntimeNodeState> {
    for node in nodes {
        if node.status == RuntimeNodeStatus::Running
            && matches!(
                node.kind,
                PlanNodeKind::Function | PlanNodeKind::LocalEffect
            )
        {
            return Some(node);
        }
        if let Some(found) = find_recoverable_node(&node.children) {
            return Some(found);
        }
    }
    None
}

fn running_human_gate_coordinates(run: &LifecycleRun) -> Vec<RuntimeNodeCoordinate> {
    let mut coordinates = Vec::new();
    for orchestration in &run.orchestrations {
        collect_running_human_gates(
            run.id,
            orchestration.orchestration_id,
            &orchestration.node_tree,
            &mut coordinates,
        );
    }
    coordinates
}

fn collect_running_human_gates(
    run_id: Uuid,
    orchestration_id: Uuid,
    nodes: &[RuntimeNodeState],
    coordinates: &mut Vec<RuntimeNodeCoordinate>,
) {
    for node in nodes {
        if node.status == RuntimeNodeStatus::Running && node.kind == PlanNodeKind::HumanGate {
            coordinates.push(RuntimeNodeCoordinate::new(
                run_id,
                orchestration_id,
                node.node_path.clone(),
                node.attempt,
            ));
        }
        collect_running_human_gates(run_id, orchestration_id, &node.children, coordinates);
    }
}

fn find_runtime_node<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}

fn is_executor_terminal(status: RuntimeNodeStatus) -> bool {
    matches!(
        status,
        RuntimeNodeStatus::Completed
            | RuntimeNodeStatus::Failed
            | RuntimeNodeStatus::Cancelled
            | RuntimeNodeStatus::Skipped
    )
}

fn executor_effect_error(
    error: agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
) -> WorkflowApplicationError {
    match error {
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::PayloadConflict {
            ..
        } => WorkflowApplicationError::Conflict(error.to_string()),
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::Persistence(_) => {
            WorkflowApplicationError::Internal(error.to_string())
        }
    }
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

#[cfg(test)]
#[derive(Default)]
struct RecordingWorkflowExecutorEffectRepository {
    functions: tokio::sync::Mutex<
        std::collections::BTreeMap<
            String,
            agentdash_domain::workflow::WorkflowFunctionEffectRecord,
        >,
    >,
    gate_opens: tokio::sync::Mutex<
        std::collections::BTreeMap<
            String,
            agentdash_domain::workflow::WorkflowHumanGateOpenReceipt,
        >,
    >,
    gate_resolutions: tokio::sync::Mutex<
        std::collections::BTreeMap<
            Uuid,
            agentdash_domain::workflow::WorkflowHumanGateResolutionReceipt,
        >,
    >,
    gates: tokio::sync::Mutex<
        std::collections::BTreeMap<Uuid, agentdash_domain::workflow::LifecycleGate>,
    >,
}

#[cfg(test)]
#[async_trait::async_trait]
impl WorkflowExecutorEffectRepository for RecordingWorkflowExecutorEffectRepository {
    async fn prepare_function(
        &self,
        request: agentdash_domain::workflow::WorkflowFunctionEffectRequest,
    ) -> Result<
        agentdash_domain::workflow::WorkflowFunctionEffectRecord,
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
    > {
        let mut functions = self.functions.lock().await;
        if let Some(existing) = functions.get(&request.identity.effect_id) {
            if existing.request != request {
                return Err(
                    agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::PayloadConflict {
                        effect_id: request.identity.effect_id,
                    },
                );
            }
            return Ok(existing.clone());
        }
        let now = chrono::Utc::now();
        let record = agentdash_domain::workflow::WorkflowFunctionEffectRecord {
            request,
            terminal: None,
            created_at: now,
            updated_at: now,
        };
        functions.insert(record.request.identity.effect_id.clone(), record.clone());
        Ok(record)
    }

    async fn commit_function_terminal(
        &self,
        request: agentdash_domain::workflow::WorkflowFunctionEffectRequest,
        terminal: WorkflowFunctionTerminalResult,
    ) -> Result<
        agentdash_domain::workflow::WorkflowFunctionEffectRecord,
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
    > {
        let mut functions = self.functions.lock().await;
        let Some(existing) = functions.get_mut(&request.identity.effect_id) else {
            return Err(
                agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::Persistence(
                    "Function effect was not prepared".to_owned(),
                ),
            );
        };
        if existing.request != request
            || existing
                .terminal
                .as_ref()
                .is_some_and(|stored| stored != &terminal)
        {
            return Err(
                    agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::PayloadConflict {
                    effect_id: request.identity.effect_id,
                },
            );
        }
        existing.terminal = Some(terminal);
        existing.updated_at = chrono::Utc::now();
        Ok(existing.clone())
    }

    async fn get_function(
        &self,
        effect_id: &str,
    ) -> Result<
        Option<agentdash_domain::workflow::WorkflowFunctionEffectRecord>,
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
    > {
        Ok(self.functions.lock().await.get(effect_id).cloned())
    }

    async fn open_human_gate(
        &self,
        effect: agentdash_domain::workflow::WorkflowHumanGateOpenEffect,
    ) -> Result<
        agentdash_domain::workflow::WorkflowHumanGateOpenReceipt,
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
    > {
        let mut opens = self.gate_opens.lock().await;
        if let Some(existing) = opens.get(&effect.identity.effect_id) {
            if existing.effect.identity != effect.identity
                || existing.effect.payload_digest != effect.payload_digest
                || existing.effect.gate.id != effect.gate.id
            {
                return Err(
                    agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::PayloadConflict {
                        effect_id: effect.identity.effect_id,
                    },
                );
            }
            return Ok(existing.clone());
        }
        let receipt = agentdash_domain::workflow::WorkflowHumanGateOpenReceipt {
            effect: effect.clone(),
            committed_at: chrono::Utc::now(),
        };
        self.gates.lock().await.insert(effect.gate.id, effect.gate);
        opens.insert(receipt.effect.identity.effect_id.clone(), receipt.clone());
        Ok(receipt)
    }

    async fn get_human_gate_open(
        &self,
        effect_id: &str,
    ) -> Result<
        Option<agentdash_domain::workflow::WorkflowHumanGateOpenReceipt>,
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
    > {
        Ok(self.gate_opens.lock().await.get(effect_id).cloned())
    }

    async fn resolve_human_gate(
        &self,
        effect: agentdash_domain::workflow::WorkflowHumanGateResolutionEffect,
    ) -> Result<
        agentdash_domain::workflow::WorkflowHumanGateResolutionReceipt,
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
    > {
        let mut resolutions = self.gate_resolutions.lock().await;
        if let Some(existing) = resolutions.get(&effect.gate_id) {
            if existing.effect != effect {
                return Err(
                    agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::PayloadConflict {
                        effect_id: effect.identity.effect_id,
                    },
                );
            }
            return Ok(existing.clone());
        }
        let mut gates = self.gates.lock().await;
        let gate = gates.get_mut(&effect.gate_id).ok_or_else(|| {
            agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError::Persistence(
                "HumanGate open receipt does not exist".to_owned(),
            )
        })?;
        gate.payload_json = Some(effect.decision.clone());
        gate.resolve(effect.resolved_by.clone());
        let receipt = agentdash_domain::workflow::WorkflowHumanGateResolutionReceipt {
            effect: effect.clone(),
            committed_at: chrono::Utc::now(),
        };
        resolutions.insert(effect.gate_id, receipt.clone());
        Ok(receipt)
    }

    async fn get_human_gate_resolution(
        &self,
        gate_id: Uuid,
    ) -> Result<
        Option<agentdash_domain::workflow::WorkflowHumanGateResolutionReceipt>,
        agentdash_domain::workflow::WorkflowExecutorEffectRepositoryError,
    > {
        Ok(self.gate_resolutions.lock().await.get(&gate_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use agentdash_domain::{
        DomainError,
        workflow::{
            ActivationRule, ActivityCompletionPolicy, ActivityJoinPolicy, AgentProcedure,
            AgentProcedureContract, AgentProcedureExecutionSpec, AgentProcedureRepository,
            AgentReusePolicy, ApiRequestExecutorSpec, ExecutorSpec, FunctionActivityExecutorSpec,
            HumanActivityExecutorSpec, HumanApprovalExecutorSpec, LifecycleGate,
            LifecycleGateRepository, OrchestrationLimits, OrchestrationPlanSnapshot,
            OrchestrationSourceRef, OutputPortDefinition, PlanNode, RuntimeNodeStatus,
            RuntimeThreadPolicy, TransitionCondition, WorkflowAgentCallRuntimeState,
        },
    };
    use agentdash_platform_spi::{
        ApiRequestOutcome, BashExecOutcome, FunctionEffectObservation, FunctionEffectRawOutcome,
        FunctionEffectRequest,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::Mutex;

    use super::*;
    use crate::orchestration::{
        WorkflowAgentCallDispatchError, WorkflowAgentCallDispatchOutcome,
        WorkflowAgentCallDispatchPort, WorkflowAgentCallMailboxState, WorkflowAgentCallRequest,
        WorkflowAgentCallTargetIntent, activate_root_orchestration,
    };

    #[derive(Default)]
    struct RunRepo {
        run: Mutex<Option<LifecycleRun>>,
        fail_cas_at_expected_revision: Mutex<Option<u64>>,
        conflict_cas_at_expected_revision: Mutex<Option<u64>>,
        attempted_claim_ids: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for RunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            *self.run.lock().await = Some(run.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self.run.lock().await.clone().filter(|run| run.id == id))
        }
        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .get_by_id(ids.first().copied().unwrap_or_default())
                .await?
                .into_iter()
                .collect())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .run
                .lock()
                .await
                .clone()
                .filter(|run| run.project_id == project_id)
                .into_iter()
                .collect())
        }
        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            *self.run.lock().await = Some(run.clone());
            Ok(())
        }
        async fn compare_and_swap(
            &self,
            expected_revision: u64,
            run: &LifecycleRun,
        ) -> Result<(), LifecycleRunWriteError> {
            if let Some(claim_id) = run
                .orchestrations
                .iter()
                .flat_map(|orchestration| orchestration.node_tree.iter())
                .find_map(|node| node.agent_call.as_ref()?.claim_id.clone())
            {
                self.attempted_claim_ids.lock().await.push(claim_id);
            }
            let mut conflict_at = self.conflict_cas_at_expected_revision.lock().await;
            if *conflict_at == Some(expected_revision) {
                *conflict_at = None;
                return Err(LifecycleRunWriteError::RevisionConflict {
                    run_id: run.id,
                    expected_revision,
                    actual_revision: expected_revision + 1,
                });
            }
            drop(conflict_at);
            let mut fail_at = self.fail_cas_at_expected_revision.lock().await;
            if *fail_at == Some(expected_revision) {
                *fail_at = None;
                return Err(LifecycleRunWriteError::Persistence(
                    DomainError::InvalidConfig("injected CAS commit failure".to_owned()),
                ));
            }
            drop(fail_at);
            let mut stored = self.run.lock().await;
            let actual_revision = stored.as_ref().map_or(0, |item| item.revision);
            if actual_revision != expected_revision || run.revision != expected_revision + 1 {
                return Err(LifecycleRunWriteError::RevisionConflict {
                    run_id: run.id,
                    expected_revision,
                    actual_revision,
                });
            }
            *stored = Some(run.clone());
            Ok(())
        }
        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct UnusedProcedureRepo;

    #[async_trait]
    impl AgentProcedureRepository for UnusedProcedureRepo {
        async fn create(&self, _procedure: &AgentProcedure) -> Result<(), DomainError> {
            Ok(())
        }
        async fn get_by_id(&self, _id: Uuid) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(None)
        }
        async fn get_by_key(&self, _key: &str) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(None)
        }
        async fn get_by_project_and_key(
            &self,
            _project_id: Uuid,
            _key: &str,
        ) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(None)
        }
        async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(Vec::new())
        }
        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(Vec::new())
        }
        async fn update(&self, _procedure: &AgentProcedure) -> Result<(), DomainError> {
            Ok(())
        }
        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct UnusedGateRepo;

    #[async_trait]
    impl LifecycleGateRepository for UnusedGateRepo {
        async fn create(&self, _gate: &LifecycleGate) -> Result<(), DomainError> {
            Ok(())
        }
        async fn get(&self, _id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(None)
        }
        async fn list_open_for_agent(
            &self,
            _agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(Vec::new())
        }
        async fn list_open_gate_wait_policies(
            &self,
            _limit: usize,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(Vec::new())
        }
        async fn list_by_wait_producer(
            &self,
            _producer: &agentdash_domain::workflow::WaitProducerRef,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(Vec::new())
        }
        async fn find_by_agent_and_correlation(
            &self,
            _agent_id: Uuid,
            _correlation_id: &str,
        ) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(None)
        }
        async fn update(&self, _gate: &LifecycleGate) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingDispatch {
        calls: AtomicUsize,
        requests: Mutex<Vec<WorkflowAgentCallRequest>>,
        pending_once: AtomicBool,
    }

    #[async_trait]
    impl WorkflowAgentCallDispatchPort for RecordingDispatch {
        async fn dispatch(
            &self,
            request: WorkflowAgentCallRequest,
        ) -> Result<WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let target = request.target_intent.target().clone();
            let authority = match &request.target_intent {
                crate::orchestration::WorkflowAgentCallTargetIntent::CreateNew { .. } => (
                    "runtime-thread-workflow-1".to_owned(),
                    agentdash_domain::workflow::WorkflowAgentCallSourceBindingRef {
                        source_ref: "source:workflow-agent".to_owned(),
                        committed_at_revision: 2,
                        applied_surface_revision: 3,
                        activated_at_revision: Some(4),
                    },
                ),
                crate::orchestration::WorkflowAgentCallTargetIntent::ContinueCurrent {
                    runtime_thread_id,
                    source_binding,
                    ..
                } => (runtime_thread_id.clone(), source_binding.clone()),
            };
            self.requests.lock().await.push(request);
            if self.pending_once.swap(false, Ordering::SeqCst) {
                return Ok(WorkflowAgentCallDispatchOutcome::Pending);
            }
            Ok(WorkflowAgentCallDispatchOutcome::Accepted {
                target,
                runtime_thread_id: authority.0,
                source_binding: authority.1,
                mailbox_state: WorkflowAgentCallMailboxState::Submitted,
            })
        }
    }

    struct RecordingStableFunctionRunner {
        executions: AtomicUsize,
        observations: Mutex<std::collections::BTreeMap<String, FunctionEffectObservation>>,
        execute_observation: FunctionEffectObservation,
        lose_receipt_after_side_effect: bool,
    }

    impl Default for RecordingStableFunctionRunner {
        fn default() -> Self {
            Self {
                executions: AtomicUsize::new(0),
                observations: Mutex::new(std::collections::BTreeMap::new()),
                execute_observation: FunctionEffectObservation::Succeeded(
                    FunctionEffectRawOutcome::ApiRequest(ApiRequestOutcome {
                        status: 200,
                        body_text: "ok".to_owned(),
                        body_json: Some(serde_json::json!({"value": 7})),
                    }),
                ),
                lose_receipt_after_side_effect: false,
            }
        }
    }

    impl RecordingStableFunctionRunner {
        fn observing(effect_id: String, observation: FunctionEffectObservation) -> Self {
            Self {
                observations: Mutex::new(std::collections::BTreeMap::from([(
                    effect_id,
                    observation,
                )])),
                ..Self::default()
            }
        }

        fn returning(execute_observation: FunctionEffectObservation) -> Self {
            Self {
                execute_observation,
                ..Self::default()
            }
        }

        fn losing_receipt() -> Self {
            Self {
                lose_receipt_after_side_effect: true,
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl FunctionRunner for RecordingStableFunctionRunner {
        async fn run_api_request(
            &self,
            _spec: &ApiRequestExecutorSpec,
            _context: &Value,
        ) -> Result<ApiRequestOutcome, String> {
            panic!("legacy raw FunctionRunner path must not be called")
        }

        async fn run_bash(
            &self,
            _spec: &agentdash_domain::workflow::BashExecExecutorSpec,
            _context: &Value,
        ) -> Result<BashExecOutcome, String> {
            panic!("legacy raw FunctionRunner path must not be called")
        }

        async fn execute_effect(
            &self,
            request: FunctionEffectRequest,
        ) -> Result<FunctionEffectObservation, String> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            if self.lose_receipt_after_side_effect {
                self.observations
                    .lock()
                    .await
                    .insert(request.effect_id, FunctionEffectObservation::InFlight);
                return Err("runner receipt lost after external side effect".to_owned());
            }
            let outcome = self.execute_observation.clone();
            self.observations
                .lock()
                .await
                .insert(request.effect_id, outcome.clone());
            Ok(outcome)
        }

        async fn inspect_effect(
            &self,
            effect_id: &str,
        ) -> Result<FunctionEffectObservation, String> {
            Ok(self
                .observations
                .lock()
                .await
                .get(effect_id)
                .cloned()
                .unwrap_or(FunctionEffectObservation::NotApplied))
        }
    }

    fn run_with_agent_policy(
        reuse: AgentReusePolicy,
        session: RuntimeThreadPolicy,
    ) -> LifecycleRun {
        let source_ref = OrchestrationSourceRef::Inline {
            source_digest: "sha256:agent-call-test".to_owned(),
        };
        let plan = OrchestrationPlanSnapshot {
            plan_digest: "sha256:agent-call-plan".to_owned(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: "agent".to_owned(),
                node_path: "agent".to_owned(),
                parent_node_id: None,
                kind: PlanNodeKind::AgentCall,
                label: None,
                executor: Some(ExecutorSpec::AgentProcedure {
                    procedure: AgentProcedureExecutionSpec::Snapshot {
                        procedure_key: Some("review".to_owned()),
                        name: Some("Review".to_owned()),
                        contract: Box::new(AgentProcedureContract::default()),
                        source_ref: None,
                        contract_digest: None,
                    },
                    agent_reuse_policy: reuse,
                    runtime_thread_policy: session,
                }),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec!["agent".to_owned()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        run.add_orchestration(activate_root_orchestration(source_ref, plan));
        run
    }

    fn run_with_function_node() -> LifecycleRun {
        run_with_single_executor_node(
            PlanNodeKind::Function,
            ExecutorSpec::Function {
                spec: FunctionActivityExecutorSpec::ApiRequest(ApiRequestExecutorSpec {
                    method: "POST".to_owned(),
                    url_template: "https://example.invalid".to_owned(),
                    body_template: Some(serde_json::json!({"input": true})),
                }),
            },
            Some(ActivityCompletionPolicy::OutputPorts {
                required_ports: vec!["result".to_owned()],
            }),
            vec![OutputPortDefinition {
                key: "result".to_owned(),
                description: "result".to_owned(),
                gate_strategy: agentdash_domain::workflow::GateStrategy::Existence,
                gate_params: None,
            }],
        )
    }

    fn run_with_human_gate_node() -> LifecycleRun {
        run_with_single_executor_node(
            PlanNodeKind::HumanGate,
            ExecutorSpec::Human {
                spec: HumanActivityExecutorSpec::Approval(HumanApprovalExecutorSpec {
                    form_schema_key: "approval.test".to_owned(),
                    title: Some("Approve".to_owned()),
                }),
            },
            Some(ActivityCompletionPolicy::HumanDecision {
                decision_port: "decision".to_owned(),
            }),
            vec![OutputPortDefinition {
                key: "decision".to_owned(),
                description: "decision".to_owned(),
                gate_strategy: agentdash_domain::workflow::GateStrategy::Existence,
                gate_params: None,
            }],
        )
    }

    fn nest_only_runtime_node(run: &mut LifecycleRun) {
        let orchestration = &mut run.orchestrations[0];
        let child = orchestration.node_tree.remove(0);
        orchestration.node_tree.push(RuntimeNodeState {
            node_id: "phase".to_owned(),
            node_path: "phase".to_owned(),
            kind: PlanNodeKind::Phase,
            status: RuntimeNodeStatus::Running,
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: None,
            agent_call: None,
            children: vec![child],
            phase_path: Vec::new(),
            started_at: Some(Utc::now()),
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        });
    }

    fn run_with_single_executor_node(
        kind: PlanNodeKind,
        executor: ExecutorSpec,
        completion_policy: Option<ActivityCompletionPolicy>,
        output_ports: Vec<OutputPortDefinition>,
    ) -> LifecycleRun {
        let source_ref = OrchestrationSourceRef::Inline {
            source_digest: "sha256:executor-effect-test".to_owned(),
        };
        let plan = OrchestrationPlanSnapshot {
            plan_digest: "sha256:executor-effect-plan".to_owned(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: "effect".to_owned(),
                node_path: "effect".to_owned(),
                parent_node_id: None,
                kind,
                label: Some("Effect".to_owned()),
                executor: Some(executor),
                input_ports: Vec::new(),
                output_ports,
                completion_policy,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec!["effect".to_owned()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        run.add_orchestration(activate_root_orchestration(source_ref, plan));
        run
    }

    fn launcher(
        repo: Arc<RunRepo>,
        dispatch: Arc<RecordingDispatch>,
    ) -> OrchestrationExecutorLauncher {
        OrchestrationExecutorLauncher::new(WorkflowRepositorySet {
            lifecycle_run_repo: repo,
            lifecycle_gate_repo: Arc::new(UnusedGateRepo),
            agent_procedure_repo: Arc::new(UnusedProcedureRepo),
        })
        .with_agent_call_dispatch(dispatch)
    }

    fn launcher_with_effects(
        repo: Arc<RunRepo>,
        effects: Arc<RecordingWorkflowExecutorEffectRepository>,
    ) -> OrchestrationExecutorLauncher {
        OrchestrationExecutorLauncher::new_for_test(
            WorkflowRepositorySet {
                lifecycle_run_repo: repo,
                lifecycle_gate_repo: Arc::new(UnusedGateRepo),
                agent_procedure_repo: Arc::new(UnusedProcedureRepo),
            },
            effects,
        )
    }

    fn run_with_continue_successor() -> LifecycleRun {
        let mut run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeThreadPolicy::CreateNew,
        );
        let orchestration = &mut run.orchestrations[0];
        let mut plan_node = orchestration.plan_snapshot.nodes[0].clone();
        plan_node.node_id = "continue".to_owned();
        plan_node.node_path = "continue".to_owned();
        plan_node.executor = Some(ExecutorSpec::AgentProcedure {
            procedure: AgentProcedureExecutionSpec::Snapshot {
                procedure_key: Some("continue".to_owned()),
                name: Some("Continue".to_owned()),
                contract: Box::new(AgentProcedureContract::default()),
                source_ref: None,
                contract_digest: None,
            },
            agent_reuse_policy: AgentReusePolicy::ContinueCurrentAgent,
            runtime_thread_policy: RuntimeThreadPolicy::DeliverToCurrentThread,
        });
        orchestration.plan_snapshot.nodes.push(plan_node);
        orchestration
            .plan_snapshot
            .activation_rules
            .push(ActivationRule::Transition {
                rule_id: "create-to-continue".to_owned(),
                from_node_id: "agent".to_owned(),
                to_node_id: "continue".to_owned(),
                condition: TransitionCondition::Always,
                join_policy: ActivityJoinPolicy::All,
                max_traversals: None,
                source_path: None,
            });
        let mut runtime_node = orchestration.node_tree[0].clone();
        runtime_node.node_id = "continue".to_owned();
        runtime_node.node_path = "continue".to_owned();
        runtime_node.status = RuntimeNodeStatus::Pending;
        runtime_node.executor_run_ref = None;
        runtime_node.agent_call = None;
        runtime_node.started_at = None;
        runtime_node.trace_refs.clear();
        orchestration.node_tree.push(runtime_node);
        run
    }

    fn run_with_ambiguous_predecessor_authorities() -> LifecycleRun {
        let mut run = run_with_continue_successor();
        let run_id = run.id;
        let orchestration = &mut run.orchestrations[0];
        let first_target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id,
            agent_id: Uuid::new_v4(),
        };
        let first = &mut orchestration.node_tree[0];
        first.status = RuntimeNodeStatus::Completed;
        first.started_at = Some(Utc::now());
        first.executor_run_ref = Some(ExecutorRunRef::AgentRun {
            run_id: first_target.run_id,
            agent_id: first_target.agent_id,
        });
        first.agent_call = Some(WorkflowAgentCallRuntimeState {
            request_id: "first".to_owned(),
            payload_digest: "sha256:first".to_owned(),
            target: first_target,
            request: serde_json::json!({}),
            prepared_at: Utc::now(),
            dispatched_at: Some(Utc::now()),
            runtime_thread_id: Some("thread:first".to_owned()),
            source_binding: Some(
                agentdash_domain::workflow::WorkflowAgentCallSourceBindingRef {
                    source_ref: "source:first".to_owned(),
                    committed_at_revision: 1,
                    applied_surface_revision: 2,
                    activated_at_revision: Some(3),
                },
            ),
            claim_id: Some("claim:first".to_owned()),
        });

        let mut second_plan = orchestration.plan_snapshot.nodes[0].clone();
        second_plan.node_id = "agent-2".to_owned();
        second_plan.node_path = "agent-2".to_owned();
        orchestration.plan_snapshot.nodes.push(second_plan);
        orchestration
            .plan_snapshot
            .activation_rules
            .push(ActivationRule::Transition {
                rule_id: "second-to-continue".to_owned(),
                from_node_id: "agent-2".to_owned(),
                to_node_id: "continue".to_owned(),
                condition: TransitionCondition::Always,
                join_policy: ActivityJoinPolicy::All,
                max_traversals: None,
                source_path: None,
            });
        let mut second = orchestration.node_tree[0].clone();
        let second_target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id,
            agent_id: Uuid::new_v4(),
        };
        second.node_id = "agent-2".to_owned();
        second.node_path = "agent-2".to_owned();
        second.executor_run_ref = Some(ExecutorRunRef::AgentRun {
            run_id: second_target.run_id,
            agent_id: second_target.agent_id,
        });
        let history = second.agent_call.as_mut().expect("second history");
        history.target = second_target;
        history.runtime_thread_id = Some("thread:second".to_owned());
        history.source_binding.as_mut().expect("binding").source_ref = "source:second".to_owned();
        orchestration.node_tree.push(second);
        orchestration.node_tree[1].status = RuntimeNodeStatus::Ready;
        orchestration.dispatch.ready_node_ids = vec!["continue".to_owned()];
        orchestration.status = agentdash_domain::workflow::OrchestrationStatus::Running;
        run
    }

    #[tokio::test]
    async fn drain_persists_prepared_dispatched_and_agent_run_identity() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeThreadPolicy::CreateNew,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch::default());
        let result = launcher(repo.clone(), dispatch.clone())
            .drain_ready_nodes(run_id)
            .await
            .expect("drain");

        assert_eq!(result.launched_agent_nodes.len(), 1);
        let launched = &result.launched_agent_nodes[0];
        assert_eq!(launched.runtime_thread_id, "runtime-thread-workflow-1");
        assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
        let run = repo.get_by_id(run_id).await.unwrap().unwrap();
        let node = &run.orchestrations[0].node_tree[0];
        assert_eq!(
            node.status,
            agentdash_domain::workflow::RuntimeNodeStatus::Running
        );
        assert_eq!(
            node.executor_run_ref,
            Some(ExecutorRunRef::AgentRun {
                run_id,
                agent_id: launched.agent_id,
            })
        );
        let history = node.agent_call.as_ref().expect("AgentCall history");
        assert_eq!(history.target.agent_id, launched.agent_id);
        assert!(history.dispatched_at.is_some());
        assert_eq!(
            history.runtime_thread_id.as_deref(),
            Some("runtime-thread-workflow-1")
        );
    }

    #[tokio::test]
    async fn unsupported_policy_is_rejected_before_dispatch_side_effect() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeThreadPolicy::DeliverToCurrentThread,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch::default());
        let result = launcher(repo, dispatch.clone())
            .drain_ready_nodes(run_id)
            .await
            .expect("drain");

        assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
        assert_eq!(result.failed_nodes, vec!["agent"]);
    }

    #[tokio::test]
    async fn continue_current_missing_authority_blocks_before_product_effect() {
        let run = run_with_agent_policy(
            AgentReusePolicy::ContinueCurrentAgent,
            RuntimeThreadPolicy::DeliverToCurrentThread,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch::default());
        let result = launcher(repo, dispatch.clone())
            .drain_ready_nodes(run_id)
            .await
            .expect("drain");

        assert_eq!(result.failed_nodes, vec!["agent"]);
        assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn continue_current_ambiguous_authority_blocks_before_product_effect() {
        let run = run_with_ambiguous_predecessor_authorities();
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch::default());
        let result = launcher(repo, dispatch.clone())
            .drain_ready_nodes(run_id)
            .await
            .expect("drain");

        assert_eq!(result.failed_nodes, vec!["continue"]);
        assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn pending_retry_replays_byte_identical_durable_request() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeThreadPolicy::CreateNew,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch {
            pending_once: AtomicBool::new(true),
            ..Default::default()
        });
        let launcher = launcher(repo, dispatch.clone());

        let first = launcher.drain_ready_nodes(run_id).await.expect("pending");
        assert!(first.launched_agent_nodes.is_empty());
        let second = launcher.drain_ready_nodes(run_id).await.expect("retry");
        assert_eq!(second.launched_agent_nodes.len(), 1, "{second:?}");
        let requests = dispatch.requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            serde_json::to_vec(&requests[0]).unwrap(),
            serde_json::to_vec(&requests[1]).unwrap()
        );
    }

    #[tokio::test]
    async fn continue_current_traces_unique_started_predecessor_authority() {
        let run = run_with_continue_successor();
        let run_id = run.id;
        let orchestration_id = run.orchestrations[0].orchestration_id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch::default());
        let launcher = launcher(repo.clone(), dispatch.clone());

        let first = launcher.drain_ready_nodes(run_id).await.expect("create");
        assert_eq!(first.launched_agent_nodes.len(), 1);
        let stored = repo.get_by_id(run_id).await.unwrap().unwrap();
        let expected_revision = stored.revision;
        let (completed, _) = apply_orchestration_event_to_run(
            stored,
            orchestration_id,
            OrchestrationRuntimeEvent::NodeCompleted {
                node_path: "agent".to_owned(),
                attempt: 1,
                outputs: Vec::new(),
                timestamp: Utc::now(),
            },
        )
        .expect("complete predecessor");
        repo.compare_and_swap(expected_revision, &completed)
            .await
            .expect("commit completion");

        let second = launcher.drain_ready_nodes(run_id).await.expect("continue");
        assert_eq!(second.launched_agent_nodes.len(), 1, "{second:?}");
        let requests = dispatch.requests.lock().await;
        let first_target = requests[0].target_intent.target().clone();
        let WorkflowAgentCallTargetIntent::ContinueCurrent {
            target,
            runtime_thread_id,
            source_binding,
        } = &requests[1].target_intent
        else {
            panic!("successor must continue current authority");
        };
        assert_eq!(target, &first_target);
        assert_eq!(runtime_thread_id, "runtime-thread-workflow-1");
        assert_eq!(source_binding.source_ref, "source:workflow-agent");
    }

    #[tokio::test]
    async fn concurrent_drains_commit_one_atomic_agent_start() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeThreadPolicy::CreateNew,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch::default());
        let first_launcher = launcher(repo.clone(), dispatch.clone());
        let second_launcher = launcher(repo.clone(), dispatch);

        let (first, second) = tokio::join!(
            first_launcher.drain_ready_nodes(run_id),
            second_launcher.drain_ready_nodes(run_id)
        );
        let launched = first.expect("first drain").launched_agent_nodes.len()
            + second.expect("second drain").launched_agent_nodes.len();
        assert_eq!(launched, 1);
        let stored = repo.get_by_id(run_id).await.unwrap().unwrap();
        let node = &stored.orchestrations[0].node_tree[0];
        assert_eq!(node.status, RuntimeNodeStatus::Running);
        assert!(node.agent_call.as_ref().unwrap().claim_id.is_some());
    }

    #[tokio::test]
    async fn lifecycle_run_cas_rejects_stale_writer() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeThreadPolicy::CreateNew,
        );
        let repo = RunRepo {
            run: Mutex::new(Some(run.clone())),
            ..Default::default()
        };
        let mut winner = run.clone();
        winner.revision += 1;
        repo.compare_and_swap(0, &winner).await.expect("winner");
        let mut stale = run;
        stale.revision += 1;
        let error = repo
            .compare_and_swap(0, &stale)
            .await
            .expect_err("stale writer");
        assert!(matches!(
            error,
            LifecycleRunWriteError::RevisionConflict {
                expected_revision: 0,
                actual_revision: 1,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn accepted_product_effect_replays_with_stable_claim_after_cas_failure() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeThreadPolicy::CreateNew,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            fail_cas_at_expected_revision: Mutex::new(Some(1)),
            ..Default::default()
        });
        let dispatch = Arc::new(RecordingDispatch::default());
        let launcher = launcher(repo.clone(), dispatch.clone());

        let first_error = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect_err("atomic start CAS failure");
        assert!(matches!(first_error, WorkflowApplicationError::Internal(_)));
        let prepared = repo.get_by_id(run_id).await.unwrap().unwrap();
        assert_eq!(prepared.revision, 1);
        assert!(
            prepared.orchestrations[0].node_tree[0]
                .agent_call
                .as_ref()
                .unwrap()
                .claim_id
                .is_none()
        );

        let replay = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("replay accepted Product saga");
        assert_eq!(replay.launched_agent_nodes.len(), 1);
        let claim_ids = repo.attempted_claim_ids.lock().await;
        assert_eq!(claim_ids.len(), 2);
        assert_eq!(claim_ids[0], claim_ids[1]);
        let requests = dispatch.requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            serde_json::to_vec(&requests[0]).unwrap(),
            serde_json::to_vec(&requests[1]).unwrap()
        );
    }

    #[tokio::test]
    async fn function_not_applied_dispatches_once_then_terminal_receipt_reapplies() {
        let run = run_with_function_node();
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            conflict_cas_at_expected_revision: Mutex::new(Some(1)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let runner = Arc::new(RecordingStableFunctionRunner::default());
        let launcher = launcher_with_effects(repo.clone(), effects.clone())
            .with_function_runner(runner.clone());

        let result = launcher.drain_ready_nodes(run_id).await.expect("drain");

        assert_eq!(result.completed_effect_nodes, vec!["effect"]);
        assert_eq!(runner.executions.load(Ordering::SeqCst), 1);
        assert!(
            effects
                .get_function(&format!(
                    "workflow-function:{}:effect#1",
                    repo.get_by_id(run_id)
                        .await
                        .unwrap()
                        .unwrap()
                        .orchestrations[0]
                        .orchestration_id
                ))
                .await
                .unwrap()
                .unwrap()
                .terminal
                .is_some()
        );
        assert_eq!(
            repo.get_by_id(run_id)
                .await
                .unwrap()
                .unwrap()
                .orchestrations[0]
                .node_tree[0]
                .status,
            RuntimeNodeStatus::Completed
        );
    }

    #[tokio::test]
    async fn function_accepted_recovery_only_inspects_without_execution() {
        let run = run_with_function_node();
        let run_id = run.id;
        let orchestration_id = run.orchestrations[0].orchestration_id;
        let effect_id = function_effect_id(&RuntimeNodeCoordinate::new(
            run_id,
            orchestration_id,
            "effect",
            1,
        ));
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let runner = Arc::new(RecordingStableFunctionRunner::observing(
            effect_id.clone(),
            FunctionEffectObservation::Accepted,
        ));
        let launcher =
            launcher_with_effects(repo.clone(), effects).with_function_runner(runner.clone());

        launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("accepted effect remains pending");
        launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("accepted recovery remains inspect-only");
        runner
            .observations
            .lock()
            .await
            .insert(effect_id, FunctionEffectObservation::InFlight);
        launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("in-flight recovery remains inspect-only");

        assert_eq!(runner.executions.load(Ordering::SeqCst), 0);
        assert_eq!(
            repo.get_by_id(run_id)
                .await
                .unwrap()
                .unwrap()
                .orchestrations[0]
                .node_tree[0]
                .status,
            RuntimeNodeStatus::Running
        );
    }

    #[tokio::test]
    async fn function_lost_observation_blocks_after_receipt_loss_without_reexecution() {
        let run = run_with_function_node();
        let run_id = run.id;
        let orchestration_id = run.orchestrations[0].orchestration_id;
        let effect_id = function_effect_id(&RuntimeNodeCoordinate::new(
            run_id,
            orchestration_id,
            "effect",
            1,
        ));
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let first_runner = Arc::new(RecordingStableFunctionRunner::losing_receipt());
        let first_launcher = launcher_with_effects(repo.clone(), effects.clone())
            .with_function_runner(first_runner.clone());

        first_launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("receipt loss leaves effect inspect-only");
        assert_eq!(first_runner.executions.load(Ordering::SeqCst), 1);
        assert_eq!(
            repo.get_by_id(run_id)
                .await
                .unwrap()
                .unwrap()
                .orchestrations[0]
                .node_tree[0]
                .status,
            RuntimeNodeStatus::Running
        );

        *repo.fail_cas_at_expected_revision.lock().await = Some(1);
        let recovery_runner = Arc::new(RecordingStableFunctionRunner::observing(
            effect_id,
            FunctionEffectObservation::Lost {
                reason: "HTTP source has no idempotency receipt to inspect".to_owned(),
                evidence: serde_json::json!({
                    "transport": "http",
                    "dispatch_intent_revision": 9,
                }),
            },
        ));
        let recovery_launcher = launcher_with_effects(repo.clone(), effects)
            .with_function_runner(recovery_runner.clone());
        recovery_launcher
            .drain_ready_nodes(run_id)
            .await
            .expect_err("injected Blocked projection crash");
        let recovered = recovery_launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("Lost observation reapplies as Blocked");
        assert!(recovered.failed_nodes.is_empty());
        let duplicate = recovery_launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("duplicate drain");
        assert!(duplicate.failed_nodes.is_empty());
        assert_eq!(first_runner.executions.load(Ordering::SeqCst), 1);
        assert_eq!(recovery_runner.executions.load(Ordering::SeqCst), 0);
        let stored = repo.get_by_id(run_id).await.unwrap().unwrap();
        let node = &stored.orchestrations[0].node_tree[0];
        assert_eq!(node.status, RuntimeNodeStatus::Blocked);
        let error = node.error.as_ref().expect("Lost evidence");
        assert_eq!(error.code, "function_effect_outcome_lost");
        assert!(!error.retryable);
        assert_eq!(
            error
                .detail
                .as_ref()
                .and_then(|detail| detail.pointer("/evidence/dispatch_intent_revision")),
            Some(&serde_json::json!(9))
        );
    }

    #[tokio::test]
    async fn function_failed_terminal_receipt_reapplies_without_reexecution() {
        let run = run_with_function_node();
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            fail_cas_at_expected_revision: Mutex::new(Some(1)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let runner = Arc::new(RecordingStableFunctionRunner::returning(
            FunctionEffectObservation::Failed {
                message: "remote command rejected".to_owned(),
                retryable: false,
            },
        ));
        let launcher =
            launcher_with_effects(repo.clone(), effects).with_function_runner(runner.clone());

        launcher
            .drain_ready_nodes(run_id)
            .await
            .expect_err("injected Failed projection crash");
        let replay = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("Failed receipt replay");

        assert_eq!(replay.failed_nodes, vec!["effect"]);
        assert_eq!(runner.executions.load(Ordering::SeqCst), 1);
        let stored = repo.get_by_id(run_id).await.unwrap().unwrap();
        let node = &stored.orchestrations[0].node_tree[0];
        assert_eq!(node.status, RuntimeNodeStatus::Failed);
        assert_eq!(
            node.error.as_ref().map(|error| error.code.as_str()),
            Some("function_effect_failed")
        );
    }

    #[tokio::test]
    async fn function_terminal_receipt_survives_crash_before_lifecycle_projection() {
        let run = run_with_function_node();
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            fail_cas_at_expected_revision: Mutex::new(Some(1)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let runner = Arc::new(RecordingStableFunctionRunner::default());
        let launcher =
            launcher_with_effects(repo.clone(), effects).with_function_runner(runner.clone());

        launcher
            .drain_ready_nodes(run_id)
            .await
            .expect_err("injected projection crash");
        assert_eq!(
            repo.get_by_id(run_id)
                .await
                .unwrap()
                .unwrap()
                .orchestrations[0]
                .node_tree[0]
                .status,
            RuntimeNodeStatus::Running
        );
        let replay = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("receipt replay");
        assert_eq!(replay.completed_effect_nodes, vec!["effect"]);
        assert_eq!(runner.executions.load(Ordering::SeqCst), 1);
        let duplicate = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("duplicate drain");
        assert!(duplicate.completed_effect_nodes.is_empty());
        assert_eq!(runner.executions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_human_gate_open_uses_one_stable_gate_receipt() {
        let run = run_with_human_gate_node();
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let first = launcher_with_effects(repo.clone(), effects.clone());
        let second = launcher_with_effects(repo.clone(), effects.clone());

        let (first, second) = tokio::join!(
            first.drain_ready_nodes(run_id),
            second.drain_ready_nodes(run_id)
        );
        let opened = first.expect("first").opened_human_gates.len()
            + second.expect("second").opened_human_gates.len();
        assert_eq!(opened, 1);
        assert_eq!(effects.gate_opens.lock().await.len(), 1);
        assert_eq!(effects.gates.lock().await.len(), 1);
        let node = &repo
            .get_by_id(run_id)
            .await
            .unwrap()
            .unwrap()
            .orchestrations[0]
            .node_tree[0];
        assert_eq!(node.status, RuntimeNodeStatus::Running);
    }

    #[tokio::test]
    async fn human_gate_resolution_receipt_recovers_running_node_after_cas_crash() {
        let run = run_with_human_gate_node();
        let run_id = run.id;
        let orchestration_id = run.orchestrations[0].orchestration_id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let launcher = launcher_with_effects(repo.clone(), effects.clone());
        let opened = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("open gate")
            .opened_human_gates
            .pop()
            .expect("gate");
        *repo.fail_cas_at_expected_revision.lock().await = Some(1);
        let input = SubmitHumanGateDecisionInput {
            run_id,
            orchestration_id,
            node_path: "effect".to_owned(),
            attempt: 1,
            decision: serde_json::json!({"approved": true}),
            resolved_by: "reviewer".to_owned(),
        };

        launcher
            .submit_human_gate_decision(input.clone())
            .await
            .expect_err("injected projection crash");
        assert_eq!(effects.gate_resolutions.lock().await.len(), 1);
        assert_eq!(
            repo.get_by_id(run_id)
                .await
                .unwrap()
                .unwrap()
                .orchestrations[0]
                .node_tree[0]
                .status,
            RuntimeNodeStatus::Running
        );

        launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("recover resolution");
        let replay = launcher
            .submit_human_gate_decision(input.clone())
            .await
            .expect("idempotent decision replay");
        assert_eq!(replay.gate_id, opened.gate_id);
        assert_eq!(effects.gate_resolutions.lock().await.len(), 1);
        let conflicting = SubmitHumanGateDecisionInput {
            decision: serde_json::json!({"approved": false}),
            ..input
        };
        launcher
            .submit_human_gate_decision(conflicting)
            .await
            .expect_err("different terminal decision must conflict");
        assert_eq!(effects.gate_resolutions.lock().await.len(), 1);
        assert_eq!(
            repo.get_by_id(run_id)
                .await
                .unwrap()
                .unwrap()
                .orchestrations[0]
                .node_tree[0]
                .status,
            RuntimeNodeStatus::Completed
        );
    }

    #[tokio::test]
    async fn nested_human_gate_terminal_decision_replay_uses_durable_receipt() {
        let mut run = run_with_human_gate_node();
        nest_only_runtime_node(&mut run);
        let run_id = run.id;
        let orchestration_id = run.orchestrations[0].orchestration_id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let launcher = launcher_with_effects(repo.clone(), effects.clone());
        let opened = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("open nested gate")
            .opened_human_gates
            .pop()
            .expect("nested gate");
        let input = SubmitHumanGateDecisionInput {
            run_id,
            orchestration_id,
            node_path: "effect".to_owned(),
            attempt: 1,
            decision: serde_json::json!({"approved": true}),
            resolved_by: "reviewer".to_owned(),
        };

        launcher
            .submit_human_gate_decision(input.clone())
            .await
            .expect("complete nested gate");
        let replay = launcher
            .submit_human_gate_decision(input)
            .await
            .expect("replay nested terminal decision");

        assert_eq!(replay.gate_id, opened.gate_id);
        assert_eq!(effects.gate_resolutions.lock().await.len(), 1);
        assert_eq!(
            repo.get_by_id(run_id)
                .await
                .unwrap()
                .unwrap()
                .orchestrations[0]
                .node_tree[0]
                .children[0]
                .status,
            RuntimeNodeStatus::Completed
        );
    }

    #[tokio::test]
    async fn same_node_path_in_distinct_orchestrations_keeps_gate_receipts_isolated() {
        let mut run = run_with_human_gate_node();
        let mut other = run_with_human_gate_node().orchestrations.remove(0);
        other.orchestration_id = Uuid::new_v4();
        run.orchestrations.push(other);
        let run_id = run.id;
        let first_orchestration_id = run.orchestrations[0].orchestration_id;
        let second_orchestration_id = run.orchestrations[1].orchestration_id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
            ..Default::default()
        });
        let effects = Arc::new(RecordingWorkflowExecutorEffectRepository::default());
        let launcher = launcher_with_effects(repo.clone(), effects.clone());
        let opened = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("open both gates")
            .opened_human_gates;
        assert_eq!(opened.len(), 2);
        let second_gate_id = opened
            .iter()
            .find(|gate| gate.orchestration_id == second_orchestration_id)
            .expect("second gate")
            .gate_id;
        let second_input = SubmitHumanGateDecisionInput {
            run_id,
            orchestration_id: second_orchestration_id,
            node_path: "effect".to_owned(),
            attempt: 1,
            decision: serde_json::json!({"approved": true}),
            resolved_by: "second-reviewer".to_owned(),
        };

        launcher
            .submit_human_gate_decision(second_input.clone())
            .await
            .expect("resolve second gate");
        let replay = launcher
            .submit_human_gate_decision(second_input)
            .await
            .expect("replay second gate");

        assert_eq!(replay.gate_id, second_gate_id);
        assert_eq!(effects.gate_resolutions.lock().await.len(), 1);
        let stored = repo.get_by_id(run_id).await.unwrap().unwrap();
        let first = stored
            .orchestrations
            .iter()
            .find(|item| item.orchestration_id == first_orchestration_id)
            .expect("first orchestration");
        let second = stored
            .orchestrations
            .iter()
            .find(|item| item.orchestration_id == second_orchestration_id)
            .expect("second orchestration");
        assert_eq!(first.node_tree[0].status, RuntimeNodeStatus::Running);
        assert_eq!(second.node_tree[0].status, RuntimeNodeStatus::Completed);
    }
}
