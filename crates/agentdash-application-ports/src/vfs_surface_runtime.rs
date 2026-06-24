use agentdash_spi::{Mount, Vfs};
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
    SessionRuntime {
        session_id: String,
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
            Self::SessionRuntime { session_id } => {
                format!("session-runtime:{}", session_id.trim())
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
