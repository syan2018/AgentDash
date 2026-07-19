use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurfaceQueryError, AgentRunAppliedResourceSurfaceQueryPort,
    AppliedVfsMount, AppliedVfsOperation,
};
use agentdash_application_ports::vfs_surface_runtime::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource, VfsSurfaceRuntimeProjection,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::project::Project;
use agentdash_domain::project_vfs_mount::ProjectVfsMount;
use agentdash_domain::story::Story;
use agentdash_platform_spi::{Mount, MountCapability, Vfs};
use uuid::Uuid;

use crate::ApplicationError;
use crate::repository_set::RepositorySet;
use crate::skill_asset::SkillAssetService;
use crate::task::plan::find_task_plan_item_for_subject;

use crate::vfs::{
    SessionMountTarget, VfsService, build_project_agent_knowledge_vfs,
    build_project_skill_asset_management_mount, build_project_vfs_mount_mount,
    build_surface_summary,
};

#[derive(Clone)]
pub struct VfsSurfaceResolver {
    repos: RepositorySet,
    vfs_service: Arc<VfsService>,
    applied_resource_surfaces: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
}

#[derive(Clone)]
pub struct VfsSurfaceResolverDeps {
    pub repos: RepositorySet,
    pub vfs_service: Arc<VfsService>,
    pub applied_resource_surfaces: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
}

pub struct ResolvedVfsSurfaceBundle {
    pub surface: ResolvedVfsSurface,
    pub vfs: Vfs,
    pub project_id: Uuid,
}

impl VfsSurfaceResolver {
    pub fn new(deps: VfsSurfaceResolverDeps) -> Self {
        Self {
            repos: deps.repos,
            vfs_service: deps.vfs_service,
            applied_resource_surfaces: deps.applied_resource_surfaces,
        }
    }

    pub async fn resolve_surface(
        &self,
        runtime: &dyn VfsSurfaceRuntimeProjection,
        source: &ResolvedVfsSurfaceSource,
    ) -> Result<ResolvedVfsSurface, ApplicationError> {
        self.resolve_surface_bundle(runtime, source)
            .await
            .map(|bundle| bundle.surface)
    }

    pub async fn resolve_surface_bundle(
        &self,
        runtime: &dyn VfsSurfaceRuntimeProjection,
        source: &ResolvedVfsSurfaceSource,
    ) -> Result<ResolvedVfsSurfaceBundle, ApplicationError> {
        self.resolve_surface_bundle_with_target(runtime, source, None)
            .await
    }

    pub async fn resolve_agent_run_surface_bundle(
        &self,
        runtime: &dyn VfsSurfaceRuntimeProjection,
        source: &ResolvedVfsSurfaceSource,
        target: &AgentRunTarget,
    ) -> Result<ResolvedVfsSurfaceBundle, ApplicationError> {
        if let ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id } = source
            && (*run_id != target.run_id || *agent_id != target.agent_id)
        {
            return Err(ApplicationError::Conflict(
                "Product runtime binding 与请求的 AgentRun target 不一致".to_string(),
            ));
        }
        if !matches!(
            source,
            ResolvedVfsSurfaceSource::RuntimeThread { .. }
                | ResolvedVfsSurfaceSource::AgentRun { .. }
        ) {
            return Err(ApplicationError::BadRequest(
                "AgentRun AppliedResourceSurface 只能解析 runtime 或 AgentRun surface".to_string(),
            ));
        }
        self.resolve_surface_bundle_with_target(runtime, source, Some(target))
            .await
    }

    async fn resolve_surface_bundle_with_target(
        &self,
        runtime: &dyn VfsSurfaceRuntimeProjection,
        source: &ResolvedVfsSurfaceSource,
        bound_target: Option<&AgentRunTarget>,
    ) -> Result<ResolvedVfsSurfaceBundle, ApplicationError> {
        let (vfs, project_id) = match source {
            ResolvedVfsSurfaceSource::ProjectPreview { project_id } => {
                let project = self.load_project(*project_id).await?;
                let vfs = self
                    .build_project_vfs(&project, None, SessionMountTarget::Project)
                    .await?;
                (vfs, project.id)
            }
            ResolvedVfsSurfaceSource::StoryPreview {
                project_id,
                story_id,
            } => {
                let story = self.load_story(*story_id).await?;
                if story.project_id != *project_id {
                    return Err(ApplicationError::Conflict(
                        "story_id 与 project_id 不属于同一 Project".to_string(),
                    ));
                }
                let project = self.load_project(*project_id).await?;
                let vfs = self
                    .build_project_vfs(&project, Some(&story), SessionMountTarget::Story)
                    .await?;
                (vfs, project.id)
            }
            ResolvedVfsSurfaceSource::TaskPreview {
                project_id,
                task_id,
            } => {
                let project = self.load_project(*project_id).await?;
                let located = find_task_plan_item_for_subject(
                    self.repos.lifecycle_run_repo.as_ref(),
                    self.repos.lifecycle_subject_association_repo.as_ref(),
                    project.id,
                    *task_id,
                )
                .await?
                .ok_or_else(|| ApplicationError::NotFound(format!("Task {task_id} 不存在")))?;
                let story = if let Some(story_ref) = located
                    .task
                    .story_ref
                    .as_ref()
                    .filter(|subject| subject.kind == "story")
                {
                    Some(self.load_story(story_ref.id).await?)
                } else {
                    None
                };
                let vfs = self
                    .build_project_vfs(&project, story.as_ref(), SessionMountTarget::Task)
                    .await?;
                (vfs, project.id)
            }
            ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
                project_id,
                project_agent_id,
            } => {
                self.load_project(*project_id).await?;
                let agent = self
                    .repos
                    .project_agent_repo
                    .get_by_project_and_id(*project_id, *project_agent_id)
                    .await
                    .map_err(ApplicationError::from)?
                    .ok_or_else(|| ApplicationError::NotFound("Project Agent 不存在".into()))?;
                let vfs = build_project_agent_knowledge_vfs(&agent).map_err(|error| {
                    ApplicationError::Internal(format!("构建 Agent 知识库 VFS 失败: {error}"))
                })?;
                (vfs, *project_id)
            }
            ResolvedVfsSurfaceSource::ProjectSkillAssets { project_id } => {
                self.load_project(*project_id).await?;
                let service = SkillAssetService::new(self.repos.skill_asset_repo.as_ref());
                let keys = service
                    .list(*project_id)
                    .await?
                    .into_iter()
                    .map(|asset| asset.key)
                    .collect::<Vec<_>>();
                let vfs = Vfs {
                    mounts: vec![build_project_skill_asset_management_mount(
                        *project_id,
                        &keys,
                    )],
                    default_mount_id: Some("skill-assets".to_string()),
                    source_project_id: Some(project_id.to_string()),
                    source_story_id: None,
                    links: Vec::new(),
                };
                (vfs, *project_id)
            }
            ResolvedVfsSurfaceSource::ProjectVfsMount {
                project_id,
                mount_id,
            } => {
                self.load_project(*project_id).await?;
                let mount = self
                    .repos
                    .project_vfs_mount_repo
                    .get_by_project_and_mount_id(*project_id, mount_id)
                    .await
                    .map_err(ApplicationError::from)?
                    .ok_or_else(|| ApplicationError::NotFound("Project VFS Mount 不存在".into()))?;
                let runtime_mount = build_project_vfs_mount_mount(&mount).map_err(|error| {
                    ApplicationError::Internal(format!("构建 Project VFS Mount 失败: {error}"))
                })?;
                let default_mount_id = runtime_mount.id.clone();
                let vfs = Vfs {
                    mounts: vec![runtime_mount],
                    default_mount_id: Some(default_mount_id),
                    source_project_id: Some(project_id.to_string()),
                    source_story_id: None,
                    links: Vec::new(),
                };
                (vfs, *project_id)
            }
            ResolvedVfsSurfaceSource::RuntimeThread { .. } => {
                let target = bound_target.ok_or_else(|| {
                    ApplicationError::Conflict(
                        "runtime surface 缺少 canonical Product runtime binding".to_string(),
                    )
                })?;
                self.load_applied_agent_run_vfs(target).await?
            }
            ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id } => {
                let target = AgentRunTarget {
                    run_id: *run_id,
                    agent_id: *agent_id,
                };
                self.load_applied_agent_run_vfs(bound_target.unwrap_or(&target))
                    .await?
            }
        };

        let surface =
            build_surface_summary(self.repos.inline_file_repo.as_ref(), runtime, source, &vfs)
                .await;

        Ok(ResolvedVfsSurfaceBundle {
            surface,
            vfs,
            project_id,
        })
    }

    async fn load_applied_agent_run_vfs(
        &self,
        target: &AgentRunTarget,
    ) -> Result<(Vfs, Uuid), ApplicationError> {
        let snapshot = self
            .applied_resource_surfaces
            .applied_resource_surface(target, None)
            .await
            .map_err(applied_resource_surface_query_error)?;
        let surface = snapshot.surface;
        let project_id = surface.project_id;
        let vfs = Vfs {
            mounts: surface
                .vfs_mounts
                .into_iter()
                .map(applied_vfs_mount)
                .collect(),
            default_mount_id: surface.default_mount_id,
            source_project_id: Some(project_id.to_string()),
            source_story_id: None,
            links: Vec::new(),
        };
        Ok((vfs, project_id))
    }

    async fn load_project(&self, project_id: Uuid) -> Result<Project, ApplicationError> {
        self.repos
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| ApplicationError::NotFound(format!("Project {project_id} 不存在")))
    }

    async fn load_story(&self, story_id: Uuid) -> Result<Story, ApplicationError> {
        self.repos
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(ApplicationError::from)
            .and_then(|story| {
                story.ok_or_else(|| ApplicationError::NotFound(format!("Story {story_id} 不存在")))
            })
    }

    async fn load_project_vfs_mounts(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectVfsMount>, ApplicationError> {
        self.repos
            .project_vfs_mount_repo
            .list_by_project(project_id)
            .await
            .map_err(ApplicationError::from)
    }

    async fn build_project_vfs(
        &self,
        project: &Project,
        story: Option<&Story>,
        target: SessionMountTarget,
    ) -> Result<Vfs, ApplicationError> {
        let workspace = if let Some(workspace_id) = project.config.default_workspace_id {
            self.repos
                .workspace_repo
                .get_by_id(workspace_id)
                .await
                .map_err(ApplicationError::from)?
        } else {
            None
        };
        let project_vfs_mounts = self.load_project_vfs_mounts(project.id).await?;
        self.vfs_service
            .build_vfs(
                project,
                &project_vfs_mounts,
                story,
                workspace.as_ref(),
                target,
                None,
            )
            .map_err(|error| ApplicationError::Internal(format!("构建 VFS 失败: {error}")))
    }
}

fn applied_vfs_mount(mount: AppliedVfsMount) -> Mount {
    Mount {
        id: mount.mount_id,
        provider: mount.provider,
        backend_id: mount.backend_id,
        root_ref: mount.root_ref,
        capabilities: mount
            .capabilities
            .into_iter()
            .map(|operation| match operation {
                AppliedVfsOperation::Read => MountCapability::Read,
                AppliedVfsOperation::List => MountCapability::List,
                AppliedVfsOperation::Search => MountCapability::Search,
                AppliedVfsOperation::Write => MountCapability::Write,
                AppliedVfsOperation::Exec => MountCapability::Exec,
            })
            .collect(),
        default_write: mount.default_write,
        display_name: mount.display_name,
        metadata: mount.metadata,
    }
}

fn applied_resource_surface_query_error(
    error: AgentRunAppliedResourceSurfaceQueryError,
) -> ApplicationError {
    match error {
        AgentRunAppliedResourceSurfaceQueryError::SurfaceNotApplied
        | AgentRunAppliedResourceSurfaceQueryError::TargetMismatch
        | AgentRunAppliedResourceSurfaceQueryError::ProjectionStale { .. }
        | AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence { .. } => {
            ApplicationError::Conflict(error.to_string())
        }
        AgentRunAppliedResourceSurfaceQueryError::Repository { message } => {
            ApplicationError::Internal(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn applied_mount_maps_without_runtime_frame_fallback() {
        let mount = applied_vfs_mount(AppliedVfsMount {
            mount_id: "workspace".to_string(),
            provider: "workspace_fs".to_string(),
            backend_id: "backend-1".to_string(),
            root_ref: "project".to_string(),
            capabilities: BTreeSet::from([AppliedVfsOperation::Read, AppliedVfsOperation::Write]),
            default_write: true,
            display_name: "Workspace".to_string(),
            metadata: serde_json::json!({
                "run_id": "3f9f9df5-5916-4134-9603-c1db7cf93444",
                "agent_id": "df988c1e-6147-4f7e-a9c3-f5e8ed55f6f3",
                "orchestration_id": "cc2396a9-c45e-451b-8d67-60a148df829f",
                "node_path": "draft/review",
                "attempt": 2,
            }),
        });

        assert_eq!(mount.id, "workspace");
        assert_eq!(
            mount.capabilities,
            vec![MountCapability::Read, MountCapability::Write]
        );
        assert!(mount.default_write);
        assert_eq!(
            mount.metadata,
            serde_json::json!({
                "run_id": "3f9f9df5-5916-4134-9603-c1db7cf93444",
                "agent_id": "df988c1e-6147-4f7e-a9c3-f5e8ed55f6f3",
                "orchestration_id": "cc2396a9-c45e-451b-8d67-60a148df829f",
                "node_path": "draft/review",
                "attempt": 2,
            })
        );
    }
}
