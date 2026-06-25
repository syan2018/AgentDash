use uuid::Uuid;

use agentdash_domain::story::Story;
use agentdash_domain::workflow::SubjectRef;
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{CapabilityScopeCtx, ContextFragment, MergeStrategy};

use crate::agent_run::resolve_project_workspace;
use crate::context::{
    Contribution, contribute_workspace_static_sources, resolve_workspace_declared_sources,
};
use crate::repository_set::RepositorySet;
use crate::story::context_builder::{StoryContextBuildInput, contribute_story_context};
use crate::workspace::BackendAvailability;
use agentdash_application_vfs::VfsService;

pub(super) struct SubjectContextAssignment {
    pub workspace: Option<Workspace>,
    pub contributions: Vec<Contribution>,
    pub capability_scope: CapabilityScopeCtx,
}

pub(super) struct SubjectContextAssignmentResolver<'a> {
    repos: &'a RepositorySet,
    availability: &'a dyn BackendAvailability,
    vfs_service: &'a VfsService,
}

impl<'a> SubjectContextAssignmentResolver<'a> {
    pub fn new(
        repos: &'a RepositorySet,
        availability: &'a dyn BackendAvailability,
        vfs_service: &'a VfsService,
    ) -> Self {
        Self {
            repos,
            availability,
            vfs_service,
        }
    }

    pub async fn resolve(
        &self,
        project_id: Uuid,
        subject_ref: SubjectRef,
    ) -> Result<SubjectContextAssignment, String> {
        match subject_ref.kind.as_str() {
            "project" => self.resolve_project(project_id, subject_ref).await,
            "story" => self.resolve_story(project_id, subject_ref).await,
            "task" => self.resolve_task(project_id, subject_ref).await,
            kind => Err(format!("不支持的 subject kind: {kind}")),
        }
    }

    async fn resolve_project(
        &self,
        project_id: Uuid,
        subject_ref: SubjectRef,
    ) -> Result<SubjectContextAssignment, String> {
        if subject_ref.id != project_id {
            return Err(format!(
                "Project subject {} 不属于当前 Project {}",
                subject_ref.id, project_id
            ));
        }
        let project = self
            .repos
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Project {project_id} 不存在"))?;
        let workspace = resolve_project_workspace(self.repos, &project).await?;

        Ok(SubjectContextAssignment {
            workspace,
            contributions: Vec::new(),
            capability_scope: CapabilityScopeCtx::Project { project_id },
        })
    }

    async fn resolve_story(
        &self,
        project_id: Uuid,
        subject_ref: SubjectRef,
    ) -> Result<SubjectContextAssignment, String> {
        let story = self
            .repos
            .story_repo
            .get_by_id(subject_ref.id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Story {} 不存在", subject_ref.id))?;
        if story.project_id != project_id {
            return Err(format!(
                "Story {} 不属于当前 Project {}",
                story.id, project_id
            ));
        }
        let project = self
            .repos
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Project {project_id} 不存在"))?;
        let workspace = if let Some(workspace_id) = story.default_workspace_id {
            Some(
                self.repos
                    .workspace_repo
                    .get_by_id(workspace_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("Story 默认 Workspace {workspace_id} 不存在"))?,
            )
        } else {
            resolve_project_workspace(self.repos, &project).await?
        };
        let resolved_sources = resolve_workspace_declared_sources(
            self.availability,
            self.vfs_service,
            &story.context.source_refs,
            workspace.as_ref(),
            60,
        )
        .await
        .map_err(|error| error.to_string())?;
        let story_id = story.id;
        let contribution = contribute_story_context(StoryContextBuildInput {
            story: &story,
            project: &project,
            workspace: workspace.as_ref(),
            workspace_source_fragments: resolved_sources.fragments,
            workspace_source_warnings: resolved_sources.warnings,
        });

        Ok(SubjectContextAssignment {
            workspace,
            contributions: vec![contribution],
            capability_scope: CapabilityScopeCtx::Story {
                project_id,
                story_id,
            },
        })
    }

    async fn resolve_task(
        &self,
        project_id: Uuid,
        subject_ref: SubjectRef,
    ) -> Result<SubjectContextAssignment, String> {
        let located = crate::task::plan::find_task_plan_item_for_subject(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            project_id,
            subject_ref.id,
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Task {} 不存在", subject_ref.id))?;
        if located.run.project_id != project_id {
            return Err(format!(
                "Task {} 不属于当前 Project {}",
                located.task.id, project_id
            ));
        }
        let story = self
            .resolve_story_for_task(&located.run, located.task.story_ref.as_ref())
            .await?;
        let project = self
            .repos
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Project {project_id} 不存在"))?;
        let workspace = if let Some(story) = &story {
            if let Some(workspace_id) = story.default_workspace_id {
                Some(
                    self.repos
                        .workspace_repo
                        .get_by_id(workspace_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| format!("Story 默认 Workspace {workspace_id} 不存在"))?,
                )
            } else {
                resolve_project_workspace(self.repos, &project).await?
            }
        } else {
            resolve_project_workspace(self.repos, &project).await?
        };
        let mut declared_sources = story
            .as_ref()
            .map(|story| story.context.source_refs.clone())
            .unwrap_or_default();
        declared_sources.extend(located.task.context_refs.clone());
        let resolved_sources = resolve_workspace_declared_sources(
            self.availability,
            self.vfs_service,
            &declared_sources,
            workspace.as_ref(),
            86,
        )
        .await
        .map_err(|error| error.to_string())?;
        let task_id = located.task.id;
        let story_id = story.as_ref().map(|story| story.id);
        let mut contributions = vec![contribute_lifecycle_task_binding(&located.task)];
        if let Some(story) = &story {
            contributions.push(contribute_story_context(StoryContextBuildInput {
                story,
                project: &project,
                workspace: workspace.as_ref(),
                workspace_source_fragments: resolved_sources.fragments,
                workspace_source_warnings: resolved_sources.warnings,
            }));
        } else {
            contributions.push(contribute_workspace_static_sources(
                resolved_sources.fragments,
            ));
            if !resolved_sources.warnings.is_empty() {
                contributions.push(contribute_task_source_warnings(resolved_sources.warnings));
            }
        }

        Ok(SubjectContextAssignment {
            workspace,
            contributions,
            capability_scope: CapabilityScopeCtx::Task {
                project_id,
                story_id,
                task_id,
            },
        })
    }

    async fn resolve_story_for_task(
        &self,
        run: &agentdash_domain::workflow::LifecycleRun,
        story_ref: Option<&SubjectRef>,
    ) -> Result<Option<Story>, String> {
        let story_id = if let Some(story_ref) = story_ref {
            if story_ref.kind != "story" {
                return Ok(None);
            }
            Some(story_ref.id)
        } else {
            self.repos
                .lifecycle_subject_association_repo
                .list_by_anchor(run.id, None)
                .await
                .map_err(|error| error.to_string())?
                .into_iter()
                .filter(|assoc| assoc.subject_kind == "story")
                .min_by_key(|assoc| (role_rank(&assoc.role), assoc.created_at, assoc.id))
                .map(|assoc| assoc.subject_id)
        };

        let Some(story_id) = story_id else {
            return Ok(None);
        };
        let story = self
            .repos
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Story {story_id} 不存在"))?;
        if story.project_id != run.project_id {
            return Err(format!(
                "Task owning run {} 与 Story {} 不属于同一 Project",
                run.id, story.id
            ));
        }
        Ok(Some(story))
    }
}

fn contribute_lifecycle_task_binding(
    task: &agentdash_domain::workflow::LifecycleTaskPlanItem,
) -> Contribution {
    Contribution::fragments_only(vec![ContextFragment {
        slot: "task".to_string(),
        label: "task_plan_core".to_string(),
        order: crate::context::slot_orders::TASK_CORE,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "context_contributor:lifecycle_task_plan".to_string(),
        content: format!(
            "## Task\n- id: {}\n- title: {}\n- body: {}\n- status: {:?}",
            task.id,
            crate::context::trim_or_dash(&task.title),
            crate::context::trim_or_dash(task.body.as_deref().unwrap_or_default()),
            task.status
        ),
    }])
}

fn contribute_task_source_warnings(warnings: Vec<String>) -> Contribution {
    Contribution::fragments_only(vec![ContextFragment {
        slot: "references".to_string(),
        label: "task_source_warnings".to_string(),
        order: crate::context::slot_orders::WORKSPACE_SOURCES_WARNINGS,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "context_contributor:lifecycle_task_plan".to_string(),
        content: format!("## Injection Notes\n{}", warnings.join("\n")),
    }])
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
