use uuid::Uuid;

use agentdash_domain::workflow::SubjectRef;
use agentdash_domain::workspace::Workspace;
use agentdash_spi::CapabilityScopeCtx;

use crate::context::{
    Contribution, contribute_binding_initial_context, contribute_declared_sources,
    contribute_task_binding, resolve_workspace_declared_sources,
};
use crate::repository_set::RepositorySet;
use crate::session::construction_planner::resolve_project_workspace;
use crate::story::context_builder::{StoryContextBuildInput, contribute_story_context};
use crate::task::gateway::resolve_effective_task_workspace;
use crate::vfs::VfsService;
use crate::lifecycle::WorkflowApplicationError;
use crate::workspace::BackendAvailability;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectWorkspacePolicy {
    SubjectDefault,
}

pub struct SubjectContextAssignmentRequest {
    pub project_id: Uuid,
    pub subject_ref: SubjectRef,
    pub workspace_policy: SubjectWorkspacePolicy,
}

pub struct SubjectContextAssignment {
    pub subject_ref: SubjectRef,
    pub workspace: Option<Workspace>,
    pub contributions: Vec<Contribution>,
    pub capability_scope: CapabilityScopeCtx,
}

pub struct SubjectContextAssignmentResolver<'a> {
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
        request: SubjectContextAssignmentRequest,
    ) -> Result<SubjectContextAssignment, WorkflowApplicationError> {
        match request.subject_ref.kind.as_str() {
            "project" => {
                self.resolve_project(request.project_id, request.subject_ref)
                    .await
            }
            "story" => {
                self.resolve_story(request.project_id, request.subject_ref)
                    .await
            }
            "task" => {
                self.resolve_task(request.project_id, request.subject_ref)
                    .await
            }
            kind => Err(WorkflowApplicationError::BadRequest(format!(
                "不支持的 subject kind: {kind}"
            ))),
        }
    }

    async fn resolve_project(
        &self,
        project_id: Uuid,
        subject_ref: SubjectRef,
    ) -> Result<SubjectContextAssignment, WorkflowApplicationError> {
        if subject_ref.id != project_id {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "Project subject {} 不属于当前 Project {}",
                subject_ref.id, project_id
            )));
        }
        let project = self
            .repos
            .project_repo
            .get_by_id(project_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("Project {project_id} 不存在"))
            })?;
        let workspace = resolve_project_workspace(self.repos, &project)
            .await
            .map_err(WorkflowApplicationError::Internal)?;

        Ok(SubjectContextAssignment {
            subject_ref,
            workspace,
            contributions: Vec::new(),
            capability_scope: CapabilityScopeCtx::Project { project_id },
        })
    }

    async fn resolve_story(
        &self,
        project_id: Uuid,
        subject_ref: SubjectRef,
    ) -> Result<SubjectContextAssignment, WorkflowApplicationError> {
        let story = self
            .repos
            .story_repo
            .get_by_id(subject_ref.id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("Story {} 不存在", subject_ref.id))
            })?;
        if story.project_id != project_id {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "Story {} 不属于当前 Project {}",
                story.id, project_id
            )));
        }
        let project = self
            .repos
            .project_repo
            .get_by_id(project_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("Project {project_id} 不存在"))
            })?;
        let workspace = if let Some(workspace_id) = story.default_workspace_id {
            Some(
                self.repos
                    .workspace_repo
                    .get_by_id(workspace_id)
                    .await?
                    .ok_or_else(|| {
                        WorkflowApplicationError::NotFound(format!(
                            "Story 默认 Workspace {workspace_id} 不存在"
                        ))
                    })?,
            )
        } else {
            resolve_project_workspace(self.repos, &project)
                .await
                .map_err(WorkflowApplicationError::Internal)?
        };
        let resolved_sources = resolve_workspace_declared_sources(
            self.availability,
            self.vfs_service,
            &story.context.source_refs,
            workspace.as_ref(),
            60,
        )
        .await
        .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        let story_id = story.id;
        let contribution = contribute_story_context(StoryContextBuildInput {
            story: &story,
            project: &project,
            workspace: workspace.as_ref(),
            workspace_source_fragments: resolved_sources.fragments,
            workspace_source_warnings: resolved_sources.warnings,
        });

        Ok(SubjectContextAssignment {
            subject_ref,
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
    ) -> Result<SubjectContextAssignment, WorkflowApplicationError> {
        let task = crate::task::load_task(self.repos.story_repo.as_ref(), subject_ref.id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("Task {} 不存在", subject_ref.id))
            })?;
        if task.project_id != project_id {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "Task {} 不属于当前 Project {}",
                task.id, project_id
            )));
        }
        let story = self
            .repos
            .story_repo
            .get_by_id(task.story_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "Task 所属 Story {} 不存在",
                    task.story_id
                ))
            })?;
        let project = self
            .repos
            .project_repo
            .get_by_id(project_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("Project {project_id} 不存在"))
            })?;
        let workspace = resolve_effective_task_workspace(self.repos, &task, &story, &project)
            .await
            .map_err(|error| WorkflowApplicationError::BadRequest(error.to_string()))?;
        let mut declared_sources = story.context.source_refs.clone();
        declared_sources.extend(task.dispatch_preference.context_sources.clone());
        let resolved_sources = resolve_workspace_declared_sources(
            self.availability,
            self.vfs_service,
            &declared_sources,
            workspace.as_ref(),
            86,
        )
        .await
        .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        let story_id = story.id;
        let task_id = task.id;
        let story_contribution = contribute_story_context(StoryContextBuildInput {
            story: &story,
            project: &project,
            workspace: workspace.as_ref(),
            workspace_source_fragments: resolved_sources.fragments,
            workspace_source_warnings: resolved_sources.warnings,
        });
        let contributions = vec![
            contribute_task_binding(&task),
            story_contribution,
            contribute_binding_initial_context(&task),
            contribute_declared_sources(&task, &story),
        ];

        Ok(SubjectContextAssignment {
            subject_ref,
            workspace,
            contributions,
            capability_scope: CapabilityScopeCtx::Task {
                project_id,
                story_id,
                task_id,
            },
        })
    }
}
