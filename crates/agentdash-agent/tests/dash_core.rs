use agentdash_agent::dash::{
    AgentHistory, AgentSessionId, AgentTurnId, BranchId, DashCancellation, DashCoreContext,
    DashCoreError, DashCoreTurn, DashExecutionCallbacks, DashExecutionEvent, DashFinishReason,
    DashProvider, DashProviderEvent, DashProviderEventStream, DashProviderRequest, DashToolCall,
    DashToolCallbacks, DashToolResult, HistoryContribution, HistoryEntryId, HistoryPayload,
};
use async_trait::async_trait;
use futures::stream;

struct Provider;

#[async_trait]
impl DashProvider for Provider {
    async fn stream(
        &self,
        _: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError> {
        Ok(Box::pin(stream::iter([
            Ok(DashProviderEvent::TextDelta {
                delta: "answer".into(),
            }),
            Ok(DashProviderEvent::Completed {
                finish_reason: DashFinishReason::Stop,
                input_tokens: 2,
                output_tokens: 1,
            }),
        ])))
    }
}

struct NoTools;

#[async_trait]
impl DashToolCallbacks for NoTools {
    async fn invoke(
        &self,
        _: &AgentTurnId,
        _: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        panic!("provider did not request tools")
    }
}

struct NoopCallbacks;

#[async_trait]
impl DashExecutionCallbacks for NoopCallbacks {
    async fn emit(&self, _: DashExecutionEvent) -> Result<(), DashCoreError> {
        Ok(())
    }
}

#[tokio::test]
async fn dash_agent_maps_explicit_core_output_back_into_history() {
    let turn_id = AgentTurnId::new("turn-1");
    let mut history =
        AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
    history
        .append(HistoryContribution {
            entry_id: HistoryEntryId::new("turn-start"),
            payload: HistoryPayload::TurnStarted {
                turn_id: turn_id.clone(),
                started_at_ms: 1_000,
            },
        })
        .unwrap();

    let result = DashCoreTurn {
        turn_id: turn_id.clone(),
        input: "question".into(),
        context: DashCoreContext {
            system_prompt: "answer".into(),
            history: vec![],
            tools: vec![],
        },
        output_started_entry_id: HistoryEntryId::new("turn-output-started"),
        output_entry_id: HistoryEntryId::new("turn-output"),
        output_completed_entry_id: HistoryEntryId::new("turn-output-completed"),
        terminal_entry_id: HistoryEntryId::new("turn-complete"),
    }
    .run(&Provider, &NoTools, &NoopCallbacks, DashCancellation::new())
    .await
    .unwrap();
    history.append_batch(result.history).unwrap();

    let state = history.state().unwrap();
    let turn = state.turns.get(&turn_id).unwrap();
    assert_eq!(turn.output.as_deref(), Some("answer"));
    assert!(state.active_turn.is_none());
}
