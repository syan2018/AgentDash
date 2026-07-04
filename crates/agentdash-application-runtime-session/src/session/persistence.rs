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
