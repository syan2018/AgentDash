use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsMaterializePayload {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub plan_id: String,
    pub plan_kind: MaterializationPlanKind,
    pub source_uri: String,
    pub root_uri: String,
    pub mount_id: String,
    pub provider: String,
    pub primary_relative_path: String,
    pub target_kind: MaterializationTargetKind,
    pub access_mode: MaterializationAccessMode,
    pub entries: Vec<VfsMaterializeEntry>,
    pub cache_scope: MaterializationCacheScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaterializationPlanKind {
    SingleFile,
    DirectorySubtree,
    SkillResourceSet,
    WritableWorkingCopy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaterializationTargetKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaterializationAccessMode {
    ReadOnly,
    WritableWorkdir,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaterializationCacheScope {
    Public,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsMaterializeEntry {
    pub relative_path: String,
    pub content: VfsMaterializeContent,
    pub digest: String,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_hint: Option<String>,
    #[serde(default)]
    pub executable_hint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "encoding", rename_all = "snake_case")]
pub enum VfsMaterializeContent {
    Utf8Text { text: String },
    Base64Bytes { data: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsMaterializeResponse {
    pub source_uri: String,
    pub local_root_path: String,
    pub primary_local_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_local_url: Option<String>,
    pub access_mode: MaterializationAccessMode,
    pub manifest_digest: String,
    pub total_size_bytes: u64,
    pub entry_count: usize,
    #[serde(default)]
    pub dirty: bool,
    #[serde(default)]
    pub cache_hit: bool,
}
