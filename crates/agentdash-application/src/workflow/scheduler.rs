use agentdash_domain::workflow::{
    ActivityAttemptStatus, ActivityExecutionClaim, ActivityExecutionClaimRepository,
    ActivityExecutionClaimStatus, WorkflowGraph, AgentAssignment,
    AgentAssignmentRepository, ExecutorRunRef,
};
use uuid::Uuid;

use super::{
    ActivityEvent, ActivityLifecycleRunState, LifecycleEngine, LifecycleEngineError,
    WorkflowApplicationError,
};

pub struct ActivityExecutorScheduler<'a, R: ?Sized, A: ?Sized = dyn AgentAssignmentRepository> {
    claim_repo: &'a R,
    assignment_repo: Option<&'a A>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityExecutorStartError {
    pub message: String,
    pub retryable: bool,
}

impl ActivityExecutorStartError {
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }

    pub fn terminal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActivityExecutorLaunchOutcome {
    pub claim: ActivityExecutionClaim,
    pub started: bool,
    pub error: Option<String>,
    pub assignment: Option<AgentAssignment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityExecutorStartResult {
    pub executor_run: ExecutorRunRef,
    pub immediate_events: Vec<ActivityEvent>,
}

impl ActivityExecutorStartResult {
    pub fn started(executor_run: ExecutorRunRef) -> Self {
        Self {
            executor_run,
            immediate_events: Vec::new(),
        }
    }

    pub fn with_events(executor_run: ExecutorRunRef, immediate_events: Vec<ActivityEvent>) -> Self {
        Self {
            executor_run,
            immediate_events,
        }
    }
}

#[async_trait::async_trait]
pub trait ActivityExecutorLauncher: Send + Sync {
    async fn start(
        &self,
        definition: &WorkflowGraph,
        state: &ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError>;
}

impl<'a, R: ?Sized, A: ?Sized> ActivityExecutorScheduler<'a, R, A>
where
    R: ActivityExecutionClaimRepository,
    A: AgentAssignmentRepository,
{
    pub fn new(claim_repo: &'a R) -> Self {
        Self {
            claim_repo,
            assignment_repo: None,
        }
    }

    pub fn with_assignment_repo(mut self, assignment_repo: &'a A) -> Self {
        self.assignment_repo = Some(assignment_repo);
        self
    }

    pub async fn claim_ready_attempts(
        &self,
        run_id: Uuid,
        definition: &WorkflowGraph,
        state: &mut ActivityLifecycleRunState,
    ) -> Result<Vec<ActivityExecutionClaim>, WorkflowApplicationError> {
        let ready_attempts = state
            .attempts
            .iter()
            .filter(|attempt| attempt.status == ActivityAttemptStatus::Ready)
            .map(|attempt| (attempt.activity_key.clone(), attempt.attempt))
            .collect::<Vec<_>>();

        let mut claims = Vec::new();
        for (activity_key, attempt) in ready_attempts {
            let activity = definition
                .activities
                .iter()
                .find(|activity| activity.key == activity_key)
                .ok_or_else(|| {
                    WorkflowApplicationError::BadRequest(format!("activity 不存在: {activity_key}"))
                })?;
            let requested_claim = ActivityExecutionClaim::new(
                run_id,
                state.graph_instance_id,
                activity_key.clone(),
                attempt,
                activity.executor.kind(),
            );
            let claim = self.claim_repo.create_or_get(&requested_claim).await?;
            if claim.status == ActivityExecutionClaimStatus::Claiming {
                LifecycleEngine::apply_event(
                    definition,
                    state,
                    ActivityEvent::SchedulerClaimAccepted {
                        activity_key,
                        attempt,
                    },
                )
                .map_err(map_engine_error)?;
            }
            if claim.status.is_active() {
                claims.push(claim);
            }
        }
        Ok(claims)
    }

    pub async fn launch_ready_attempts<L>(
        &self,
        run_id: Uuid,
        definition: &WorkflowGraph,
        state: &mut ActivityLifecycleRunState,
        launcher: &L,
    ) -> Result<Vec<ActivityExecutorLaunchOutcome>, WorkflowApplicationError>
    where
        L: ActivityExecutorLauncher,
    {
        let claims = self.claim_ready_attempts(run_id, definition, state).await?;
        let mut outcomes = Vec::new();
        for claim in claims {
            if claim.status != ActivityExecutionClaimStatus::Claiming {
                outcomes.push(ActivityExecutorLaunchOutcome {
                    claim,
                    started: false,
                    error: None,
                    assignment: None,
                });
                continue;
            }
            match launcher.start(definition, state, &claim).await {
                Ok(start_result) => {
                    let updated = self
                        .record_executor_started(
                            definition,
                            state,
                            &claim,
                            start_result.executor_run,
                        )
                        .await?;
                    let assignment = self
                        .maybe_create_assignment(run_id, &updated)
                        .await?;
                    for event in start_result.immediate_events {
                        LifecycleEngine::apply_event(definition, state, event)
                            .map_err(map_engine_error)?;
                    }
                    outcomes.push(ActivityExecutorLaunchOutcome {
                        claim: updated,
                        started: true,
                        error: None,
                        assignment,
                    });
                }
                Err(error) => {
                    let updated = self
                        .record_executor_start_failed(
                            definition,
                            state,
                            &claim,
                            error.message.clone(),
                            error.retryable,
                        )
                        .await?;
                    outcomes.push(ActivityExecutorLaunchOutcome {
                        claim: updated,
                        started: false,
                        error: Some(error.message),
                        assignment: None,
                    });
                }
            }
        }
        Ok(outcomes)
    }

    pub async fn record_executor_started(
        &self,
        definition: &WorkflowGraph,
        state: &mut ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
        executor_run_ref: ExecutorRunRef,
    ) -> Result<ActivityExecutionClaim, WorkflowApplicationError> {
        let mut updated = claim.clone();
        updated.status = ActivityExecutionClaimStatus::Running;
        updated.executor_run_ref = Some(executor_run_ref.clone());
        updated.updated_at = chrono::Utc::now();
        self.claim_repo.update(&updated).await?;
        LifecycleEngine::apply_event(
            definition,
            state,
            ActivityEvent::ExecutorStarted {
                activity_key: updated.activity_key.clone(),
                attempt: updated.attempt,
                executor_run: executor_run_ref,
            },
        )
        .map_err(map_engine_error)?;
        Ok(updated)
    }

    pub async fn record_executor_start_failed(
        &self,
        definition: &WorkflowGraph,
        state: &mut ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
        error: impl Into<String>,
        retryable: bool,
    ) -> Result<ActivityExecutionClaim, WorkflowApplicationError> {
        let error = error.into();
        let mut updated = claim.clone();
        updated.status = ActivityExecutionClaimStatus::Failed;
        updated.updated_at = chrono::Utc::now();
        self.claim_repo.update(&updated).await?;
        LifecycleEngine::apply_event(
            definition,
            state,
            ActivityEvent::SchedulerStartFailed {
                activity_key: updated.activity_key.clone(),
                attempt: updated.attempt,
                error,
                retryable,
            },
        )
        .map_err(map_engine_error)?;
        Ok(updated)
    }

    pub async fn abandon_claiming_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ActivityExecutionClaim>, WorkflowApplicationError> {
        self.claim_repo
            .abandon_claiming_before(cutoff)
            .await
            .map_err(Into::into)
    }

    /// 在 executor 启动成功后创建 AgentAssignment（如果 assignment_repo 已接入）。
    ///
    /// 当前阶段 agent_id / frame_id 使用 placeholder（`Uuid::nil()`），
    /// 后续由 AgentFrame construction 任务填充真实值。
    async fn maybe_create_assignment(
        &self,
        run_id: Uuid,
        claim: &ActivityExecutionClaim,
    ) -> Result<Option<AgentAssignment>, WorkflowApplicationError> {
        let Some(repo) = self.assignment_repo else {
            return Ok(None);
        };
        let assignment = AgentAssignment::new(
            run_id,
            claim.graph_instance_id,
            claim.activity_key.clone(),
            claim.attempt as i32,
            // TODO(agent-frame-construction): 使用真实 agent_id / frame_id
            Uuid::nil(),
            Uuid::nil(),
        );
        repo.create(&assignment).await?;
        Ok(Some(assignment))
    }
}

fn map_engine_error(error: LifecycleEngineError) -> WorkflowApplicationError {
    match error {
        LifecycleEngineError::ActivityNotFound(activity_key) => {
            WorkflowApplicationError::BadRequest(format!("activity 不存在: {activity_key}"))
        }
        LifecycleEngineError::AttemptNotFound {
            activity_key,
            attempt,
        } => WorkflowApplicationError::BadRequest(format!(
            "activity attempt 不存在: {activity_key}#{attempt}"
        )),
        LifecycleEngineError::InvalidAttemptStatus { .. }
        | LifecycleEngineError::CompletionPolicyRejected(_)
        | LifecycleEngineError::AttemptLimitReached { .. }
        | LifecycleEngineError::ArtifactMissing(_) => {
            WorkflowApplicationError::Conflict(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec,
        ActivityIterationPolicy, ActivityTransition, ActivityTransitionKind,
        AgentActivityExecutorSpec, AgentSessionPolicy, ArtifactAliasPolicy, ArtifactBinding,
        InputPortDefinition, OutputPortDefinition, TransitionCondition, WorkflowBindingKind,
        WorkflowDefinitionSource,
    };
    use serde_json::json;

    use super::*;
    use crate::workflow::{ActivityPortValue, LifecycleEngine};

    fn test_graph_instance_id() -> Uuid {
        Uuid::nil()
    }

    #[derive(Default)]
    struct InMemoryClaimRepo {
        claims: Mutex<Vec<ActivityExecutionClaim>>,
    }

    struct FakeLauncher {
        result: Mutex<Result<ActivityExecutorStartResult, ActivityExecutorStartError>>,
        starts: Mutex<u32>,
    }

    impl FakeLauncher {
        fn started(session_id: &str) -> Self {
            Self {
                result: Mutex::new(Ok(ActivityExecutorStartResult::started(
                    agentdash_domain::workflow::ExecutorRunRef::AgentSession {
                        session_id: session_id.to_string(),
                    },
                ))),
                starts: Mutex::new(0),
            }
        }

        fn failed(error: ActivityExecutorStartError) -> Self {
            Self {
                result: Mutex::new(Err(error)),
                starts: Mutex::new(0),
            }
        }

        fn completed(output_port: &str) -> Self {
            Self {
                result: Mutex::new(Ok(ActivityExecutorStartResult::with_events(
                    agentdash_domain::workflow::ExecutorRunRef::FunctionRun {
                        run_id: "function-run-1".to_string(),
                    },
                    vec![ActivityEvent::ActivityCompleted {
                        activity_key: "plan".to_string(),
                        attempt: 1,
                        outputs: vec![ActivityPortValue {
                            port_key: output_port.to_string(),
                            value: json!({ "ok": true }),
                        }],
                        summary: Some("done".to_string()),
                    }],
                ))),
                starts: Mutex::new(0),
            }
        }

        fn start_count(&self) -> u32 {
            *self.starts.lock().expect("start count lock")
        }
    }

    #[async_trait::async_trait]
    impl ActivityExecutorLauncher for FakeLauncher {
        async fn start(
            &self,
            _definition: &WorkflowGraph,
            _state: &ActivityLifecycleRunState,
            _claim: &ActivityExecutionClaim,
        ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError> {
            *self.starts.lock().expect("start count lock") += 1;
            self.result.lock().expect("launcher result lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl ActivityExecutionClaimRepository for InMemoryClaimRepo {
        async fn create_or_get(
            &self,
            claim: &ActivityExecutionClaim,
        ) -> Result<ActivityExecutionClaim, DomainError> {
            let mut claims = self.claims.lock().expect("claim repo lock");
            if let Some(existing) = claims
                .iter()
                .find(|item| item.idempotency_key == claim.idempotency_key)
            {
                return Ok(existing.clone());
            }
            claims.push(claim.clone());
            Ok(claim.clone())
        }

        async fn get_by_idempotency_key(
            &self,
            idempotency_key: &str,
        ) -> Result<Option<ActivityExecutionClaim>, DomainError> {
            Ok(self
                .claims
                .lock()
                .expect("claim repo lock")
                .iter()
                .find(|claim| claim.idempotency_key == idempotency_key)
                .cloned())
        }

        async fn list_active_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
            Ok(self
                .claims
                .lock()
                .expect("claim repo lock")
                .iter()
                .filter(|claim| claim.run_id == run_id && claim.status.is_active())
                .cloned()
                .collect())
        }

        async fn update(&self, claim: &ActivityExecutionClaim) -> Result<(), DomainError> {
            let mut claims = self.claims.lock().expect("claim repo lock");
            let Some(existing) = claims
                .iter_mut()
                .find(|item| item.claim_id == claim.claim_id)
            else {
                return Err(DomainError::NotFound {
                    entity: "activity_execution_claim",
                    id: claim.claim_id.to_string(),
                });
            };
            *existing = claim.clone();
            Ok(())
        }

        async fn abandon_claiming_before(
            &self,
            cutoff: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
            let mut claims = self.claims.lock().expect("claim repo lock");
            let mut abandoned = Vec::new();
            for claim in claims.iter_mut().filter(|claim| {
                claim.status == agentdash_domain::workflow::ActivityExecutionClaimStatus::Claiming
                    && claim.updated_at < cutoff
            }) {
                claim.status = agentdash_domain::workflow::ActivityExecutionClaimStatus::Abandoned;
                abandoned.push(claim.clone());
            }
            Ok(abandoned)
        }

        async fn find_running_by_executor_session(
            &self,
            session_id: &str,
        ) -> Result<Option<ActivityExecutionClaim>, DomainError> {
            Ok(self
                .claims
                .lock()
                .expect("claim repo lock")
                .iter()
                .find(|claim| {
                    claim.status
                        == agentdash_domain::workflow::ActivityExecutionClaimStatus::Running
                        && matches!(
                            &claim.executor_run_ref,
                            Some(agentdash_domain::workflow::ExecutorRunRef::AgentSession { session_id: sid })
                                if sid == session_id
                        )
                })
                .cloned())
        }
    }

    fn port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: format!("{key} output"),
            gate_strategy: Default::default(),
            gate_params: None,
        }
    }

    fn input(key: &str) -> InputPortDefinition {
        InputPortDefinition {
            key: key.to_string(),
            description: format!("{key} input"),
            context_strategy: Default::default(),
            context_template: None,
            standalone_fulfillment: Default::default(),
        }
    }

    fn definition() -> WorkflowGraph {
        WorkflowGraph::new(
            Uuid::new_v4(),
            "claim_flow",
            "Claim flow",
            "",
            vec![WorkflowBindingKind::Story],
            WorkflowDefinitionSource::UserAuthored,
            "plan",
            vec![
                ActivityDefinition {
                    key: "plan".to_string(),
                    description: "plan".to_string(),
                    executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                        procedure_key: "wf_plan".to_string(),
                        session_policy: AgentSessionPolicy::SpawnChild,
                    }),
                    input_ports: vec![],
                    output_ports: vec![port("proposal")],
                    completion_policy: ActivityCompletionPolicy::OutputPorts {
                        required_ports: vec!["proposal".to_string()],
                    },
                    iteration_policy: ActivityIterationPolicy {
                        max_attempts: Some(2),
                        artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
                    },
                    join_policy: Default::default(),
                },
                ActivityDefinition {
                    key: "implement".to_string(),
                    description: "implement".to_string(),
                    executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                        procedure_key: "wf_implement".to_string(),
                        session_policy: AgentSessionPolicy::SpawnChild,
                    }),
                    input_ports: vec![input("approved_plan")],
                    output_ports: vec![port("summary")],
                    completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                    iteration_policy: Default::default(),
                    join_policy: Default::default(),
                },
            ],
            vec![ActivityTransition {
                from: "plan".to_string(),
                to: "implement".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::Always,
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "proposal".to_string(),
                    to_port: "approved_plan".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            }],
        )
        .expect("definition")
    }

    #[tokio::test]
    async fn scheduler_claims_ready_attempt_once() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        let claims = scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claims");

        assert_eq!(claims.len(), 1);
        let gi = test_graph_instance_id();
        assert_eq!(claims[0].idempotency_key, format!("{run_id}:{gi}:plan:1"));
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Claiming
        }));

        let claims = scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claims");
        assert!(claims.is_empty());
    }

    #[tokio::test]
    async fn scheduler_claims_successor_after_engine_marks_ready() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claim plan");
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ExecutorStarted {
                activity_key: "plan".to_string(),
                attempt: 1,
                executor_run: agentdash_domain::workflow::ExecutorRunRef::AgentSession {
                    session_id: "plan-child".to_string(),
                },
            },
        )
        .expect("start plan");
        LifecycleEngine::apply_event(
            &definition,
            &mut state,
            ActivityEvent::ActivityCompleted {
                activity_key: "plan".to_string(),
                attempt: 1,
                outputs: vec![ActivityPortValue {
                    port_key: "proposal".to_string(),
                    value: json!({"title": "v1"}),
                }],
                summary: None,
            },
        )
        .expect("complete plan");

        let claims = scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claim implement");

        assert_eq!(claims.len(), 1);
        let gi = test_graph_instance_id();
        assert_eq!(
            claims[0].idempotency_key,
            format!("{run_id}:{gi}:implement:1")
        );
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "implement"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Claiming
        }));
    }

    #[tokio::test]
    async fn scheduler_records_executor_started() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        let claim = scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claim")
            .pop()
            .expect("claim");

        let updated = scheduler
            .record_executor_started(
                &definition,
                &mut state,
                &claim,
                agentdash_domain::workflow::ExecutorRunRef::AgentSession {
                    session_id: "plan-child".to_string(),
                },
            )
            .await
            .expect("started");

        assert_eq!(updated.status, ActivityExecutionClaimStatus::Running);
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Running
        }));
    }

    #[tokio::test]
    async fn scheduler_retryable_start_failure_returns_attempt_to_ready() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        let claim = scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claim")
            .pop()
            .expect("claim");

        let updated = scheduler
            .record_executor_start_failed(&definition, &mut state, &claim, "prompt rejected", true)
            .await
            .expect("failed");

        assert_eq!(updated.status, ActivityExecutionClaimStatus::Failed);
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Ready
        }));
    }

    #[tokio::test]
    async fn scheduler_non_retryable_start_failure_marks_attempt_failed() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");
        let claim = scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claim")
            .pop()
            .expect("claim");

        scheduler
            .record_executor_start_failed(&definition, &mut state, &claim, "bad config", false)
            .await
            .expect("failed");

        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Failed
        }));
    }

    #[tokio::test]
    async fn scheduler_launches_claimed_attempt_once() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let launcher = FakeLauncher::started("plan-child");
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        let outcomes = scheduler
            .launch_ready_attempts(run_id, &definition, &mut state, &launcher)
            .await
            .expect("launch");
        let second = scheduler
            .launch_ready_attempts(run_id, &definition, &mut state, &launcher)
            .await
            .expect("launch again");

        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].started);
        assert!(second.is_empty());
        assert_eq!(launcher.start_count(), 1);
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Running
        }));
    }

    #[tokio::test]
    async fn scheduler_applies_immediate_completion_event() {
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let definition = definition();
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("state");
        let launcher = FakeLauncher::completed("proposal");

        scheduler
            .launch_ready_attempts(run_id, &definition, &mut state, &launcher)
            .await
            .expect("launch");

        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Completed
        }));
        assert_eq!(state.outputs[0].port_key, "proposal");
    }

    #[tokio::test]
    async fn scheduler_launch_failure_does_not_leave_running_attempt() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let launcher =
            FakeLauncher::failed(ActivityExecutorStartError::retryable("prompt not accepted"));
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        let outcomes = scheduler
            .launch_ready_attempts(run_id, &definition, &mut state, &launcher)
            .await
            .expect("launch");

        assert_eq!(outcomes.len(), 1);
        assert!(!outcomes[0].started);
        assert_eq!(outcomes[0].error.as_deref(), Some("prompt not accepted"));
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "plan"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Ready
        }));
        assert!(
            !state
                .attempts
                .iter()
                .any(|attempt| attempt.status == ActivityAttemptStatus::Running)
        );
    }

    #[tokio::test]
    async fn scheduler_abandons_stale_claiming_claims() {
        let definition = definition();
        let repo = InMemoryClaimRepo::default();
        let scheduler = ActivityExecutorScheduler::new(&repo);
        let run_id = Uuid::new_v4();
        let mut state = LifecycleEngine::initialize(&definition, test_graph_instance_id()).expect("init");

        scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claim");

        let abandoned = scheduler
            .abandon_claiming_before(chrono::Utc::now() + chrono::Duration::seconds(1))
            .await
            .expect("abandon");

        assert_eq!(abandoned.len(), 1);
        assert_eq!(abandoned[0].status, ActivityExecutionClaimStatus::Abandoned);
    }
}
