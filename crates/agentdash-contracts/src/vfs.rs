use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SelectorHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trigger: Option<String>,
    pub placeholder: String,
    pub result_item_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct VfsDescriptor {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub provider: String,
    pub supports: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selector: Option<SelectorHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ListVfssResponse {
    pub spaces: Vec<VfsDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct VfsEntry {
    pub address: String,
    pub label: String,
    pub entry_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_dir: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ListEntriesResponse {
    pub entries: Vec<VfsEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ConfigurableProviderInfo {
    pub service_id: String,
    pub display_name: String,
    pub root_ref_hint: String,
    pub supported_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum ResolvedVfsSurfaceSource {
    ProjectPreview {
        project_id: String,
    },
    StoryPreview {
        project_id: String,
        story_id: String,
    },
    TaskPreview {
        project_id: String,
        task_id: String,
    },
    SessionRuntime {
        session_id: String,
    },
    ProjectSkillAssets {
        project_id: String,
    },
    ProjectVfsMount {
        project_id: String,
        mount_id: String,
    },
    ProjectAgentKnowledge {
        project_id: String,
        project_agent_id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
pub struct ResolvedMountEditCapabilities {
    pub create: bool,
    pub delete: bool,
    pub rename: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ResolvedMountSummary {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub backend_id: String,
    pub capabilities: Vec<String>,
    pub default_write: bool,
    pub purpose: ResolvedMountPurpose,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backend_online: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub file_count: Option<usize>,
    pub edit_capabilities: ResolvedMountEditCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ResolvedVfsSurface {
    pub surface_ref: String,
    pub source: ResolvedVfsSurfaceSource,
    pub mounts: Vec<ResolvedMountSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_mount_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ResolveSurfaceRequest {
    pub source: ResolvedVfsSurfaceSource,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceEntriesQuery {
    #[serde(default)]
    #[ts(optional)]
    pub path: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub pattern: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub recursive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceMountEntry {
    pub path: String,
    pub entry_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceEntriesResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub entries: Vec<SurfaceMountEntry>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceReadFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceReadFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
    #[ts(type = "number")]
    pub size: u64,
    pub content_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceWriteFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceWriteFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub persisted: bool,
    pub content_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceCreateFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceCreateFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub content_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceDeleteFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceDeleteFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceRenameFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceRenameFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceStatFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceStatFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub entry_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub modified_at: Option<i64>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceApplyPatchRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub patch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceApplyPatchResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SurfaceReadBinaryFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SurfaceUploadBinaryFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub content_kind: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VfsCapabilityDto {
    Read,
    Write,
    List,
    Search,
    Exec,
    /// 订阅内容变更事件。
    Watch,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectVfsMountContentDto {
    Inline,
    ExternalService {
        service_id: String,
        root_ref: String,
    },
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateProjectVfsMountRequest {
    pub mount_id: String,
    pub display_name: String,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<VfsCapabilityDto>,
    pub content: ProjectVfsMountContentDto,
}

pub type UpdateProjectVfsMountRequest = CreateProjectVfsMountRequest;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectVfsMountResponse {
    pub project_id: String,
    pub mount_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub capabilities: Vec<VfsCapabilityDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed_source: Option<InstalledAssetSourceResponse>,
    pub content: ProjectVfsMountContentDto,
    pub surface_ref: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct InstalledAssetSourceResponse {
    pub library_asset_id: String,
    pub source_ref: String,
    pub source_version: String,
    pub source_digest: String,
    pub installed_at: String,
}
