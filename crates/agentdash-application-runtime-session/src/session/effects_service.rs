use std::io;

use super::terminal_effects::{
    SessionTerminalEffectDispatcher, TerminalCallbackDispatchInput, TerminalEffectDeps,
    TerminalEffectDispatchInput,
};

#[derive(Clone)]
pub struct SessionEffectsService {
    deps: TerminalEffectDeps,
}

impl SessionEffectsService {
    pub(crate) fn new(deps: TerminalEffectDeps) -> Self {
        Self { deps }
    }

    pub async fn replay_terminal_effect_outbox(&self, limit: u32) -> io::Result<usize> {
        SessionTerminalEffectDispatcher::new(self.deps.clone())
            .replay_durable_outbox(limit)
            .await
    }

    pub(crate) async fn dispatch_terminal_callback(&self, input: TerminalCallbackDispatchInput) {
        let dispatcher = SessionTerminalEffectDispatcher::new(self.deps.clone());
        let terminal_callback = dispatcher.enqueue_terminal_callback_effect(input).await;
        dispatcher.execute_enqueued(terminal_callback).await;
    }

    pub(crate) async fn dispatch_terminal_effects(&self, input: TerminalEffectDispatchInput) {
        let dispatcher = SessionTerminalEffectDispatcher::new(self.deps.clone());
        let terminal_effects = dispatcher.enqueue_terminal_effects(input).await;
        dispatcher.execute_enqueued(terminal_effects).await;
    }
}
