use std::sync::Arc;

pub use agentdash_spi::session_persistence::{
    AgentFrameTransitionRecord, CompactionProjectionCommitResult, NewCompactionProjectionCommit,
    NewTerminalEffectRecord, PersistedSessionEvent, RuntimeCommandRecord, RuntimeCommandStatus,
    RuntimeDeliveryCommand, SessionCompactionRecord, SessionCompactionStatus,
    SessionCompactionStore, SessionEventBacklog, SessionEventPage, SessionEventStore,
    SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus, SessionLineageStore,
    SessionMeta, SessionMetaStore, SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
    SessionProjectionStore, SessionRuntimeCommandStore, SessionStoreError, SessionStoreResult,
    SessionTerminalEffectStore, TerminalEffectRecord, TerminalEffectStatus,
};

#[derive(Clone)]
pub struct SessionStoreSet {
    pub meta: Arc<dyn SessionMetaStore>,
    pub events: Arc<dyn SessionEventStore>,
    pub terminal_effects: Arc<dyn SessionTerminalEffectStore>,
    pub runtime_commands: Arc<dyn SessionRuntimeCommandStore>,
    pub compactions: Arc<dyn SessionCompactionStore>,
    pub projections: Arc<dyn SessionProjectionStore>,
    pub lineage: Arc<dyn SessionLineageStore>,
}

#[derive(Clone)]
pub(crate) struct SessionCoreStores {
    pub(crate) meta: Arc<dyn SessionMetaStore>,
}

#[derive(Clone)]
pub(crate) struct ContextProjectionStores {
    pub(crate) events: Arc<dyn SessionEventStore>,
    pub(crate) compactions: Arc<dyn SessionCompactionStore>,
    pub(crate) projections: Arc<dyn SessionProjectionStore>,
}

#[derive(Clone)]
pub(crate) struct SessionEventingStores {
    pub(crate) meta: Arc<dyn SessionMetaStore>,
    pub(crate) events: Arc<dyn SessionEventStore>,
    pub(crate) compactions: Arc<dyn SessionCompactionStore>,
    pub(crate) projections: Arc<dyn SessionProjectionStore>,
}

#[derive(Clone)]
pub(crate) struct SessionBranchingStores {
    pub(crate) meta: Arc<dyn SessionMetaStore>,
    pub(crate) events: Arc<dyn SessionEventStore>,
    pub(crate) compactions: Arc<dyn SessionCompactionStore>,
    pub(crate) projections: Arc<dyn SessionProjectionStore>,
    pub(crate) lineage: Arc<dyn SessionLineageStore>,
}

#[derive(Clone)]
pub(crate) struct SessionRuntimeStores {
    pub(crate) meta: Arc<dyn SessionMetaStore>,
    pub(crate) events: Arc<dyn SessionEventStore>,
}

#[derive(Clone)]
pub(in crate::session) struct SessionLaunchStores {
    pub(super) meta: Arc<dyn SessionMetaStore>,
    pub(super) runtime_commands: Arc<dyn SessionRuntimeCommandStore>,
}

impl SessionStoreSet {
    pub fn new(
        meta: Arc<dyn SessionMetaStore>,
        events: Arc<dyn SessionEventStore>,
        terminal_effects: Arc<dyn SessionTerminalEffectStore>,
        runtime_commands: Arc<dyn SessionRuntimeCommandStore>,
        compactions: Arc<dyn SessionCompactionStore>,
        projections: Arc<dyn SessionProjectionStore>,
        lineage: Arc<dyn SessionLineageStore>,
    ) -> Self {
        Self {
            meta,
            events,
            terminal_effects,
            runtime_commands,
            compactions,
            projections,
            lineage,
        }
    }

    pub(crate) fn core_stores(&self) -> SessionCoreStores {
        SessionCoreStores {
            meta: self.meta.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn projection_stores(&self) -> ContextProjectionStores {
        ContextProjectionStores {
            events: self.events.clone(),
            compactions: self.compactions.clone(),
            projections: self.projections.clone(),
        }
    }

    pub(crate) fn eventing_stores(&self) -> SessionEventingStores {
        SessionEventingStores {
            meta: self.meta.clone(),
            events: self.events.clone(),
            compactions: self.compactions.clone(),
            projections: self.projections.clone(),
        }
    }

    pub(crate) fn branching_stores(&self) -> SessionBranchingStores {
        SessionBranchingStores {
            meta: self.meta.clone(),
            events: self.events.clone(),
            compactions: self.compactions.clone(),
            projections: self.projections.clone(),
            lineage: self.lineage.clone(),
        }
    }

    pub(crate) fn runtime_stores(&self) -> SessionRuntimeStores {
        SessionRuntimeStores {
            meta: self.meta.clone(),
            events: self.events.clone(),
        }
    }

    pub(in crate::session) fn launch_stores(&self) -> SessionLaunchStores {
        SessionLaunchStores {
            meta: self.meta.clone(),
            runtime_commands: self.runtime_commands.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_runtime_trace_test_store<T>(store: Arc<T>) -> Self
    where
        T: SessionMetaStore
            + SessionEventStore
            + SessionTerminalEffectStore
            + SessionRuntimeCommandStore
            + SessionCompactionStore
            + SessionProjectionStore
            + SessionLineageStore
            + 'static,
    {
        Self::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store,
        )
    }
}

impl SessionEventingStores {
    pub(crate) fn projection_stores(&self) -> ContextProjectionStores {
        ContextProjectionStores {
            events: self.events.clone(),
            compactions: self.compactions.clone(),
            projections: self.projections.clone(),
        }
    }
}

impl SessionBranchingStores {
    pub(crate) fn projection_stores(&self) -> ContextProjectionStores {
        ContextProjectionStores {
            events: self.events.clone(),
            compactions: self.compactions.clone(),
            projections: self.projections.clone(),
        }
    }
}
