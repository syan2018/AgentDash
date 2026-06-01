use agentdash_domain::story::StoryRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
};
use agentdash_spi::CapabilityScope;
use agentdash_spi::hooks::SessionRunContext;
use uuid::Uuid;

use crate::ApplicationError;

pub struct SessionRunContextResolver<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    story_repo: &'a dyn StoryRepository,
}

impl<'a> SessionRunContextResolver<'a> {
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        story_repo: &'a dyn StoryRepository,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_subject_association_repo,
            agent_frame_repo,
            lifecycle_agent_repo,
            story_repo,
        }
    }

    /// RuntimeSession → AgentFrame → LifecycleAgent → LifecycleRun → SubjectAssociations → context
    pub async fn resolve_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionRunContext>, ApplicationError> {
        let Some(frame) = self
            .agent_frame_repo
            .find_by_runtime_session(session_id)
            .await
            .map_err(ApplicationError::from)?
        else {
            return Ok(None);
        };
        let Some(agent) = self
            .lifecycle_agent_repo
            .get(frame.agent_id)
            .await
            .map_err(ApplicationError::from)?
        else {
            return Ok(None);
        };
        let Some(run) = self
            .lifecycle_run_repo
            .get_by_id(agent.run_id)
            .await
            .map_err(ApplicationError::from)?
        else {
            return Ok(None);
        };
        let mut associations = self
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, Some(agent.id))
            .await
            .map_err(ApplicationError::from)?;
        if associations.is_empty() {
            associations = self
                .lifecycle_subject_association_repo
                .list_by_anchor(run.id, None)
                .await
                .map_err(ApplicationError::from)?;
        }
        build_session_run_context(run.project_id, &associations, self.story_repo)
            .await
            .map(Some)
    }

    pub async fn resolve_for_run(
        &self,
        run: &LifecycleRun,
    ) -> Result<SessionRunContext, ApplicationError> {
        let associations = self
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await
            .map_err(ApplicationError::from)?;
        build_session_run_context(run.project_id, &associations, self.story_repo).await
    }
}

pub async fn build_session_run_context(
    project_id: Uuid,
    associations: &[LifecycleSubjectAssociation],
    story_repo: &dyn StoryRepository,
) -> Result<SessionRunContext, ApplicationError> {
    if let Some(assoc) = select_association(associations, "task") {
        return task_context(project_id, assoc.subject_id, story_repo).await;
    }

    if let Some(assoc) = select_association(associations, "story") {
        return story_context(project_id, assoc.subject_id, story_repo).await;
    }

    Ok(SessionRunContext {
        project_id,
        story_id: None,
        task_id: None,
        story_title: None,
        task_title: None,
        scope: CapabilityScope::Project,
    })
}

fn select_association<'a>(
    associations: &'a [LifecycleSubjectAssociation],
    kind: &str,
) -> Option<&'a LifecycleSubjectAssociation> {
    associations
        .iter()
        .filter(|assoc| assoc.subject_kind == kind)
        .min_by_key(|assoc| (role_rank(&assoc.role), assoc.created_at, assoc.id))
}

fn role_rank(role: &str) -> u8 {
    match role {
        "subject" => 0,
        "projection_target" => 1,
        "control_scope" => 2,
        "source" => 3,
        "lineage" => 4,
        _ => 9,
    }
}

async fn task_context(
    project_id: Uuid,
    task_id: Uuid,
    story_repo: &dyn StoryRepository,
) -> Result<SessionRunContext, ApplicationError> {
    let story = story_repo
        .find_by_task_id(task_id)
        .await
        .map_err(ApplicationError::from)?;
    let (story_id, story_title, task_title) = story
        .as_ref()
        .map(|story| {
            (
                Some(story.id),
                Some(story.title.clone()),
                story.find_task(task_id).map(|task| task.title.clone()),
            )
        })
        .unwrap_or((None, None, None));

    Ok(SessionRunContext {
        project_id,
        story_id,
        task_id: Some(task_id),
        story_title,
        task_title,
        scope: CapabilityScope::Task,
    })
}

async fn story_context(
    project_id: Uuid,
    story_id: Uuid,
    story_repo: &dyn StoryRepository,
) -> Result<SessionRunContext, ApplicationError> {
    let story = story_repo
        .get_by_id(story_id)
        .await
        .map_err(ApplicationError::from)?;
    Ok(SessionRunContext {
        project_id,
        story_id: Some(story_id),
        task_id: None,
        story_title: story.map(|story| story.title),
        task_title: None,
        scope: CapabilityScope::Story,
    })
}
