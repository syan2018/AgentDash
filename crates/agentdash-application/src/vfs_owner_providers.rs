use std::sync::Arc;

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::routine::RoutineExecutionRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::LifecycleRunRepository;

use crate::canvas::CanvasFsMountProvider;
use crate::lifecycle::LifecycleMountProvider;
use crate::session::{SessionPersistence, SessionToolResultCache};
use crate::vfs::provider::MountProviderRegistryBuilder;
use crate::vfs::{InlineFsMountProvider, RoutineMountProvider, SkillAssetFsMountProvider};

pub trait MountProviderRegistryBuilderOwnerExt {
    fn with_application_builtins(
        self,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self;
}

impl MountProviderRegistryBuilderOwnerExt for MountProviderRegistryBuilder {
    fn with_application_builtins(
        self,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
        _tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self {
        self.register(Arc::new(InlineFsMountProvider::new(
            inline_file_repo.clone(),
        )))
        .register(Arc::new(LifecycleMountProvider::new(
            lifecycle_run_repo,
            inline_file_repo.clone(),
            skill_asset_repo.clone(),
            session_persistence,
        )))
        .register(Arc::new(RoutineMountProvider::new(
            routine_execution_repo,
            inline_file_repo.clone(),
        )))
        .register(Arc::new(CanvasFsMountProvider::new(canvas_repo)))
        .register(Arc::new(SkillAssetFsMountProvider::new(skill_asset_repo)))
    }
}
