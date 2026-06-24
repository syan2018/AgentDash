use std::sync::Arc;

use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::routine::RoutineExecutionRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::LifecycleRunRepository;

use super::provider::MountProviderRegistryBuilder;
use crate::session::{SessionPersistence, SessionToolResultCache};

impl MountProviderRegistryBuilder {
    /// 在 generic VFS registry core 之外注册 application owner-backed providers。
    pub fn with_builtins(
        self,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self {
        self.register(Arc::new(
            super::provider_inline::InlineFsMountProvider::new(inline_file_repo.clone()),
        ))
        .register(Arc::new(
            super::provider_lifecycle::LifecycleMountProvider::new_with_tool_result_cache(
                lifecycle_run_repo,
                inline_file_repo.clone(),
                skill_asset_repo.clone(),
                session_persistence,
                tool_result_cache,
            ),
        ))
        .register(Arc::new(
            super::provider_routine::RoutineMountProvider::new(
                routine_execution_repo,
                inline_file_repo.clone(),
            ),
        ))
        .register(Arc::new(
            super::provider_canvas::CanvasFsMountProvider::new(canvas_repo),
        ))
        .register(Arc::new(
            super::provider_skill_asset::SkillAssetFsMountProvider::new(skill_asset_repo),
        ))
    }
}
