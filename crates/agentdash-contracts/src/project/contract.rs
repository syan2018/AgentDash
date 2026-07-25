use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use agentdash_application_ports::project_projection_notification::ControlPlaneProjectionChanged;

use crate::context::ContextContainerDefinition;
use crate::story::StoryResponse;
use crate::workspace::WorkspaceResponse;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStateChangeKind {
    StoryCreated,
    StoryUpdated,
    StoryStatusChanged,
    StoryDeleted,
    TaskCreated,
    TaskUpdated,
    TaskStatusChanged,
    TaskDeleted,
}

impl From<agentdash_domain::story::ChangeKind> for ProjectStateChangeKind {
    fn from(value: agentdash_domain::story::ChangeKind) -> Self {
        match value {
            agentdash_domain::story::ChangeKind::StoryCreated => Self::StoryCreated,
            agentdash_domain::story::ChangeKind::StoryUpdated => Self::StoryUpdated,
            agentdash_domain::story::ChangeKind::StoryStatusChanged => Self::StoryStatusChanged,
            agentdash_domain::story::ChangeKind::StoryDeleted => Self::StoryDeleted,
            agentdash_domain::story::ChangeKind::TaskCreated => Self::TaskCreated,
            agentdash_domain::story::ChangeKind::TaskUpdated => Self::TaskUpdated,
            agentdash_domain::story::ChangeKind::TaskStatusChanged => Self::TaskStatusChanged,
            agentdash_domain::story::ChangeKind::TaskDeleted => Self::TaskDeleted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectStateChange {
    #[ts(type = "number")]
    pub id: i64,
    pub project_id: String,
    pub entity_id: String,
    pub kind: ProjectStateChangeKind,
    #[ts(type = "Record<string, JsonValue>")]
    pub payload: BTreeMap<String, Value>,
    pub backend_id: Option<String>,
    pub created_at: String,
}

impl ProjectStateChange {
    pub fn from_domain(value: agentdash_domain::story::StateChange) -> Option<Self> {
        let payload = match value.payload {
            Value::Object(payload) => payload.into_iter().collect(),
            _ => return None,
        };

        Some(Self {
            id: value.id,
            project_id: value.project_id.to_string(),
            entity_id: value.entity_id.to_string(),
            kind: ProjectStateChangeKind::from(value.kind),
            payload,
            backend_id: value.backend_id,
            created_at: value.created_at.to_rfc3339(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectControlPlaneProjectionChanged {
    pub project_id: String,
    pub change: ControlPlaneProjectionChanged,
}

impl ProjectControlPlaneProjectionChanged {
    pub fn new(project_id: impl Into<String>, change: ControlPlaneProjectionChanged) -> Self {
        Self {
            project_id: project_id.into(),
            change,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum ProjectEventStreamEnvelope {
    Connected {
        #[ts(type = "number")]
        last_event_id: i64,
    },
    StateChanged(ProjectStateChange),
    ControlPlaneProjectionChanged(Box<ProjectControlPlaneProjectionChanged>),
    BackendRuntimeChanged {
        backend_id: String,
    },
    Heartbeat {
        #[ts(type = "number")]
        timestamp: i64,
    },
}

impl ProjectEventStreamEnvelope {
    pub fn connected(last_event_id: i64) -> Self {
        Self::Connected { last_event_id }
    }

    pub fn state_changed(
        change: agentdash_domain::story::StateChange,
    ) -> Option<ProjectEventStreamEnvelope> {
        ProjectStateChange::from_domain(change).map(Self::StateChanged)
    }

    pub fn backend_runtime_changed(backend_id: String) -> Self {
        Self::BackendRuntimeChanged { backend_id }
    }

    pub fn control_plane_projection_changed(event: ProjectControlPlaneProjectionChanged) -> Self {
        Self::ControlPlaneProjectionChanged(Box::new(event))
    }

    pub fn heartbeat(timestamp: i64) -> Self {
        Self::Heartbeat { timestamp }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SchedulingConfig {
    #[serde(default)]
    #[ts(type = "number | null")]
    pub stall_timeout_ms: Option<u64>,
}

impl From<agentdash_domain::project::SchedulingConfig> for SchedulingConfig {
    fn from(value: agentdash_domain::project::SchedulingConfig) -> Self {
        Self {
            stall_timeout_ms: value.stall_timeout_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AgentPreset {
    pub name: String,
    pub agent_type: String,
    pub config: Value,
}

impl From<agentdash_domain::project::AgentPreset> for AgentPreset {
    fn from(value: agentdash_domain::project::AgentPreset) -> Self {
        Self {
            name: value.name,
            agent_type: value.agent_type,
            config: value.config,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectConfig {
    pub default_agent_type: Option<String>,
    pub default_workspace_id: Option<String>,
    pub agent_presets: Vec<AgentPreset>,
    pub context_containers: Vec<ContextContainerDefinition>,
    pub scheduling: SchedulingConfig,
}

impl From<agentdash_domain::project::ProjectConfig> for ProjectConfig {
    fn from(value: agentdash_domain::project::ProjectConfig) -> Self {
        Self {
            default_agent_type: value.default_agent_type,
            default_workspace_id: value.default_workspace_id.map(|id| id.to_string()),
            agent_presets: value
                .agent_presets
                .into_iter()
                .map(AgentPreset::from)
                .collect(),
            context_containers: value
                .context_containers
                .into_iter()
                .map(ContextContainerDefinition::from)
                .collect(),
            scheduling: SchedulingConfig::from(value.scheduling),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectVisibility {
    Private,
    TemplateVisible,
}

impl From<agentdash_domain::project::ProjectVisibility> for ProjectVisibility {
    fn from(value: agentdash_domain::project::ProjectVisibility) -> Self {
        match value {
            agentdash_domain::project::ProjectVisibility::Private => Self::Private,
            agentdash_domain::project::ProjectVisibility::TemplateVisible => Self::TemplateVisible,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRole {
    Owner,
    Editor,
    Member,
}

impl From<agentdash_domain::project::ProjectRole> for ProjectRole {
    fn from(value: agentdash_domain::project::ProjectRole) -> Self {
        match value {
            agentdash_domain::project::ProjectRole::Owner => Self::Owner,
            agentdash_domain::project::ProjectRole::Editor => Self::Editor,
            agentdash_domain::project::ProjectRole::Member => Self::Member,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSubjectType {
    User,
    Group,
}

impl From<agentdash_domain::project::ProjectSubjectType> for ProjectSubjectType {
    fn from(value: agentdash_domain::project::ProjectSubjectType) -> Self {
        match value {
            agentdash_domain::project::ProjectSubjectType::User => Self::User,
            agentdash_domain::project::ProjectSubjectType::Group => Self::Group,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectAccessSummaryResponse {
    pub role: Option<ProjectRole>,
    pub can_use: bool,
    pub can_configure: bool,
    pub can_manage_sharing: bool,
    pub via_admin_bypass: bool,
    pub via_template_visibility: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub config: ProjectConfig,
    pub created_by_user_id: String,
    pub updated_by_user_id: String,
    pub visibility: ProjectVisibility,
    pub is_template: bool,
    pub cloned_from_project_id: Option<String>,
    pub access: ProjectAccessSummaryResponse,
    pub created_at: String,
    pub updated_at: String,
}

impl ProjectResponse {
    pub fn from_project(
        value: agentdash_domain::project::Project,
        access: ProjectAccessSummaryResponse,
    ) -> Self {
        Self {
            id: value.id.to_string(),
            name: value.name,
            description: value.description,
            config: ProjectConfig::from(value.config),
            created_by_user_id: value.created_by_user_id,
            updated_by_user_id: value.updated_by_user_id,
            visibility: ProjectVisibility::from(value.visibility),
            is_template: value.is_template,
            cloned_from_project_id: value.cloned_from_project_id.map(|id| id.to_string()),
            access,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectSubjectGrantResponse {
    pub project_id: String,
    pub subject_type: ProjectSubjectType,
    pub subject_id: String,
    pub role: ProjectRole,
    pub granted_by_user_id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::project::ProjectSubjectGrant> for ProjectSubjectGrantResponse {
    fn from(value: agentdash_domain::project::ProjectSubjectGrant) -> Self {
        Self {
            project_id: value.project_id.to_string(),
            subject_type: ProjectSubjectType::from(value.subject_type),
            subject_id: value.subject_id,
            role: ProjectRole::from(value.role),
            granted_by_user_id: value.granted_by_user_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeletedProjectSubjectGrantResponse {
    pub project_id: String,
    pub subject_type: ProjectSubjectType,
    pub subject_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RevokeProjectGrantResponse {
    pub deleted: DeletedProjectSubjectGrantResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectDetailResponse {
    #[serde(flatten)]
    pub project: ProjectResponse,
    pub workspaces: Vec<WorkspaceResponse>,
    pub stories: Vec<StoryResponse>,
}

impl ProjectDetailResponse {
    pub fn from_parts(
        project: ProjectResponse,
        workspaces: Vec<agentdash_domain::workspace::Workspace>,
        stories: Vec<agentdash_domain::story::Story>,
    ) -> Self {
        Self {
            project,
            workspaces: workspaces
                .into_iter()
                .map(WorkspaceResponse::from)
                .collect(),
            stories: stories.into_iter().map(StoryResponse::from).collect(),
        }
    }
}
