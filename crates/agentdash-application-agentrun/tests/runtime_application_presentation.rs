use std::str::FromStr;

use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::AgentRunRuntimeApplicationPresentationProjector;

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid id")
}

fn context(
    terminal: RuntimeTurnTerminal,
    message: Option<&str>,
) -> RuntimeTerminalPresentationContext {
    RuntimeTerminalPresentationContext {
        presentation_thread_id: id("session-terminal-0001"),
        runtime_turn_id: id("runtime-turn-terminal-0001"),
        presentation_turn_id: id("turn-terminal-0001"),
        terminal,
        message: message.map(str::to_string),
        diagnostic: None,
        started_at_ms: Some(1_783_684_800_000),
        completed_at_ms: 1_783_684_801_250,
        prior_records: Vec::new(),
    }
}

fn presentation_record(
    sequence: u64,
    source_turn_id: &str,
    source_entry_index: Option<u32>,
    key: &str,
    value: serde_json::Value,
) -> RuntimeJournalRecord {
    RuntimeJournalRecord::new(
        RuntimeCarrierMetadata {
            thread_id: id("runtime-thread-terminal-0001"),
            recorded_at_ms: 1_783_684_799_000 + sequence,
            sequence: Some(EventSequence(sequence)),
            transient: None,
            revision: RuntimeRevision(sequence),
            operation_id: None,
            append_idempotency_key: None,
            binding_id: None,
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: None,
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: Some("session-terminal-0001".into()),
                source_turn_id: Some(source_turn_id.into()),
                source_item_id: None,
                source_request_id: None,
                source_entry_index,
            },
        },
        RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
            PresentationDurability::Durable,
            agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                    key: key.into(),
                    value,
                },
            ),
        )),
    )
    .expect("valid presentation record")
}

#[test]
fn completed_terminal_matches_main_meta_body_exactly() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    let events = AgentRunRuntimeApplicationPresentationProjector
        .project_terminal(context(RuntimeTurnTerminal::Completed, None))
        .expect("project completed terminal");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.durability, PresentationDurability::Durable);
    assert_eq!(
        serde_json::to_value(&events[0].event.event).expect("serialize body"),
        golden["cases"]["completed_with_timing"]
    );
}

#[test]
fn completed_terminal_without_start_time_omits_all_timing_fields_exactly() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    let mut input = context(RuntimeTurnTerminal::Completed, None);
    input.started_at_ms = None;
    let events = AgentRunRuntimeApplicationPresentationProjector
        .project_terminal(input)
        .expect("project terminal without timing");
    assert_eq!(
        serde_json::to_value(&events[0].event.event).expect("serialize body"),
        golden["cases"]["completed_without_timing"]
    );
}

#[test]
fn failed_terminal_appends_main_rewind_body_after_terminal_meta() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    let events = AgentRunRuntimeApplicationPresentationProjector
        .project_terminal(context(
            RuntimeTurnTerminal::Failed,
            Some(" provider failed "),
        ))
        .expect("project failed terminal");
    assert_eq!(events.len(), 2);
    assert_eq!(
        serde_json::to_value(&events[1].event.event).expect("serialize rewind"),
        golden["cases"]["failed_rewind"]
    );
}

#[test]
fn interrupted_and_lost_terminals_match_pinned_main_meta_and_rewind_exactly() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    for (terminal, message, meta_case, rewind_case) in [
        (
            RuntimeTurnTerminal::Interrupted,
            "cancelled",
            "interrupted_without_timing",
            "interrupted_rewind",
        ),
        (
            RuntimeTurnTerminal::Lost,
            "driver lost",
            "lost_without_timing",
            "lost_rewind",
        ),
    ] {
        let mut input = context(terminal, Some(message));
        input.started_at_ms = None;
        let events = AgentRunRuntimeApplicationPresentationProjector
            .project_terminal(input)
            .expect("project terminal");
        assert_eq!(events.len(), 2);
        assert_eq!(
            serde_json::to_value(&events[0].event.event).expect("serialize terminal meta"),
            golden["cases"][meta_case]
        );
        assert_eq!(
            serde_json::to_value(&events[1].event.event).expect("serialize terminal rewind"),
            golden["cases"][rewind_case]
        );
    }
}

#[test]
fn explicit_terminal_diagnostic_matches_main_body_exactly() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    let mut input = context(RuntimeTurnTerminal::Failed, Some("provider failed"));
    input.diagnostic = Some(agentdash_agent_protocol::RuntimeTerminalDiagnostic {
        kind: "provider".into(),
        code: Some("rate_limit".into()),
        http_status: Some(429),
        provider: Some("openai".into()),
        model: Some("gpt-5".into()),
        message: "rate limited".into(),
        retryable: true,
    });
    let events = AgentRunRuntimeApplicationPresentationProjector
        .project_terminal(input)
        .expect("project diagnostic terminal");
    assert_eq!(
        serde_json::to_value(&events[0].event.event).expect("serialize body"),
        golden["cases"]["failed_with_diagnostic"]
    );
}

#[test]
fn rewind_message_html_redaction_matches_pinned_main_writer() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    let mut input = context(
        RuntimeTurnTerminal::Failed,
        Some(" upstream failed: <html><body>gateway secret</body></html> "),
    );
    input.presentation_turn_id = id("turn-discarded");
    let events = AgentRunRuntimeApplicationPresentationProjector
        .project_terminal(input)
        .expect("project HTML failure");
    assert_eq!(
        serde_json::to_value(&events[1].event.event).expect("serialize rewind"),
        golden["cases"]["rewind_html_runtime_failure"]
    );
}

#[test]
fn rewind_long_message_matches_pinned_main_512_character_bound_exactly() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    let mut input = context(RuntimeTurnTerminal::Failed, None);
    input.presentation_turn_id = id("turn-discarded");
    input.message = Some("x".repeat(600));
    let events = AgentRunRuntimeApplicationPresentationProjector
        .project_terminal(input)
        .expect("project long failure");
    assert_eq!(
        serde_json::to_value(&events[1].event.event).expect("serialize rewind"),
        golden["cases"]["rewind_long_runtime_failure"]
    );
}

#[test]
fn rewind_stable_boundary_matches_pinned_main_writer_coordinates() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/turn-terminal.json"
    ))
    .expect("Main terminal golden");
    let mut input = context(
        RuntimeTurnTerminal::Failed,
        Some("retry after provider failure"),
    );
    input.presentation_turn_id = id("turn-discarded");
    input.prior_records = vec![
        presentation_record(
            42,
            "turn-stable",
            Some(2),
            "turn_terminal",
            serde_json::json!({"terminal_type": "turn_completed"}),
        ),
        presentation_record(
            43,
            "turn-discarded",
            Some(3),
            "non_terminal",
            serde_json::json!({}),
        ),
    ];
    let events = AgentRunRuntimeApplicationPresentationProjector
        .project_terminal(input)
        .expect("project rewind with stable boundary");
    assert_eq!(
        serde_json::to_value(&events[1].event.event).expect("serialize rewind"),
        golden["cases"]["rewind_stable_runtime_failure"]
    );
}
