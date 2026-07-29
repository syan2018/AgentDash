use agentdash_agent_protocol::codex_app_server_protocol as owned;
use agentdash_agent_protocol::generated::codex_v2::server_notification::ThreadItem as ServerThreadItem;
use agentdash_agent_protocol::{
    AgentDashCompactionStatus, AgentDashNativeThreadItem, BackboneEnvelope, BackboneEvent,
    CanonicalConversationPresentation, CanonicalConversationRecord, ItemCompletedNotification,
    ItemStartedNotification, PresentationDurability, SourceInfo,
};
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::vendor_generated::codex_v2::server_notification::ServerNotification;

pub(crate) fn notification_record(
    source_thread_id: &str,
    sequence: u64,
    notification: &ServerNotification,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    use PresentationDurability::{Durable, Ephemeral};
    use ServerNotification as Source;

    if let Source::TurnCompleted(value) = notification {
        let completed: owned::TurnCompletedNotification = transcode(value)?;
        let status = match completed.turn.status {
            owned::TurnStatus::Completed => AgentDashCompactionStatus::Succeeded,
            owned::TurnStatus::Failed => AgentDashCompactionStatus::Failed,
            owned::TurnStatus::Interrupted => AgentDashCompactionStatus::Cancelled,
            owned::TurnStatus::InProgress => {
                return Err(serde::de::Error::custom(
                    "turn/completed cannot carry an in-progress turn",
                ));
            }
        };
        let error = completed
            .turn
            .error
            .as_ref()
            .and_then(Option::as_ref)
            .map(|error| error.message.clone());
        let completed_at_ms = completed
            .turn
            .completed_at
            .flatten()
            .and_then(|seconds| seconds.checked_mul(1_000))
            .unwrap_or_default();
        let started_at_ms = completed
            .turn
            .started_at
            .flatten()
            .and_then(|seconds| u64::try_from(seconds.checked_mul(1_000)?).ok());
        let mut records = completed
            .turn
            .items
            .iter()
            .filter_map(|item| match item {
                ServerThreadItem::ContextCompaction { id } => Some(id),
                _ => None,
            })
            .map(|id| {
                let item = AgentDashNativeThreadItem::ContextCompaction {
                    id: id.clone(),
                    status,
                    error: error.clone(),
                    started_at_ms,
                    completed_at_ms: u64::try_from(completed_at_ms).ok(),
                    context_revision: None,
                };
                let mut terminal = ItemCompletedNotification::new(
                    item,
                    source_thread_id,
                    completed.turn.id.clone(),
                )
                .map_err(serde::de::Error::custom)?;
                terminal.completed_at_ms = completed_at_ms;
                Ok(record(
                    format!("codex:{source_thread_id}:{sequence}:compaction:{id}:terminal"),
                    Durable,
                    source_thread_id,
                    BackboneEvent::ItemCompleted(terminal),
                ))
            })
            .collect::<std::result::Result<Vec<_>, serde_json::Error>>()?;
        records.push(record(
            format!("codex:{source_thread_id}:{sequence}"),
            Durable,
            source_thread_id,
            BackboneEvent::TurnCompleted(completed.into()),
        ));
        return Ok(records);
    }

    let mapped = match notification {
        Source::TurnStarted(value) => {
            Some((Durable, BackboneEvent::TurnStarted(transcode(value)?)))
        }
        Source::TurnCompleted(_) => unreachable!("handled above"),
        Source::ItemStarted(value) => {
            let value: owned::ItemStartedNotification = transcode(value)?;
            Some((
                Durable,
                BackboneEvent::ItemStarted(ItemStartedNotification::from_codex(value)),
            ))
        }
        Source::ItemCompleted(value) => {
            let value: owned::ItemCompletedNotification = transcode(value)?;
            if matches!(value.item, ServerThreadItem::ContextCompaction { .. }) {
                None
            } else {
                Some((
                    Durable,
                    BackboneEvent::ItemCompleted(
                        ItemCompletedNotification::from_codex(value)
                            .map_err(serde::de::Error::custom)?,
                    ),
                ))
            }
        }
        Source::ItemAgentMessageDelta(value) => Some((
            Ephemeral,
            BackboneEvent::AgentMessageDelta(transcode(value)?),
        )),
        Source::ItemReasoningTextDelta(value) => Some((
            Ephemeral,
            BackboneEvent::ReasoningTextDelta(transcode(value)?),
        )),
        Source::ItemReasoningSummaryTextDelta(value) => Some((
            Ephemeral,
            BackboneEvent::ReasoningSummaryDelta(transcode(value)?),
        )),
        Source::ItemPlanDelta(value) => {
            Some((Ephemeral, BackboneEvent::PlanDelta(transcode(value)?)))
        }
        Source::ItemCommandExecutionOutputDelta(value) => Some((
            Ephemeral,
            BackboneEvent::CommandOutputDelta(transcode(value)?),
        )),
        Source::ItemFileChangeOutputDelta(value) => {
            Some((Ephemeral, BackboneEvent::FileChangeDelta(transcode(value)?)))
        }
        Source::ItemMcpToolCallProgress(value) => Some((
            Ephemeral,
            BackboneEvent::McpToolCallProgress(transcode(value)?),
        )),
        Source::ItemCommandExecutionTerminalInteraction(value) => Some((
            Durable,
            BackboneEvent::TerminalInteraction(transcode(value)?),
        )),
        Source::ItemFileChangePatchUpdated(value) => Some((
            Ephemeral,
            BackboneEvent::FileChangePatchUpdated(transcode(value)?),
        )),
        Source::ServerRequestResolved(value) => Some((
            Durable,
            BackboneEvent::ServerRequestResolved(transcode(value)?),
        )),
        Source::TurnDiffUpdated(value) => {
            Some((Durable, BackboneEvent::TurnDiffUpdated(transcode(value)?)))
        }
        Source::TurnPlanUpdated(value) => {
            Some((Durable, BackboneEvent::TurnPlanUpdated(transcode(value)?)))
        }
        Source::ItemReasoningSummaryPartAdded(value) => Some((
            Durable,
            BackboneEvent::ReasoningSummaryPartAdded(transcode(value)?),
        )),
        Source::ItemAutoApprovalReviewStarted(value) => Some((
            Durable,
            BackboneEvent::AutoApprovalReviewStarted(transcode(value)?),
        )),
        Source::ItemAutoApprovalReviewCompleted(value) => Some((
            Durable,
            BackboneEvent::AutoApprovalReviewCompleted(transcode(value)?),
        )),
        Source::ThreadTokenUsageUpdated(value) => {
            let value: owned::ThreadTokenUsageUpdatedNotification = transcode(value)?;
            Some((Durable, BackboneEvent::TokenUsageUpdated(value.into())))
        }
        Source::ThreadStatusChanged(value) => Some((
            Durable,
            BackboneEvent::ThreadStatusChanged(transcode(value)?),
        )),
        Source::ThreadNameUpdated(value) => {
            Some((Durable, BackboneEvent::ThreadNameUpdated(transcode(value)?)))
        }
        Source::ThreadCompacted(value) => Some((
            Durable,
            BackboneEvent::ExecutorContextCompacted(transcode(value)?),
        )),
        Source::ModelRerouted(value) => {
            Some((Durable, BackboneEvent::ModelRerouted(transcode(value)?)))
        }
        Source::ModelVerification(value) => {
            Some((Durable, BackboneEvent::ModelVerification(transcode(value)?)))
        }
        Source::TurnModerationMetadata(value) => Some((
            Durable,
            BackboneEvent::TurnModerationMetadata(transcode(value)?),
        )),
        Source::ModelSafetyBufferingUpdated(value) => Some((
            Ephemeral,
            BackboneEvent::ModelSafetyBufferingUpdated(transcode(value)?),
        )),
        Source::Warning(value) => Some((Durable, BackboneEvent::Warning(transcode(value)?))),
        Source::GuardianWarning(value) => {
            Some((Durable, BackboneEvent::GuardianWarning(transcode(value)?)))
        }
        Source::DeprecationNotice(value) => {
            Some((Durable, BackboneEvent::DeprecationNotice(transcode(value)?)))
        }
        Source::ConfigWarning(value) => {
            Some((Durable, BackboneEvent::ConfigWarning(transcode(value)?)))
        }
        Source::Error(value) => Some((Durable, BackboneEvent::Error(transcode(value)?))),
        Source::ThreadStarted(_)
        | Source::ThreadArchived(_)
        | Source::ThreadDeleted(_)
        | Source::ThreadUnarchived(_)
        | Source::ThreadClosed(_)
        | Source::SkillsChanged(_)
        | Source::ThreadGoalUpdated(_)
        | Source::ThreadGoalCleared(_)
        | Source::ThreadSettingsUpdated(_)
        | Source::HookStarted(_)
        | Source::HookCompleted(_)
        | Source::CommandExecOutputDelta(_)
        | Source::ProcessOutputDelta(_)
        | Source::ProcessExited(_)
        | Source::McpServerOauthLoginCompleted(_)
        | Source::McpServerStartupStatusUpdated(_)
        | Source::AccountUpdated(_)
        | Source::AccountRateLimitsUpdated(_)
        | Source::AppListUpdated(_)
        | Source::RemoteControlStatusChanged(_)
        | Source::ExternalAgentConfigImportProgress(_)
        | Source::ExternalAgentConfigImportCompleted(_)
        | Source::FsChanged(_)
        | Source::FuzzyFileSearchSessionUpdated(_)
        | Source::FuzzyFileSearchSessionCompleted(_)
        | Source::ThreadRealtimeStarted(_)
        | Source::ThreadRealtimeItemAdded(_)
        | Source::ThreadRealtimeTranscriptDelta(_)
        | Source::ThreadRealtimeTranscriptDone(_)
        | Source::ThreadRealtimeOutputAudioDelta(_)
        | Source::ThreadRealtimeSdp(_)
        | Source::ThreadRealtimeError(_)
        | Source::ThreadRealtimeClosed(_)
        | Source::WindowsWorldWritableWarning(_)
        | Source::WindowsSandboxSetupCompleted(_)
        | Source::AccountLoginCompleted(_) => None,
    };

    Ok(mapped
        .map(|(durability, event)| {
            vec![record(
                format!("codex:{source_thread_id}:{sequence}"),
                durability,
                source_thread_id,
                event,
            )]
        })
        .unwrap_or_default())
}

pub(crate) fn snapshot_records(
    source_thread_id: &str,
    result: &Value,
) -> Result<Vec<CanonicalConversationRecord>> {
    let turns = result
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .context("thread/read response misses thread.turns")?;
    let mut records = Vec::new();
    for (turn_index, turn) in turns.iter().enumerate() {
        let turn_id = turn
            .get("id")
            .and_then(Value::as_str)
            .context("thread/read turn misses id")?;
        let turn_status = turn
            .get("status")
            .and_then(Value::as_str)
            .context("thread/read turn misses status")?;
        if !matches!(
            turn_status,
            "completed" | "failed" | "interrupted" | "inProgress"
        ) {
            anyhow::bail!("thread/read turn carries unknown status `{turn_status}`");
        }
        let started = serde_json::from_value::<owned::TurnStartedNotification>(
            serde_json::json!({"threadId": source_thread_id, "turn": turn}),
        )
        .context("thread/read turn cannot enter owned TurnStarted notification")?;
        records.push(record(
            format!("codex:{source_thread_id}:snapshot:{turn_index}:started"),
            PresentationDurability::Durable,
            source_thread_id,
            BackboneEvent::TurnStarted(started.into()),
        ));
        let items = turn
            .get("items")
            .and_then(Value::as_array)
            .context("thread/read turn misses items")?;
        for (item_index, item) in items.iter().enumerate() {
            let completed_at = item.get("completedAt").and_then(Value::as_i64).or_else(|| {
                turn.get("completedAt")
                    .and_then(Value::as_i64)
                    .and_then(|seconds| seconds.checked_mul(1_000))
            });
            let started_at = item.get("startedAt").and_then(Value::as_i64).or_else(|| {
                turn.get("startedAt")
                    .and_then(Value::as_i64)
                    .and_then(|seconds| seconds.checked_mul(1_000))
            });
            let item_status = item.get("status").and_then(Value::as_str);
            let turn_terminal = matches!(turn_status, "completed" | "failed" | "interrupted");
            let terminal = match item_status {
                Some(status) => {
                    matches!(
                        status,
                        "completed" | "failed" | "declined" | "cancelled" | "interrupted"
                    )
                }
                None => turn_terminal,
            };
            if terminal {
                let Some(completed_at_ms) = completed_at else {
                    continue;
                };
                if item.get("type").and_then(Value::as_str) == Some("contextCompaction") {
                    let Some(id) = item.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    let status = match turn_status {
                        "completed" => AgentDashCompactionStatus::Succeeded,
                        "failed" => AgentDashCompactionStatus::Failed,
                        "interrupted" => AgentDashCompactionStatus::Cancelled,
                        _ => continue,
                    };
                    let error = turn
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let canonical = AgentDashNativeThreadItem::ContextCompaction {
                        id: id.to_owned(),
                        status,
                        error,
                        started_at_ms: started_at.and_then(|value| u64::try_from(value).ok()),
                        completed_at_ms: u64::try_from(completed_at_ms).ok(),
                        context_revision: None,
                    };
                    let mut notification =
                        ItemCompletedNotification::new(canonical, source_thread_id, turn_id)?;
                    notification.completed_at_ms = completed_at_ms;
                    records.push(record(
                        format!(
                            "codex:{source_thread_id}:snapshot:{turn_index}:{item_index}:completed"
                        ),
                        PresentationDurability::Durable,
                        source_thread_id,
                        BackboneEvent::ItemCompleted(notification),
                    ));
                    continue;
                }
                let value =
                    serde_json::from_value::<owned::ItemCompletedNotification>(serde_json::json!({
                        "threadId": source_thread_id,
                        "turnId": turn_id,
                        "item": item,
                        "completedAtMs": completed_at_ms,
                    }))
                    .context("thread/read item cannot enter owned ItemCompleted notification")?;
                records.push(record(
                    format!(
                        "codex:{source_thread_id}:snapshot:{turn_index}:{item_index}:completed"
                    ),
                    PresentationDurability::Durable,
                    source_thread_id,
                    BackboneEvent::ItemCompleted(ItemCompletedNotification::from_codex(value)?),
                ));
            } else {
                let Some(started_at_ms) = started_at else {
                    continue;
                };
                let value =
                    serde_json::from_value::<owned::ItemStartedNotification>(serde_json::json!({
                        "threadId": source_thread_id,
                        "turnId": turn_id,
                        "item": item,
                        "startedAtMs": started_at_ms,
                    }))
                    .context("thread/read item cannot enter owned ItemStarted notification")?;
                records.push(record(
                    format!("codex:{source_thread_id}:snapshot:{turn_index}:{item_index}:started"),
                    PresentationDurability::Durable,
                    source_thread_id,
                    BackboneEvent::ItemStarted(ItemStartedNotification::from_codex(value)),
                ));
            }
        }
        let terminal = matches!(turn_status, "completed" | "failed" | "interrupted");
        if terminal {
            let completed = serde_json::from_value::<owned::TurnCompletedNotification>(
                serde_json::json!({"threadId": source_thread_id, "turn": turn}),
            )
            .context("thread/read turn cannot enter owned TurnCompleted notification")?;
            records.push(record(
                format!("codex:{source_thread_id}:snapshot:{turn_index}:completed"),
                PresentationDurability::Durable,
                source_thread_id,
                BackboneEvent::TurnCompleted(completed.into()),
            ));
        }
    }
    Ok(records)
}

fn record(
    presentation_id: String,
    durability: PresentationDurability,
    source_thread_id: &str,
    event: BackboneEvent,
) -> CanonicalConversationRecord {
    CanonicalConversationRecord::new(
        presentation_id,
        CanonicalConversationPresentation::new(
            durability,
            BackboneEnvelope::new(
                event,
                source_thread_id,
                SourceInfo {
                    connector_id: "codex-app-server".to_owned(),
                    connector_type: "codex".to_owned(),
                    executor_id: None,
                },
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use agentdash_agent_protocol::{
        AgentDashCompactionStatus, AgentDashItemTerminalOutcome, AgentDashNativeThreadItem,
        AgentDashThreadItem, BackboneEvent,
    };
    use serde_json::json;

    use super::{ServerNotification, notification_record, snapshot_records};

    #[test]
    fn turn_completed_emits_one_failed_compaction_terminal_before_turn_terminal() {
        let notification: ServerNotification = serde_json::from_value(json!({
            "method": "turn/completed",
            "params": {
                "threadId": "thread-1",
                "turn": {
                    "id": "turn-1",
                    "status": "failed",
                    "error": {"message": "compaction failed"},
                    "items": [{"type": "contextCompaction", "id": "compact-1"}],
                    "itemsView": "full",
                    "startedAt": 10,
                    "completedAt": 20
                }
            }
        }))
        .expect("deserialize turn/completed");

        let records =
            notification_record("thread-1", 7, &notification).expect("project notification");

        assert_eq!(records.len(), 2);
        let BackboneEvent::ItemCompleted(completed) = &records[0].presentation.envelope.event
        else {
            panic!("compaction terminal must precede turn terminal");
        };
        assert_eq!(
            completed.terminal.outcome,
            AgentDashItemTerminalOutcome::Failed
        );
        assert_eq!(
            completed.terminal.error.as_deref(),
            Some("compaction failed")
        );
        assert!(matches!(
            &completed.item,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ContextCompaction {
                status: AgentDashCompactionStatus::Failed,
                ..
            })
        ));
        assert!(matches!(
            records[1].presentation.envelope.event,
            BackboneEvent::TurnCompleted(_)
        ));
    }

    #[test]
    fn vendor_compaction_item_completed_is_suppressed_until_turn_terminal() {
        let notification: ServerNotification = serde_json::from_value(json!({
            "method": "item/completed",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "item": {"type": "contextCompaction", "id": "compact-1"},
                "completedAtMs": 20_000
            }
        }))
        .expect("deserialize item/completed");

        let records =
            notification_record("thread-1", 6, &notification).expect("project notification");

        assert!(records.is_empty());
    }

    #[test]
    fn snapshot_and_live_projection_share_cancelled_compaction_terminal_shape() {
        let records = snapshot_records(
            "thread-1",
            &json!({
                "thread": {
                    "turns": [{
                        "id": "turn-1",
                        "status": "interrupted",
                        "items": [{
                            "type": "contextCompaction",
                            "id": "compact-1"
                        }],
                        "itemsView": "full",
                        "startedAt": 10,
                        "completedAt": 20
                    }]
                }
            }),
        )
        .expect("project snapshot");

        let completed = records
            .iter()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::ItemCompleted(value) => Some(value),
                _ => None,
            })
            .expect("snapshot compaction terminal");
        assert_eq!(
            completed.terminal.outcome,
            AgentDashItemTerminalOutcome::Cancelled
        );
        assert_eq!(completed.completed_at_ms, 20_000);
        assert!(matches!(
            &completed.item,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ContextCompaction {
                status: AgentDashCompactionStatus::Cancelled,
                started_at_ms: Some(10_000),
                completed_at_ms: Some(20_000),
                ..
            })
        ));
    }
}

fn transcode<T, S>(source: &S) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
    S: Serialize,
{
    serde_json::from_value(serde_json::to_value(source)?)
}
