use agentdash_agent_protocol::SourceInfo;
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectPort, AgentRunTerminalControlInput, AgentRunTerminalHookEffects,
};
use agentdash_application_ports::frame_launch_envelope::TerminalHookEffectBinding;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_spi::hooks::{
    HookEffect, HookRuntimeAccess, HookTraceTrigger, HookTrigger, SharedHookRuntime,
};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::hub_support::TurnTerminalKind;
use super::post_turn_handler::DynPostTurnHandler;

pub(crate) struct RuntimeTerminalBoundaryEvidence {
    pub session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub terminal_kind: TurnTerminalKind,
    pub terminal_message: Option<String>,
    pub source: SourceInfo,
    pub hook_runtime: Option<SharedHookRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

pub(crate) struct RuntimeTerminalBoundaryDispatcher {
    deps: RuntimeTerminalBoundaryDeps,
}

#[derive(Clone)]
pub(crate) struct RuntimeTerminalBoundaryDeps {
    pub hook_trigger: Arc<dyn TerminalHookTriggerPort>,
    pub control_effect_port: Arc<RwLock<Option<Arc<dyn AgentRunControlEffectPort>>>>,
}

pub(crate) struct TerminalHookTriggerRequest<'a> {
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub trigger: HookTrigger,
    pub payload: Option<serde_json::Value>,
    pub refresh_reason: &'static str,
    pub source: SourceInfo,
}

#[async_trait::async_trait]
pub(crate) trait TerminalHookTriggerPort: Send + Sync {
    async fn emit_terminal_hook_trigger(
        &self,
        hook_runtime: &dyn HookRuntimeAccess,
        input: TerminalHookTriggerRequest<'_>,
    ) -> Vec<HookEffect>;
}

impl RuntimeTerminalBoundaryDispatcher {
    pub fn new(deps: RuntimeTerminalBoundaryDeps) -> Self {
        Self { deps }
    }

    pub async fn observe_terminal_boundary(&self, input: RuntimeTerminalBoundaryEvidence) {
        let terminal_state = input.terminal_kind.state_tag().to_string();
        let terminal_hook_outputs = self
            .collect_terminal_hook_outputs(&input, &terminal_state)
            .await;
        let before_stop_continue_observed =
            observed_before_stop_continue(input.terminal_kind, input.hook_runtime.as_ref());

        let Some(port) = self.deps.control_effect_port.read().await.clone() else {
            let context = DiagnosticErrorContext::new("session.terminal_control", "missing_port");
            diag_error!(
                Warn,
                Subsystem::AgentRun,
                context = &context,
                error = &std::io::Error::other("AgentRun control effect port 未注入"),
                session_id = %input.session_id,
                turn_id = %input.turn_id,
                terminal_event_seq = input.terminal_event_seq,
                terminal_state = %terminal_state,
                "RuntimeSession terminal evidence 未能交给 AgentRun control effect intake"
            );
            return;
        };

        if let Err(error) = port
            .observe_runtime_terminal(AgentRunTerminalControlInput {
                delivery_runtime_session_id: input.session_id.clone(),
                turn_id: input.turn_id.clone(),
                terminal_event_seq: input.terminal_event_seq,
                terminal_state: terminal_state.clone(),
                terminal_message: input.terminal_message.clone(),
                terminal_hook_outputs,
                before_stop_continue_observed,
            })
            .await
        {
            let context = DiagnosticErrorContext::new("session.terminal_control", "intake");
            diag_error!(
                Warn,
                Subsystem::AgentRun,
                context = &context,
                error = &std::io::Error::other(error),
                session_id = %input.session_id,
                turn_id = %input.turn_id,
                terminal_event_seq = input.terminal_event_seq,
                terminal_state = %terminal_state,
                "AgentRun control effect intake 处理 RuntimeSession terminal evidence 失败"
            );
        }
    }

    async fn collect_terminal_hook_outputs(
        &self,
        input: &RuntimeTerminalBoundaryEvidence,
        terminal_state: &str,
    ) -> Option<AgentRunTerminalHookEffects> {
        let hook_runtime = input.hook_runtime.as_ref()?;
        let effects = self
            .deps
            .hook_trigger
            .emit_terminal_hook_trigger(
                hook_runtime.as_ref(),
                TerminalHookTriggerRequest {
                    session_id: &input.session_id,
                    turn_id: Some(&input.turn_id),
                    trigger: HookTrigger::SessionTerminal,
                    payload: Some(serde_json::json!({
                        "terminal_state": terminal_state,
                        "message": input.terminal_message.clone(),
                    })),
                    refresh_reason: "trigger:session_terminal",
                    source: input.source.clone(),
                },
            )
            .await;
        if effects.is_empty() {
            return None;
        }

        let durable_binding =
            input
                .post_turn_handler
                .as_ref()
                .map(|handler| TerminalHookEffectBinding {
                    handler: handler
                        .durable_effect_handler()
                        .unwrap_or(serde_json::Value::Null),
                    supported_effect_kinds: handler
                        .supported_effect_kinds()
                        .iter()
                        .map(|kind| (*kind).to_string())
                        .collect(),
                });

        Some(AgentRunTerminalHookEffects {
            control_target: Some(hook_runtime.control_target()),
            effects,
            handler: input.post_turn_handler.clone(),
            durable_binding,
        })
    }
}

fn observed_before_stop_continue(
    terminal_kind: TurnTerminalKind,
    hook_runtime: Option<&SharedHookRuntime>,
) -> bool {
    matches!(terminal_kind, TurnTerminalKind::Completed)
        && hook_runtime.as_ref().is_some_and(|hook_runtime| {
            let trace = hook_runtime.trace();
            trace
                .iter()
                .rev()
                .find(|entry| matches!(entry.trigger, HookTraceTrigger::BeforeStop))
                .is_some_and(|entry| entry.decision == "continue")
        })
}
