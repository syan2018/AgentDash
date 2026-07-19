use agentdash_agent_protocol::codex_app_server_protocol as owned;
use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, CanonicalConversationPresentation,
    CanonicalConversationRecord, ItemCompletedNotification, ItemStartedNotification,
    PresentationDurability, SourceInfo,
};
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::vendor_generated::codex_v2::server_notification::ServerNotification;

pub(crate) fn notification_record(
    source_thread_id: &str,
    sequence: u64,
    notification: &ServerNotification,
) -> Result<Option<CanonicalConversationRecord>, serde_json::Error> {
    use PresentationDurability::{Durable, Ephemeral};
    use ServerNotification as Source;

    let mapped = match notification {
        Source::TurnStarted(value) => {
            Some((Durable, BackboneEvent::TurnStarted(transcode(value)?)))
        }
        Source::TurnCompleted(value) => {
            Some((Durable, BackboneEvent::TurnCompleted(transcode(value)?)))
        }
        Source::ItemStarted(value) => {
            let value: owned::ItemStartedNotification = transcode(value)?;
            Some((
                Durable,
                BackboneEvent::ItemStarted(ItemStartedNotification::from_codex(value)),
            ))
        }
        Source::ItemCompleted(value) => {
            let value: owned::ItemCompletedNotification = transcode(value)?;
            Some((
                Durable,
                BackboneEvent::ItemCompleted(ItemCompletedNotification::from_codex(value)),
            ))
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

    Ok(mapped.map(|(durability, event)| {
        CanonicalConversationRecord::new(
            format!("codex:{source_thread_id}:{sequence}"),
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
    }))
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
        let started = serde_json::from_value::<owned::TurnStartedNotification>(
            serde_json::json!({"threadId": source_thread_id, "turn": turn}),
        )
        .context("thread/read turn cannot enter owned TurnStarted notification")?;
        records.push(record(
            format!("codex:{source_thread_id}:snapshot:{turn_index}:started"),
            PresentationDurability::Durable,
            source_thread_id,
            BackboneEvent::TurnStarted(started),
        ));
        let items = turn
            .get("items")
            .and_then(Value::as_array)
            .context("thread/read turn misses items")?;
        for (item_index, item) in items.iter().enumerate() {
            let completed_at = item
                .get("completedAt")
                .and_then(Value::as_i64)
                .or_else(|| turn.get("completedAt").and_then(Value::as_i64));
            let started_at = item
                .get("startedAt")
                .and_then(Value::as_i64)
                .or_else(|| turn.get("startedAt").and_then(Value::as_i64));
            let terminal = item
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| {
                    matches!(
                        status,
                        "completed" | "failed" | "declined" | "cancelled" | "interrupted"
                    )
                });
            if terminal {
                let completed_at_ms =
                    completed_at.context("terminal thread/read item misses completedAt")?;
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
                    BackboneEvent::ItemCompleted(ItemCompletedNotification::from_codex(value)),
                ));
            } else {
                let started_at_ms =
                    started_at.context("active thread/read item misses startedAt")?;
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
        let terminal = turn
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "completed" | "failed" | "interrupted"));
        if terminal {
            let completed = serde_json::from_value::<owned::TurnCompletedNotification>(
                serde_json::json!({"threadId": source_thread_id, "turn": turn}),
            )
            .context("thread/read turn cannot enter owned TurnCompleted notification")?;
            records.push(record(
                format!("codex:{source_thread_id}:snapshot:{turn_index}:completed"),
                PresentationDurability::Durable,
                source_thread_id,
                BackboneEvent::TurnCompleted(completed),
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

fn transcode<T, S>(source: &S) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
    S: Serialize,
{
    serde_json::from_value(serde_json::to_value(source)?)
}
