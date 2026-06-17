use std::sync::Arc;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
};
use agentdash_spi::hooks::SubjectRunContext;

use crate::ApplicationError;
use crate::lifecycle::build_subject_run_context;

/// 从 LifecycleSubjectAssociation 反查 project/story/task 实体，构建 SubjectRunContext。
pub struct SessionOwnerResolver {
    project_repo: Arc<dyn ProjectRepository>,
    story_repo: Arc<dyn StoryRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
}

impl SessionOwnerResolver {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
            lifecycle_run_repo,
            lifecycle_subject_association_repo,
        }
    }

    pub fn story_repo(&self) -> &dyn StoryRepository {
        self.story_repo.as_ref()
    }

    pub fn project_repo(&self) -> &dyn ProjectRepository {
        self.project_repo.as_ref()
    }

    pub async fn resolve_run_context(
        &self,
        run: &LifecycleRun,
    ) -> Result<SubjectRunContext, ApplicationError> {
        let associations = self
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await
            .map_err(ApplicationError::from)?;
        build_subject_run_context(
            run.project_id,
            &associations,
            self.lifecycle_run_repo.as_ref(),
            self.story_repo.as_ref(),
        )
        .await
    }
}
