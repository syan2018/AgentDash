use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentHistoryEntry, AgentHistoryState, AgentItemId,
    HistoryPayload, ItemDetails,
};
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentDashThreadItem, BackboneEnvelope, BackboneEvent, CanonicalConversationPresentation,
    CanonicalConversationRecord, ItemCompletedNotification, ItemStartedNotification,
    ItemUpdatedNotification, PresentationDurability, SourceInfo, TraceInfo, UserInputSource,
    UserInputSubmissionKind, UserInputSubmittedNotification,
};

pub(crate) fn history_records(
    history: &AgentHistory,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut records = Vec::new();
    for entry in history.entries() {
        let state = history
            .state_at(entry.sequence)
            .expect("validated Dash history prefix must fold");
        records.extend(entry_records(&history.session_id.0, entry, &state)?);
    }
    Ok(records)
}

pub(crate) fn entry_records(
    session_id: &str,
    entry: &AgentHistoryEntry,
    state: &AgentHistoryState,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut events = Vec::new();
    match &entry.payload {
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
        HistoryPayload::TurnStarted { turn_id } => {
            events.push(BackboneEvent::TurnStarted(serde_json::from_value(
                serde_json::json!({
                    "threadId": session_id,
                    "turn": turn_json(state, turn_id)?,
                }),
            )?));
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
        }
        | HistoryPayload::ToolResult {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemUpdated(ItemUpdatedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                updated_at_ms: 0,
            }));
        }
        HistoryPayload::ItemCompleted { turn_id, item_id } => {
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                completed_at_ms: 0,
            }));
        }
        HistoryPayload::CompactionStarted { compaction_id, .. } => {
            events.push(BackboneEvent::ItemStarted(ItemStartedNotification {
                item: codex::ThreadItem::ContextCompaction {
                    id: compaction_id.0.clone(),
                }
                .into(),
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                started_at_ms: 0,
            }));
        }
        HistoryPayload::CompactionApplied { compaction_id, .. } => {
            events.push(BackboneEvent::ExecutorContextCompacted(
                codex::ContextCompactedNotification {
                    thread_id: session_id.to_owned(),
                    turn_id: compaction_id.0.clone(),
                },
            ));
        }
        HistoryPayload::CompactionCompleted { compaction_id }
        | HistoryPayload::CompactionFailed { compaction_id, .. } => {
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                item: codex::ThreadItem::ContextCompaction {
                    id: compaction_id.0.clone(),
                }
                .into(),
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                completed_at_ms: 0,
            }));
        }
        HistoryPayload::TurnCompleted { turn_id }
        | HistoryPayload::TurnFailed { turn_id, .. }
        | HistoryPayload::TurnInterrupted { turn_id } => {
            events.push(BackboneEvent::TurnCompleted(serde_json::from_value(
                serde_json::json!({
                    "threadId": session_id,
                    "turn": turn_json(state, turn_id)?,
                }),
            )?));
        }
        HistoryPayload::InitialContextInstalled { .. }
        | HistoryPayload::ItemStarted { .. }
        | HistoryPayload::InteractionRequested { .. }
        | HistoryPayload::InteractionResolved { .. }
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
        ItemDetails::ToolCall { name, arguments } => serde_json::json!({
            "type": "dynamicToolCall",
            "id": item_id.0,
            "tool": name,
            "arguments": serde_json::from_str::<serde_json::Value>(arguments)
                .unwrap_or_else(|_| serde_json::Value::String(arguments.clone())),
            "status": status(item.status),
        }),
        ItemDetails::ToolResult {
            name,
            content,
            is_error,
        } => serde_json::json!({
            "type": "dynamicToolCall",
            "id": item_id.0,
            "tool": name.as_deref().unwrap_or("unknown"),
            "arguments": {},
            "status": if *is_error { "failed" } else { status(item.status) },
            "contentItems": [{"type": "inputText", "text": content}],
            "success": !is_error,
        }),
        ItemDetails::Interaction { prompt } => serde_json::json!({
            "type": "dynamicToolCall",
            "id": item_id.0,
            "tool": "user_input",
            "arguments": {"prompt": prompt},
            "status": status(item.status),
        }),
        ItemDetails::ContextCompaction => serde_json::json!({
            "type": "contextCompaction",
            "id": item_id.0,
        }),
        ItemDetails::Pending => serde_json::json!({
            "type": "dynamicToolCall",
            "id": item_id.0,
            "tool": format!("{:?}", item.kind).to_ascii_lowercase(),
            "arguments": {},
            "status": status(item.status),
        }),
    };
    serde_json::from_value::<codex::ThreadItem>(value).map(Into::into)
}

fn turn_json(
    state: &AgentHistoryState,
    turn_id: &agentdash_agent::dash::AgentTurnId,
) -> Result<serde_json::Value, serde_json::Error> {
    let turn = state.turns.get(turn_id).expect("folded turn must exist");
    let items = state
        .items
        .iter()
        .filter(|(_, item_state)| item_state.turn_id == *turn_id)
        .map(|(item_id, _)| item(state, item_id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::json!({
        "id": turn_id.0,
        "items": items,
        "status": turn_status(turn.status),
        "startedAt": null,
        "completedAt": null,
        "durationMs": null,
        "error": null,
    }))
}

fn status(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Active => "inProgress",
        ActivityStatus::Completed => "completed",
        ActivityStatus::Failed | ActivityStatus::Lost | ActivityStatus::Interrupted => "failed",
    }
}

fn turn_status(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Active => "inProgress",
        ActivityStatus::Completed => "completed",
        ActivityStatus::Failed | ActivityStatus::Lost => "failed",
        ActivityStatus::Interrupted => "interrupted",
    }
}

fn turn_id(payload: &HistoryPayload) -> Option<&str> {
    match payload {
        HistoryPayload::TurnStarted { turn_id }
        | HistoryPayload::ItemStarted { turn_id, .. }
        | HistoryPayload::ItemCompleted { turn_id, .. }
        | HistoryPayload::AgentOutput { turn_id, .. }
        | HistoryPayload::ToolCall { turn_id, .. }
        | HistoryPayload::ToolResult { turn_id, .. }
        | HistoryPayload::InteractionRequested { turn_id, .. }
        | HistoryPayload::TurnCompleted { turn_id }
        | HistoryPayload::TurnFailed { turn_id, .. }
        | HistoryPayload::TurnInterrupted { turn_id } => Some(&turn_id.0),
        HistoryPayload::CompactionStarted { compaction_id, .. }
        | HistoryPayload::CompactionApplied { compaction_id, .. }
        | HistoryPayload::CompactionCompleted { compaction_id }
        | HistoryPayload::CompactionFailed { compaction_id, .. } => Some(&compaction_id.0),
        HistoryPayload::InitialContextInstalled { .. }
        | HistoryPayload::InputAccepted { .. }
        | HistoryPayload::InteractionResolved { .. }
        | HistoryPayload::Closed => None,
    }
}
