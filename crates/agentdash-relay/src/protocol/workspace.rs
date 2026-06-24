use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::prompt::WorkspaceIdentityKindRelay;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandWorkspaceDetectPayload {
    /// 待检测的 workspace 根目录。
    /// 本机必须先校验它存在、是目录且可读取；该命令只做 workspace facts 探测。
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandWorkspaceDetectGitPayload {
    /// 待检测的 workspace 根目录。
    /// 本机必须先校验它存在、是目录且可读取；该命令只做 workspace facts 探测。
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandWorkspaceDiscoverByIdentityPayload {
    pub workspaces: Vec<WorkspaceIdentityDiscoveryWorkspaceRelay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIdentityDiscoveryWorkspaceRelay {
    pub workspace_id: String,
    pub identity_kind: WorkspaceIdentityKindRelay,
    pub identity_payload: Value,
}

// ── command.browse_directory ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandBrowseDirectoryPayload {
    /// 要浏览的路径。为空或 None 时返回盘符列表（Windows）或根目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGitProbePayload {
    pub repo_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceP4ProbePayload {
    pub workspace_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWorkspaceDetectPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<WorkspaceGitProbePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p4: Option<WorkspaceP4ProbePayload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWorkspaceDiscoverByIdentityPayload {
    pub candidates: Vec<WorkspaceIdentityDiscoveryCandidateRelay>,
    pub skipped: Vec<WorkspaceIdentityDiscoverySkippedRelay>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIdentityDiscoveryCandidateRelay {
    pub workspace_id: String,
    pub root_ref: String,
    pub identity_kind: WorkspaceIdentityKindRelay,
    pub identity_payload: Value,
    pub detected_facts: Value,
    pub confidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIdentityDiscoverySkippedRelay {
    pub workspace_id: String,
    pub identity_kind: WorkspaceIdentityKindRelay,
    pub reason: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntryRelay {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<i64>,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWorkspaceDetectGitPayload {
    pub is_git: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
}

// ── browse_directory 响应 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseBrowseDirectoryPayload {
    /// 当前浏览的绝对路径（若为根则为空字符串）
    pub current_path: String,
    pub entries: Vec<BrowseDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseDirectoryEntry {
    pub name: String,
    /// 完整绝对路径
    pub path: String,
    pub is_dir: bool,
}
