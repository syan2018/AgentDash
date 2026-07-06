pub use agentdash_application_ports::agent_run_control_effect::{
    AgentRunHookEffectHandlerRegistry as TerminalHookEffectHandlerRegistry,
    AgentRunPostTurnHandler as PostTurnHandler,
    DynAgentRunHookEffectHandlerRegistry as DynTerminalHookEffectHandlerRegistry,
    DynAgentRunPostTurnHandler as DynPostTurnHandler,
    EmptyAgentRunHookEffectHandlerRegistry as EmptyTerminalHookEffectHandlerRegistry,
};
pub use agentdash_application_ports::frame_launch_envelope::TerminalHookEffectBinding;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_registry_allows_missing_handler_identity() {
        let registry = EmptyTerminalHookEffectHandlerRegistry;

        let result = registry
            .handler_for("session-1", &serde_json::json!({"handler": null}))
            .await
            .expect("missing handler identity should be valid");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn empty_registry_rejects_unknown_handler_identity() {
        let registry = EmptyTerminalHookEffectHandlerRegistry;

        let error = match registry
            .handler_for(
                "session-1",
                &serde_json::json!({"handler": {"kind": "task_effect"}}),
            )
            .await
        {
            Ok(_) => panic!("unknown handler identity should be explicit"),
            Err(error) => error,
        };

        assert!(error.contains("未注册 durable AgentRun hook effect handler"));
    }
}
