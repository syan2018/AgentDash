use agentdash_agent_protocol::{BackboneEvent, PlatformEvent, SessionRewindReason, SessionRewound};
use agentdash_agent_runtime_contract::{
    ImmutablePresentationEvent, PresentationDurability,
    RuntimeApplicationPresentationProjectionError, RuntimeApplicationPresentationProjector,
    RuntimeJournalFact, RuntimePresentationCoordinate, RuntimePresentationInput,
    RuntimeTerminalPresentationContext, RuntimeTurnTerminal,
};

#[derive(Debug, Default)]
pub struct AgentRunRuntimeApplicationPresentationProjector;

impl RuntimeApplicationPresentationProjector for AgentRunRuntimeApplicationPresentationProjector {
    fn project_terminal(
        &self,
        context: RuntimeTerminalPresentationContext,
    ) -> Result<Vec<RuntimePresentationInput>, RuntimeApplicationPresentationProjectionError> {
        let terminal_type = terminal_type(context.terminal);
        let mut terminal_value = serde_json::json!({
            "terminal_type": terminal_type,
            "message": context.message,
            "diagnostic": context.diagnostic,
        });
        if let Some(started_at_ms) = context.started_at_ms {
            let completed_at_ms = context.completed_at_ms.max(started_at_ms);
            let value = terminal_value
                .as_object_mut()
                .expect("terminal value object");
            value.insert("started_at_ms".into(), started_at_ms.into());
            value.insert("completed_at_ms".into(), completed_at_ms.into());
            value.insert(
                "duration_ms".into(),
                completed_at_ms.saturating_sub(started_at_ms).into(),
            );
        }
        let coordinate = RuntimePresentationCoordinate {
            runtime_turn_id: Some(context.runtime_turn_id.clone()),
            runtime_item_id: None,
            interaction_id: None,
            source_thread_id: Some(context.presentation_thread_id.to_string()),
            source_turn_id: Some(context.presentation_turn_id.to_string()),
            source_item_id: None,
            source_request_id: Some(format!(
                "turn-terminal:{}:{terminal_type}",
                context.runtime_turn_id
            )),
            source_entry_index: None,
        };
        let mut events = vec![RuntimePresentationInput {
            coordinate: coordinate.clone(),
            event: ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "turn_terminal".into(),
                    value: terminal_value,
                }),
            ),
        }];
        if requires_rewind(context.terminal) {
            let (stable_event_seq, stable_turn_id) = latest_stable_terminal(&context);
            let discarded_entry_index = context
                .prior_records
                .iter()
                .filter(|record| {
                    record.carrier().coordinate.source_turn_id.as_deref()
                        == Some(context.presentation_turn_id.as_str())
                })
                .filter_map(|record| record.carrier().coordinate.source_entry_index)
                .max();
            events.push(RuntimePresentationInput {
                coordinate,
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    BackboneEvent::Platform(PlatformEvent::SessionRewound(SessionRewound {
                        discarded_turn_id: context.presentation_turn_id.to_string(),
                        discarded_entry_index,
                        stable_event_seq,
                        stable_turn_id,
                        reason: SessionRewindReason::RuntimeFailure,
                        replacement_turn_id: None,
                        message: bounded_rewind_message(context.message),
                    })),
                ),
            });
        }
        Ok(events)
    }
}

fn terminal_type(terminal: RuntimeTurnTerminal) -> &'static str {
    match terminal {
        RuntimeTurnTerminal::Completed => "turn_completed",
        RuntimeTurnTerminal::Interrupted => "turn_interrupted",
        RuntimeTurnTerminal::Lost => "turn_lost",
        RuntimeTurnTerminal::Refused
        | RuntimeTurnTerminal::LimitReached
        | RuntimeTurnTerminal::Failed => "turn_failed",
    }
}

fn requires_rewind(terminal: RuntimeTurnTerminal) -> bool {
    !matches!(terminal, RuntimeTurnTerminal::Completed)
}

fn latest_stable_terminal(context: &RuntimeTerminalPresentationContext) -> (u64, Option<String>) {
    context
        .prior_records
        .iter()
        .rev()
        .find_map(|record| {
            let RuntimeJournalFact::Presentation(event) = record.fact() else {
                return None;
            };
            let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
                &event.event
            else {
                return None;
            };
            (key == "turn_terminal"
                && value
                    .get("terminal_type")
                    .and_then(serde_json::Value::as_str)
                    == Some("turn_completed"))
            .then(|| {
                (
                    record
                        .carrier()
                        .sequence
                        .as_ref()
                        .map(|sequence| sequence.0)
                        .unwrap_or_default(),
                    record.carrier().coordinate.source_turn_id.clone(),
                )
            })
        })
        .unwrap_or((0, None))
}

fn bounded_rewind_message(message: Option<String>) -> Option<String> {
    const LIMIT: usize = 512;
    let collapsed = message?.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(html_start) = lower
        .find("<html")
        .or_else(|| lower.find("<!doctype"))
        .or_else(|| lower.find("<head"))
    {
        let prefix = trimmed[..html_start].trim().trim_end_matches(':').trim();
        return Some(if prefix.is_empty() {
            "HTML error response body omitted".into()
        } else {
            format!("{prefix}; HTML error response body omitted")
        });
    }
    if trimmed.chars().count() <= LIMIT {
        return Some(trimmed.to_string());
    }
    let mut bounded = trimmed.chars().take(LIMIT).collect::<String>();
    bounded.push_str("...");
    Some(bounded)
}
