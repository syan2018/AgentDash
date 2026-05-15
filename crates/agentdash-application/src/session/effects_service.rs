use std::io;

use super::hub::SessionHub;
use super::terminal_effects::SessionTerminalEffectDispatcher;

#[derive(Clone)]
pub struct SessionEffectsService {
    hub: SessionHub,
}

impl SessionEffectsService {
    pub(super) fn new(hub: SessionHub) -> Self {
        Self { hub }
    }

    pub async fn replay_terminal_effect_outbox(&self, limit: u32) -> io::Result<usize> {
        SessionTerminalEffectDispatcher::new(&self.hub)
            .replay_durable_outbox(limit)
            .await
    }
}
