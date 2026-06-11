use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct VfsMaterializeRequest {
    pub session_id: String,
    pub turn_id: Option<String>,
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
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializationPlanKind {
    SingleFile,
    DirectorySubtree,
    SkillResourceSet,
    WritableWorkingCopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializationTargetKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializationAccessMode {
    ReadOnly,
    WritableWorkdir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializationCacheScope {
    Public,
    Session,
}

#[derive(Debug, Clone)]
pub struct VfsMaterializeEntry {
    pub relative_path: String,
    pub content: VfsMaterializeContent,
    pub digest: String,
    pub size_bytes: u64,
    pub mime_hint: Option<String>,
    pub executable_hint: bool,
}

#[derive(Debug, Clone)]
pub enum VfsMaterializeContent {
    Utf8Text { text: String },
    Base64Bytes { data: String },
}

#[derive(Debug, Clone)]
pub struct VfsMaterializeResponse {
    pub source_uri: String,
    pub local_root_path: String,
    pub primary_local_path: String,
    pub primary_local_url: Option<String>,
    pub access_mode: MaterializationAccessMode,
    pub manifest_digest: String,
    pub total_size_bytes: u64,
    pub entry_count: usize,
    pub dirty: bool,
    pub cache_hit: bool,
}

#[async_trait]
pub trait VfsMaterializationTransport: Send + Sync {
    async fn materialize(
        &self,
        backend_id: &str,
        request: VfsMaterializeRequest,
    ) -> Result<VfsMaterializeResponse, String>;
}
