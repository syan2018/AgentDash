use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DeletedIdResponse {
    pub deleted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DeletedFlagResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct UpdatedIdResponse {
    pub updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct RevokedIdResponse {
    pub revoked: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct PendingExecutionResponse {
    pub execution_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Local,
    Remote,
}

impl From<agentdash_domain::backend::BackendType> for BackendType {
    fn from(value: agentdash_domain::backend::BackendType) -> Self {
        match value {
            agentdash_domain::backend::BackendType::Local => Self::Local,
            agentdash_domain::backend::BackendType::Remote => Self::Remote,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendVisibility {
    Private,
    Shared,
    System,
}

impl From<agentdash_domain::backend::BackendVisibility> for BackendVisibility {
    fn from(value: agentdash_domain::backend::BackendVisibility) -> Self {
        match value {
            agentdash_domain::backend::BackendVisibility::Private => Self::Private,
            agentdash_domain::backend::BackendVisibility::Shared => Self::Shared,
            agentdash_domain::backend::BackendVisibility::System => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendShareScopeKind {
    User,
    Project,
    System,
}

impl From<agentdash_domain::backend::BackendShareScopeKind> for BackendShareScopeKind {
    fn from(value: agentdash_domain::backend::BackendShareScopeKind) -> Self {
        match value {
            agentdash_domain::backend::BackendShareScopeKind::User => Self::User,
            agentdash_domain::backend::BackendShareScopeKind::Project => Self::Project,
            agentdash_domain::backend::BackendShareScopeKind::System => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealthStatus {
    Online,
    Offline,
    Starting,
    Degraded,
    Stopping,
    Error,
}

impl From<agentdash_domain::backend::RuntimeHealthStatus> for RuntimeHealthStatus {
    fn from(value: agentdash_domain::backend::RuntimeHealthStatus) -> Self {
        match value {
            agentdash_domain::backend::RuntimeHealthStatus::Online => Self::Online,
            agentdash_domain::backend::RuntimeHealthStatus::Offline => Self::Offline,
            agentdash_domain::backend::RuntimeHealthStatus::Starting => Self::Starting,
            agentdash_domain::backend::RuntimeHealthStatus::Degraded => Self::Degraded,
            agentdash_domain::backend::RuntimeHealthStatus::Stopping => Self::Stopping,
            agentdash_domain::backend::RuntimeHealthStatus::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendRuntimeHealthResponse {
    pub backend_id: String,
    pub profile_id: Option<String>,
    pub name: String,
    pub status: RuntimeHealthStatus,
    pub online: bool,
    pub version: Option<String>,
    pub capabilities: Value,
    pub workspace_roots: Vec<String>,
    pub device: Value,
    pub connected_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub disconnected_at: Option<DateTime<Utc>>,
    pub disconnect_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendExecutorCapabilityResponse {
    pub id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendMcpServerCapabilityResponse {
    pub name: String,
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendCapabilitiesResponse {
    pub executors: Vec<BackendExecutorCapabilityResponse>,
    pub supports_cancel: bool,
    pub supports_discover_options: bool,
    pub mcp_servers: Vec<BackendMcpServerCapabilityResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendResponse {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub enabled: bool,
    pub backend_type: BackendType,
    pub owner_user_id: Option<String>,
    pub profile_id: Option<String>,
    pub device_id: Option<String>,
    pub machine_id: Option<String>,
    pub machine_label: Option<String>,
    pub visibility: BackendVisibility,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub device: Value,
    pub last_claimed_at: Option<DateTime<Utc>>,
}

impl From<agentdash_domain::backend::BackendConfig> for BackendResponse {
    fn from(value: agentdash_domain::backend::BackendConfig) -> Self {
        Self {
            id: value.id,
            name: value.name,
            endpoint: value.endpoint,
            enabled: value.enabled,
            backend_type: BackendType::from(value.backend_type),
            owner_user_id: value.owner_user_id,
            profile_id: value.profile_id,
            device_id: value.device_id,
            machine_id: value.machine_id,
            machine_label: value.machine_label,
            visibility: BackendVisibility::from(value.visibility),
            share_scope_kind: BackendShareScopeKind::from(value.share_scope_kind),
            share_scope_id: value.share_scope_id,
            capability_slot: value.capability_slot,
            device: value.device,
            last_claimed_at: value.last_claimed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendWithStatusResponse {
    #[serde(flatten)]
    pub backend: BackendResponse,
    pub online: bool,
    pub runtime_health: Option<BackendRuntimeHealthResponse>,
    pub workspace_roots: Option<Vec<String>>,
    pub capabilities: Option<BackendCapabilitiesResponse>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VfsCapabilityDto {
    Read,
    Write,
    List,
    Search,
    Exec,
    Watch,
}

impl From<agentdash_domain::common::MountCapability> for VfsCapabilityDto {
    fn from(value: agentdash_domain::common::MountCapability) -> Self {
        match value {
            agentdash_domain::common::MountCapability::Read => Self::Read,
            agentdash_domain::common::MountCapability::Write => Self::Write,
            agentdash_domain::common::MountCapability::List => Self::List,
            agentdash_domain::common::MountCapability::Search => Self::Search,
            agentdash_domain::common::MountCapability::Exec => Self::Exec,
            agentdash_domain::common::MountCapability::Watch => Self::Watch,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ContextContainerFile {
    pub path: String,
    pub content: String,
}

impl From<agentdash_domain::context_container::ContextContainerFile> for ContextContainerFile {
    fn from(value: agentdash_domain::context_container::ContextContainerFile) -> Self {
        Self {
            path: value.path,
            content: value.content,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextContainerProvider {
    InlineFiles {
        files: Vec<ContextContainerFile>,
    },
    ExternalService {
        service_id: String,
        root_ref: String,
    },
}

impl From<agentdash_domain::context_container::ContextContainerProvider>
    for ContextContainerProvider
{
    fn from(value: agentdash_domain::context_container::ContextContainerProvider) -> Self {
        match value {
            agentdash_domain::context_container::ContextContainerProvider::InlineFiles {
                files,
            } => Self::InlineFiles {
                files: files.into_iter().map(ContextContainerFile::from).collect(),
            },
            agentdash_domain::context_container::ContextContainerProvider::ExternalService {
                service_id,
                root_ref,
            } => Self::ExternalService {
                service_id,
                root_ref,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ContextContainerDefinition {
    pub mount_id: String,
    pub display_name: String,
    pub provider: ContextContainerProvider,
    pub capabilities: Vec<VfsCapabilityDto>,
    pub default_write: bool,
}

impl From<agentdash_domain::context_container::ContextContainerDefinition>
    for ContextContainerDefinition
{
    fn from(value: agentdash_domain::context_container::ContextContainerDefinition) -> Self {
        Self {
            mount_id: value.mount_id,
            display_name: value.display_name,
            provider: ContextContainerProvider::from(value.provider),
            capabilities: value
                .capabilities
                .into_iter()
                .map(VfsCapabilityDto::from)
                .collect(),
            default_write: value.default_write,
        }
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
    Viewer,
}

impl From<agentdash_domain::project::ProjectRole> for ProjectRole {
    fn from(value: agentdash_domain::project::ProjectRole) -> Self {
        match value {
            agentdash_domain::project::ProjectRole::Owner => Self::Owner,
            agentdash_domain::project::ProjectRole::Editor => Self::Editor,
            agentdash_domain::project::ProjectRole::Viewer => Self::Viewer,
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
    pub can_view: bool,
    pub can_edit: bool,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIdentityKind {
    GitRepo,
    P4Workspace,
    LocalDir,
}

impl From<agentdash_domain::workspace::WorkspaceIdentityKind> for WorkspaceIdentityKind {
    fn from(value: agentdash_domain::workspace::WorkspaceIdentityKind) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceIdentityKind::GitRepo => Self::GitRepo,
            agentdash_domain::workspace::WorkspaceIdentityKind::P4Workspace => Self::P4Workspace,
            agentdash_domain::workspace::WorkspaceIdentityKind::LocalDir => Self::LocalDir,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBindingStatus {
    Pending,
    Ready,
    Offline,
    Error,
}

impl From<agentdash_domain::workspace::WorkspaceBindingStatus> for WorkspaceBindingStatus {
    fn from(value: agentdash_domain::workspace::WorkspaceBindingStatus) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceBindingStatus::Pending => Self::Pending,
            agentdash_domain::workspace::WorkspaceBindingStatus::Ready => Self::Ready,
            agentdash_domain::workspace::WorkspaceBindingStatus::Offline => Self::Offline,
            agentdash_domain::workspace::WorkspaceBindingStatus::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceResolutionPolicy {
    PreferDefaultBinding,
    PreferOnline,
}

impl From<agentdash_domain::workspace::WorkspaceResolutionPolicy> for WorkspaceResolutionPolicy {
    fn from(value: agentdash_domain::workspace::WorkspaceResolutionPolicy) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceResolutionPolicy::PreferDefaultBinding => {
                Self::PreferDefaultBinding
            }
            agentdash_domain::workspace::WorkspaceResolutionPolicy::PreferOnline => {
                Self::PreferOnline
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Pending,
    Ready,
    Active,
    Archived,
    Error,
}

impl From<agentdash_domain::workspace::WorkspaceStatus> for WorkspaceStatus {
    fn from(value: agentdash_domain::workspace::WorkspaceStatus) -> Self {
        match value {
            agentdash_domain::workspace::WorkspaceStatus::Pending => Self::Pending,
            agentdash_domain::workspace::WorkspaceStatus::Ready => Self::Ready,
            agentdash_domain::workspace::WorkspaceStatus::Active => Self::Active,
            agentdash_domain::workspace::WorkspaceStatus::Archived => Self::Archived,
            agentdash_domain::workspace::WorkspaceStatus::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceBindingResponse {
    pub id: String,
    pub workspace_id: String,
    pub backend_id: String,
    pub root_ref: String,
    pub status: WorkspaceBindingStatus,
    pub detected_facts: Value,
    pub last_verified_at: Option<String>,
    pub priority: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::workspace::WorkspaceBinding> for WorkspaceBindingResponse {
    fn from(value: agentdash_domain::workspace::WorkspaceBinding) -> Self {
        Self {
            id: value.id.to_string(),
            workspace_id: value.workspace_id.to_string(),
            backend_id: value.backend_id,
            root_ref: value.root_ref,
            status: WorkspaceBindingStatus::from(value.status),
            detected_facts: value.detected_facts,
            last_verified_at: value.last_verified_at.map(|time| time.to_rfc3339()),
            priority: value.priority,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceResponse {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub resolution_policy: WorkspaceResolutionPolicy,
    pub default_binding_id: Option<String>,
    pub status: WorkspaceStatus,
    pub bindings: Vec<WorkspaceBindingResponse>,
    pub mount_capabilities: Vec<VfsCapabilityDto>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::workspace::Workspace> for WorkspaceResponse {
    fn from(value: agentdash_domain::workspace::Workspace) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: value.project_id.to_string(),
            name: value.name,
            identity_kind: WorkspaceIdentityKind::from(value.identity_kind),
            identity_payload: value.identity_payload,
            resolution_policy: WorkspaceResolutionPolicy::from(value.resolution_policy),
            default_binding_id: value.default_binding_id.map(|id| id.to_string()),
            status: WorkspaceStatus::from(value.status),
            bindings: value
                .bindings
                .into_iter()
                .map(WorkspaceBindingResponse::from)
                .collect(),
            mount_capabilities: value
                .mount_capabilities
                .into_iter()
                .map(VfsCapabilityDto::from)
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    ManualText,
    File,
    ProjectSnapshot,
    HttpFetch,
    McpResource,
    EntityRef,
}

impl From<agentdash_domain::context_source::ContextSourceKind> for ContextSourceKind {
    fn from(value: agentdash_domain::context_source::ContextSourceKind) -> Self {
        match value {
            agentdash_domain::context_source::ContextSourceKind::ManualText => Self::ManualText,
            agentdash_domain::context_source::ContextSourceKind::File => Self::File,
            agentdash_domain::context_source::ContextSourceKind::ProjectSnapshot => {
                Self::ProjectSnapshot
            }
            agentdash_domain::context_source::ContextSourceKind::HttpFetch => Self::HttpFetch,
            agentdash_domain::context_source::ContextSourceKind::McpResource => Self::McpResource,
            agentdash_domain::context_source::ContextSourceKind::EntityRef => Self::EntityRef,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSlot {
    Requirements,
    Constraints,
    Codebase,
    References,
    InstructionAppend,
}

impl From<agentdash_domain::context_source::ContextSlot> for ContextSlot {
    fn from(value: agentdash_domain::context_source::ContextSlot) -> Self {
        match value {
            agentdash_domain::context_source::ContextSlot::Requirements => Self::Requirements,
            agentdash_domain::context_source::ContextSlot::Constraints => Self::Constraints,
            agentdash_domain::context_source::ContextSlot::Codebase => Self::Codebase,
            agentdash_domain::context_source::ContextSlot::References => Self::References,
            agentdash_domain::context_source::ContextSlot::InstructionAppend => {
                Self::InstructionAppend
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextDelivery {
    Inline,
    Resource,
    Lazy,
}

impl From<agentdash_domain::context_source::ContextDelivery> for ContextDelivery {
    fn from(value: agentdash_domain::context_source::ContextDelivery) -> Self {
        match value {
            agentdash_domain::context_source::ContextDelivery::Inline => Self::Inline,
            agentdash_domain::context_source::ContextDelivery::Resource => Self::Resource,
            agentdash_domain::context_source::ContextDelivery::Lazy => Self::Lazy,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ContextSourceRef {
    pub kind: ContextSourceKind,
    pub locator: String,
    pub label: Option<String>,
    pub slot: ContextSlot,
    pub priority: i32,
    pub required: bool,
    pub max_chars: Option<usize>,
    pub delivery: ContextDelivery,
}

impl From<agentdash_domain::context_source::ContextSourceRef> for ContextSourceRef {
    fn from(value: agentdash_domain::context_source::ContextSourceRef) -> Self {
        Self {
            kind: ContextSourceKind::from(value.kind),
            locator: value.locator,
            label: value.label,
            slot: ContextSlot::from(value.slot),
            priority: value.priority,
            required: value.required,
            max_chars: value.max_chars,
            delivery: ContextDelivery::from(value.delivery),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct SessionRequiredContextBlock {
    pub title: String,
    pub content: String,
}

impl From<agentdash_domain::session_composition::SessionRequiredContextBlock>
    for SessionRequiredContextBlock
{
    fn from(value: agentdash_domain::session_composition::SessionRequiredContextBlock) -> Self {
        Self {
            title: value.title,
            content: value.content,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct SessionComposition {
    pub persona_label: Option<String>,
    pub persona_prompt: Option<String>,
    pub workflow_steps: Vec<String>,
    pub required_context_blocks: Vec<SessionRequiredContextBlock>,
}

impl From<agentdash_domain::session_composition::SessionComposition> for SessionComposition {
    fn from(value: agentdash_domain::session_composition::SessionComposition) -> Self {
        Self {
            persona_label: value.persona_label,
            persona_prompt: value.persona_prompt,
            workflow_steps: value.workflow_steps,
            required_context_blocks: value
                .required_context_blocks
                .into_iter()
                .map(SessionRequiredContextBlock::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StoryContext {
    pub source_refs: Vec<ContextSourceRef>,
    pub context_containers: Vec<ContextContainerDefinition>,
    pub disabled_container_ids: Vec<String>,
    pub session_composition: Option<SessionComposition>,
}

impl From<agentdash_domain::story::StoryContext> for StoryContext {
    fn from(value: agentdash_domain::story::StoryContext) -> Self {
        Self {
            source_refs: value
                .source_refs
                .into_iter()
                .map(ContextSourceRef::from)
                .collect(),
            context_containers: value
                .context_containers
                .into_iter()
                .map(ContextContainerDefinition::from)
                .collect(),
            disabled_container_ids: value.disabled_container_ids,
            session_composition: value.session_composition.map(SessionComposition::from),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryStatus {
    Created,
    ContextReady,
    Decomposed,
    Executing,
    Completed,
    Failed,
    Cancelled,
}

impl From<agentdash_domain::story::StoryStatus> for StoryStatus {
    fn from(value: agentdash_domain::story::StoryStatus) -> Self {
        match value {
            agentdash_domain::story::StoryStatus::Created => Self::Created,
            agentdash_domain::story::StoryStatus::ContextReady => Self::ContextReady,
            agentdash_domain::story::StoryStatus::Decomposed => Self::Decomposed,
            agentdash_domain::story::StoryStatus::Executing => Self::Executing,
            agentdash_domain::story::StoryStatus::Completed => Self::Completed,
            agentdash_domain::story::StoryStatus::Failed => Self::Failed,
            agentdash_domain::story::StoryStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryPriority {
    P0,
    P1,
    P2,
    P3,
}

impl From<agentdash_domain::story::StoryPriority> for StoryPriority {
    fn from(value: agentdash_domain::story::StoryPriority) -> Self {
        match value {
            agentdash_domain::story::StoryPriority::P0 => Self::P0,
            agentdash_domain::story::StoryPriority::P1 => Self::P1,
            agentdash_domain::story::StoryPriority::P2 => Self::P2,
            agentdash_domain::story::StoryPriority::P3 => Self::P3,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryType {
    Feature,
    Bugfix,
    Refactor,
    Docs,
    Test,
    Other,
}

impl From<agentdash_domain::story::StoryType> for StoryType {
    fn from(value: agentdash_domain::story::StoryType) -> Self {
        match value {
            agentdash_domain::story::StoryType::Feature => Self::Feature,
            agentdash_domain::story::StoryType::Bugfix => Self::Bugfix,
            agentdash_domain::story::StoryType::Refactor => Self::Refactor,
            agentdash_domain::story::StoryType::Docs => Self::Docs,
            agentdash_domain::story::StoryType::Test => Self::Test,
            agentdash_domain::story::StoryType::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StoryResponse {
    pub id: String,
    pub project_id: String,
    pub default_workspace_id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    pub priority: StoryPriority,
    pub story_type: StoryType,
    pub tags: Vec<String>,
    pub task_count: u32,
    pub context: StoryContext,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::story::Story> for StoryResponse {
    fn from(value: agentdash_domain::story::Story) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: value.project_id.to_string(),
            default_workspace_id: value.default_workspace_id.map(|id| id.to_string()),
            title: value.title,
            description: value.description,
            status: StoryStatus::from(value.status),
            priority: StoryPriority::from(value.priority),
            story_type: StoryType::from(value.story_type),
            tags: value.tags,
            task_count: value.task_count,
            context: StoryContext::from(value.context),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Assigned,
    Running,
    AwaitingVerification,
    Completed,
    Failed,
    Cancelled,
}

impl From<agentdash_domain::task::TaskStatus> for TaskStatus {
    fn from(value: agentdash_domain::task::TaskStatus) -> Self {
        match value {
            agentdash_domain::task::TaskStatus::Pending => Self::Pending,
            agentdash_domain::task::TaskStatus::Assigned => Self::Assigned,
            agentdash_domain::task::TaskStatus::Running => Self::Running,
            agentdash_domain::task::TaskStatus::AwaitingVerification => Self::AwaitingVerification,
            agentdash_domain::task::TaskStatus::Completed => Self::Completed,
            agentdash_domain::task::TaskStatus::Failed => Self::Failed,
            agentdash_domain::task::TaskStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    CodeChange,
    TestResult,
    LogOutput,
    File,
    ToolExecution,
}

impl From<agentdash_domain::task::ArtifactType> for ArtifactType {
    fn from(value: agentdash_domain::task::ArtifactType) -> Self {
        match value {
            agentdash_domain::task::ArtifactType::CodeChange => Self::CodeChange,
            agentdash_domain::task::ArtifactType::TestResult => Self::TestResult,
            agentdash_domain::task::ArtifactType::LogOutput => Self::LogOutput,
            agentdash_domain::task::ArtifactType::File => Self::File,
            agentdash_domain::task::ArtifactType::ToolExecution => Self::ToolExecution,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Artifact {
    pub id: String,
    pub artifact_type: ArtifactType,
    pub content: Value,
    pub created_at: String,
}

impl From<agentdash_domain::task::Artifact> for Artifact {
    fn from(value: agentdash_domain::task::Artifact) -> Self {
        Self {
            id: value.id.to_string(),
            artifact_type: ArtifactType::from(value.artifact_type),
            content: value.content,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskDispatchPreference {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
    pub prompt_template: Option<String>,
    pub initial_context: Option<String>,
    pub context_sources: Vec<ContextSourceRef>,
}

impl From<agentdash_domain::task::TaskDispatchPreference> for TaskDispatchPreference {
    fn from(value: agentdash_domain::task::TaskDispatchPreference) -> Self {
        Self {
            agent_type: value.agent_type,
            agent_pid: value.agent_pid,
            preset_name: value.preset_name,
            prompt_template: value.prompt_template,
            initial_context: value.initial_context,
            context_sources: value
                .context_sources
                .into_iter()
                .map(ContextSourceRef::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskResponse {
    pub id: String,
    pub project_id: String,
    pub story_id: String,
    pub workspace_id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub dispatch_preference: TaskDispatchPreference,
    pub artifacts: Vec<Artifact>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<agentdash_domain::task::Task> for TaskResponse {
    fn from(value: agentdash_domain::task::Task) -> Self {
        Self {
            id: value.id.to_string(),
            project_id: value.project_id.to_string(),
            story_id: value.story_id.to_string(),
            workspace_id: value.workspace_id.map(|id| id.to_string()),
            title: value.title.clone(),
            description: value.description.clone(),
            status: TaskStatus::from(value.status().clone()),
            dispatch_preference: TaskDispatchPreference::from(value.dispatch_preference.clone()),
            artifacts: value
                .artifacts()
                .iter()
                .cloned()
                .map(Artifact::from)
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}
