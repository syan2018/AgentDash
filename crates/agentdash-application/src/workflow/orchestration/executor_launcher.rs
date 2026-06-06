use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentProcedureRef, AgentProcedureRepository,
    AgentReusePolicy, ArtifactAliasPolicy, ExecutorRunRef, ExecutorSpec,
    FunctionActivityExecutorSpec, LifecycleAgent, LifecycleAgentRepository, LifecycleGate,
    LifecycleGateRepository, LifecycleRun, LifecycleRunRepository, NodePortValue,
    OrchestrationInstance, OrchestrationStatus, PlanNode, PlanNodeKind, RuntimeNodeError,
    RuntimeNodeState, RuntimeNodeStatus, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository, RuntimeSessionPolicy,
};
use agentdash_spi::{ApiRequestOutcome, BashExecOutcome, FunctionRunner};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::workflow::frame_builder::AgentFrameBuilder;
use crate::workflow::{RuntimeSessionCreationRequest, WorkflowApplicationError};

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
    function_runner: Option<Arc<dyn FunctionRunner>>,
}

#[derive(Clone)]
struct OrchestrationExecutorRepositories {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    runtime_session_creator: Arc<dyn crate::workflow::RuntimeSessionCreator>,
}

impl From<RepositorySet> for OrchestrationExecutorRepositories {
    fn from(repos: RepositorySet) -> Self {
        Self {
            lifecycle_run_repo: repos.lifecycle_run_repo,
            agent_procedure_repo: repos.agent_procedure_repo,
            lifecycle_agent_repo: repos.lifecycle_agent_repo,
            agent_frame_repo: repos.agent_frame_repo,
            lifecycle_gate_repo: repos.lifecycle_gate_repo,
            execution_anchor_repo: repos.execution_anchor_repo,
            runtime_session_creator: repos.runtime_session_creator,
        }
    }
}

impl OrchestrationExecutorLauncher {
    pub fn new(repos: RepositorySet) -> Self {
        Self {
            repos: repos.into(),
            function_runner: None,
        }
    }

    #[cfg(test)]
    fn from_repositories(repos: OrchestrationExecutorRepositories) -> Self {
        Self {
            repos,
            function_runner: None,
        }
    }

    pub fn with_function_runner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.function_runner = Some(runner);
        self
    }

    pub async fn drain_ready_nodes(
        &self,
        run_id: Uuid,
    ) -> Result<OrchestrationExecutorDrainResult, WorkflowApplicationError> {
        let mut result = OrchestrationExecutorDrainResult::default();
        for _ in 0..MAX_DRAIN_STEPS {
            let run = self.load_run(run_id).await?;
            let Some(target) = ReadyNodeTarget::next(&run) else {
                return Ok(result);
            };
            if let Some((code, message)) = unsupported_attempt_policy(&target.plan_node) {
                let node_path = target.node_path.clone();
                self.block_ready_node(run, target, code, &message, false)
                    .await?;
                result.failed_nodes.push(node_path);
                continue;
            }
            match target.kind {
                PlanNodeKind::AgentCall => {
                    if let Some(launched) = self.launch_agent_node(run, target).await? {
                        result.launched_agent_nodes.push(launched);
                    }
                }
                PlanNodeKind::Function | PlanNodeKind::LocalEffect => {
                    let node_path = target.node_path.clone();
                    match self.launch_function_node(run, target).await? {
                        FunctionLaunchTerminal::Completed => {
                            result.completed_effect_nodes.push(node_path);
                        }
                        FunctionLaunchTerminal::Failed => {
                            result.failed_nodes.push(node_path);
                        }
                    }
                }
                PlanNodeKind::HumanGate => {
                    if let Some(opened) = self.open_human_gate(run, target).await? {
                        result.opened_human_gates.push(opened);
                    }
                }
                _ => {
                    let node_path = target.node_path.clone();
                    self.block_ready_node(
                        run,
                        target,
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
        let target = ReadyNodeTarget::from_running_node(
            &run,
            input.orchestration_id,
            &input.node_path,
            input.attempt,
        )?;
        if target.kind != PlanNodeKind::HumanGate {
            return Err(WorkflowApplicationError::Conflict(format!(
                "node {} 不是 HumanGate",
                input.node_path
            )));
        }
        let gate_id = human_gate_id_from_node(&target.runtime_node)?;
        let mut gate = self
            .repos
            .lifecycle_gate_repo
            .get(gate_id)
            .await?
            .ok_or_else(|| WorkflowApplicationError::NotFound(format!("gate 不存在: {gate_id}")))?;
        if !gate.is_open() {
            return Err(WorkflowApplicationError::Conflict(format!(
                "gate {gate_id} 已经 resolved"
            )));
        }
        gate.payload_json = Some(input.decision.clone());
        gate.resolve(input.resolved_by);
        self.repos.lifecycle_gate_repo.update(&gate).await?;

        let outputs = human_decision_outputs(&target.plan_node, input.decision);
        let run = self
            .apply_event(
                run,
                target.orchestration_id,
                OrchestrationRuntimeEvent::NodeCompleted {
                    node_path: target.node_path.clone(),
                    attempt: target.attempt,
                    outputs,
                    timestamp: chrono::Utc::now(),
                },
            )
            .await?;
        let drain_result = self.drain_ready_nodes(run.id).await?;
        let final_run = self.load_run(run.id).await?;
        Ok(SubmitHumanGateDecisionResult {
            run: final_run,
            gate_id,
            drain_result,
        })
    }

    async fn launch_agent_node(
        &self,
        run: LifecycleRun,
        target: ReadyNodeTarget,
    ) -> Result<Option<LaunchedAgentNode>, WorkflowApplicationError> {
        let executor = target.plan_node.executor.clone();
        let Some(ExecutorSpec::AgentProcedure {
            procedure_key,
            agent_reuse_policy,
            runtime_session_policy,
        }) = executor
        else {
            self.block_ready_node(
                run,
                target,
                "agent_executor_missing",
                "AgentCall node 缺少 AgentProcedure executor spec",
                false,
            )
            .await?;
            return Ok(None);
        };
        let procedure = match self
            .repos
            .agent_procedure_repo
            .get_by_project_and_key(run.project_id, &procedure_key)
            .await?
        {
            Some(procedure) => procedure,
            None => {
                self.block_ready_node(
                    run,
                    target,
                    "agent_procedure_not_found",
                    &format!("AgentProcedure `{procedure_key}` 不存在"),
                    false,
                )
                .await?;
                return Ok(None);
            }
        };

        let (mut agent, session_id) = match (agent_reuse_policy, runtime_session_policy) {
            (AgentReusePolicy::CreateActivityAgent, RuntimeSessionPolicy::CreateNew) => {
                let agent = LifecycleAgent::new_root(run.id, run.project_id, "workflow_agent")
                    .with_bootstrap_status(
                        agentdash_domain::workflow::bootstrap_status::NOT_APPLICABLE,
                    );
                self.repos.lifecycle_agent_repo.create(&agent).await?;
                let session_id = self
                    .repos
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
                self.block_ready_node(
                    run,
                    target,
                    "agent_executor_policy_not_supported",
                    "ContinueCurrentAgent + DeliverToCurrentTrace 需要 connector delivery surface，当前 orchestration executor 不伪造已投递状态",
                    false,
                )
                .await?;
                return Ok(None);
            }
            _ => {
                self.block_ready_node(
                    run,
                    target,
                    "agent_executor_policy_not_supported",
                    "AgentCall executor policy 当前 scheduler 不支持",
                    false,
                )
                .await?;
                return Ok(None);
            }
        };

        let frame = self
            .create_frame(
                &agent,
                &target,
                Some(session_id.clone()),
                Some(procedure.id),
            )
            .await?;
        agent.set_current_frame(frame.id);
        self.repos.lifecycle_agent_repo.update(&agent).await?;
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            session_id.clone(),
            run.id,
            frame.id,
            agent.id,
            target.orchestration_id,
            target.node_path.clone(),
            target.attempt,
        );
        self.repos.execution_anchor_repo.upsert(&anchor).await?;

        self.apply_event(
            run,
            target.orchestration_id,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path: target.node_path.clone(),
                attempt: target.attempt,
                executor_run_ref: Some(ExecutorRunRef::RuntimeSession {
                    session_id: session_id.clone(),
                }),
                timestamp: chrono::Utc::now(),
            },
        )
        .await?;

        Ok(Some(LaunchedAgentNode {
            run_id: anchor.run_id,
            orchestration_id: target.orchestration_id,
            node_path: target.node_path,
            attempt: target.attempt,
            runtime_session_id: session_id,
        }))
    }

    async fn launch_function_node(
        &self,
        run: LifecycleRun,
        target: ReadyNodeTarget,
    ) -> Result<FunctionLaunchTerminal, WorkflowApplicationError> {
        let function_run_id = Uuid::new_v4().to_string();
        let run = self
            .apply_event(
                run,
                target.orchestration_id,
                OrchestrationRuntimeEvent::NodeStarted {
                    node_path: target.node_path.clone(),
                    attempt: target.attempt,
                    executor_run_ref: Some(ExecutorRunRef::FunctionRun {
                        run_id: function_run_id.clone(),
                    }),
                    timestamp: chrono::Utc::now(),
                },
            )
            .await?;

        let terminal = match self.execute_function_like_node(&run, &target).await {
            Ok(outputs) => OrchestrationRuntimeEvent::NodeCompleted {
                node_path: target.node_path.clone(),
                attempt: target.attempt,
                outputs,
                timestamp: chrono::Utc::now(),
            },
            Err(error) => OrchestrationRuntimeEvent::NodeFailed {
                node_path: target.node_path.clone(),
                attempt: target.attempt,
                error,
                timestamp: chrono::Utc::now(),
            },
        };
        let completed = matches!(terminal, OrchestrationRuntimeEvent::NodeCompleted { .. });
        let run_id = run.id;
        if let Err(error) = self
            .apply_event(run, target.orchestration_id, terminal)
            .await
        {
            let latest_run = self.load_run(run_id).await?;
            self.apply_event(
                latest_run,
                target.orchestration_id,
                OrchestrationRuntimeEvent::NodeFailed {
                    node_path: target.node_path.clone(),
                    attempt: target.attempt,
                    error: RuntimeNodeError {
                        code: "terminal_materialization_failed".to_string(),
                        message: error.to_string(),
                        retryable: false,
                        detail: Some(json!({ "node_id": target.node_id })),
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
        target: ReadyNodeTarget,
    ) -> Result<Option<OpenedHumanGate>, WorkflowApplicationError> {
        match target.plan_node.executor.clone() {
            Some(ExecutorSpec::Human { .. }) => {}
            None => {
                self.block_ready_node(
                    run,
                    target,
                    "human_gate_executor_missing",
                    "HumanGate node 缺少 Human executor spec",
                    false,
                )
                .await?;
                return Ok(None);
            }
            Some(_) => {
                self.block_ready_node(
                    run,
                    target,
                    "human_gate_executor_mismatch",
                    "HumanGate node 的 executor spec 类型不匹配",
                    false,
                )
                .await?;
                return Ok(None);
            }
        }
        let gate = LifecycleGate::open(
            run.id,
            None,
            None,
            "orchestration_human_gate",
            human_gate_correlation_id(target.orchestration_id, &target.node_path, target.attempt),
            Some(json!({
                "contract": "orchestration_human_gate.v1",
                "run_id": run.id,
                "orchestration_id": target.orchestration_id,
                "node_path": target.node_path.clone(),
                "attempt": target.attempt,
                "plan_node_id": target.node_id.clone(),
                "label": target.plan_node.label.clone(),
                "executor": target.plan_node.executor.clone(),
            })),
        );
        let gate_id = gate.id;
        self.repos.lifecycle_gate_repo.create(&gate).await?;

        self.apply_event(
            run,
            target.orchestration_id,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path: target.node_path.clone(),
                attempt: target.attempt,
                executor_run_ref: Some(ExecutorRunRef::HumanDecision {
                    decision_id: gate_id.to_string(),
                }),
                timestamp: chrono::Utc::now(),
            },
        )
        .await?;

        Ok(Some(OpenedHumanGate {
            run_id: target.run_id,
            orchestration_id: target.orchestration_id,
            node_path: target.node_path,
            attempt: target.attempt,
            gate_id,
        }))
    }

    async fn block_ready_node(
        &self,
        run: LifecycleRun,
        target: ReadyNodeTarget,
        code: &str,
        message: &str,
        retryable: bool,
    ) -> Result<(), WorkflowApplicationError> {
        self.apply_event(
            run,
            target.orchestration_id,
            OrchestrationRuntimeEvent::NodeBlocked {
                node_path: target.node_path,
                attempt: target.attempt,
                error: RuntimeNodeError {
                    code: code.to_string(),
                    message: message.to_string(),
                    retryable,
                    detail: Some(json!({ "node_id": target.node_id })),
                },
                timestamp: chrono::Utc::now(),
            },
        )
        .await?;
        Ok(())
    }

    async fn execute_function_like_node(
        &self,
        run: &LifecycleRun,
        target: &ReadyNodeTarget,
    ) -> Result<Vec<NodePortValue>, RuntimeNodeError> {
        let runner = self
            .function_runner
            .as_ref()
            .ok_or_else(|| RuntimeNodeError {
                code: "function_runner_unavailable".to_string(),
                message: "orchestration executor 缺少 FunctionRunner".to_string(),
                retryable: true,
                detail: None,
            })?;
        let context = function_context(run, target);
        let Some(executor) = target.plan_node.executor.as_ref() else {
            return Err(RuntimeNodeError {
                code: "executor_spec_missing".to_string(),
                message: "Function/LocalEffect node 缺少 executor spec".to_string(),
                retryable: false,
                detail: Some(json!({ "node_id": target.node_id })),
            });
        };
        let ExecutorSpec::Function { spec } = executor else {
            if let ExecutorSpec::LocalEffect {
                capability_key,
                input,
            } = executor
            {
                return Err(RuntimeNodeError {
                    code: "local_effect_capability_not_supported".to_string(),
                    message: format!(
                        "LocalEffect capability `{capability_key}` 尚未接入具体 effect executor"
                    ),
                    retryable: false,
                    detail: Some(json!({
                        "contract": "orchestration_node_coordinate.v1",
                        "orchestration_id": target.orchestration_id,
                        "node_path": target.node_path.clone(),
                        "attempt": target.attempt,
                        "capability_key": capability_key,
                        "input": input,
                    })),
                });
            }
            return Err(RuntimeNodeError {
                code: "executor_spec_mismatch".to_string(),
                message: "Function/LocalEffect node 的 executor spec 类型不匹配".to_string(),
                retryable: false,
                detail: Some(json!({ "node_id": target.node_id })),
            });
        };
        match spec {
            FunctionActivityExecutorSpec::ApiRequest(spec) => {
                let outcome = runner
                    .run_api_request(spec, &context)
                    .await
                    .map_err(|error| RuntimeNodeError {
                        code: "api_request_failed".to_string(),
                        message: error,
                        retryable: true,
                        detail: None,
                    })?;
                if !(200..300).contains(&outcome.status) {
                    return Err(RuntimeNodeError {
                        code: "api_request_status_failed".to_string(),
                        message: format!("API request 返回非成功状态: {}", outcome.status),
                        retryable: false,
                        detail: Some(json!({
                            "status": outcome.status,
                            "body_text": outcome.body_text,
                            "body_json": outcome.body_json,
                        })),
                    });
                }
                Ok(api_request_outputs(&target.plan_node, outcome))
            }
            FunctionActivityExecutorSpec::BashExec(spec) => {
                let outcome =
                    runner
                        .run_bash(spec, &context)
                        .await
                        .map_err(|error| RuntimeNodeError {
                            code: "bash_exec_failed".to_string(),
                            message: error,
                            retryable: true,
                            detail: None,
                        })?;
                if outcome.success {
                    Ok(bash_exec_outputs(&target.plan_node, outcome))
                } else {
                    Err(RuntimeNodeError {
                        code: "bash_exec_nonzero".to_string(),
                        message: format!(
                            "bash exec failed: exit_code={:?}, stderr={}",
                            outcome.exit_code, outcome.stderr
                        ),
                        retryable: false,
                        detail: Some(json!({
                            "exit_code": outcome.exit_code,
                            "stdout": outcome.stdout,
                            "stderr": outcome.stderr,
                        })),
                    })
                }
            }
        }
    }

    async fn create_frame(
        &self,
        agent: &LifecycleAgent,
        target: &ReadyNodeTarget,
        runtime_session_ref: Option<String>,
        procedure_id: Option<Uuid>,
    ) -> Result<AgentFrame, WorkflowApplicationError> {
        let mut builder = AgentFrameBuilder::new(agent.id).with_created_by(
            "orchestration_executor",
            Some(format!(
                "{}:{}#{}",
                target.orchestration_id, target.node_path, target.attempt
            )),
        );
        if let Some(procedure_id) = procedure_id {
            builder = builder.with_procedure(AgentProcedureRef::ById(procedure_id));
        }
        if let Some(session_id) = runtime_session_ref {
            builder = builder.with_runtime_session(session_id);
        }
        Ok(builder.build(self.repos.agent_frame_repo.as_ref()).await?)
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

#[derive(Debug, Clone)]
struct ReadyNodeTarget {
    run_id: Uuid,
    orchestration_id: Uuid,
    node_id: String,
    node_path: String,
    attempt: u32,
    kind: PlanNodeKind,
    plan_node: PlanNode,
    runtime_node: RuntimeNodeState,
    state_snapshot: Value,
}

impl ReadyNodeTarget {
    fn next(run: &LifecycleRun) -> Option<Self> {
        for orchestration in &run.orchestrations {
            if matches!(
                orchestration.status,
                OrchestrationStatus::Completed
                    | OrchestrationStatus::Failed
                    | OrchestrationStatus::Cancelled
            ) {
                continue;
            }
            for ready_node_id in &orchestration.dispatch.ready_node_ids {
                if let Some(target) = Self::from_node_id(run, orchestration, ready_node_id) {
                    return Some(target);
                }
            }
        }
        None
    }

    fn from_node_id(
        run: &LifecycleRun,
        orchestration: &OrchestrationInstance,
        node_id: &str,
    ) -> Option<Self> {
        let runtime_node = find_runtime_node_by_id(&orchestration.node_tree, node_id)?;
        if runtime_node.status != RuntimeNodeStatus::Ready {
            return None;
        }
        let plan_node = orchestration
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == runtime_node.node_id)?
            .clone();
        Some(Self {
            run_id: run.id,
            orchestration_id: orchestration.orchestration_id,
            node_id: runtime_node.node_id.clone(),
            node_path: runtime_node.node_path.clone(),
            attempt: runtime_node.attempt,
            kind: plan_node.kind,
            plan_node,
            runtime_node: runtime_node.clone(),
            state_snapshot: serde_json::to_value(&orchestration.state_snapshot)
                .unwrap_or(Value::Null),
        })
    }

    fn from_running_node(
        run: &LifecycleRun,
        orchestration_id: Uuid,
        node_path: &str,
        attempt: u32,
    ) -> Result<Self, WorkflowApplicationError> {
        let orchestration = run
            .orchestrations
            .iter()
            .find(|item| item.orchestration_id == orchestration_id)
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "orchestration 不存在: {orchestration_id}"
                ))
            })?;
        let runtime_node = find_runtime_node(&orchestration.node_tree, node_path, attempt)
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "runtime node 不存在: {node_path}#{attempt}"
                ))
            })?;
        if runtime_node.status != RuntimeNodeStatus::Running {
            return Err(WorkflowApplicationError::Conflict(format!(
                "runtime node {} 当前不是 Running",
                runtime_node.node_path
            )));
        }
        let plan_node = orchestration
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == runtime_node.node_id)
            .ok_or_else(|| {
                WorkflowApplicationError::Internal(format!(
                    "plan node 不存在: {}",
                    runtime_node.node_id
                ))
            })?
            .clone();
        Ok(Self {
            run_id: run.id,
            orchestration_id,
            node_id: runtime_node.node_id.clone(),
            node_path: runtime_node.node_path.clone(),
            attempt: runtime_node.attempt,
            kind: plan_node.kind,
            plan_node,
            runtime_node: runtime_node.clone(),
            state_snapshot: serde_json::to_value(&orchestration.state_snapshot)
                .unwrap_or(Value::Null),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionLaunchTerminal {
    Completed,
    Failed,
}

fn function_context(run: &LifecycleRun, target: &ReadyNodeTarget) -> Value {
    let inputs = target
        .runtime_node
        .inputs
        .iter()
        .map(|input| (input.port_key.clone(), input.value.clone()))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "run_id": run.id,
        "project_id": run.project_id,
        "orchestration_id": target.orchestration_id,
        "node": {
            "id": target.node_id.clone(),
            "path": target.node_path.clone(),
            "attempt": target.attempt,
            "inputs": inputs,
        },
        "state": target.state_snapshot.clone(),
    })
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

fn api_request_outputs(plan_node: &PlanNode, outcome: ApiRequestOutcome) -> Vec<NodePortValue> {
    let raw = json!({
        "status": outcome.status,
        "body_text": outcome.body_text,
        "body_json": outcome.body_json,
    });
    map_declared_outputs(plan_node, raw)
}

fn bash_exec_outputs(plan_node: &PlanNode, outcome: BashExecOutcome) -> Vec<NodePortValue> {
    let raw = json!({
        "success": outcome.success,
        "exit_code": outcome.exit_code,
        "stdout": outcome.stdout,
        "stderr": outcome.stderr,
    });
    map_declared_outputs(plan_node, raw)
}

fn map_declared_outputs(plan_node: &PlanNode, raw: Value) -> Vec<NodePortValue> {
    if plan_node.output_ports.is_empty() {
        return Vec::new();
    }
    if plan_node.output_ports.len() == 1 {
        return vec![NodePortValue {
            port_key: plan_node.output_ports[0].key.clone(),
            value: raw,
        }];
    }
    plan_node
        .output_ports
        .iter()
        .map(|port| NodePortValue {
            port_key: port.key.clone(),
            value: raw.get(&port.key).cloned().unwrap_or(Value::Null),
        })
        .collect()
}

fn human_decision_outputs(plan_node: &PlanNode, decision: Value) -> Vec<NodePortValue> {
    let decision_port = match &plan_node.completion_policy {
        Some(agentdash_domain::workflow::ActivityCompletionPolicy::HumanDecision {
            decision_port,
        }) => decision_port.clone(),
        _ => plan_node
            .output_ports
            .iter()
            .find(|port| port.key == "decision")
            .or_else(|| plan_node.output_ports.first())
            .map(|port| port.key.clone())
            .unwrap_or_else(|| "decision".to_string()),
    };
    vec![NodePortValue {
        port_key: decision_port,
        value: decision,
    }]
}

fn human_gate_id_from_node(node: &RuntimeNodeState) -> Result<Uuid, WorkflowApplicationError> {
    match &node.executor_run_ref {
        Some(ExecutorRunRef::HumanDecision { decision_id }) => Uuid::parse_str(decision_id)
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!("decision_id 非 UUID: {error}"))
            }),
        _ => Err(WorkflowApplicationError::Conflict(format!(
            "runtime node {} 没有关联 human decision gate",
            node.node_path
        ))),
    }
}

fn human_gate_correlation_id(orchestration_id: Uuid, node_path: &str, attempt: u32) -> String {
    format!("orchestration:{orchestration_id}:node:{node_path}:attempt:{attempt}")
}

fn find_runtime_node_by_id<'a>(
    nodes: &'a [RuntimeNodeState],
    node_id: &str,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_id == node_id {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_by_id(&node.children, node_id) {
            return Some(found);
        }
    }
    None
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

#[cfg(test)]
mod launcher_drain_tests {
    use std::sync::{Arc, Mutex};

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivationRule, ActivityCompletionPolicy, ActivityIterationPolicy, AgentFrameRepository,
        AgentProcedure, AgentProcedureContract, AgentProcedureRepository, ApiRequestExecutorSpec,
        BashExecExecutorSpec, DefinitionSource, GateStrategy, HumanActivityExecutorSpec,
        HumanApprovalExecutorSpec, LifecycleAgentRepository, LifecycleGateRepository,
        LifecycleRunRepository, OrchestrationLimits, OrchestrationSourceRef, OutputPortDefinition,
        RuntimeTraceRef,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;

    use crate::workflow::RuntimeSessionCreator;
    use crate::workflow::orchestration::runtime::activate_root_orchestration;

    use super::*;

    #[derive(Default)]
    struct InMemoryRunRepo {
        items: Mutex<Vec<LifecycleRun>>,
    }

    impl InMemoryRunRepo {
        fn insert(&self, run: LifecycleRun) {
            self.items.lock().unwrap().push(run);
        }
    }

    #[async_trait]
    impl LifecycleRunRepository for InMemoryRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.insert(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_root_graph(
            &self,
            root_graph_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.root_graph_id == Some(root_graph_id))
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
            } else {
                items.push(run.clone());
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryProcedureRepo {
        items: Mutex<Vec<AgentProcedure>>,
    }

    impl InMemoryProcedureRepo {
        fn insert(&self, procedure: AgentProcedure) {
            self.items.lock().unwrap().push(procedure);
        }
    }

    #[async_trait]
    impl AgentProcedureRepository for InMemoryProcedureRepo {
        async fn create(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
            self.insert(procedure.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|procedure| procedure.id == id)
                .cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|procedure| procedure.key == key)
                .cloned())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|procedure| procedure.project_id == project_id && procedure.key == key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self.items.lock().unwrap().clone())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|procedure| procedure.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items
                .iter_mut()
                .find(|existing| existing.id == procedure.id)
            {
                *existing = procedure.clone();
            }
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct EmptyAgentRepo;

    #[async_trait]
    impl LifecycleAgentRepository for EmptyAgentRepo {
        async fn create(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, _id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(None)
        }

        async fn list_by_run(&self, _run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(Vec::new())
        }

        async fn update(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct EmptyFrameRepo;

    #[async_trait]
    impl AgentFrameRepository for EmptyFrameRepo {
        async fn create(&self, _frame: &AgentFrame) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, _frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(None)
        }

        async fn get_current(&self, _agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(None)
        }

        async fn list_by_agent(&self, _agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(Vec::new())
        }

        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryGateRepo {
        items: Mutex<Vec<LifecycleGate>>,
    }

    #[async_trait]
    impl LifecycleGateRepository for InMemoryGateRepo {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(gate.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|gate| gate.id == id)
                .cloned())
        }

        async fn list_open_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
                .cloned()
                .collect())
        }

        async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|existing| existing.id == gate.id) {
                *existing = gate.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct EmptyAnchorRepo;

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for EmptyAnchorRepo {
        async fn upsert(&self, _anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_session(&self, _runtime_session_id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn find_by_session(
            &self,
            _runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(None)
        }

        async fn list_by_run(
            &self,
            _run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_by_agent(
            &self,
            _agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_by_project_session_ids(
            &self,
            _runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(Vec::new())
        }

        async fn latest_for_agent(
            &self,
            _agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct EmptyRuntimeSessionCreator;

    #[async_trait]
    impl RuntimeSessionCreator for EmptyRuntimeSessionCreator {
        async fn create_runtime_session(
            &self,
            _request: RuntimeSessionCreationRequest,
        ) -> Result<Uuid, WorkflowApplicationError> {
            Ok(Uuid::new_v4())
        }
    }

    struct TestFunctionRunner {
        api_outcome: ApiRequestOutcome,
        contexts: Mutex<Vec<Value>>,
    }

    #[async_trait]
    impl FunctionRunner for TestFunctionRunner {
        async fn run_api_request(
            &self,
            _spec: &ApiRequestExecutorSpec,
            context: &Value,
        ) -> Result<ApiRequestOutcome, String> {
            self.contexts.lock().unwrap().push(context.clone());
            Ok(self.api_outcome.clone())
        }

        async fn run_bash(
            &self,
            _spec: &BashExecExecutorSpec,
            context: &Value,
        ) -> Result<BashExecOutcome, String> {
            self.contexts.lock().unwrap().push(context.clone());
            Ok(BashExecOutcome {
                exit_code: Some(0),
                stdout: "ok".to_string(),
                stderr: String::new(),
                success: true,
            })
        }
    }

    fn launcher(
        run_repo: Arc<InMemoryRunRepo>,
        gate_repo: Arc<InMemoryGateRepo>,
    ) -> OrchestrationExecutorLauncher {
        launcher_with_procedure_repo(
            run_repo,
            gate_repo,
            Arc::new(InMemoryProcedureRepo::default()),
        )
    }

    fn launcher_with_procedure_repo(
        run_repo: Arc<InMemoryRunRepo>,
        gate_repo: Arc<InMemoryGateRepo>,
        procedure_repo: Arc<InMemoryProcedureRepo>,
    ) -> OrchestrationExecutorLauncher {
        OrchestrationExecutorLauncher::from_repositories(OrchestrationExecutorRepositories {
            lifecycle_run_repo: run_repo,
            agent_procedure_repo: procedure_repo,
            lifecycle_agent_repo: Arc::new(EmptyAgentRepo),
            agent_frame_repo: Arc::new(EmptyFrameRepo),
            lifecycle_gate_repo: gate_repo,
            execution_anchor_repo: Arc::new(EmptyAnchorRepo),
            runtime_session_creator: Arc::new(EmptyRuntimeSessionCreator),
        })
    }

    fn workflow_source(graph_id: Uuid) -> OrchestrationSourceRef {
        OrchestrationSourceRef::WorkflowGraph {
            graph_id,
            graph_version: Some(1),
        }
    }

    fn output_port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: String::new(),
            gate_strategy: GateStrategy::Existence,
            gate_params: None,
        }
    }

    fn plan_node(node_id: &str, kind: PlanNodeKind, executor: Option<ExecutorSpec>) -> PlanNode {
        PlanNode {
            node_id: node_id.to_string(),
            node_path: node_id.to_string(),
            parent_node_id: None,
            kind,
            label: Some(node_id.to_string()),
            executor,
            input_ports: Vec::new(),
            output_ports: vec![output_port("result")],
            completion_policy: None,
            iteration_policy: None,
            join_policy: None,
            result_contract: None,
            metadata: None,
        }
    }

    fn run_with_node(plan_node: PlanNode) -> LifecycleRun {
        let graph_id = Uuid::new_v4();
        let source_ref = workflow_source(graph_id);
        let plan_snapshot = agentdash_domain::workflow::OrchestrationPlanSnapshot {
            plan_digest: format!("sha256:{}", plan_node.node_id),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![plan_node.clone()],
            entry_node_ids: vec![plan_node.node_id.clone()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: plan_node.node_id.clone(),
            }],
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        let orchestration = activate_root_orchestration(source_ref, plan_snapshot);
        let mut run = LifecycleRun::new_control(Uuid::new_v4(), graph_id);
        assert!(run.add_orchestration(orchestration));
        run
    }

    fn latest_run(repo: &InMemoryRunRepo, run_id: Uuid) -> LifecycleRun {
        repo.items
            .lock()
            .unwrap()
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .expect("run persisted")
    }

    fn runtime_node<'a>(run: &'a LifecycleRun, node_id: &str) -> &'a RuntimeNodeState {
        run.orchestrations[0]
            .node_tree
            .iter()
            .find(|node| node.node_id == node_id)
            .expect("runtime node")
    }

    fn api_executor() -> ExecutorSpec {
        ExecutorSpec::Function {
            spec: FunctionActivityExecutorSpec::ApiRequest(ApiRequestExecutorSpec {
                method: "POST".to_string(),
                url_template: "https://example.test/workflow".to_string(),
                body_template: None,
            }),
        }
    }

    fn function_runner() -> Arc<TestFunctionRunner> {
        Arc::new(TestFunctionRunner {
            api_outcome: ApiRequestOutcome {
                status: 201,
                body_text: "{\"ok\":true}".to_string(),
                body_json: Some(json!({"ok": true})),
            },
            contexts: Mutex::new(Vec::new()),
        })
    }

    fn agent_executor(procedure_key: &str) -> ExecutorSpec {
        ExecutorSpec::AgentProcedure {
            procedure_key: procedure_key.to_string(),
            agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
            runtime_session_policy: RuntimeSessionPolicy::CreateNew,
        }
    }

    fn procedure(project_id: Uuid, key: &str) -> AgentProcedure {
        AgentProcedure::new(
            project_id,
            key,
            "Agent Review",
            "",
            DefinitionSource::UserAuthored,
            AgentProcedureContract::default(),
        )
        .expect("agent procedure")
    }

    #[tokio::test]
    async fn launcher_runs_function_node_through_started_then_completed_state() {
        let mut node = plan_node("function", PlanNodeKind::Function, Some(api_executor()));
        node.completion_policy = Some(ActivityCompletionPolicy::OutputPorts {
            required_ports: vec!["result".to_string()],
        });
        let run = run_with_node(node);
        let run_id = run.id;
        let run_repo = Arc::new(InMemoryRunRepo::default());
        run_repo.insert(run);
        let gate_repo = Arc::new(InMemoryGateRepo::default());
        let runner = function_runner();
        let launcher = launcher(run_repo.clone(), gate_repo).with_function_runner(runner.clone());

        let result = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("drain ready nodes");

        assert_eq!(result.completed_effect_nodes, vec!["function"]);
        let latest = latest_run(&run_repo, run_id);
        let orchestration = &latest.orchestrations[0];
        assert!(orchestration.dispatch.ready_node_ids.is_empty());
        let node = runtime_node(&latest, "function");
        assert_eq!(node.status, RuntimeNodeStatus::Completed);
        assert!(node.started_at.is_some());
        assert!(node.completed_at.is_some());
        assert!(matches!(
            node.executor_run_ref,
            Some(ExecutorRunRef::FunctionRun { .. })
        ));
        assert!(matches!(
            node.trace_refs.as_slice(),
            [RuntimeTraceRef::FunctionRun { .. }]
        ));
        assert_eq!(node.outputs[0].port_key, "result");
        assert_eq!(node.outputs[0].value["body_json"], json!({"ok": true}));
        assert_eq!(
            runner.contexts.lock().unwrap()[0]["node"]["path"],
            json!("function")
        );
    }

    #[tokio::test]
    async fn launcher_records_local_effect_as_started_then_failed_with_node_coordinate() {
        let node = plan_node(
            "effect",
            PlanNodeKind::LocalEffect,
            Some(ExecutorSpec::LocalEffect {
                capability_key: "workspace.write".to_string(),
                input: Some(json!({"path": "result.txt"})),
            }),
        );
        let run = run_with_node(node);
        let run_id = run.id;
        let orchestration_id = run.orchestrations[0].orchestration_id;
        let run_repo = Arc::new(InMemoryRunRepo::default());
        run_repo.insert(run);
        let launcher = launcher(run_repo.clone(), Arc::new(InMemoryGateRepo::default()))
            .with_function_runner(function_runner());

        let result = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("drain ready nodes");

        assert_eq!(result.failed_nodes, vec!["effect"]);
        let latest = latest_run(&run_repo, run_id);
        let node = runtime_node(&latest, "effect");
        assert_eq!(node.status, RuntimeNodeStatus::Failed);
        assert!(node.started_at.is_some());
        assert!(node.completed_at.is_some());
        assert!(matches!(
            node.executor_run_ref,
            Some(ExecutorRunRef::FunctionRun { .. })
        ));
        let error = node.error.as_ref().expect("local effect error");
        assert_eq!(error.code, "local_effect_capability_not_supported");
        assert_eq!(
            error.detail.as_ref().expect("detail")["contract"],
            json!("orchestration_node_coordinate.v1")
        );
        assert_eq!(
            error.detail.as_ref().expect("detail")["orchestration_id"],
            json!(orchestration_id)
        );
        assert_eq!(
            error.detail.as_ref().expect("detail")["node_path"],
            json!("effect")
        );
    }

    #[tokio::test]
    async fn launcher_launches_agent_call_and_records_runtime_session_start() {
        let procedure_key = "agent.review";
        let node = plan_node(
            "agent",
            PlanNodeKind::AgentCall,
            Some(agent_executor(procedure_key)),
        );
        let run = run_with_node(node);
        let run_id = run.id;
        let project_id = run.project_id;
        let run_repo = Arc::new(InMemoryRunRepo::default());
        run_repo.insert(run);
        let procedure_repo = Arc::new(InMemoryProcedureRepo::default());
        procedure_repo.insert(procedure(project_id, procedure_key));
        let launcher = launcher_with_procedure_repo(
            run_repo.clone(),
            Arc::new(InMemoryGateRepo::default()),
            procedure_repo,
        );

        let result = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("drain ready nodes");

        assert_eq!(result.launched_agent_nodes.len(), 1);
        assert_eq!(result.launched_agent_nodes[0].node_path, "agent");
        let latest = latest_run(&run_repo, run_id);
        let node = runtime_node(&latest, "agent");
        assert_eq!(node.status, RuntimeNodeStatus::Running);
        assert!(node.started_at.is_some());
        assert_eq!(
            node.executor_run_ref,
            Some(ExecutorRunRef::RuntimeSession {
                session_id: result.launched_agent_nodes[0].runtime_session_id.clone(),
            })
        );
        assert_eq!(
            node.trace_refs,
            vec![RuntimeTraceRef::RuntimeSession {
                session_id: result.launched_agent_nodes[0].runtime_session_id.clone(),
            }]
        );
        assert!(latest.orchestrations[0].dispatch.ready_node_ids.is_empty());
    }

    #[tokio::test]
    async fn launcher_opens_human_gate_with_orchestration_node_contract() {
        let mut node = plan_node(
            "review",
            PlanNodeKind::HumanGate,
            Some(ExecutorSpec::Human {
                spec: HumanActivityExecutorSpec::Approval(HumanApprovalExecutorSpec {
                    form_schema_key: "approval.review".to_string(),
                    title: Some("Review".to_string()),
                }),
            }),
        );
        node.output_ports = vec![output_port("decision")];
        node.completion_policy = Some(ActivityCompletionPolicy::HumanDecision {
            decision_port: "decision".to_string(),
        });
        let run = run_with_node(node);
        let run_id = run.id;
        let orchestration_id = run.orchestrations[0].orchestration_id;
        let run_repo = Arc::new(InMemoryRunRepo::default());
        run_repo.insert(run);
        let gate_repo = Arc::new(InMemoryGateRepo::default());
        let launcher = launcher(run_repo.clone(), gate_repo.clone());

        let result = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("drain ready nodes");

        assert_eq!(result.opened_human_gates.len(), 1);
        let latest = latest_run(&run_repo, run_id);
        let node = runtime_node(&latest, "review");
        assert_eq!(node.status, RuntimeNodeStatus::Running);
        assert!(node.started_at.is_some());
        assert!(matches!(
            node.executor_run_ref,
            Some(ExecutorRunRef::HumanDecision { .. })
        ));
        assert!(matches!(
            node.trace_refs.as_slice(),
            [RuntimeTraceRef::HumanDecision { .. }]
        ));
        assert!(latest.orchestrations[0].dispatch.ready_node_ids.is_empty());

        let gates = gate_repo.items.lock().unwrap();
        assert_eq!(gates.len(), 1);
        let payload = gates[0].payload_json.as_ref().expect("gate payload");
        assert_eq!(payload["contract"], json!("orchestration_human_gate.v1"));
        assert_eq!(payload["orchestration_id"], json!(orchestration_id));
        assert_eq!(payload["node_path"], json!("review"));
        assert_eq!(payload["attempt"], json!(1));
        assert_eq!(gates[0].gate_kind, "orchestration_human_gate");
    }

    #[tokio::test]
    async fn launcher_blocks_unsupported_attempt_policy_before_executor_side_effects() {
        let mut node = plan_node("retrying", PlanNodeKind::Function, Some(api_executor()));
        node.iteration_policy = Some(ActivityIterationPolicy {
            max_attempts: Some(2),
            artifact_alias: ArtifactAliasPolicy::Latest,
        });
        let run = run_with_node(node);
        let run_id = run.id;
        let run_repo = Arc::new(InMemoryRunRepo::default());
        run_repo.insert(run);
        let launcher = launcher(run_repo.clone(), Arc::new(InMemoryGateRepo::default()));

        let result = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("drain ready nodes");

        assert_eq!(result.failed_nodes, vec!["retrying"]);
        let latest = latest_run(&run_repo, run_id);
        assert_eq!(
            latest.status,
            agentdash_domain::workflow::LifecycleRunStatus::Blocked
        );
        let orchestration = &latest.orchestrations[0];
        assert_eq!(orchestration.status, OrchestrationStatus::Paused);
        assert!(orchestration.dispatch.ready_node_ids.is_empty());
        let node = runtime_node(&latest, "retrying");
        assert_eq!(node.status, RuntimeNodeStatus::Blocked);
        assert!(node.started_at.is_none());
        assert_eq!(
            node.error.as_ref().map(|error| error.code.as_str()),
            Some("attempt_policy_not_supported")
        );
    }
}
