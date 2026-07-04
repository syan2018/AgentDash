use std::sync::Arc;

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::routine::RoutineExecutionRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::LifecycleRunRepository;
use agentdash_spi::session_persistence::{
    SessionCompactionStore, SessionEventStore, SessionMetaStore,
};

use crate::canvas::CanvasFsMountProvider;
use crate::lifecycle::{
    LifecycleMountProvider, SessionToolResultCacheStatus as LifecycleToolResultCacheStatus,
    SessionToolResultCacheStatusKind as LifecycleToolResultCacheStatusKind,
};
use crate::session::{
    SessionToolResultCache, SessionToolResultCacheRead as RuntimeToolResultCacheRead,
    SessionToolResultCacheStatusKind as RuntimeToolResultCacheStatusKind,
};
use crate::vfs::provider::MountProviderRegistryBuilder;
use crate::vfs::{InlineFsMountProvider, RoutineMountProvider, SkillAssetFsMountProvider};
use agentdash_application_lifecycle::lifecycle::surface::journey::{
    SessionToolResultCacheReadResult, SessionToolResultCacheReader,
};

pub trait MountProviderRegistryBuilderOwnerExt {
    #[allow(clippy::too_many_arguments)]
    fn with_application_builtins(
        self,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_meta_store: Arc<dyn SessionMetaStore>,
        session_event_store: Arc<dyn SessionEventStore>,
        session_compaction_store: Arc<dyn SessionCompactionStore>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self;
}

impl MountProviderRegistryBuilderOwnerExt for MountProviderRegistryBuilder {
    #[allow(clippy::too_many_arguments)]
    fn with_application_builtins(
        self,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_meta_store: Arc<dyn SessionMetaStore>,
        session_event_store: Arc<dyn SessionEventStore>,
        session_compaction_store: Arc<dyn SessionCompactionStore>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self {
        self.register(Arc::new(InlineFsMountProvider::new(
            inline_file_repo.clone(),
        )))
        .register(Arc::new(
            LifecycleMountProvider::new_with_tool_result_cache(
                lifecycle_run_repo,
                inline_file_repo.clone(),
                skill_asset_repo.clone(),
                session_meta_store,
                session_event_store,
                session_compaction_store,
                Arc::new(RuntimeSessionToolResultCacheReader { tool_result_cache }),
            ),
        ))
        .register(Arc::new(RoutineMountProvider::new(
            routine_execution_repo,
            inline_file_repo.clone(),
        )))
        .register(Arc::new(CanvasFsMountProvider::new(canvas_repo)))
        .register(Arc::new(SkillAssetFsMountProvider::new(skill_asset_repo)))
    }
}

struct RuntimeSessionToolResultCacheReader {
    tool_result_cache: Arc<SessionToolResultCache>,
}

impl SessionToolResultCacheReader for RuntimeSessionToolResultCacheReader {
    fn read_text(&self, session_id: &str, item_id: &str) -> SessionToolResultCacheReadResult {
        match self.tool_result_cache.read_text(session_id, item_id) {
            RuntimeToolResultCacheRead::Available { metadata, text } => {
                SessionToolResultCacheReadResult::Available {
                    text,
                    stored_bytes: metadata.stored_bytes,
                    original_bytes: metadata.original_bytes,
                }
            }
            RuntimeToolResultCacheRead::Unavailable(status) => {
                SessionToolResultCacheReadResult::Unavailable(LifecycleToolResultCacheStatus {
                    status: map_tool_result_cache_status(status.status),
                    session_id: status.session_id,
                    item_id: status.item_id,
                    lifecycle_path: status.lifecycle_path,
                    message: status.message,
                })
            }
        }
    }
}

fn map_tool_result_cache_status(
    status: RuntimeToolResultCacheStatusKind,
) -> LifecycleToolResultCacheStatusKind {
    match status {
        RuntimeToolResultCacheStatusKind::Missing => LifecycleToolResultCacheStatusKind::Missing,
        RuntimeToolResultCacheStatusKind::Expired => LifecycleToolResultCacheStatusKind::Expired,
    }
}
