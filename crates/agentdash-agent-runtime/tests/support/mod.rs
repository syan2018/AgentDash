use agentdash_agent_runtime_contract::{
    ImmutablePresentationEvent, PresentationDurability,
    RuntimeApplicationPresentationProjectionError, RuntimeApplicationPresentationProjector,
    RuntimePresentationCoordinate, RuntimePresentationInput, RuntimeTerminalPresentationContext,
    RuntimeTurnTerminal,
};

#[derive(Debug, Default)]
pub struct TestTerminalPresentationProjector;

impl RuntimeApplicationPresentationProjector for TestTerminalPresentationProjector {
    fn project_terminal(
        &self,
        context: RuntimeTerminalPresentationContext,
    ) -> Result<Vec<RuntimePresentationInput>, RuntimeApplicationPresentationProjectionError> {
        let terminal_type = match context.terminal {
            RuntimeTurnTerminal::Completed => "turn_completed",
            RuntimeTurnTerminal::Interrupted => "turn_interrupted",
            RuntimeTurnTerminal::Lost => "turn_lost",
            RuntimeTurnTerminal::Refused
            | RuntimeTurnTerminal::LimitReached
            | RuntimeTurnTerminal::Failed => "turn_failed",
        };
        let mut value = serde_json::json!({
            "terminal_type": terminal_type,
            "message": context.message,
            "diagnostic": context.diagnostic,
        });
        if let Some(started_at_ms) = context.started_at_ms {
            let completed_at_ms = context.completed_at_ms.max(started_at_ms);
            let value = value.as_object_mut().expect("终态值必须是对象");
            value.insert("started_at_ms".into(), started_at_ms.into());
            value.insert("completed_at_ms".into(), completed_at_ms.into());
            value.insert(
                "duration_ms".into(),
                completed_at_ms.saturating_sub(started_at_ms).into(),
            );
        }
        Ok(vec![RuntimePresentationInput {
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: Some(context.runtime_turn_id.clone()),
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: Some(context.presentation_thread_id.to_string()),
                source_turn_id: Some(context.presentation_turn_id.to_string()),
                source_item_id: None,
                source_request_id: Some(format!(
                    "test-turn-terminal:{}:{terminal_type}",
                    context.runtime_turn_id
                )),
                source_entry_index: None,
            },
            event: ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                        key: "turn_terminal".into(),
                        value,
                    },
                ),
            ),
        }])
    }
}
