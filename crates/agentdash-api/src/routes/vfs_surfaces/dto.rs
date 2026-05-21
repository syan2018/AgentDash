use serde::{Deserialize, Serialize};

use agentdash_application::vfs::ResolvedVfsSurfaceSource;

#[derive(Debug, Deserialize)]
pub struct ResolveSurfaceRequest {
    pub source: ResolvedVfsSurfaceSource,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceEntriesQuery {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub recursive: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SurfaceMountEntry {
    pub path: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
pub struct SurfaceEntriesResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub entries: Vec<SurfaceMountEntry>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceReadFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceReadFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
    pub size: u64,
    pub content_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceWriteFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceWriteFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub size: u64,
    pub persisted: bool,
    pub content_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceCreateFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceCreateFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub size: u64,
    pub content_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceDeleteFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceDeleteFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub deleted: bool,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceRenameFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceRenameFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceStatFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceStatFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub entry_type: String,
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub modified_at: Option<i64>,
    pub is_dir: bool,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceApplyPatchRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub patch: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceApplyPatchResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SurfaceReadBinaryFileRequest {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct SurfaceUploadBinaryFileResponse {
    pub surface_ref: String,
    pub mount_id: String,
    pub path: String,
    pub size: u64,
    pub content_kind: String,
    pub mime_type: String,
}
