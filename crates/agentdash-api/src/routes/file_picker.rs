//! File Picker API — 前端 @ 文件引用选择器的后端入口。
//!
//! 通过 VFS 统一访问层实现文件列表/读取，
//! 为前端 @ 引用选择器提供 workspace 级别的文件浏览能力。

use std::path::{Component, Path};
use std::sync::Arc;

use agentdash_application::vfs::selected_workspace_binding;
use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_workspace_and_project_with_permission};
use crate::rpc::ApiError;
use agentdash_application::vfs::{ListOptions, ResourceRef};

pub(crate) const MAX_FILE_SIZE: u64 = 100 * 1024; // 100KB
pub(crate) const MAX_TOTAL_SIZE: u64 = 500 * 1024; // 500KB
pub(crate) const MAX_REFERENCES: usize = 10;

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub pattern: Option<String>,
    pub workspace_id: Option<String>,
}

/// 文件条目 — 保持 camelCase 与前端 DTO 对齐
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub rel_path: String,
    pub size: u64,
    pub is_text: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesResponse {
    pub files: Vec<FileEntry>,
    pub root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileRequest {
    pub rel_path: String,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResponse {
    pub rel_path: String,
    pub uri: String,
    pub mime_type: String,
    pub content: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReadFilesRequest {
    pub paths: Vec<String>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReadFilesResponse {
    pub files: Vec<ReadFileResult>,
    pub total_size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResult {
    pub rel_path: String,
    pub uri: String,
    pub mime_type: String,
    pub content: Option<String>,
    pub size: u64,
    pub error: Option<String>,
}

/// GET /api/file-picker
pub async fn list_files(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<ListFilesResponse>, ApiError> {
    let workspace_id = parse_workspace_id(query.workspace_id.as_deref())?;
    let pattern = query.pattern.clone().unwrap_or_default();
    let (workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::View,
    )
    .await?;
    let backend_id = require_online_backend(&state, &workspace).await?;
    relay_list_files(&state, backend_id, &workspace, &pattern).await
}

/// POST /api/file-picker/read
pub async fn read_file(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ReadFileRequest>,
) -> Result<Json<ReadFileResponse>, ApiError> {
    let rel = req.rel_path.trim().to_string();
    validate_path_safe(&rel)?;
    let workspace_id = parse_workspace_id(req.workspace_id.as_deref())?;
    let (workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::View,
    )
    .await?;
    let backend_id = require_online_backend(&state, &workspace).await?;
    relay_read_file(&state, backend_id, &workspace, &rel).await
}

/// POST /api/file-picker/batch-read
pub async fn batch_read_files(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<BatchReadFilesRequest>,
) -> Result<Json<BatchReadFilesResponse>, ApiError> {
    if req.paths.len() > MAX_REFERENCES {
        return Err(ApiError::BadRequest(format!(
            "引用文件数量超限，最多 {} 个",
            MAX_REFERENCES
        )));
    }

    let workspace_id = parse_workspace_id(req.workspace_id.as_deref())?;
    let (workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::View,
    )
    .await?;
    let backend_id = require_online_backend(&state, &workspace).await?;
    relay_batch_read_files(&state, backend_id, &workspace, &req.paths).await
}

pub(crate) fn validate_path_safe(rel_path: &str) -> Result<(), ApiError> {
    let path = Path::new(rel_path);
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(ApiError::BadRequest("路径不允许包含 ..".into()));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ApiError::BadRequest("不允许使用绝对路径".into()));
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_workspace_id(workspace_id: Option<&str>) -> Result<Uuid, ApiError> {
    let raw = workspace_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::BadRequest("workspace_id 不能为空".into()))?;
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))
}

fn is_likely_text(rel_path: &str) -> bool {
    let text_exts = [
        "rs",
        "ts",
        "tsx",
        "js",
        "jsx",
        "json",
        "toml",
        "yaml",
        "yml",
        "md",
        "txt",
        "html",
        "css",
        "scss",
        "py",
        "sh",
        "bash",
        "zsh",
        "sql",
        "xml",
        "svg",
        "lock",
        "cfg",
        "ini",
        "env",
        "gitignore",
        "editorconfig",
        "prettierrc",
        "eslintrc",
    ];
    if let Some(ext) = Path::new(rel_path).extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        return text_exts.contains(&ext_str.as_str());
    }
    let base_name = Path::new(rel_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    matches!(
        base_name.as_str(),
        "Makefile"
            | "Dockerfile"
            | "Cargo.lock"
            | "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
            | ".gitignore"
    )
}

pub(crate) fn guess_mime(rel_path: &str) -> String {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "text/x-rust",
        "ts" | "tsx" => "text/x-typescript",
        "js" | "jsx" => "text/javascript",
        "json" => "application/json",
        "toml" => "text/x-toml",
        "yaml" | "yml" => "text/x-yaml",
        "md" => "text/markdown",
        "html" => "text/html",
        "css" | "scss" => "text/css",
        "py" => "text/x-python",
        "sh" | "bash" => "text/x-shellscript",
        "sql" => "text/x-sql",
        "xml" => "text/xml",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        _ => "text/plain",
    }
    .to_string()
}

async fn require_online_backend<'a>(
    state: &Arc<AppState>,
    workspace: &'a agentdash_domain::workspace::Workspace,
) -> Result<&'a str, ApiError> {
    let backend_id = selected_workspace_binding(workspace)
        .map(|binding| binding.backend_id.as_str())
        .unwrap_or("");
    let trimmed = backend_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(
            "Workspace 当前没有可用 binding.backend_id".to_string(),
        ));
    }
    if !state.services.backend_registry.is_online(trimmed).await {
        return Err(ApiError::Conflict(format!(
            "Workspace 所属 Backend 当前不在线: {trimmed}"
        )));
    }
    Ok(trimmed)
}

// ─── VFS 访问辅助函数 ──────────────────────────────

async fn relay_list_files(
    state: &Arc<AppState>,
    _backend_id: &str,
    workspace: &agentdash_domain::workspace::Workspace,
    pattern: &str,
) -> Result<Json<ListFilesResponse>, ApiError> {
    let session = state
        .services
        .vfs_service
        .session_for_workspace(workspace)
        .map_err(ApiError::BadRequest)?;
    let listed = state
        .services
        .vfs_service
        .list(
            &session,
            "main",
            ListOptions {
                path: ".".to_string(),
                pattern: if pattern.is_empty() {
                    None
                } else {
                    Some(pattern.to_string())
                },
                recursive: true,
            },
            None,
            None,
        )
        .await
        .map_err(ApiError::Internal)?;

    let files = listed
        .entries
        .into_iter()
        .filter(|entry| !entry.is_dir)
        .map(|entry| FileEntry {
            rel_path: entry.path.clone(),
            size: entry.size.unwrap_or(0),
            is_text: is_likely_text(&entry.path),
        })
        .collect();
    Ok(Json(ListFilesResponse {
        files,
        root: selected_workspace_binding(workspace)
            .map(|binding| binding.root_ref.clone())
            .unwrap_or_default(),
    }))
}

async fn relay_read_file(
    state: &Arc<AppState>,
    _backend_id: &str,
    workspace: &agentdash_domain::workspace::Workspace,
    rel_path: &str,
) -> Result<Json<ReadFileResponse>, ApiError> {
    let session = state
        .services
        .vfs_service
        .session_for_workspace(workspace)
        .map_err(ApiError::BadRequest)?;
    let read = state
        .services
        .vfs_service
        .read_text(
            &session,
            &ResourceRef {
                mount_id: "main".to_string(),
                path: rel_path.to_string(),
            },
            None,
            None,
        )
        .await
        .map_err(ApiError::Internal)?;
    let mime = guess_mime(&read.path);
    let size = read.content.len() as u64;
    Ok(Json(ReadFileResponse {
        rel_path: read.path,
        uri: String::new(),
        mime_type: mime,
        content: read.content,
        size,
    }))
}

async fn relay_batch_read_files(
    state: &Arc<AppState>,
    _backend_id: &str,
    workspace: &agentdash_domain::workspace::Workspace,
    paths: &[String],
) -> Result<Json<BatchReadFilesResponse>, ApiError> {
    let session = state
        .services
        .vfs_service
        .session_for_workspace(workspace)
        .map_err(ApiError::BadRequest)?;
    let mut results = Vec::new();
    let mut total_size: u64 = 0;

    for raw_rel in paths {
        let rel = raw_rel.trim().to_string();
        if let Err(err) = validate_path_safe(&rel) {
            results.push(ReadFileResult {
                rel_path: rel,
                uri: String::new(),
                mime_type: String::new(),
                content: None,
                size: 0,
                error: Some(err.to_string()),
            });
            continue;
        }

        let read = match state
            .services
            .vfs_service
            .read_text(
                &session,
                &ResourceRef {
                    mount_id: "main".to_string(),
                    path: rel.clone(),
                },
                None,
                None,
            )
            .await
        {
            Ok(value) => value,
            Err(err) => {
                results.push(ReadFileResult {
                    rel_path: rel,
                    uri: String::new(),
                    mime_type: String::new(),
                    content: None,
                    size: 0,
                    error: Some(err),
                });
                continue;
            }
        };

        let size = read.content.len() as u64;
        if size > MAX_FILE_SIZE {
            results.push(ReadFileResult {
                rel_path: read.path.clone(),
                uri: String::new(),
                mime_type: guess_mime(&read.path),
                content: None,
                size,
                error: Some(format!("文件过大 ({} bytes)", size)),
            });
            continue;
        }

        if total_size + size > MAX_TOTAL_SIZE {
            results.push(ReadFileResult {
                rel_path: read.path.clone(),
                uri: String::new(),
                mime_type: guess_mime(&read.path),
                content: None,
                size,
                error: Some("总嵌入大小超限".into()),
            });
            continue;
        }

        total_size += size;
        results.push(ReadFileResult {
            rel_path: read.path.clone(),
            uri: String::new(),
            mime_type: guess_mime(&read.path),
            content: Some(read.content),
            size,
            error: None,
        });
    }

    Ok(Json(BatchReadFilesResponse {
        files: results,
        total_size,
    }))
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::BadRequest(msg) => write!(f, "{}", msg),
            ApiError::Unauthorized(msg) => write!(f, "{}", msg),
            ApiError::Forbidden(msg) => write!(f, "{}", msg),
            ApiError::NotFound(msg) => write!(f, "{}", msg),
            ApiError::Conflict(msg) => write!(f, "{}", msg),
            ApiError::UnprocessableEntity(msg) => write!(f, "{}", msg),
            ApiError::ServiceUnavailable(msg) => write!(f, "{}", msg),
            ApiError::Internal(msg) => write!(f, "{}", msg),
        }
    }
}
