use std::io;

use super::hub::SessionHub;
use super::terminal_effects::{
    SessionTerminalEffectDispatcher, TerminalEffectDeps, TerminalEffectDispatchInput,
};

#[derive(Clone)]
pub struct SessionEffectsService {
    hub: SessionHub,
}

impl SessionEffectsService {
    pub(super) fn new(hub: SessionHub) -> Self {
        Self { hub }
    }

    pub async fn replay_terminal_effect_outbox(&self, limit: u32) -> io::Result<usize> {
        SessionTerminalEffectDispatcher::new(TerminalEffectDeps::from_hub(&self.hub))
            .replay_durable_outbox(limit)
            .await
    }

    pub(crate) async fn dispatch_terminal_effects(&self, input: TerminalEffectDispatchInput) {
        let dispatcher =
            SessionTerminalEffectDispatcher::new(TerminalEffectDeps::from_hub(&self.hub));
        let terminal_effects = dispatcher.enqueue_terminal_effects(input).await;
        dispatcher.execute_enqueued(terminal_effects).await;
    }
}
