use std::sync::Arc;

use agentdash_application_ports::lifecycle_materialization::WorkflowAgentNodeMaterializationPort;
use agentdash_domain::workflow::{
    AgentProcedureRepository, ArtifactAliasPolicy, ExecutorRunRef, LifecycleGateRepository,
    LifecycleRun, LifecycleRunRepository, PlanNode, PlanNodeKind, RuntimeNodeError,
};
use agentdash_spi::FunctionRunner;
use serde_json::Value;
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
}

impl From<WorkflowRepositorySet> for OrchestrationExecutorRepositories {
    fn from(repos: WorkflowRepositorySet) -> Self {
        Self {
            lifecycle_run_repo: repos.lifecycle_run_repo,
            agent_procedure_repo: repos.agent_procedure_repo,
            lifecycle_gate_repo: repos.lifecycle_gate_repo,
            workflow_agent_node_materialization: repos.workflow_agent_node_materialization,
        }
    }
}

impl OrchestrationExecutorLauncher {
    pub fn new(repos: WorkflowRepositorySet) -> Self {
        Self::from_executor_repositories(repos.into())
    }

    #[cfg(test)]
    fn from_repositories(repos: OrchestrationExecutorRepositories) -> Self {
        Self::from_executor_repositories(repos)
    }

    fn from_executor_repositories(repos: OrchestrationExecutorRepositories) -> Self {
        let agent_node_launcher = AgentNodeLauncher::new(
            repos.agent_procedure_repo.clone(),
            repos.workflow_agent_node_materialization.clone(),
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
                PlanNodeKind::AgentCall => {
                    if let Some(launched) = self.launch_agent_node(run, coordinate).await? {
                        result.launched_agent_nodes.push(launched);
                    }
                }
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
mod launcher_drain_tests {
    use std::sync::{Arc, Mutex};

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivationRule, ActivityCompletionPolicy, ActivityIterationPolicy, AgentFrame,
        AgentFrameRepository, AgentProcedure, AgentProcedureContract, AgentProcedureExecutionSpec,
        AgentProcedureRepository, AgentReusePolicy, AgentRuntimeRefs, ApiRequestExecutorSpec,
        BashExecExecutorSpec, DefinitionSource, ExecutorSpec, FunctionActivityExecutorSpec,
        GateStrategy, HumanActivityExecutorSpec, HumanApprovalExecutorSpec, LifecycleGate,
        LifecycleGateRepository, LifecycleRunRepository, OrchestrationLimits,
        OrchestrationSourceRef, OrchestrationStatus, OutputPortDefinition, RuntimeNodeState,
        RuntimeNodeStatus, RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
        RuntimeSessionPolicy, RuntimeTraceRef, WaitObligationDeclaration, WaitProducerRef,
        WorkflowInjectionSpec,
    };
    use agentdash_spi::{ApiRequestOutcome, BashExecOutcome};
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;

    use crate::orchestration::runtime::activate_root_orchestration;

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

    struct RejectingProcedureRepo;

    #[async_trait]
    impl AgentProcedureRepository for RejectingProcedureRepo {
        async fn create(&self, _procedure: &AgentProcedure) -> Result<(), DomainError> {
            Err(repo_lookup_error())
        }

        async fn get_by_id(&self, _id: Uuid) -> Result<Option<AgentProcedure>, DomainError> {
            Err(repo_lookup_error())
        }

        async fn get_by_key(&self, _key: &str) -> Result<Option<AgentProcedure>, DomainError> {
            Err(repo_lookup_error())
        }

        async fn get_by_project_and_key(
            &self,
            _project_id: Uuid,
            _key: &str,
        ) -> Result<Option<AgentProcedure>, DomainError> {
            Err(repo_lookup_error())
        }

        async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
            Err(repo_lookup_error())
        }

        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<AgentProcedure>, DomainError> {
            Err(repo_lookup_error())
        }

        async fn update(&self, _procedure: &AgentProcedure) -> Result<(), DomainError> {
            Err(repo_lookup_error())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Err(repo_lookup_error())
        }
    }

    fn repo_lookup_error() -> DomainError {
        DomainError::InvalidConfig("snapshot procedure must not query repository".to_string())
    }

    #[derive(Default)]
    struct InMemoryFrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }

    impl InMemoryFrameRepo {
        fn latest(&self) -> AgentFrame {
            self.items
                .lock()
                .unwrap()
                .last()
                .cloned()
                .expect("frame persisted")
        }
    }

    #[async_trait]
    impl AgentFrameRepository for InMemoryFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            let items = self.items.lock().unwrap();
            let mut frames: Vec<_> = items
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .collect();
            frames.sort_by_key(|frame| frame.revision);
            Ok(frames.last().cloned().cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CapturedAgentNodeComposition {
        node_path: String,
        attempt: u32,
        runtime_session_id: Option<String>,
        contract_output_ports: Vec<String>,
    }

    struct CapturingLifecycleFrameMaterializer {
        frame_repo: Arc<InMemoryFrameRepo>,
        anchor_repo: Arc<InMemoryAnchorRepo>,
        calls: Mutex<Vec<CapturedAgentNodeComposition>>,
    }

    impl CapturingLifecycleFrameMaterializer {
        fn new(frame_repo: Arc<InMemoryFrameRepo>, anchor_repo: Arc<InMemoryAnchorRepo>) -> Self {
            Self {
                frame_repo,
                anchor_repo,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<CapturedAgentNodeComposition> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl WorkflowAgentNodeMaterializationPort for CapturingLifecycleFrameMaterializer {
        async fn materialize_workflow_agent_node(
            &self,
            input: agentdash_application_ports::lifecycle_materialization::WorkflowAgentNodeMaterializationRequest,
        ) -> Result<
            agentdash_application_ports::lifecycle_materialization::WorkflowAgentNodeMaterializationResult,
            agentdash_application_ports::lifecycle_materialization::LifecycleMaterializationError,
        >{
            let agent_id = Uuid::new_v4();
            let runtime_session_id = Uuid::new_v4();
            self.calls
                .lock()
                .unwrap()
                .push(CapturedAgentNodeComposition {
                    node_path: input.orchestration_binding.node_path.clone(),
                    attempt: input.orchestration_binding.attempt,
                    runtime_session_id: Some(runtime_session_id.to_string()),
                    contract_output_ports: input
                        .workflow_contract
                        .as_ref()
                        .map(|contract| {
                            contract
                                .output_ports
                                .iter()
                                .map(|port| port.key.clone())
                                .collect()
                        })
                        .unwrap_or_default(),
                });
            let mut frame =
                AgentFrame::new_revision(agent_id, 1, "workflow_agent_node_materialization");
            frame.created_by_id = input.frame_created_by_id;
            frame.vfs_surface_json = Some(serde_json::json!({
                "mounts": [{
                    "id": "lifecycle",
                    "provider": "lifecycle_vfs"
                }]
            }));
            self.frame_repo.create(&frame).await.map_err(|error| {
                agentdash_application_ports::lifecycle_materialization::LifecycleMaterializationError::Repository {
                    operation: "create_agent_frame",
                    message: error.to_string(),
                }
            })?;
            let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
                runtime_session_id.to_string(),
                input.run_id,
                frame.id,
                agent_id,
                input.orchestration_binding.orchestration_ref,
                input.orchestration_binding.node_path.clone(),
                input.orchestration_binding.attempt,
            );
            self.anchor_repo.create_once(&anchor).await.map_err(|error| {
                agentdash_application_ports::lifecycle_materialization::LifecycleMaterializationError::Repository {
                    operation: "create_runtime_session_execution_anchor",
                    message: error.to_string(),
                }
            })?;
            Ok(
                agentdash_application_ports::lifecycle_materialization::WorkflowAgentNodeMaterializationResult {
                    runtime_refs: AgentRuntimeRefs::new(
                        input.run_id,
                        agent_id,
                        frame.id,
                        Some(input.orchestration_binding),
                    ),
                    delivery_runtime_ref: runtime_session_id,
                },
            )
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

        async fn list_by_wait_producer(
            &self,
            producer: &WaitProducerRef,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|gate| {
                    gate.payload_json
                        .as_ref()
                        .and_then(WaitObligationDeclaration::from_payload)
                        .is_some_and(|declaration| declaration.wait_source.producer == *producer)
                })
                .cloned()
                .collect())
        }

        async fn find_by_agent_and_correlation(
            &self,
            agent_id: Uuid,
            correlation_id: &str,
        ) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|gate| {
                    gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id
                })
                .cloned())
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
    struct InMemoryAnchorRepo {
        items: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    impl InMemoryAnchorRepo {
        fn find(&self, runtime_session_id: &str) -> RuntimeSessionExecutionAnchor {
            self.items
                .lock()
                .unwrap()
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned()
                .expect("runtime session execution anchor persisted")
        }
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for InMemoryAnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items
                .iter()
                .find(|existing| existing.runtime_session_id == anchor.runtime_session_id)
            {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            items.push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.items
                .lock()
                .unwrap()
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(runtime_session_ids
                .iter()
                .filter_map(|id| {
                    self.items
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|anchor| anchor.runtime_session_id == *id)
                        .cloned()
                })
                .collect())
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
        procedure_repo: Arc<dyn AgentProcedureRepository>,
    ) -> OrchestrationExecutorLauncher {
        launcher_with_procedure_and_frame_repo(run_repo, gate_repo, procedure_repo).0
    }

    fn launcher_with_procedure_and_frame_repo(
        run_repo: Arc<InMemoryRunRepo>,
        gate_repo: Arc<InMemoryGateRepo>,
        procedure_repo: Arc<dyn AgentProcedureRepository>,
    ) -> (
        OrchestrationExecutorLauncher,
        Arc<InMemoryFrameRepo>,
        Arc<InMemoryAnchorRepo>,
    ) {
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let anchor_repo = Arc::new(InMemoryAnchorRepo::default());
        let frame_materializer = Arc::new(CapturingLifecycleFrameMaterializer::new(
            frame_repo.clone(),
            anchor_repo.clone(),
        ));
        launcher_with_procedure_frame_repo_and_materializer(
            run_repo,
            gate_repo,
            procedure_repo,
            frame_repo,
            anchor_repo,
            frame_materializer,
        )
    }

    fn launcher_with_procedure_frame_repo_and_materializer(
        run_repo: Arc<InMemoryRunRepo>,
        gate_repo: Arc<InMemoryGateRepo>,
        procedure_repo: Arc<dyn AgentProcedureRepository>,
        frame_repo: Arc<InMemoryFrameRepo>,
        anchor_repo: Arc<InMemoryAnchorRepo>,
        frame_materializer: Arc<dyn WorkflowAgentNodeMaterializationPort>,
    ) -> (
        OrchestrationExecutorLauncher,
        Arc<InMemoryFrameRepo>,
        Arc<InMemoryAnchorRepo>,
    ) {
        let launcher =
            OrchestrationExecutorLauncher::from_repositories(OrchestrationExecutorRepositories {
                lifecycle_run_repo: run_repo,
                agent_procedure_repo: procedure_repo,
                lifecycle_gate_repo: gate_repo,
                workflow_agent_node_materialization: frame_materializer,
            });
        (launcher, frame_repo, anchor_repo)
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

    fn contract_output_port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: format!("{key} output"),
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
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
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
            procedure: AgentProcedureExecutionSpec::by_key(procedure_key),
            agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
            runtime_session_policy: RuntimeSessionPolicy::CreateNew,
        }
    }

    fn snapshot_agent_executor() -> ExecutorSpec {
        ExecutorSpec::AgentProcedure {
            procedure: AgentProcedureExecutionSpec::Snapshot {
                procedure_key: None,
                name: Some("Inline Review".to_string()),
                contract: Box::new(AgentProcedureContract::default()),
                source_ref: None,
                contract_digest: Some("sha256:inline".to_string()),
            },
            agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
            runtime_session_policy: RuntimeSessionPolicy::CreateNew,
        }
    }

    fn procedure(project_id: Uuid, key: &str) -> AgentProcedure {
        procedure_with_contract(project_id, key, AgentProcedureContract::default())
    }

    fn procedure_with_contract(
        project_id: Uuid,
        key: &str,
        contract: AgentProcedureContract,
    ) -> AgentProcedure {
        AgentProcedure::new(
            project_id,
            key,
            "Agent Review",
            "Agent review procedure used by orchestration launcher tests.",
            DefinitionSource::UserAuthored,
            contract,
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
        let mut node = plan_node(
            "effect",
            PlanNodeKind::LocalEffect,
            Some(ExecutorSpec::LocalEffect {
                capability_key: "workspace.write".to_string(),
                input: Some(json!({"path": "result.txt"})),
            }),
        );
        node.node_path = "effects.workspace_write".to_string();
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

        assert_eq!(result.failed_nodes, vec!["effects.workspace_write"]);
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
            error.detail.as_ref().expect("detail")["run_id"],
            json!(run_id)
        );
        assert_eq!(
            error.detail.as_ref().expect("detail")["orchestration_id"],
            json!(orchestration_id)
        );
        assert_eq!(
            error.detail.as_ref().expect("detail")["node_path"],
            json!("effects.workspace_write")
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
        let (launcher, frame_repo, anchor_repo) = launcher_with_procedure_and_frame_repo(
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
        assert_eq!(node.status, RuntimeNodeStatus::Claiming);
        assert!(node.started_at.is_none());
        assert_eq!(node.executor_run_ref, None);
        assert!(node.trace_refs.is_empty());
        assert!(latest.orchestrations[0].dispatch.ready_node_ids.is_empty());

        let frame = frame_repo.latest();
        let anchor = anchor_repo.find(&result.launched_agent_nodes[0].runtime_session_id);
        assert_eq!(anchor.launch_frame_id, frame.id);
        assert_eq!(anchor.run_id, run_id);
        assert_eq!(
            anchor.orchestration_id,
            Some(latest.orchestrations[0].orchestration_id)
        );
        assert_eq!(anchor.node_path.as_deref(), Some("agent"));
        assert_eq!(anchor.node_attempt, Some(1));

        let mounts = frame
            .vfs_surface_json
            .as_ref()
            .and_then(|surface| surface.get("mounts"))
            .and_then(serde_json::Value::as_array)
            .expect("agent call frame should persist lifecycle VFS mounts");
        assert!(mounts.iter().any(|mount| {
            mount.get("id").and_then(serde_json::Value::as_str) == Some("lifecycle")
                && mount.get("provider").and_then(serde_json::Value::as_str)
                    == Some("lifecycle_vfs")
        }));
    }

    #[tokio::test]
    async fn launcher_materializes_agent_call_contract_through_frame_materializer_and_node_claimed()
    {
        let procedure_key = "agent.contract.review";
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
        let contract = AgentProcedureContract {
            injection: WorkflowInjectionSpec {
                guidance: Some("Use the review contract.".to_string()),
                ..WorkflowInjectionSpec::default()
            },
            output_ports: vec![contract_output_port("contract_result")],
            ..AgentProcedureContract::default()
        };
        procedure_repo.insert(procedure_with_contract(project_id, procedure_key, contract));
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let anchor_repo = Arc::new(InMemoryAnchorRepo::default());
        let materializer = Arc::new(CapturingLifecycleFrameMaterializer::new(
            frame_repo.clone(),
            anchor_repo.clone(),
        ));
        let (launcher, frame_repo, anchor_repo) =
            launcher_with_procedure_frame_repo_and_materializer(
                run_repo.clone(),
                Arc::new(InMemoryGateRepo::default()),
                procedure_repo,
                frame_repo,
                anchor_repo,
                materializer.clone(),
            );

        let result = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("drain ready nodes");

        assert_eq!(result.launched_agent_nodes.len(), 1);
        let launched = &result.launched_agent_nodes[0];
        let calls = materializer.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].node_path, "agent");
        assert_eq!(calls[0].attempt, 1);
        assert_eq!(
            calls[0].runtime_session_id.as_deref(),
            Some(launched.runtime_session_id.as_str())
        );
        assert_eq!(
            calls[0].contract_output_ports,
            vec!["contract_result".to_string()]
        );

        let latest = latest_run(&run_repo, run_id);
        let node = runtime_node(&latest, "agent");
        assert_eq!(node.status, RuntimeNodeStatus::Claiming);
        assert_eq!(node.executor_run_ref, None);
        assert!(node.trace_refs.is_empty());

        let frame = frame_repo.latest();
        let anchor = anchor_repo.find(&launched.runtime_session_id);
        assert_eq!(anchor.launch_frame_id, frame.id);
        assert_eq!(anchor.run_id, run_id);
        assert_eq!(
            anchor.orchestration_id,
            Some(latest.orchestrations[0].orchestration_id)
        );
        assert_eq!(anchor.node_path.as_deref(), Some("agent"));
        assert_eq!(anchor.node_attempt, Some(1));
    }

    #[tokio::test]
    async fn launcher_launches_snapshot_agent_without_procedure_repository_lookup() {
        let node = plan_node(
            "agent",
            PlanNodeKind::AgentCall,
            Some(snapshot_agent_executor()),
        );
        let run = run_with_node(node);
        let run_id = run.id;
        let run_repo = Arc::new(InMemoryRunRepo::default());
        run_repo.insert(run);
        let launcher = launcher_with_procedure_repo(
            run_repo.clone(),
            Arc::new(InMemoryGateRepo::default()),
            Arc::new(RejectingProcedureRepo),
        );

        let result = launcher
            .drain_ready_nodes(run_id)
            .await
            .expect("snapshot agent should launch without repository lookup");

        assert_eq!(result.launched_agent_nodes.len(), 1);
        let latest = latest_run(&run_repo, run_id);
        let node = runtime_node(&latest, "agent");
        assert_eq!(node.status, RuntimeNodeStatus::Claiming);
        assert_eq!(node.executor_run_ref, None);
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
