use agentdash_domain::workflow::{
    ActivityAttemptStatus, ActivityExecutionClaim, ActivityExecutionClaimRepository,
    ActivityExecutionClaimStatus, ActivityLifecycleDefinition,
};
use uuid::Uuid;

use super::{
    ActivityEvent, ActivityLifecycleRunState, LifecycleEngine, LifecycleEngineError,
    WorkflowApplicationError,
};

pub struct ActivityExecutorScheduler<'a, R: ?Sized> {
    claim_repo: &'a R,
}

impl<'a, R: ?Sized> ActivityExecutorScheduler<'a, R>
where
    R: ActivityExecutionClaimRepository,
{
    pub fn new(claim_repo: &'a R) -> Self {
        Self { claim_repo }
    }

    pub async fn claim_ready_attempts(
        &self,
        run_id: Uuid,
        definition: &ActivityLifecycleDefinition,
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

    #[derive(Default)]
    struct InMemoryClaimRepo {
        claims: Mutex<Vec<ActivityExecutionClaim>>,
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

    fn definition() -> ActivityLifecycleDefinition {
        ActivityLifecycleDefinition::new(
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
                        workflow_key: "wf_plan".to_string(),
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
                        workflow_key: "wf_implement".to_string(),
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
        let mut state = LifecycleEngine::initialize(&definition).expect("init");

        let claims = scheduler
            .claim_ready_attempts(run_id, &definition, &mut state)
            .await
            .expect("claims");

        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].idempotency_key, format!("{run_id}:plan:1"));
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
        let mut state = LifecycleEngine::initialize(&definition).expect("init");

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
        assert_eq!(claims[0].idempotency_key, format!("{run_id}:implement:1"));
        assert!(state.attempts.iter().any(|attempt| {
            attempt.activity_key == "implement"
                && attempt.attempt == 1
                && attempt.status == ActivityAttemptStatus::Claiming
        }));
    }
}
