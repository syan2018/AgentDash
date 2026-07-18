use std::sync::{Arc, Mutex};

use agentdash_agent::dash::{DashToolCall, DashToolCallbacks};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentHookDecision, AgentHookInvocation,
    AgentHostCallbackError, AgentHostCallbacks, AgentSourceCoordinate, AgentToolInvocation,
    AgentToolResult,
};
use agentdash_integration_native_agent::DashAgentCoreToolCallbacks;
use async_trait::async_trait;

#[derive(Default)]
struct RecordingCallbacks {
    tools: Mutex<Vec<AgentToolInvocation>>,
}

#[async_trait]
impl AgentHostCallbacks for RecordingCallbacks {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.tools.lock().unwrap().push(call);
        Ok(AgentToolResult::Completed {
            output: serde_json::json!({"value": 7}),
        })
    }

    async fn invoke_hook(
        &self,
        _: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Ok(AgentHookDecision::Allow)
    }
}

#[tokio::test]
async fn core_tool_calls_only_cross_the_typed_host_callback_route() {
    let host = Arc::new(RecordingCallbacks::default());
    let callbacks = DashAgentCoreToolCallbacks::new(
        host.clone(),
        AgentCallbackRouteId::new("route-1").unwrap(),
        AgentBindingGeneration(7),
        AgentSourceCoordinate::new("source-1").unwrap(),
        9_000,
    );

    let result = callbacks
        .invoke(
            &agentdash_agent::dash::AgentTurnId::new("turn-1"),
            DashToolCall {
                call_id: "item-1".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "README.md"}),
            },
        )
        .await
        .unwrap();

    assert!(!result.is_error);
    let calls = host.tools.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].meta.binding_generation, AgentBindingGeneration(7));
    assert_eq!(calls[0].meta.turn_id.as_str(), "turn-1");
    assert_eq!(calls[0].meta.item_id.as_ref().unwrap().as_str(), "item-1");
    assert_eq!(calls[0].meta.effect_id.as_str(), "tool:item-1");
}
