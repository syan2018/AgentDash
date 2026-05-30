use agentdash_domain::story::StoryRepository;
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleRunLink, LifecycleRunLinkRepository, LifecycleRunRepository,
    RunLinkRole, RunLinkSubjectKind,
};
use agentdash_spi::CapabilityScope;
use agentdash_spi::hooks::SessionRunContext;
use uuid::Uuid;

use crate::ApplicationError;
use crate::workflow::select_active_run;

pub struct SessionRunContextResolver<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_run_link_repo: &'a dyn LifecycleRunLinkRepository,
    story_repo: &'a dyn StoryRepository,
}

impl<'a> SessionRunContextResolver<'a> {
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_run_link_repo: &'a dyn LifecycleRunLinkRepository,
        story_repo: &'a dyn StoryRepository,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_run_link_repo,
            story_repo,
        }
    }

    pub async fn resolve_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionRunContext>, ApplicationError> {
        let runs = self
            .lifecycle_run_repo
            .list_by_session(session_id)
            .await
            .map_err(ApplicationError::from)?;
        let Some(run) = choose_session_run(runs) else {
            return Ok(None);
        };
        self.resolve_for_run(&run).await.map(Some)
    }

    pub async fn resolve_for_run(
        &self,
        run: &LifecycleRun,
    ) -> Result<SessionRunContext, ApplicationError> {
        let links = self
            .lifecycle_run_link_repo
            .list_by_run(run.id)
            .await
            .map_err(ApplicationError::from)?;
        build_session_run_context(run.project_id, &links, self.story_repo).await
    }
}

pub async fn build_session_run_context(
    project_id: Uuid,
    links: &[LifecycleRunLink],
    story_repo: &dyn StoryRepository,
) -> Result<SessionRunContext, ApplicationError> {
    if let Some(link) = select_link(links, RunLinkSubjectKind::Task) {
        return task_context(project_id, link.subject_id, story_repo).await;
    }

    if let Some(link) = select_link(links, RunLinkSubjectKind::Story) {
        return story_context(project_id, link.subject_id, story_repo).await;
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

fn choose_session_run(runs: Vec<LifecycleRun>) -> Option<LifecycleRun> {
    select_active_run(runs.clone()).or_else(|| runs.into_iter().max_by_key(|run| run.updated_at))
}

fn select_link(
    links: &[LifecycleRunLink],
    kind: RunLinkSubjectKind,
) -> Option<&LifecycleRunLink> {
    links
        .iter()
        .filter(|link| link.subject_kind == kind)
        .min_by_key(|link| (role_rank(link.role), link.created_at, link.id))
}

fn role_rank(role: RunLinkRole) -> u8 {
    match role {
        RunLinkRole::Subject => 0,
        RunLinkRole::ProjectionTarget => 1,
        RunLinkRole::ControlScope => 2,
        RunLinkRole::Source => 3,
        RunLinkRole::SpawnedBy => 4,
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
