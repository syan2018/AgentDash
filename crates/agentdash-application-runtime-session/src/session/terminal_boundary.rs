use agentdash_agent_protocol::SourceInfo;
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectPort, AgentRunTerminalControlInput, AgentRunTerminalHookContext,
};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_spi::hooks::SharedHookRuntime;
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

#[derive(Clone)]
pub struct RuntimeTerminalBoundaryService {
    deps: RuntimeTerminalBoundaryDeps,
}

#[derive(Clone)]
pub(crate) struct RuntimeTerminalBoundaryDeps {
    pub control_effect_port: Arc<RwLock<Option<Arc<dyn AgentRunControlEffectPort>>>>,
}

impl RuntimeTerminalBoundaryService {
    pub(crate) fn new(deps: RuntimeTerminalBoundaryDeps) -> Self {
        Self { deps }
    }

    pub(crate) async fn observe_terminal_boundary(&self, input: RuntimeTerminalBoundaryEvidence) {
        let terminal_state = input.terminal_kind.state_tag().to_string();
        let terminal_hook_context =
            input
                .hook_runtime
                .clone()
                .map(|hook_runtime| AgentRunTerminalHookContext {
                    hook_runtime,
                    post_turn_handler: input.post_turn_handler.clone(),
                    source: input.source.clone(),
                });

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
                terminal_hook_context,
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
}
