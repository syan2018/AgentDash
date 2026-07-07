use std::sync::Arc;

use agentdash_agent_types::{
    AgentRuntimeError, CompactionFailureInput, CompactionImplementation, CompactionMetadata,
    CompactionNoopInput, CompactionParams, CompactionPhase, CompactionReason, CompactionResult,
    CompactionStrategy, CompactionTrigger, CompactionTriggerStats, DynRuntimeCompactionDelegate,
    EvaluateCompactionInput, RuntimeCompactionDelegate,
};
use agentdash_domain::workflow::{
    ManualContextCompactionRequest, ManualContextCompactionRequestRepository,
    ManualContextCompactionRequestedMode,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const DEFAULT_MANUAL_KEEP_LAST_N: u32 = 20;
const DEFAULT_MANUAL_RESERVE_TOKENS: u64 = 16_384;

pub(crate) struct ManualContextCompactionDelegate {
    session_id: String,
    turn_id: String,
    repo: Arc<dyn ManualContextCompactionRequestRepository>,
    inner: Option<DynRuntimeCompactionDelegate>,
}

impl ManualContextCompactionDelegate {
    pub(crate) fn wrap(
        session_id: String,
        turn_id: String,
        repo: Arc<dyn ManualContextCompactionRequestRepository>,
        inner: Option<DynRuntimeCompactionDelegate>,
    ) -> DynRuntimeCompactionDelegate {
        Arc::new(Self {
            session_id,
            turn_id,
            repo,
            inner,
        })
    }

    fn manual_metadata(request: &ManualContextCompactionRequest) -> CompactionMetadata {
        CompactionMetadata {
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::UserRequested,
            phase: match request.requested_mode {
                ManualContextCompactionRequestedMode::NextTurn => CompactionPhase::PreProvider,
                ManualContextCompactionRequestedMode::CompactOnly => {
                    CompactionPhase::StandaloneCompactTurn
                }
            },
            strategy: CompactionStrategy::SummaryPrefix,
            implementation: CompactionImplementation::LocalSummary,
            request_id: Some(request.id.to_string()),
        }
    }

    fn params_for_request(request: &ManualContextCompactionRequest) -> CompactionParams {
        let keep_last_n = request
            .keep_last_n
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MANUAL_KEEP_LAST_N);
        let reserve_tokens = request
            .reserve_tokens
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MANUAL_RESERVE_TOKENS);
        CompactionParams {
            keep_last_n,
            reserve_tokens,
            custom_summary: None,
            custom_prompt: None,
            trigger_stats: CompactionTriggerStats {
                input_tokens: 0,
                context_window: 0,
                reserve_tokens,
            },
            metadata: Self::manual_metadata(request),
        }
    }

    fn manual_request_id(metadata: Option<&CompactionMetadata>) -> Option<Uuid> {
        let metadata = metadata?;
        if metadata.trigger != CompactionTrigger::Manual {
            return None;
        }
        metadata
            .request_id
            .as_deref()
            .and_then(|request_id| Uuid::parse_str(request_id).ok())
    }

    fn should_defer_next_turn_request(&self, request: &ManualContextCompactionRequest) -> bool {
        if request.requested_mode != ManualContextCompactionRequestedMode::NextTurn {
            return false;
        }
        request
            .request_metadata
            .as_ref()
            .and_then(|value| value.get("active_turn_id"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|active_turn_id| active_turn_id == self.turn_id)
    }
}

#[async_trait]
impl RuntimeCompactionDelegate for ManualContextCompactionDelegate {
    async fn evaluate_compaction(
        &self,
        input: EvaluateCompactionInput,
        cancel: CancellationToken,
    ) -> Result<Option<CompactionParams>, AgentRuntimeError> {
        let request = self
            .repo
            .find_requested_by_session(&self.session_id)
            .await
            .map_err(|error| {
                AgentRuntimeError::Runtime(format!(
                    "读取手动 context compaction request 失败: {error}"
                ))
            })?;

        if let Some(request) = request
            && !self.should_defer_next_turn_request(&request)
        {
            self.repo
                .mark_consumed(request.id, self.turn_id.clone())
                .await
                .map_err(|error| {
                    AgentRuntimeError::Runtime(format!(
                        "标记手动 context compaction request consumed 失败: {error}"
                    ))
                })?;
            let mut params = Self::params_for_request(&request);
            if let Some(stats) = input.provider_visible.as_ref() {
                params.trigger_stats.input_tokens = stats.estimated_input_tokens;
            }
            return Ok(Some(params));
        }

        let Some(inner) = self.inner.as_ref() else {
            return Ok(None);
        };
        inner.evaluate_compaction(input, cancel).await
    }

    async fn after_compaction(
        &self,
        result: CompactionResult,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        if let Some(inner) = self.inner.as_ref() {
            inner.after_compaction(result, cancel).await?;
        }
        Ok(())
    }

    async fn after_compaction_failed(
        &self,
        input: CompactionFailureInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        if let Some(request_id) = Self::manual_request_id(input.metadata.as_ref()) {
            self.repo
                .mark_failed(
                    request_id,
                    Some(serde_json::json!({
                        "status": "failed",
                        "reason": "compaction_failed",
                        "lifecycle_item_id": input.item_id,
                        "error": input.error,
                        "metadata": input.metadata,
                    })),
                )
                .await
                .map_err(|error| {
                    AgentRuntimeError::Runtime(format!(
                        "标记手动 context compaction request failed 失败: {error}"
                    ))
                })?;
            return Ok(());
        }

        let Some(inner) = self.inner.as_ref() else {
            return Ok(());
        };
        inner.after_compaction_failed(input, cancel).await
    }

    async fn after_compaction_noop(
        &self,
        input: CompactionNoopInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        if let Some(request_id) = Self::manual_request_id(Some(&input.metadata)) {
            self.repo
                .mark_noop(
                    request_id,
                    Some(serde_json::json!({
                        "status": "noop",
                        "reason": input.reason,
                        "lifecycle_item_id": input.item_id,
                        "metadata": input.metadata,
                    })),
                )
                .await
                .map_err(|error| {
                    AgentRuntimeError::Runtime(format!(
                        "标记手动 context compaction request noop 失败: {error}"
                    ))
                })?;
            return Ok(());
        }

        let Some(inner) = self.inner.as_ref() else {
            return Ok(());
        };
        inner.after_compaction_noop(input, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent_types::{AgentContext, ProviderVisibleContextStats};
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::workflow::{
        ManualContextCompactionRequestStatus, NewManualContextCompactionRequest,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct ManualRequestFixtureRepo {
        requests: Mutex<Vec<ManualContextCompactionRequest>>,
    }

    #[async_trait]
    impl ManualContextCompactionRequestRepository for ManualRequestFixtureRepo {
        async fn create_requested(
            &self,
            request: NewManualContextCompactionRequest,
        ) -> Result<ManualContextCompactionRequest, DomainError> {
            let now = Utc::now();
            let record = ManualContextCompactionRequest {
                id: Uuid::new_v4(),
                session_id: request.session_id,
                run_id: request.run_id,
                agent_id: request.agent_id,
                command_receipt_id: request.command_receipt_id,
                status: ManualContextCompactionRequestStatus::Requested,
                requested_mode: request.requested_mode,
                keep_last_n: request.keep_last_n,
                reserve_tokens: request.reserve_tokens,
                request_metadata: request.request_metadata,
                result_metadata: None,
                requested_at: now,
                updated_at: now,
                consumed_turn_id: None,
                completed_compaction_id: None,
                compacted_until_ref: None,
                first_kept_ref: None,
            };
            self.requests.lock().await.push(record.clone());
            Ok(record)
        }

        async fn get_by_id(
            &self,
            id: Uuid,
        ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
            Ok(self
                .requests
                .lock()
                .await
                .iter()
                .find(|request| request.id == id)
                .cloned())
        }

        async fn get_by_command_receipt(
            &self,
            command_receipt_id: Uuid,
        ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
            Ok(self
                .requests
                .lock()
                .await
                .iter()
                .find(|request| request.command_receipt_id == command_receipt_id)
                .cloned())
        }

        async fn find_requested_by_session(
            &self,
            session_id: &str,
        ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
            Ok(self
                .requests
                .lock()
                .await
                .iter()
                .find(|request| {
                    request.session_id == session_id
                        && request.status == ManualContextCompactionRequestStatus::Requested
                })
                .cloned())
        }

        async fn mark_consumed(
            &self,
            id: Uuid,
            turn_id: String,
        ) -> Result<ManualContextCompactionRequest, DomainError> {
            self.update(id, |request| {
                request.status = ManualContextCompactionRequestStatus::Consumed;
                request.consumed_turn_id = Some(turn_id);
            })
            .await
        }

        async fn mark_completed(
            &self,
            id: Uuid,
            compaction_id: String,
            compacted_until_ref: Option<serde_json::Value>,
            first_kept_ref: Option<serde_json::Value>,
            result_metadata: Option<serde_json::Value>,
        ) -> Result<ManualContextCompactionRequest, DomainError> {
            self.update(id, |request| {
                request.status = ManualContextCompactionRequestStatus::Completed;
                request.completed_compaction_id = Some(compaction_id);
                request.compacted_until_ref = compacted_until_ref;
                request.first_kept_ref = first_kept_ref;
                request.result_metadata = result_metadata;
            })
            .await
        }

        async fn mark_noop(
            &self,
            id: Uuid,
            result_metadata: Option<serde_json::Value>,
        ) -> Result<ManualContextCompactionRequest, DomainError> {
            self.update(id, |request| {
                request.status = ManualContextCompactionRequestStatus::Noop;
                request.result_metadata = result_metadata;
            })
            .await
        }

        async fn mark_failed(
            &self,
            id: Uuid,
            result_metadata: Option<serde_json::Value>,
        ) -> Result<ManualContextCompactionRequest, DomainError> {
            self.update(id, |request| {
                request.status = ManualContextCompactionRequestStatus::Failed;
                request.result_metadata = result_metadata;
            })
            .await
        }
    }

    impl ManualRequestFixtureRepo {
        async fn update(
            &self,
            id: Uuid,
            apply: impl FnOnce(&mut ManualContextCompactionRequest),
        ) -> Result<ManualContextCompactionRequest, DomainError> {
            let mut requests = self.requests.lock().await;
            let request = requests
                .iter_mut()
                .find(|request| request.id == id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "runtime_session_compaction_request",
                    id: id.to_string(),
                })?;
            apply(request);
            request.updated_at = Utc::now();
            Ok(request.clone())
        }

        async fn only_request(&self) -> ManualContextCompactionRequest {
            self.requests
                .lock()
                .await
                .first()
                .cloned()
                .expect("request should exist")
        }
    }

    fn evaluate_input() -> EvaluateCompactionInput {
        EvaluateCompactionInput {
            context: AgentContext {
                system_prompt: String::new(),
                messages: Vec::new(),
                message_refs: Vec::new(),
                tools: Vec::new(),
            },
            provider_visible: Some(ProviderVisibleContextStats {
                estimated_input_tokens: 123,
                ..ProviderVisibleContextStats::default()
            }),
        }
    }

    async fn seed_request(
        repo: &ManualRequestFixtureRepo,
        active_turn_id: &str,
    ) -> ManualContextCompactionRequest {
        repo.create_requested(NewManualContextCompactionRequest {
            session_id: "session-1".to_string(),
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            command_receipt_id: Uuid::new_v4(),
            requested_mode: ManualContextCompactionRequestedMode::NextTurn,
            keep_last_n: None,
            reserve_tokens: None,
            request_metadata: Some(serde_json::json!({
                "trigger": "manual",
                "fulfillment": "schedule_for_next_turn",
                "active_turn_id": active_turn_id,
            })),
        })
        .await
        .expect("seed request")
    }

    #[tokio::test]
    async fn next_turn_request_is_not_consumed_by_the_active_turn_that_scheduled_it() {
        let repo = Arc::new(ManualRequestFixtureRepo::default());
        seed_request(&repo, "turn-active").await;
        let delegate = ManualContextCompactionDelegate {
            session_id: "session-1".to_string(),
            turn_id: "turn-active".to_string(),
            repo: repo.clone(),
            inner: None,
        };

        let params = delegate
            .evaluate_compaction(evaluate_input(), CancellationToken::new())
            .await
            .expect("evaluate compaction");

        assert!(params.is_none());
        let request = repo.only_request().await;
        assert_eq!(
            request.status,
            ManualContextCompactionRequestStatus::Requested
        );
        assert_eq!(request.consumed_turn_id, None);
    }

    #[tokio::test]
    async fn next_turn_request_is_consumed_by_a_later_turn_pre_provider_boundary() {
        let repo = Arc::new(ManualRequestFixtureRepo::default());
        let request = seed_request(&repo, "turn-active").await;
        let delegate = ManualContextCompactionDelegate {
            session_id: "session-1".to_string(),
            turn_id: "turn-next".to_string(),
            repo: repo.clone(),
            inner: None,
        };

        let params = delegate
            .evaluate_compaction(evaluate_input(), CancellationToken::new())
            .await
            .expect("evaluate compaction")
            .expect("manual compaction should be requested");

        assert_eq!(params.metadata.trigger, CompactionTrigger::Manual);
        assert_eq!(params.metadata.reason, CompactionReason::UserRequested);
        assert_eq!(params.metadata.phase, CompactionPhase::PreProvider);
        assert_eq!(params.metadata.request_id, Some(request.id.to_string()));

        let request = repo.only_request().await;
        assert_eq!(
            request.status,
            ManualContextCompactionRequestStatus::Consumed
        );
        assert_eq!(request.consumed_turn_id.as_deref(), Some("turn-next"));
    }
}
