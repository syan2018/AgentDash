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
    pub fn from_shared_store<T>(store: Arc<T>) -> Self
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
        Self {
            meta: store.clone(),
            events: store.clone(),
            terminal_effects: store.clone(),
            runtime_commands: store.clone(),
            compactions: store.clone(),
            projections: store.clone(),
            lineage: store,
        }
    }
}
