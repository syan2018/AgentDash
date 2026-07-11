use std::sync::Arc;

use agentdash_domain::interaction::{OperationEffectIntent, OperationEffectIntentRepository};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};

use super::{InteractionApplicationError, InteractionApplicationResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionEffectExecutionOutcome {
    Succeeded,
    RetryAt {
        at: DateTime<Utc>,
        failure_code: String,
    },
    Terminal {
        failure_code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Operation effect execution failed: {failure_code}")]
pub struct InteractionEffectExecutionError {
    pub failure_code: String,
    pub retry_at: DateTime<Utc>,
}

/// Adapter 必须重新进入当前 Operation admission；intent 内 admission_audit 只用于审计。
#[async_trait]
pub trait InteractionEffectOperationExecutor: Send + Sync {
    async fn execute(
        &self,
        intent: &OperationEffectIntent,
    ) -> Result<InteractionEffectExecutionOutcome, InteractionEffectExecutionError>;
}

#[derive(Clone)]
pub struct InteractionEffectDispatcher {
    intents: Arc<dyn OperationEffectIntentRepository>,
    executor: Arc<dyn InteractionEffectOperationExecutor>,
    claim_lease: Duration,
}

impl InteractionEffectDispatcher {
    pub fn new(
        intents: Arc<dyn OperationEffectIntentRepository>,
        executor: Arc<dyn InteractionEffectOperationExecutor>,
        claim_lease: Duration,
    ) -> InteractionApplicationResult<Self> {
        if claim_lease <= Duration::zero() {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: "effect claim lease 必须大于 0".to_string(),
            });
        }
        Ok(Self {
            intents,
            executor,
            claim_lease,
        })
    }

    pub async fn dispatch_due(
        &self,
        limit: usize,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<usize> {
        let mut count = 0;
        while count < limit {
            let Some(intent) = self
                .intents
                .claim_due(1, now, now + self.claim_lease)
                .await?
                .into_iter()
                .next()
            else {
                break;
            };
            count += 1;
            let claim_token = intent.claim_token.ok_or_else(|| {
                InteractionApplicationError::ContractUnavailable {
                    reason: format!("claimed effect 缺少 claim_token: {}", intent.effect_id),
                }
            })?;
            match self.executor.execute(&intent).await {
                Ok(InteractionEffectExecutionOutcome::Succeeded) => {
                    self.intents
                        .mark_succeeded(intent.effect_id, claim_token, now)
                        .await?
                }
                Ok(InteractionEffectExecutionOutcome::RetryAt { at, failure_code }) => {
                    let at = if at <= now {
                        now + Duration::milliseconds(1)
                    } else {
                        at
                    };
                    self.intents
                        .mark_failed(intent.effect_id, claim_token, at, &failure_code, false)
                        .await?
                }
                Ok(InteractionEffectExecutionOutcome::Terminal { failure_code }) => {
                    self.intents
                        .mark_failed(intent.effect_id, claim_token, now, &failure_code, true)
                        .await?
                }
                Err(error) => {
                    self.intents
                        .mark_failed(
                            intent.effect_id,
                            claim_token,
                            error.retry_at,
                            &error.failure_code,
                            false,
                        )
                        .await?
                }
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::interaction::*;
    use agentdash_domain::operation::OperationRef;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct FixtureIntentRepository {
        effects: Mutex<Vec<OperationEffectIntent>>,
    }
    #[async_trait]
    impl OperationEffectIntentRepository for FixtureIntentRepository {
        async fn claim_due(
            &self,
            limit: usize,
            claimed_at: DateTime<Utc>,
            expires_at: DateTime<Utc>,
        ) -> Result<Vec<OperationEffectIntent>, InteractionError> {
            let mut effects = self.effects.lock().map_err(|_| fixture_error())?;
            let mut claimed = vec![];
            for effect in effects
                .iter_mut()
                .filter(|effect| {
                    matches!(
                        effect.status,
                        OperationEffectStatus::Pending | OperationEffectStatus::RetryScheduled
                    ) && effect.next_attempt_at <= claimed_at
                })
                .take(limit)
            {
                effect.claim(Uuid::new_v4(), claimed_at, expires_at)?;
                claimed.push(effect.clone());
            }
            Ok(claimed)
        }
        async fn mark_succeeded(
            &self,
            id: Uuid,
            token: Uuid,
            at: DateTime<Utc>,
        ) -> Result<(), InteractionError> {
            let mut effects = self.effects.lock().map_err(|_| fixture_error())?;
            let effect = effects
                .iter_mut()
                .find(|effect| effect.effect_id == id)
                .ok_or_else(fixture_error)?;
            effect.mark_succeeded(token, at)
        }
        async fn mark_failed(
            &self,
            id: Uuid,
            token: Uuid,
            next: DateTime<Utc>,
            code: &str,
            terminal: bool,
        ) -> Result<(), InteractionError> {
            let mut effects = self.effects.lock().map_err(|_| fixture_error())?;
            let effect = effects
                .iter_mut()
                .find(|effect| effect.effect_id == id)
                .ok_or_else(fixture_error)?;
            if terminal {
                effect.mark_terminal_failed(token, next, code)
            } else {
                effect.schedule_retry(token, next, code)
            }
        }
    }
    struct FixtureExecutor {
        first_id: Uuid,
    }
    #[async_trait]
    impl InteractionEffectOperationExecutor for FixtureExecutor {
        async fn execute(
            &self,
            intent: &OperationEffectIntent,
        ) -> Result<InteractionEffectExecutionOutcome, InteractionEffectExecutionError> {
            if intent.effect_id == self.first_id {
                Err(InteractionEffectExecutionError {
                    failure_code: "temporary".into(),
                    retry_at: Utc::now() + Duration::minutes(1),
                })
            } else {
                Ok(InteractionEffectExecutionOutcome::Succeeded)
            }
        }
    }
    fn fixture_error() -> InteractionError {
        InteractionError::Persistence {
            operation: "fixture",
            message: "fixture failure".into(),
        }
    }
    fn effect(id: Uuid, now: DateTime<Utc>) -> OperationEffectIntent {
        OperationEffectIntent {
            effect_id: id,
            instance_id: Uuid::new_v4(),
            source_event_id: Uuid::new_v4(),
            operation_ref: OperationRef::new("host", "core", "notify", 1).expect("operation"),
            validated_input: serde_json::json!({}),
            admission_audit: OperationEffectAdmissionAudit {
                principal: OperationEffectPrincipalRef::Human {
                    user_id: "u".into(),
                },
                scope: InteractionOwner::User("u".into()),
                capability_revision_ref: "cap:1".into(),
                admitted_at: now,
            },
            idempotency_key: id.to_string(),
            safety: OperationEffectSafety::Idempotent,
            status: OperationEffectStatus::Pending,
            attempt: 0,
            next_attempt_at: now,
            claim_token: None,
            claimed_at: None,
            claim_expires_at: None,
            completed_at: None,
            last_failure_code: None,
        }
    }

    #[tokio::test]
    async fn executor_failure_isolated_and_batch_continues() {
        let now = Utc::now();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let repo = Arc::new(FixtureIntentRepository {
            effects: Mutex::new(vec![effect(first, now), effect(second, now)]),
        });
        let dispatcher = InteractionEffectDispatcher::new(
            repo.clone(),
            Arc::new(FixtureExecutor { first_id: first }),
            Duration::seconds(30),
        )
        .expect("dispatcher");
        assert_eq!(dispatcher.dispatch_due(2, now).await.expect("dispatch"), 2);
        let effects = repo.effects.lock().expect("lock");
        assert_eq!(effects[0].status, OperationEffectStatus::RetryScheduled);
        assert_eq!(effects[1].status, OperationEffectStatus::Succeeded);
    }

    #[test]
    fn claim_lease_must_be_positive() {
        let repo = Arc::new(FixtureIntentRepository {
            effects: Mutex::new(vec![]),
        });
        assert!(
            InteractionEffectDispatcher::new(
                repo,
                Arc::new(FixtureExecutor {
                    first_id: Uuid::new_v4()
                }),
                Duration::zero()
            )
            .is_err()
        );
    }
}
