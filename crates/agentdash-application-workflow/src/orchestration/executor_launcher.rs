use std::sync::Arc;

use agentdash_domain::workflow::{
    ArtifactAliasPolicy, ExecutorRunRef, LifecycleGateRepository, LifecycleRun,
    LifecycleRunRepository, PlanNode, PlanNodeKind, RuntimeNodeError,
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
                            self.apply_event(run, coordinate.orchestration_id, event)
                                .await?;
                        }
                        WorkflowAgentCallLaunchOutcome::Pending => return Ok(result),
                        WorkflowAgentCallLaunchOutcome::Accepted {
                            target,
                            runtime_thread_id,
                            dispatch_event,
                        } => {
                            let run = if let Some(event) = dispatch_event {
                                self.apply_event(run, coordinate.orchestration_id, event)
                                    .await?
                            } else {
                                run
                            };
                            let run = self
                                .apply_event(
                                    run,
                                    coordinate.orchestration_id,
                                    OrchestrationRuntimeEvent::NodeClaimed {
                                        node_path: coordinate.node_path.clone(),
                                        attempt: coordinate.attempt,
                                        timestamp: chrono::Utc::now(),
                                    },
                                )
                                .await?;
                            self.apply_event(
                                run,
                                coordinate.orchestration_id,
                                OrchestrationRuntimeEvent::NodeStarted {
                                    node_path: coordinate.node_path.clone(),
                                    attempt: coordinate.attempt,
                                    executor_run_ref: Some(ExecutorRunRef::AgentRun {
                                        run_id: target.run_id,
                                        agent_id: target.agent_id,
                                    }),
                                    timestamp: chrono::Utc::now(),
                                },
                            )
                            .await?;
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use agentdash_domain::{
        DomainError,
        workflow::{
            AgentProcedure, AgentProcedureContract, AgentProcedureExecutionSpec,
            AgentProcedureRepository, AgentReusePolicy, ExecutorSpec, LifecycleGate,
            OrchestrationLimits, OrchestrationPlanSnapshot, OrchestrationSourceRef, PlanNode,
            RuntimeSessionPolicy,
        },
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::Mutex;

    use super::*;
    use crate::orchestration::{
        WorkflowAgentCallDispatchError, WorkflowAgentCallDispatchOutcome,
        WorkflowAgentCallDispatchPort, WorkflowAgentCallMailboxState, WorkflowAgentCallRequest,
        activate_root_orchestration,
    };

    #[derive(Default)]
    struct RunRepo {
        run: Mutex<Option<LifecycleRun>>,
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
            self.requests.lock().await.push(request);
            if self.pending_once.swap(false, Ordering::SeqCst) {
                return Ok(WorkflowAgentCallDispatchOutcome::Pending);
            }
            Ok(WorkflowAgentCallDispatchOutcome::Accepted {
                target,
                runtime_thread_id: "runtime-thread-workflow-1".to_owned(),
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

    #[tokio::test]
    async fn drain_persists_prepared_dispatched_and_agent_run_identity() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeSessionPolicy::CreateNew,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
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
    async fn pending_retry_replays_byte_identical_durable_request() {
        let run = run_with_agent_policy(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeSessionPolicy::CreateNew,
        );
        let run_id = run.id;
        let repo = Arc::new(RunRepo {
            run: Mutex::new(Some(run)),
        });
        let dispatch = Arc::new(RecordingDispatch {
            pending_once: AtomicBool::new(true),
            ..Default::default()
        });
        let launcher = launcher(repo, dispatch.clone());

        let first = launcher.drain_ready_nodes(run_id).await.expect("pending");
        assert!(first.launched_agent_nodes.is_empty());
        let second = launcher.drain_ready_nodes(run_id).await.expect("retry");
        assert_eq!(second.launched_agent_nodes.len(), 1);
        let requests = dispatch.requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            serde_json::to_vec(&requests[0]).unwrap(),
            serde_json::to_vec(&requests[1]).unwrap()
        );
    }
}
