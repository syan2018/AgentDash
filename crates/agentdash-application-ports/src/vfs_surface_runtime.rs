use agentdash_platform_spi::{Mount, Vfs};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedVfsSurface {
    pub surface_ref: String,
    pub source: ResolvedVfsSurfaceSource,
    pub mounts: Vec<ResolvedMountSummary>,
    pub default_mount_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum ResolvedVfsSurfaceSource {
    ProjectPreview {
        project_id: Uuid,
    },
    StoryPreview {
        project_id: Uuid,
        story_id: Uuid,
    },
    TaskPreview {
        project_id: Uuid,
        task_id: Uuid,
    },
    RuntimeThread {
        runtime_thread_id: String,
    },
    AgentRun {
        run_id: Uuid,
        agent_id: Uuid,
    },
    ProjectSkillAssets {
        project_id: Uuid,
    },
    ProjectVfsMount {
        project_id: Uuid,
        mount_id: String,
    },
    ProjectAgentKnowledge {
        project_id: Uuid,
        project_agent_id: Uuid,
    },
}

impl ResolvedVfsSurfaceSource {
    pub fn surface_ref(&self) -> String {
        match self {
            Self::ProjectPreview { project_id } => format!("project-preview:{project_id}"),
            Self::StoryPreview {
                project_id,
                story_id,
            } => format!("story-preview:{project_id}:{story_id}"),
            Self::TaskPreview {
                project_id,
                task_id,
            } => format!("task-preview:{project_id}:{task_id}"),
            Self::RuntimeThread { runtime_thread_id } => {
                format!("session-runtime:{}", runtime_thread_id.trim())
            }
            Self::AgentRun { run_id, agent_id } => format!("agent-run:{run_id}:{agent_id}"),
            Self::ProjectSkillAssets { project_id } => format!("project-skill-assets:{project_id}"),
            Self::ProjectVfsMount {
                project_id,
                mount_id,
            } => format!("project-vfs-mount:{project_id}:{mount_id}"),
            Self::ProjectAgentKnowledge {
                project_id,
                project_agent_id,
            } => format!("project-agent-knowledge:{project_id}:{project_agent_id}"),
        }
    }

    pub fn parse_surface_ref(surface_ref: &str) -> Result<Self, String> {
        let trimmed = surface_ref.trim();
        if let Some(rest) = trimmed.strip_prefix("project-preview:") {
            let project_id = Uuid::parse_str(rest)
                .map_err(|_| format!("无效的 project preview surface_ref: {trimmed}"))?;
            return Ok(Self::ProjectPreview { project_id });
        }
        if let Some(rest) = trimmed.strip_prefix("story-preview:") {
            let mut parts = rest.split(':');
            let project_id = parts
                .next()
                .ok_or_else(|| format!("无效的 story preview surface_ref: {trimmed}"))?;
            let story_id = parts
                .next()
                .ok_or_else(|| format!("无效的 story preview surface_ref: {trimmed}"))?;
            if parts.next().is_some() {
                return Err(format!("无效的 story preview surface_ref: {trimmed}"));
            }
            return Ok(Self::StoryPreview {
                project_id: Uuid::parse_str(project_id)
                    .map_err(|_| format!("无效的 story preview project_id: {project_id}"))?,
                story_id: Uuid::parse_str(story_id)
                    .map_err(|_| format!("无效的 story preview story_id: {story_id}"))?,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("task-preview:") {
            let mut parts = rest.split(':');
            let project_id = parts
                .next()
                .ok_or_else(|| format!("无效的 task preview surface_ref: {trimmed}"))?;
            let task_id = parts
                .next()
                .ok_or_else(|| format!("无效的 task preview surface_ref: {trimmed}"))?;
            if parts.next().is_some() {
                return Err(format!("无效的 task preview surface_ref: {trimmed}"));
            }
            return Ok(Self::TaskPreview {
                project_id: Uuid::parse_str(project_id)
                    .map_err(|_| format!("无效的 task preview project_id: {project_id}"))?,
                task_id: Uuid::parse_str(task_id)
                    .map_err(|_| format!("无效的 task preview task_id: {task_id}"))?,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("session-runtime:") {
            let runtime_thread_id = rest.trim();
            if runtime_thread_id.is_empty() {
                return Err(format!("无效的 session runtime surface_ref: {trimmed}"));
            }
            return Ok(Self::RuntimeThread {
                runtime_thread_id: runtime_thread_id.to_string(),
            });
        }
        if let Some(rest) = trimmed.strip_prefix("agent-run:") {
            let mut parts = rest.split(':');
            let run_id = parts
                .next()
                .ok_or_else(|| format!("无效的 agent run surface_ref: {trimmed}"))?;
            let agent_id = parts
                .next()
                .ok_or_else(|| format!("无效的 agent run surface_ref: {trimmed}"))?;
            if parts.next().is_some() {
                return Err(format!("无效的 agent run surface_ref: {trimmed}"));
            }
            return Ok(Self::AgentRun {
                run_id: Uuid::parse_str(run_id)
                    .map_err(|_| format!("无效的 agent run run_id: {run_id}"))?,
                agent_id: Uuid::parse_str(agent_id)
                    .map_err(|_| format!("无效的 agent run agent_id: {agent_id}"))?,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("project-skill-assets:") {
            let project_id = Uuid::parse_str(rest)
                .map_err(|_| format!("无效的 project skill assets surface_ref: {trimmed}"))?;
            return Ok(Self::ProjectSkillAssets { project_id });
        }
        if let Some(rest) = trimmed.strip_prefix("project-vfs-mount:") {
            let mut parts = rest.splitn(2, ':');
            let project_id = parts
                .next()
                .ok_or_else(|| format!("无效的 project vfs mount surface_ref: {trimmed}"))?;
            let mount_id = parts
                .next()
                .ok_or_else(|| format!("无效的 project vfs mount surface_ref: {trimmed}"))?;
            if mount_id.trim().is_empty() {
                return Err(format!("无效的 project vfs mount surface_ref: {trimmed}"));
            }
            return Ok(Self::ProjectVfsMount {
                project_id: Uuid::parse_str(project_id)
                    .map_err(|_| format!("无效的 project vfs mount project_id: {project_id}"))?,
                mount_id: mount_id.to_string(),
            });
        }
        if let Some(rest) = trimmed.strip_prefix("project-agent-knowledge:") {
            let mut parts = rest.split(':');
            let project_id = parts
                .next()
                .ok_or_else(|| format!("无效的 agent knowledge surface_ref: {trimmed}"))?;
            let project_agent_id = parts
                .next()
                .ok_or_else(|| format!("无效的 agent knowledge surface_ref: {trimmed}"))?;
            if parts.next().is_some() {
                return Err(format!("无效的 agent knowledge surface_ref: {trimmed}"));
            }
            return Ok(Self::ProjectAgentKnowledge {
                project_id: Uuid::parse_str(project_id)
                    .map_err(|_| format!("无效的 agent knowledge project_id: {project_id}"))?,
                project_agent_id: Uuid::parse_str(project_agent_id).map_err(|_| {
                    format!("无效的 agent knowledge project_agent_id: {project_agent_id}")
                })?,
            });
        }

        Err(format!("未知的 surface_ref: {trimmed}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedMountSummary {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub backend_id: String,
    pub capabilities: Vec<String>,
    pub default_write: bool,
    pub purpose: ResolvedMountPurpose,
    pub backend_online: Option<bool>,
    pub file_count: Option<usize>,
    pub edit_capabilities: ResolvedMountEditCapabilities,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ResolvedMountEditCapabilities {
    pub create: bool,
    pub delete: bool,
    pub rename: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedMountPurpose {
    Workspace,
    ProjectContainer,
    VfsMount,
    StoryContainer,
    AgentKnowledge,
    Lifecycle,
    Canvas,
    ExternalService,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedMountOwnerKind {
    Project,
    Story,
    Task,
    Session,
    ProjectAgent,
    Canvas,
    Workspace,
    External,
}

#[async_trait]
pub trait VfsSurfaceRuntimeProjection: Send + Sync {
    async fn is_backend_online(&self, backend_id: &str) -> bool;

    fn edit_capabilities(&self, mount: &Mount) -> ResolvedMountEditCapabilities;
}

#[derive(Debug, Clone)]
pub struct VfsSurfaceSummaryRequest {
    pub source: ResolvedVfsSurfaceSource,
    pub vfs: Vfs,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VfsSurfaceRuntimeError {
    #[error("vfs surface runtime projection failed: {message}")]
    Projection { message: String },
}

#[cfg(test)]
mod tests {
    use super::ResolvedVfsSurfaceSource;
    use uuid::Uuid;

    #[test]
    fn project_agent_knowledge_surface_ref_is_stable() {
        let project_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let project_agent_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        let source = ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
            project_id,
            project_agent_id,
        };

        assert_eq!(
            source.surface_ref(),
            "project-agent-knowledge:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222"
        );
        assert_eq!(
            ResolvedVfsSurfaceSource::parse_surface_ref(&source.surface_ref()).unwrap(),
            source
        );
    }

    #[test]
    fn task_plan_item_agent_knowledge_surface_ref_is_stable() {
        let project_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let task_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();

        let source = ResolvedVfsSurfaceSource::TaskPreview {
            project_id,
            task_id,
        };

        assert_eq!(
            source.surface_ref(),
            "task-preview:11111111-1111-1111-1111-111111111111:33333333-3333-3333-3333-333333333333"
        );
        assert_eq!(
            ResolvedVfsSurfaceSource::parse_surface_ref(&source.surface_ref()).unwrap(),
            source
        );
    }

    #[test]
    fn runtime_thread_surface_ref_trims_runtime_thread_id() {
        let source = ResolvedVfsSurfaceSource::RuntimeThread {
            runtime_thread_id: "  sess-1  ".to_string(),
        };

        assert_eq!(source.surface_ref(), "session-runtime:sess-1");
        assert_eq!(
            ResolvedVfsSurfaceSource::parse_surface_ref(&source.surface_ref()).unwrap(),
            ResolvedVfsSurfaceSource::RuntimeThread {
                runtime_thread_id: "sess-1".to_string()
            }
        );
    }

    #[test]
    fn parse_story_preview_surface_ref() {
        let parsed = ResolvedVfsSurfaceSource::parse_surface_ref(
            "story-preview:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222",
        )
        .unwrap();

        assert_eq!(
            parsed,
            ResolvedVfsSurfaceSource::StoryPreview {
                project_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                story_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            }
        );
    }

    #[test]
    fn parse_story_preview_surface_ref_rejects_extra_parts() {
        let surface_ref = "story-preview:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222:extra";

        let error = ResolvedVfsSurfaceSource::parse_surface_ref(surface_ref).unwrap_err();

        assert!(error.contains("story preview"));
    }
}
