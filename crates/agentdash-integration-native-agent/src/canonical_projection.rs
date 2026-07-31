use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentHistoryEntry, AgentHistoryReplayer, AgentHistoryState,
    AgentItemId, AgentTurnId, DashExecutionFailure, HistoryPayload, ItemDetails,
};
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentDashCompactionStatus, AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEnvelope,
    BackboneEvent, CanonicalConversationPresentation, CanonicalConversationRecord, ContextFrame,
    ContextFrameChanged, ContextFrameKind, ContextUsageSource, ItemCompletedNotification,
    ItemStartedNotification, ItemUpdatedNotification, NormalizedContextUsage, PlatformEvent,
    PresentationDurability, SourceInfo, ThreadTokenUsage, ThreadTokenUsageUpdatedNotification,
    TokenUsageBreakdown, TraceInfo, Turn, TurnCompletedNotification, TurnStartedNotification,
    UserInputSource, UserInputSubmissionKind, UserInputSubmittedNotification,
    WorkspaceModulePresentation,
};

use crate::tool_presentation::{ToolPresentationResult, project_tool_item};

pub(crate) fn history_records(
    history: &AgentHistory,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut records = Vec::new();
    let mut replay = AgentHistoryReplayer::new(history);
    for entry in history.entries() {
        let previous_surface = replay.state().surface.clone();
        let state = replay
            .apply(entry)
            .expect("validated Dash history prefix must fold");
        records.extend(entry_records(
            history,
            &history.session_id.0,
            entry,
            previous_surface.as_ref(),
            state,
        )?);
    }
    Ok(records)
}

pub(crate) fn entry_records(
    history: &AgentHistory,
    session_id: &str,
    entry: &AgentHistoryEntry,
    _previous_surface: Option<&agentdash_agent::dash::DashSurface>,
    state: &AgentHistoryState,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut events = Vec::new();
    match &entry.payload {
        HistoryPayload::InitialContextInstalled { context_frames, .. } => {
            events.extend(accepted_context_events(context_frames));
        }
        HistoryPayload::SurfaceApplied { context_frames, .. } => {
            let previous_frames = history
                .entries()
                .iter()
                .rev()
                .filter(|candidate| candidate.sequence < entry.sequence)
                .find_map(|candidate| match &candidate.payload {
                    HistoryPayload::InitialContextInstalled { context_frames, .. }
                    | HistoryPayload::SurfaceApplied { context_frames, .. }
                    | HistoryPayload::SurfaceRevoked { context_frames, .. } => {
                        Some(context_frames.as_slice())
                    }
                    _ => None,
                });
            events.extend(accepted_surface_context_events(
                previous_frames,
                context_frames,
            ));
        }
        HistoryPayload::SurfaceRevoked { context_frames, .. } => {
            let previous_frames = history
                .entries()
                .iter()
                .rev()
                .filter(|candidate| candidate.sequence < entry.sequence)
                .find_map(|candidate| match &candidate.payload {
                    HistoryPayload::InitialContextInstalled { context_frames, .. }
                    | HistoryPayload::SurfaceApplied { context_frames, .. }
                    | HistoryPayload::SurfaceRevoked { context_frames, .. } => {
                        Some(context_frames.as_slice())
                    }
                    _ => None,
                });
            events.extend(accepted_surface_context_events(
                previous_frames,
                context_frames,
            ));
        }
        HistoryPayload::ThreadNameChanged { thread_name } => {
            events.push(BackboneEvent::ThreadNameUpdated(
                codex::ThreadNameUpdatedNotification {
                    thread_id: session_id.to_owned(),
                    thread_name: Some(thread_name.clone()),
                },
            ));
        }
        HistoryPayload::InputAccepted { input_id, content } => {
            let turn_id = state
                .active_turn
                .as_ref()
                .map_or_else(|| format!("input:{input_id}"), |turn| turn.0.clone());
            events.push(BackboneEvent::UserInputSubmitted(
                UserInputSubmittedNotification::new(
                    session_id,
                    turn_id,
                    input_id,
                    UserInputSubmissionKind::Prompt,
                    UserInputSource::core_composer(),
                    agentdash_agent_protocol::text_user_input_blocks(content),
                ),
            ));
        }
        HistoryPayload::TurnStarted { turn_id, .. } => {
            events.push(BackboneEvent::TurnStarted(TurnStartedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, turn_id, None)?,
            }));
        }
        HistoryPayload::ItemStarted {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemStarted(ItemStartedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                started_at_ms: 0,
            }));
        }
        HistoryPayload::AgentOutput {
            turn_id,
            item_id: None,
            content,
        } => {
            events.push(BackboneEvent::AgentMessageDelta(
                codex::AgentMessageDeltaNotification {
                    delta: content.clone(),
                    item_id: entry.entry_id.0.clone(),
                    thread_id: session_id.to_owned(),
                    turn_id: turn_id.0.clone(),
                },
            ));
        }
        HistoryPayload::AgentOutput {
            turn_id,
            item_id: Some(item_id),
            ..
        }
        | HistoryPayload::ToolCall {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemUpdated(ItemUpdatedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                updated_at_ms: 0,
            }));
        }
        HistoryPayload::ToolResult {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemUpdated(ItemUpdatedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                updated_at_ms: 0,
            }));
            if let Some(presentation) = workspace_module_presentation(state, item_id)? {
                events.push(BackboneEvent::Platform(
                    PlatformEvent::WorkspaceModulePresentationRequested(Box::new(presentation)),
                ));
            }
        }
        HistoryPayload::ProviderUsageConfirmed {
            turn_id,
            input_tokens,
            output_tokens,
            context_window,
            ..
        } => {
            events.push(BackboneEvent::TokenUsageUpdated(
                provider_usage_notification(
                    session_id,
                    &turn_id.0,
                    *input_tokens,
                    *output_tokens,
                    state.token_usage.total_input_tokens,
                    state.token_usage.total_output_tokens,
                    *context_window,
                ),
            ));
        }
        HistoryPayload::ItemCompleted { turn_id, item_id } => {
            let item = item(state, item_id)?;
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                terminal: terminal_evidence(&item)?,
                item,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                completed_at_ms: 0,
            }));
        }
        HistoryPayload::CompactionStarted {
            compaction_id,
            started_at_ms,
            ..
        } => {
            let turn_id = AgentTurnId::new(compaction_id.0.clone());
            let item_id = AgentItemId::new(compaction_id.0.clone());
            events.push(BackboneEvent::TurnStarted(TurnStartedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, &turn_id, None)?,
            }));
            events.push(BackboneEvent::ItemStarted(ItemStartedNotification {
                item: item(state, &item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                started_at_ms: *started_at_ms as i64,
            }));
        }
        HistoryPayload::CompactionApplied {
            compaction_id,
            context_frames,
            ..
        } => {
            events.push(BackboneEvent::ExecutorContextCompacted(
                codex::ContextCompactedNotification {
                    thread_id: session_id.to_owned(),
                    turn_id: compaction_id.0.clone(),
                },
            ));
            events.extend(accepted_context_events(context_frames));
        }
        HistoryPayload::CompactionCompleted {
            compaction_id,
            completed_at_ms,
        } => {
            let turn_id = AgentTurnId::new(compaction_id.0.clone());
            let item_id = AgentItemId::new(compaction_id.0.clone());
            let item = item(state, &item_id)?;
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                terminal: terminal_evidence(&item)?,
                item,
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                completed_at_ms: *completed_at_ms as i64,
            }));
            events.push(BackboneEvent::TurnCompleted(TurnCompletedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, &turn_id, None)?,
            }));
        }
        HistoryPayload::CompactionFailed {
            compaction_id,
            error,
            lost,
            completed_at_ms,
        } => {
            let turn_id = AgentTurnId::new(compaction_id.0.clone());
            let item_id = AgentItemId::new(compaction_id.0.clone());
            let failure = DashExecutionFailure {
                code: if *lost {
                    "compaction_lost".to_owned()
                } else {
                    "compaction_failed".to_owned()
                },
                message: error.clone(),
                retryable: false,
            };
            let item = item(state, &item_id)?;
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                terminal: terminal_evidence(&item)?,
                item,
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                completed_at_ms: *completed_at_ms as i64,
            }));
            events.push(BackboneEvent::TurnCompleted(TurnCompletedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, &turn_id, Some(&failure))?,
            }));
        }
        HistoryPayload::CompactionCancelled {
            compaction_id,
            completed_at_ms,
        } => {
            let turn_id = AgentTurnId::new(compaction_id.0.clone());
            let item_id = AgentItemId::new(compaction_id.0.clone());
            let item = item(state, &item_id)?;
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                terminal: terminal_evidence(&item)?,
                item,
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                completed_at_ms: *completed_at_ms as i64,
            }));
            events.push(BackboneEvent::TurnCompleted(TurnCompletedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, &turn_id, None)?,
            }));
        }
        HistoryPayload::TurnCompleted { turn_id, .. }
        | HistoryPayload::TurnInterrupted { turn_id, .. } => {
            events.push(BackboneEvent::TurnCompleted(TurnCompletedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, turn_id, None)?,
            }));
        }
        HistoryPayload::TurnFailed { turn_id, error, .. } => {
            events.push(BackboneEvent::TurnCompleted(TurnCompletedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, turn_id, Some(error))?,
            }));
        }
        HistoryPayload::InteractionRequested { .. }
        | HistoryPayload::InteractionResolved { .. }
        | HistoryPayload::InteractionCancelled { .. }
        | HistoryPayload::CompactionQueued { .. }
        | HistoryPayload::CompactionSideEffectStarted { .. }
        | HistoryPayload::Closed => {}
    }

    Ok(events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let turn_id = turn_id(&entry.payload).map(ToOwned::to_owned);
            let envelope = BackboneEnvelope::new(
                event,
                session_id,
                SourceInfo {
                    connector_id: "dash-agent".to_owned(),
                    connector_type: "native".to_owned(),
                    executor_id: None,
                },
            )
            .with_trace(TraceInfo {
                turn_id,
                entry_index: u32::try_from(entry.sequence).ok(),
            })
            .with_observed_at_ms(0);
            CanonicalConversationRecord::new(
                format!("native:{session_id}:{}:{index}", entry.entry_id.0),
                CanonicalConversationPresentation::new(PresentationDurability::Durable, envelope),
            )
        })
        .collect())
}

fn accepted_context_events(frames: &[ContextFrame]) -> Vec<BackboneEvent> {
    frames
        .iter()
        .cloned()
        .map(|frame| {
            BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(Box::new(
                ContextFrameChanged { frame },
            )))
        })
        .collect()
}

fn terminal_evidence(
    item: &AgentDashThreadItem,
) -> Result<agentdash_agent_protocol::AgentDashItemTerminal, serde_json::Error> {
    item.terminal_evidence()
        .map_err(<serde_json::Error as serde::ser::Error>::custom)
}

fn accepted_surface_context_events(
    previous: Option<&[ContextFrame]>,
    current: &[ContextFrame],
) -> Vec<BackboneEvent> {
    let changed_frames = current.iter().filter(|frame| {
        if frame.kind == ContextFrameKind::CapabilityStateDelta {
            return true;
        }
        !previous
            .into_iter()
            .flatten()
            .find(|candidate| candidate.kind == frame.kind)
            .is_some_and(|candidate| same_surface_frame_semantics(candidate, frame))
    });
    accepted_context_events(&changed_frames.cloned().collect::<Vec<_>>())
}

fn same_surface_frame_semantics(left: &ContextFrame, right: &ContextFrame) -> bool {
    left.kind == right.kind
        && left.rendered_text == right.rendered_text
        && left.sections == right.sections
}

fn workspace_module_presentation(
    state: &AgentHistoryState,
    item_id: &AgentItemId,
) -> Result<Option<WorkspaceModulePresentation>, serde_json::Error> {
    let item = state.items.get(item_id).expect("folded item must exist");
    let ItemDetails::ToolActivity {
        result: Some(result),
        ..
    } = &item.details
    else {
        return Ok(None);
    };
    if result.is_error {
        return Ok(None);
    }
    let Some(value) = result
        .details
        .as_ref()
        .and_then(|details| details.get("workspace_module_presentation"))
    else {
        return Ok(None);
    };
    serde_json::from_value(value.clone()).map(Some)
}

fn item(
    state: &AgentHistoryState,
    item_id: &AgentItemId,
) -> Result<AgentDashThreadItem, serde_json::Error> {
    let item = state.items.get(item_id).expect("folded item must exist");
    let value = match &item.details {
        ItemDetails::AssistantMessage { content } => serde_json::json!({
            "type": "agentMessage",
            "id": item_id.0,
            "text": content,
        }),
        ItemDetails::ToolActivity {
            call_id: _,
            name,
            arguments,
            protocol_projector,
            result,
        } => {
            let arguments = serde_json::from_str::<serde_json::Value>(arguments)?;
            return project_tool_item(
                &item_id.0,
                name,
                arguments,
                protocol_projector,
                item.status == ActivityStatus::Active,
                matches!(
                    item.status,
                    ActivityStatus::Failed | ActivityStatus::Lost | ActivityStatus::Interrupted
                ),
                result.as_ref().map(|result| ToolPresentationResult {
                    content: result.content.as_slice(),
                    details: result.details.as_ref(),
                    is_error: result.is_error,
                }),
            )
            .map_err(|error| serde_json::Error::io(std::io::Error::other(error)));
        }
        ItemDetails::Interaction { prompt } => serde_json::json!({
            "type": "dynamicToolCall",
            "id": item_id.0,
            "tool": "user_input",
            "arguments": {"prompt": prompt},
            "status": status(item.status),
        }),
        ItemDetails::ContextCompaction => {
            let compaction = state
                .compactions
                .get(&agentdash_agent::dash::CompactionId::new(item_id.0.clone()))
                .ok_or_else(|| {
                    <serde_json::Error as serde::de::Error>::custom(format!(
                        "folded compaction item {} is missing compaction state",
                        item_id.0
                    ))
                })?;
            return Ok(AgentDashNativeThreadItem::ContextCompaction {
                id: item_id.0.clone(),
                status: match item.status {
                    ActivityStatus::Active => AgentDashCompactionStatus::InProgress,
                    ActivityStatus::Completed => AgentDashCompactionStatus::Succeeded,
                    ActivityStatus::Failed => AgentDashCompactionStatus::Failed,
                    ActivityStatus::Lost => AgentDashCompactionStatus::Lost,
                    ActivityStatus::Interrupted => AgentDashCompactionStatus::Cancelled,
                },
                error: compaction.error.clone(),
                started_at_ms: Some(compaction.started_at_ms),
                completed_at_ms: compaction.completed_at_ms,
                context_revision: compaction
                    .context_revision
                    .as_ref()
                    .map(|revision| revision.0.clone()),
            }
            .into());
        }
        ItemDetails::Pending => match item.kind {
            agentdash_agent::dash::ItemKind::AssistantMessage => serde_json::json!({
                "type": "agentMessage",
                "id": item_id.0,
                "text": "",
            }),
            _ => serde_json::json!({
                "type": "dynamicToolCall",
                "id": item_id.0,
                "tool": format!("{:?}", item.kind).to_ascii_lowercase(),
                "arguments": {},
                "status": status(item.status),
            }),
        },
    };
    serde_json::from_value::<codex::ThreadItem>(value).map(Into::into)
}

fn turn(
    state: &AgentHistoryState,
    turn_id: &agentdash_agent::dash::AgentTurnId,
    failure: Option<&agentdash_agent::dash::DashExecutionFailure>,
) -> Result<Turn, serde_json::Error> {
    let turn = state.turns.get(turn_id).expect("folded turn must exist");
    let items = state
        .items
        .iter()
        .filter(|(_, item_state)| item_state.turn_id == *turn_id)
        .map(|(item_id, _)| item(state, item_id))
        .collect::<Result<Vec<_>, _>>()?;
    let error = failure.map(|failure| codex::TurnError {
        message: failure.message.clone(),
        codex_error_info: None,
        additional_details: Some(Some(format!(
            "code={}; retryable={}",
            failure.code, failure.retryable
        ))),
    });
    Ok(Turn {
        id: turn_id.0.clone(),
        items,
        items_view: codex::TurnItemsView::Full,
        status: turn_status(turn.status),
        started_at: Some((turn.started_at_ms / 1_000) as i64),
        completed_at: turn
            .completed_at_ms
            .map(|completed_at_ms| (completed_at_ms / 1_000) as i64),
        duration_ms: turn
            .completed_at_ms
            .map(|completed_at_ms| completed_at_ms.saturating_sub(turn.started_at_ms) as i64),
        error,
    })
}

fn status(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Active => "inProgress",
        ActivityStatus::Completed => "completed",
        ActivityStatus::Failed | ActivityStatus::Lost | ActivityStatus::Interrupted => "failed",
    }
}

fn turn_status(status: ActivityStatus) -> codex::TurnStatus {
    match status {
        ActivityStatus::Active => codex::TurnStatus::InProgress,
        ActivityStatus::Completed => codex::TurnStatus::Completed,
        ActivityStatus::Failed | ActivityStatus::Lost => codex::TurnStatus::Failed,
        ActivityStatus::Interrupted => codex::TurnStatus::Interrupted,
    }
}

fn turn_id(payload: &HistoryPayload) -> Option<&str> {
    match payload {
        HistoryPayload::TurnStarted { turn_id, .. }
        | HistoryPayload::ItemStarted { turn_id, .. }
        | HistoryPayload::ItemCompleted { turn_id, .. }
        | HistoryPayload::AgentOutput { turn_id, .. }
        | HistoryPayload::ToolCall { turn_id, .. }
        | HistoryPayload::ToolResult { turn_id, .. }
        | HistoryPayload::ProviderUsageConfirmed { turn_id, .. }
        | HistoryPayload::InteractionRequested { turn_id, .. }
        | HistoryPayload::TurnCompleted { turn_id, .. }
        | HistoryPayload::TurnFailed { turn_id, .. }
        | HistoryPayload::TurnInterrupted { turn_id, .. } => Some(&turn_id.0),
        HistoryPayload::CompactionStarted { compaction_id, .. }
        | HistoryPayload::CompactionSideEffectStarted { compaction_id, .. }
        | HistoryPayload::CompactionApplied { compaction_id, .. }
        | HistoryPayload::CompactionCompleted { compaction_id, .. }
        | HistoryPayload::CompactionFailed { compaction_id, .. }
        | HistoryPayload::CompactionCancelled { compaction_id, .. } => Some(&compaction_id.0),
        HistoryPayload::InitialContextInstalled { .. }
        | HistoryPayload::SurfaceApplied { .. }
        | HistoryPayload::SurfaceRevoked { .. }
        | HistoryPayload::ThreadNameChanged { .. }
        | HistoryPayload::InputAccepted { .. }
        | HistoryPayload::InteractionResolved { .. }
        | HistoryPayload::InteractionCancelled { .. }
        | HistoryPayload::CompactionQueued { .. }
        | HistoryPayload::Closed => None,
    }
}

fn provider_usage_notification(
    thread_id: &str,
    turn_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    context_window: u64,
) -> ThreadTokenUsageUpdatedNotification {
    let last = token_breakdown(input_tokens, output_tokens);
    let total = token_breakdown(total_input_tokens, total_output_tokens);
    let current_context_tokens = last.total_tokens;
    let cumulative_total_tokens = total.total_tokens;
    let model_context_window = Some(saturating_i64(context_window));
    ThreadTokenUsageUpdatedNotification {
        thread_id: thread_id.to_owned(),
        turn_id: turn_id.to_owned(),
        token_usage: ThreadTokenUsage {
            total,
            last,
            model_context_window,
            context: NormalizedContextUsage {
                provider_context_tokens: current_context_tokens,
                pending_estimate_tokens: 0,
                current_context_tokens,
                cumulative_total_tokens,
                model_context_window,
                effective_context_window: model_context_window,
                reserve_tokens: 0,
                source: ContextUsageSource::Provider,
            },
        },
    }
}

fn token_breakdown(input_tokens: u64, output_tokens: u64) -> TokenUsageBreakdown {
    TokenUsageBreakdown {
        total_tokens: saturating_i64(input_tokens.saturating_add(output_tokens)),
        input_tokens: saturating_i64(input_tokens),
        cached_input_tokens: 0,
        output_tokens: saturating_i64(output_tokens),
        reasoning_output_tokens: 0,
    }
}

fn saturating_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::dash::{
        AgentSessionId, AgentTurnId, BranchId, ContextDeliveryFidelity, DashSurface,
        DashToolDefinition, HistoryContribution, HistoryEntryId, InitialContextInstallation,
        InitialContextMode, ItemKind,
    };
    use agentdash_agent_protocol::{AgentDashItemTerminalOutcome, ContextDeliveryStatus};

    fn compaction_history_with_terminal(
        id: &str,
        terminal: impl FnOnce(agentdash_agent::dash::CompactionId) -> HistoryPayload,
    ) -> AgentHistory {
        let compaction_id = agentdash_agent::dash::CompactionId::new(id);
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let source_digest = history.digest();
        history
            .append_batch(vec![
                HistoryContribution {
                    entry_id: HistoryEntryId::new(format!("{id}-started")),
                    payload: HistoryPayload::CompactionStarted {
                        compaction_id: compaction_id.clone(),
                        mode: agentdash_agent::dash::CompactionMode::Manual,
                        source_head: None,
                        source_digest,
                        started_at_ms: 1_000,
                    },
                },
                HistoryContribution {
                    entry_id: HistoryEntryId::new(format!("{id}-terminal")),
                    payload: terminal(compaction_id),
                },
            ])
            .expect("valid terminal compaction history");
        history
    }

    #[test]
    fn historical_tool_item_retains_its_accepted_projector_after_surface_revoke() {
        let surface = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: Vec::new(),
            tools: vec![DashToolDefinition {
                name: "read_document".to_owned(),
                description: "Read a document".to_owned(),
                input_schema: serde_json::json!({"type": "object"}),
                capability_key: "test/read".to_owned(),
                source: "test".to_owned(),
                tool_path: "test/read::read_document".to_owned(),
                context_usage_kind: "test_tools".to_owned(),
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::FsRead,
            }],
        };
        let turn_id = AgentTurnId::new("turn-1");
        let item_id = AgentItemId::new("item-1");
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let contributions = vec![
            HistoryPayload::SurfaceApplied {
                surface: surface.clone(),
                context_frames: Vec::new(),
            },
            HistoryPayload::TurnStarted {
                turn_id: turn_id.clone(),
                started_at_ms: 1_000,
            },
            HistoryPayload::ItemStarted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                kind: ItemKind::ToolCall,
            },
            HistoryPayload::ToolCall {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                call_id: "call-1".to_owned(),
                name: "read_document".to_owned(),
                arguments: r#"{"path":"README.md"}"#.to_owned(),
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::FsRead,
            },
            HistoryPayload::ToolResult {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                content: vec![agentdash_agent::ContentPart::text(
                    "file: README.md\n1 | first\n2 | second",
                )],
                is_error: false,
                details: None,
            },
            HistoryPayload::ItemCompleted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
            },
            HistoryPayload::TurnCompleted {
                turn_id,
                completed_at_ms: 2_000,
            },
            HistoryPayload::SurfaceRevoked {
                surface,
                context_frames: Vec::new(),
            },
        ]
        .into_iter()
        .enumerate()
        .map(|(index, payload)| HistoryContribution {
            entry_id: HistoryEntryId::new(format!("entry-{}", index + 1)),
            payload,
        })
        .collect();
        history.append_batch(contributions).expect("valid history");

        let state = history.state().expect("folded history");
        assert!(state.surface.is_none());
        let projected = item(&state, &item_id).expect("historical tool projection");
        let projected = serde_json::to_value(projected).unwrap();
        assert_eq!(projected["type"], "fsRead");
        assert_eq!(
            projected["contentItems"][0]["text"],
            "file: README.md\n1 | first\n2 | second"
        );

        let records = history_records(&history)
            .expect("canonical turn container must retain AgentDash-native items");
        let completed_turn = records
            .iter()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnCompleted(notification) => Some(&notification.turn),
                _ => None,
            })
            .expect("completed turn");
        assert_eq!(completed_turn.started_at, Some(1));
        assert_eq!(completed_turn.completed_at, Some(2));
        assert_eq!(completed_turn.duration_ms, Some(1_000));
        assert!(completed_turn.items.iter().any(|item| {
            serde_json::to_value(item).is_ok_and(|value| value["type"] == "fsRead")
        }));
    }

    #[test]
    fn surface_revoke_only_projects_semantically_changed_context_frames() {
        fn frame(id: &str, kind: ContextFrameKind, rendered_text: &str) -> ContextFrame {
            ContextFrame {
                id: id.to_owned(),
                kind,
                delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
                rendered_text: rendered_text.to_owned(),
                sections: Vec::new(),
                created_at_ms: 0,
            }
        }

        let stable_initial = frame(
            "initial:assignment",
            ContextFrameKind::AssignmentContext,
            "stable initial context",
        );
        let surface = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: Vec::new(),
            tools: Vec::new(),
        };
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        history
            .append_batch(vec![
                HistoryContribution {
                    entry_id: HistoryEntryId::new("initial"),
                    payload: HistoryPayload::InitialContextInstalled {
                        installation: InitialContextInstallation {
                            package_id: "package-1".to_owned(),
                            package_digest: "digest-1".to_owned(),
                            mode: InitialContextMode::WorkflowOnly,
                            fidelity: ContextDeliveryFidelity::TypedNative,
                            contributions: Vec::new(),
                        },
                        context_frames: vec![stable_initial],
                    },
                },
                HistoryContribution {
                    entry_id: HistoryEntryId::new("surface-applied"),
                    payload: HistoryPayload::SurfaceApplied {
                        surface: surface.clone(),
                        context_frames: vec![
                            frame(
                                "surface:1:assignment",
                                ContextFrameKind::AssignmentContext,
                                "stable initial context",
                            ),
                            frame(
                                "surface:1:capability",
                                ContextFrameKind::CapabilityStateDelta,
                                "capability added",
                            ),
                        ],
                    },
                },
                HistoryContribution {
                    entry_id: HistoryEntryId::new("surface-revoked"),
                    payload: HistoryPayload::SurfaceRevoked {
                        surface,
                        context_frames: vec![
                            frame(
                                "surface-revoke:1:assignment",
                                ContextFrameKind::AssignmentContext,
                                "stable initial context",
                            ),
                            frame(
                                "surface-revoke:1:capability",
                                ContextFrameKind::CapabilityStateDelta,
                                "capability removed",
                            ),
                        ],
                    },
                },
            ])
            .expect("valid surface lifecycle history");

        let revoked = history_records(&history)
            .expect("project canonical history")
            .into_iter()
            .filter_map(|record| match record.presentation.envelope.event {
                BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(changed))
                    if changed.frame.id.starts_with("surface-revoke:") =>
                {
                    Some(changed.frame)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(revoked.len(), 1);
        assert_eq!(revoked[0].kind, ContextFrameKind::CapabilityStateDelta);
    }

    #[test]
    fn workspace_module_presentation_is_projected_from_committed_tool_result() {
        let projector = agentdash_agent_protocol::ToolProtocolProjector::Dynamic;
        let surface = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: Vec::new(),
            tools: vec![DashToolDefinition {
                name: "workspace_module_present".to_owned(),
                description: "Present a Workspace Module".to_owned(),
                input_schema: serde_json::json!({"type": "object"}),
                capability_key: "test/workspace".to_owned(),
                source: "test".to_owned(),
                tool_path: "test/workspace::workspace_module_present".to_owned(),
                context_usage_kind: "test_tools".to_owned(),
                protocol_projector: projector.clone(),
            }],
        };
        let turn_id = AgentTurnId::new("turn-1");
        let item_id = AgentItemId::new("item-1");
        let presentation = serde_json::json!({
            "module_id": "canvas:cvs-live",
            "view_key": "default",
            "renderer_kind": "canvas",
            "presentation_uri": "canvas://cvs-live",
            "title": "Live Canvas",
        });
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let contributions = vec![
            HistoryPayload::SurfaceApplied {
                surface,
                context_frames: Vec::new(),
            },
            HistoryPayload::TurnStarted {
                turn_id: turn_id.clone(),
                started_at_ms: 1_000,
            },
            HistoryPayload::ItemStarted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                kind: ItemKind::ToolCall,
            },
            HistoryPayload::ToolCall {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                call_id: "call-1".to_owned(),
                name: "workspace_module_present".to_owned(),
                arguments: presentation.to_string(),
                protocol_projector: projector,
            },
            HistoryPayload::ToolResult {
                turn_id,
                item_id,
                content: vec![agentdash_agent::ContentPart::text("presentation requested")],
                is_error: false,
                details: Some(serde_json::json!({
                    "workspace_module_presentation": presentation,
                })),
            },
        ]
        .into_iter()
        .enumerate()
        .map(|(index, payload)| HistoryContribution {
            entry_id: HistoryEntryId::new(format!("entry-{}", index + 1)),
            payload,
        })
        .collect();
        history.append_batch(contributions).expect("valid history");

        let records = history_records(&history).expect("canonical presentation records");
        let event = records.iter().find_map(|record| {
            let BackboneEvent::Platform(PlatformEvent::WorkspaceModulePresentationRequested(
                presentation,
            )) = &record.presentation.envelope.event
            else {
                return None;
            };
            Some(presentation)
        });
        let event = event.expect("typed presentation event");
        assert_eq!(event.module_id, "canvas:cvs-live");
        assert_eq!(event.presentation_uri, "canvas://cvs-live");
    }

    #[test]
    fn compaction_projects_one_complete_canonical_turn() {
        let compaction_id = agentdash_agent::dash::CompactionId::new("compact-1");
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let source_digest = history.digest();
        let revision = agentdash_agent::dash::compaction_context_revision(
            &compaction_id,
            &source_digest,
            "summary",
            None,
        );
        let mut summary_frame = agentdash_agent::dash::accepted_compaction_summary_frame(
            &compaction_id,
            &revision,
            "summary",
            agentdash_agent::dash::CompactionMode::Manual,
            0,
            0,
            None,
            None,
            None,
            2_000,
        );
        summary_frame.id = format!(
            "compaction:{}:{}:0:compaction_summary",
            compaction_id.0, revision.0
        );
        history
            .append_batch(vec![
                HistoryContribution {
                    entry_id: HistoryEntryId::new("compact-started"),
                    payload: HistoryPayload::CompactionStarted {
                        compaction_id: compaction_id.clone(),
                        mode: agentdash_agent::dash::CompactionMode::Manual,
                        source_head: None,
                        source_digest: source_digest.clone(),
                        started_at_ms: 1_000,
                    },
                },
                HistoryContribution {
                    entry_id: HistoryEntryId::new("compact-side-effect-started"),
                    payload: HistoryPayload::CompactionSideEffectStarted {
                        compaction_id: compaction_id.clone(),
                        started_at_ms: 1_500,
                    },
                },
                HistoryContribution {
                    entry_id: HistoryEntryId::new("compact-applied"),
                    payload: HistoryPayload::CompactionApplied {
                        compaction_id: compaction_id.clone(),
                        context_revision: revision.clone(),
                        context_frames: vec![summary_frame],
                        retained_from: None,
                    },
                },
                HistoryContribution {
                    entry_id: HistoryEntryId::new("compact-completed"),
                    payload: HistoryPayload::CompactionCompleted {
                        compaction_id: compaction_id.clone(),
                        completed_at_ms: 3_000,
                    },
                },
            ])
            .expect("valid compaction history");

        let records = history_records(&history).expect("canonical compaction records");
        let event_types = records
            .iter()
            .filter_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnStarted(_) => Some("turn_started"),
                BackboneEvent::ItemStarted(_) => Some("item_started"),
                BackboneEvent::ExecutorContextCompacted(_) => Some("context_compacted"),
                BackboneEvent::ItemCompleted(_) => Some("item_completed"),
                BackboneEvent::TurnCompleted(_) => Some("turn_completed"),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            event_types,
            vec![
                "turn_started",
                "item_started",
                "context_compacted",
                "item_completed",
                "turn_completed",
            ]
        );
        let started_turn = records
            .iter()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnStarted(notification) => Some(&notification.turn),
                _ => None,
            })
            .expect("compaction turn start");
        assert_eq!(started_turn.started_at, Some(1));
        let completed_turn = records
            .iter()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnCompleted(notification) => Some(&notification.turn),
                _ => None,
            })
            .expect("compaction turn terminal");
        assert_eq!(completed_turn.id, compaction_id.0);
        assert_eq!(completed_turn.status, codex::TurnStatus::Completed);
        assert_eq!(completed_turn.items.len(), 1);
        assert_eq!(completed_turn.started_at, Some(1));
        assert_eq!(completed_turn.duration_ms, Some(2_000));
        assert!(matches!(
            completed_turn.items.as_slice(),
            [AgentDashThreadItem::AgentDash(
                AgentDashNativeThreadItem::ContextCompaction {
                    status: AgentDashCompactionStatus::Succeeded,
                    context_revision: Some(context_revision),
                    ..
                }
            )] if context_revision == &revision.0
        ));
    }

    #[test]
    fn failed_compaction_closes_the_item_and_turn_with_failed_evidence() {
        let history = compaction_history_with_terminal("compact-failed", |compaction_id| {
            HistoryPayload::CompactionFailed {
                compaction_id,
                error: "provider unavailable".to_owned(),
                lost: false,
                completed_at_ms: 3_000,
            }
        });

        let records = history_records(&history).expect("canonical compaction records");
        assert!(records.iter().any(|record| matches!(
            &record.presentation.envelope.event,
            BackboneEvent::ItemCompleted(notification)
                if matches!(
                    &notification.item,
                    AgentDashThreadItem::AgentDash(
                        AgentDashNativeThreadItem::ContextCompaction {
                            status: AgentDashCompactionStatus::Failed,
                            error: Some(error),
                            ..
                        }
                    ) if error == "provider unavailable"
                )
                && notification.terminal.outcome == AgentDashItemTerminalOutcome::Failed
        )));
        let completed_turn = records
            .iter()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnCompleted(notification) => Some(&notification.turn),
                _ => None,
            })
            .expect("failed compaction turn terminal");
        assert_eq!(completed_turn.status, codex::TurnStatus::Failed);
        assert_eq!(completed_turn.started_at, Some(1));
        assert_eq!(completed_turn.duration_ms, Some(2_000));
        assert_eq!(
            completed_turn
                .error
                .as_ref()
                .map(|error| error.message.as_str()),
            Some("provider unavailable")
        );
    }

    #[test]
    fn lost_compaction_projects_typed_terminal_item() {
        let history = compaction_history_with_terminal("compact-lost", |compaction_id| {
            HistoryPayload::CompactionFailed {
                compaction_id,
                error: "provider outcome unknown".to_owned(),
                lost: true,
                completed_at_ms: 3_000,
            }
        });

        let records = history_records(&history).expect("canonical lost compaction records");
        assert!(records.iter().any(|record| matches!(
            &record.presentation.envelope.event,
            BackboneEvent::ItemCompleted(notification)
                if matches!(
                    &notification.item,
                    AgentDashThreadItem::AgentDash(
                        AgentDashNativeThreadItem::ContextCompaction {
                            status: AgentDashCompactionStatus::Lost,
                            ..
                        }
                    )
                )
                && notification.terminal.outcome == AgentDashItemTerminalOutcome::Lost
        )));
    }

    #[test]
    fn cancelled_compaction_projects_typed_terminal_item() {
        let history = compaction_history_with_terminal("compact-cancelled", |compaction_id| {
            HistoryPayload::CompactionCancelled {
                compaction_id,
                completed_at_ms: 2_000,
            }
        });

        let records = history_records(&history).expect("canonical cancelled compaction records");
        assert!(records.iter().any(|record| matches!(
            &record.presentation.envelope.event,
            BackboneEvent::ItemCompleted(notification)
                if matches!(
                    &notification.item,
                    AgentDashThreadItem::AgentDash(
                        AgentDashNativeThreadItem::ContextCompaction {
                            status: AgentDashCompactionStatus::Cancelled,
                            ..
                        }
                    )
                )
                && notification.terminal.outcome == AgentDashItemTerminalOutcome::Cancelled
        )));
    }
}
