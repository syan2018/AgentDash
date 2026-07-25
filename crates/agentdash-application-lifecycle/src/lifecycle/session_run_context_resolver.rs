use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::workflow::{
    LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository, LifecycleSubjectAssociation,
    LifecycleSubjectAssociationRepository,
};
use agentdash_platform_spi::CapabilityScope;
use agentdash_platform_spi::hooks::SubjectRunContext;
use uuid::Uuid;

use crate::lifecycle::WorkflowApplicationError;

pub struct SubjectRunContextResolver<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    runtime_binding_repo: &'a dyn AgentRunProductRuntimeBindingRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    story_repo: &'a dyn StoryRepository,
}

impl<'a> SubjectRunContextResolver<'a> {
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        runtime_binding_repo: &'a dyn AgentRunProductRuntimeBindingRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        story_repo: &'a dyn StoryRepository,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_subject_association_repo,
            runtime_binding_repo,
            lifecycle_agent_repo,
            story_repo,
        }
    }

    /// Message stream trace → canonical runtime binding → LifecycleAgent → LifecycleRun → SubjectAssociations → context
    pub async fn resolve_from_message_stream_trace(
        &self,
        session_id: &str,
    ) -> Result<Option<SubjectRunContext>, WorkflowApplicationError> {
        let thread_id = RuntimeThreadId::new(session_id)
            .map_err(|error| WorkflowApplicationError::Conflict(error.to_string()))?;
        let Some(binding) = self
            .runtime_binding_repo
            .load_product_binding_by_runtime_thread(&thread_id)
            .await
            .map_err(|error| WorkflowApplicationError::Conflict(error.to_string()))?
        else {
            return Ok(None);
        };
        let Some(agent) = self
            .lifecycle_agent_repo
            .get(binding.target.agent_id)
            .await
            .map_err(WorkflowApplicationError::from)?
        else {
            return Ok(None);
        };
        if agent.run_id != binding.target.run_id {
            return Ok(None);
        }
        let Some(run) = self
            .lifecycle_run_repo
            .get_by_id(binding.target.run_id)
            .await
            .map_err(WorkflowApplicationError::from)?
        else {
            return Ok(None);
        };
        let mut associations = self
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, Some(agent.id))
            .await
            .map_err(WorkflowApplicationError::from)?;
        if associations.is_empty() {
            associations = self
                .lifecycle_subject_association_repo
                .list_by_anchor(run.id, None)
                .await
                .map_err(WorkflowApplicationError::from)?;
        }
        build_subject_run_context(
            run.project_id,
            &associations,
            self.lifecycle_run_repo,
            self.story_repo,
        )
        .await
        .map(Some)
    }

    pub async fn resolve_for_run(
        &self,
        run: &LifecycleRun,
    ) -> Result<SubjectRunContext, WorkflowApplicationError> {
        let associations = self
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, None)
            .await
            .map_err(WorkflowApplicationError::from)?;
        build_subject_run_context(
            run.project_id,
            &associations,
            self.lifecycle_run_repo,
            self.story_repo,
        )
        .await
    }
}

pub async fn build_subject_run_context(
    project_id: Uuid,
    associations: &[LifecycleSubjectAssociation],
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    story_repo: &dyn StoryRepository,
) -> Result<SubjectRunContext, WorkflowApplicationError> {
    if let Some(assoc) = select_association(associations, "task") {
        return task_context(
            project_id,
            assoc,
            associations,
            lifecycle_run_repo,
            story_repo,
        )
        .await;
    }

    if let Some(assoc) = select_association(associations, "story") {
        return story_context(project_id, assoc.subject_id, story_repo).await;
    }

    Ok(SubjectRunContext {
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
    task_assoc: &LifecycleSubjectAssociation,
    associations: &[LifecycleSubjectAssociation],
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    story_repo: &dyn StoryRepository,
) -> Result<SubjectRunContext, WorkflowApplicationError> {
    let task_id = task_assoc.subject_id;
    let run = lifecycle_run_repo
        .get_by_id(task_assoc.anchor_run_id)
        .await
        .map_err(WorkflowApplicationError::from)?
        .ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!(
                "Task {task_id} owning LifecycleRun {} 不存在",
                task_assoc.anchor_run_id
            ))
        })?;
    if run.project_id != project_id {
        return Err(WorkflowApplicationError::Conflict(format!(
            "Task {task_id} owning LifecycleRun 不属于当前 Project {project_id}"
        )));
    }
    let task = run.task_by_id(task_id).cloned();
    let story_id = task
        .as_ref()
        .and_then(|task| {
            task.story_ref
                .as_ref()
                .filter(|story_ref| story_ref.kind == "story")
                .map(|story_ref| story_ref.id)
        })
        .or_else(|| select_association(associations, "story").map(|assoc| assoc.subject_id));
    let story_title = if let Some(story_id) = story_id {
        story_repo
            .get_by_id(story_id)
            .await
            .map_err(WorkflowApplicationError::from)?
            .map(|story| story.title)
    } else {
        None
    };

    Ok(SubjectRunContext {
        project_id,
        story_id,
        task_id: Some(task_id),
        story_title,
        task_title: task.map(|task| task.title),
        scope: CapabilityScope::Task,
    })
}

async fn story_context(
    project_id: Uuid,
    story_id: Uuid,
    story_repo: &dyn StoryRepository,
) -> Result<SubjectRunContext, WorkflowApplicationError> {
    let story = story_repo
        .get_by_id(story_id)
        .await
        .map_err(WorkflowApplicationError::from)?;
    Ok(SubjectRunContext {
        project_id,
        story_id: Some(story_id),
        task_id: None,
        story_title: story.map(|story| story.title),
        task_title: None,
        scope: CapabilityScope::Story,
    })
}
