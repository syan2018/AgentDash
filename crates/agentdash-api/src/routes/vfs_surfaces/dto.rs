use uuid::Uuid;

pub use agentdash_contracts::vfs::{
    ResolveSurfaceRequest, ResolvedMountEditCapabilities, ResolvedMountPurpose,
    ResolvedMountSummary, ResolvedVfsSurface, ResolvedVfsSurfaceSource, SurfaceApplyPatchRequest,
    SurfaceApplyPatchResponse, SurfaceCreateFileRequest, SurfaceCreateFileResponse,
    SurfaceDeleteFileRequest, SurfaceDeleteFileResponse, SurfaceEntriesQuery,
    SurfaceEntriesResponse, SurfaceMountEntry, SurfaceReadBinaryFileRequest,
    SurfaceReadFileRequest, SurfaceReadFileResponse, SurfaceRenameFileRequest,
    SurfaceRenameFileResponse, SurfaceStatFileRequest, SurfaceStatFileResponse,
    SurfaceUploadBinaryFileResponse, SurfaceWriteFileRequest, SurfaceWriteFileResponse,
};

pub fn surface_source_to_application(
    source: &ResolvedVfsSurfaceSource,
) -> Result<agentdash_application_ports::vfs_surface_runtime::ResolvedVfsSurfaceSource, String> {
    use agentdash_application_ports::vfs_surface_runtime::ResolvedVfsSurfaceSource as AppSource;

    match source {
        ResolvedVfsSurfaceSource::ProjectPreview { project_id } => Ok(AppSource::ProjectPreview {
            project_id: parse_uuid(project_id, "project_id")?,
        }),
        ResolvedVfsSurfaceSource::StoryPreview {
            project_id,
            story_id,
        } => Ok(AppSource::StoryPreview {
            project_id: parse_uuid(project_id, "project_id")?,
            story_id: parse_uuid(story_id, "story_id")?,
        }),
        ResolvedVfsSurfaceSource::TaskPreview {
            project_id,
            task_id,
        } => Ok(AppSource::TaskPreview {
            project_id: parse_uuid(project_id, "project_id")?,
            task_id: parse_uuid(task_id, "task_id")?,
        }),
        ResolvedVfsSurfaceSource::SessionRuntime { session_id } => Ok(AppSource::SessionRuntime {
            session_id: session_id.trim().to_string(),
        }),
        ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id } => Ok(AppSource::AgentRun {
            run_id: parse_uuid(run_id, "run_id")?,
            agent_id: parse_uuid(agent_id, "agent_id")?,
        }),
        ResolvedVfsSurfaceSource::ProjectSkillAssets { project_id } => {
            Ok(AppSource::ProjectSkillAssets {
                project_id: parse_uuid(project_id, "project_id")?,
            })
        }
        ResolvedVfsSurfaceSource::ProjectVfsMount {
            project_id,
            mount_id,
        } => Ok(AppSource::ProjectVfsMount {
            project_id: parse_uuid(project_id, "project_id")?,
            mount_id: mount_id.trim().to_string(),
        }),
        ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
            project_id,
            project_agent_id,
        } => Ok(AppSource::ProjectAgentKnowledge {
            project_id: parse_uuid(project_id, "project_id")?,
            project_agent_id: parse_uuid(project_agent_id, "project_agent_id")?,
        }),
    }
}

pub fn surface_source_from_application(
    source: agentdash_application_ports::vfs_surface_runtime::ResolvedVfsSurfaceSource,
) -> ResolvedVfsSurfaceSource {
    use agentdash_application_ports::vfs_surface_runtime::ResolvedVfsSurfaceSource as AppSource;

    match source {
        AppSource::ProjectPreview { project_id } => ResolvedVfsSurfaceSource::ProjectPreview {
            project_id: project_id.to_string(),
        },
        AppSource::StoryPreview {
            project_id,
            story_id,
        } => ResolvedVfsSurfaceSource::StoryPreview {
            project_id: project_id.to_string(),
            story_id: story_id.to_string(),
        },
        AppSource::TaskPreview {
            project_id,
            task_id,
        } => ResolvedVfsSurfaceSource::TaskPreview {
            project_id: project_id.to_string(),
            task_id: task_id.to_string(),
        },
        AppSource::SessionRuntime { session_id } => {
            ResolvedVfsSurfaceSource::SessionRuntime { session_id }
        }
        AppSource::AgentRun { run_id, agent_id } => ResolvedVfsSurfaceSource::AgentRun {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        },
        AppSource::ProjectSkillAssets { project_id } => {
            ResolvedVfsSurfaceSource::ProjectSkillAssets {
                project_id: project_id.to_string(),
            }
        }
        AppSource::ProjectVfsMount {
            project_id,
            mount_id,
        } => ResolvedVfsSurfaceSource::ProjectVfsMount {
            project_id: project_id.to_string(),
            mount_id,
        },
        AppSource::ProjectAgentKnowledge {
            project_id,
            project_agent_id,
        } => ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
            project_id: project_id.to_string(),
            project_agent_id: project_agent_id.to_string(),
        },
    }
}

pub fn surface_from_application(
    surface: agentdash_application_ports::vfs_surface_runtime::ResolvedVfsSurface,
) -> ResolvedVfsSurface {
    ResolvedVfsSurface {
        surface_ref: surface.surface_ref,
        source: surface_source_from_application(surface.source),
        mounts: surface
            .mounts
            .into_iter()
            .map(mount_summary_from_application)
            .collect(),
        default_mount_id: surface.default_mount_id,
    }
}

fn mount_summary_from_application(
    mount: agentdash_application_ports::vfs_surface_runtime::ResolvedMountSummary,
) -> ResolvedMountSummary {
    ResolvedMountSummary {
        id: mount.id,
        display_name: mount.display_name,
        provider: mount.provider,
        backend_id: mount.backend_id,
        capabilities: mount.capabilities,
        default_write: mount.default_write,
        purpose: mount_purpose_from_application(mount.purpose),
        backend_online: mount.backend_online,
        file_count: mount.file_count,
        edit_capabilities: ResolvedMountEditCapabilities {
            create: mount.edit_capabilities.create,
            delete: mount.edit_capabilities.delete,
            rename: mount.edit_capabilities.rename,
        },
    }
}

fn mount_purpose_from_application(
    purpose: agentdash_application_ports::vfs_surface_runtime::ResolvedMountPurpose,
) -> ResolvedMountPurpose {
    use agentdash_application_ports::vfs_surface_runtime::ResolvedMountPurpose as AppPurpose;

    match purpose {
        AppPurpose::Workspace => ResolvedMountPurpose::Workspace,
        AppPurpose::ProjectContainer => ResolvedMountPurpose::ProjectContainer,
        AppPurpose::VfsMount => ResolvedMountPurpose::VfsMount,
        AppPurpose::StoryContainer => ResolvedMountPurpose::StoryContainer,
        AppPurpose::AgentKnowledge => ResolvedMountPurpose::AgentKnowledge,
        AppPurpose::Lifecycle => ResolvedMountPurpose::Lifecycle,
        AppPurpose::Canvas => ResolvedMountPurpose::Canvas,
        AppPurpose::ExternalService => ResolvedMountPurpose::ExternalService,
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, String> {
    Uuid::parse_str(raw).map_err(|_| format!("{field} 非法"))
}
