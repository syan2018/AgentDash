use std::sync::Arc;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::workflow::{LifecycleRun, LifecycleSubjectAssociationRepository};
use agentdash_spi::hooks::SessionRunContext;

use crate::ApplicationError;
use crate::workflow::build_session_run_context;

/// 从 LifecycleSubjectAssociation 反查 project/story/task 实体，构建 SessionRunContext。
pub struct SessionOwnerResolver {
    project_repo: Arc<dyn ProjectRepository>,
    story_repo: Arc<dyn StoryRepository>,
    lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
}

impl SessionOwnerResolver {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
        lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
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
    ) -> Result<SessionRunContext, ApplicationError> {
        let associations = self
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await
            .map_err(ApplicationError::from)?;
        build_session_run_context(run.project_id, &associations, self.story_repo.as_ref()).await
    }
}
