use std::sync::Arc;

use agentdash_domain::workflow::{
    ArtifactAliasPolicy, ExecutorRunRef, LifecycleGateRepository, LifecycleRun,
    LifecycleRunRepository, LifecycleRunWriteError, PlanNode, PlanNodeKind, RuntimeNodeError,
};
use agentdash_platform_spi::FunctionRunner;
use serde_json::Value;
use uuid::Uuid;

use crate::{WorkflowApplicationError, WorkflowRepositorySet};

use super::agent_call::{
    WorkflowAgentCallDispatchPort, WorkflowAgentCallLaunchOutcome, WorkflowAgentCallLauncher,
};
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
    lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    agent_procedure_repo: Arc<dyn agentdash_domain::workflow::AgentProcedureRepository>,
}

impl From<WorkflowRepositorySet> for OrchestrationExecutorRepositories {
    fn from(repos: WorkflowRepositorySet) -> Self {
        Self {
            lifecycle_run_repo: repos.lifecycle_run_repo,
            lifecycle_gate_repo: repos.lifecycle_gate_repo,
            agent_procedure_repo: repos.agent_procedure_repo,
        }
    }
}

impl OrchestrationExecutorLauncher {
    pub fn new(repos: WorkflowRepositorySet) -> Self {
        Self::from_executor_repositories(repos.into())
    }

    fn from_executor_repositories(repos: OrchestrationExecutorRepositories) -> Self {
        let human_gate_launcher = HumanGateLauncher::new(repos.lifecycle_gate_repo.clone());
        let agent_call_launcher =
            WorkflowAgentCallLauncher::new(repos.agent_procedure_repo.clone());
        Self {
            repos,
            function_node_runner: FunctionNodeRunner::new(),
            human_gate_launcher,
            agent_call_launcher,
        }
    }

    pub fn with_function_runner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
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
        for _ in 0..MAX_DRAIN_STEPS {
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
                PlanNodeKind::Function | PlanNodeKind::LocalEffect => {
                    let node_path = coordinate.node_path.clone();
                    match self.launch_function_node(run, coordinate).await? {
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

    async fn launch_function_node(
        &self,
        run: LifecycleRun,
        coordinate: RuntimeNodeCoordinate,
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

        let terminal = match self.function_node_runner.execute(&run, &coordinate).await {
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
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use agentdash_domain::{
        DomainError,
        workflow::{
            ActivationRule, ActivityJoinPolicy, AgentProcedure, AgentProcedureContract,
            AgentProcedureExecutionSpec, AgentProcedureRepository, AgentReusePolicy, ExecutorSpec,
            LifecycleGate, OrchestrationLimits, OrchestrationPlanSnapshot, OrchestrationSourceRef,
            PlanNode, RuntimeNodeStatus, RuntimeSessionPolicy, TransitionCondition,
            WorkflowAgentCallRuntimeState,
        },
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

    fn run_with_agent_policy(
        reuse: AgentReusePolicy,
        session: RuntimeSessionPolicy,
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
                    runtime_session_policy: session,
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

    fn run_with_continue_successor() -> LifecycleRun {
        let mut run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeSessionPolicy::CreateNew,
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
            runtime_session_policy: RuntimeSessionPolicy::DeliverToCurrentTrace,
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
            RuntimeSessionPolicy::CreateNew,
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
            RuntimeSessionPolicy::DeliverToCurrentTrace,
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
            RuntimeSessionPolicy::DeliverToCurrentTrace,
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
            RuntimeSessionPolicy::CreateNew,
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
            RuntimeSessionPolicy::CreateNew,
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
            RuntimeSessionPolicy::CreateNew,
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
            RuntimeSessionPolicy::CreateNew,
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
}
