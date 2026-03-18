use std::path::{Component, Path};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::rpc::ApiError;

pub(crate) const MAX_FILE_SIZE: u64 = 100 * 1024; // 100KB
pub(crate) const MAX_TOTAL_SIZE: u64 = 500 * 1024; // 500KB
pub(crate) const MAX_REFERENCES: usize = 10;
pub(crate) const MAX_LIST_RESULTS: usize = 200;

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub pattern: Option<String>,
    pub workspace_id: Option<String>,
}

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

/// GET /api/workspace-files
pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<ListFilesResponse>, ApiError> {
    let pattern = query.pattern.clone().unwrap_or_default();

    if let Some(ws_id) = &query.workspace_id {
        let workspace = load_workspace_by_id(&state, ws_id).await?;
        let backend_id = require_online_backend(&state, &workspace.backend_id).await?;
        return relay_list_files(
            &state,
            backend_id,
            ws_id,
            &workspace.container_ref,
            &pattern,
        )
        .await;
    }

    let root = state.executor_hub.workspace_root().to_path_buf();
    let pattern_lower = pattern.to_lowercase();

    let files =
        tokio::task::spawn_blocking(move || walk_files(&root, &pattern_lower, MAX_LIST_RESULTS))
            .await
            .map_err(|e| ApiError::Internal(format!("文件列表任务异常: {e}")))?;

    let root_display = normalize_path_display(state.executor_hub.workspace_root());

    Ok(Json(ListFilesResponse {
        files,
        root: root_display,
    }))
}

/// POST /api/workspace-files/read
pub async fn read_file(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReadFileRequest>,
) -> Result<Json<ReadFileResponse>, ApiError> {
    let rel = req.rel_path.trim().to_string();
    validate_path_safe(&rel)?;

    if let Some(ws_id) = &req.workspace_id {
        let workspace = load_workspace_by_id(&state, ws_id).await?;
        let backend_id = require_online_backend(&state, &workspace.backend_id).await?;
        return relay_read_file(&state, backend_id, ws_id, &workspace.container_ref, &rel).await;
    }

    let root = state.executor_hub.workspace_root().to_path_buf();

    let abs_path = root.join(&rel);
    let canonical = tokio::fs::canonicalize(&abs_path)
        .await
        .map_err(|_| ApiError::NotFound(format!("文件不存在: {rel}")))?;

    let canonical_root = tokio::fs::canonicalize(&root)
        .await
        .map_err(|e| ApiError::Internal(format!("根目录解析失败: {e}")))?;

    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("禁止访问工作空间外文件".into()));
    }

    let metadata = tokio::fs::metadata(&canonical)
        .await
        .map_err(|_| ApiError::NotFound(format!("文件不存在: {rel}")))?;

    if metadata.len() > MAX_FILE_SIZE {
        return Err(ApiError::BadRequest(format!(
            "文件过大 ({} bytes)，最大允许 {} bytes",
            metadata.len(),
            MAX_FILE_SIZE
        )));
    }

    let content = tokio::fs::read_to_string(&canonical)
        .await
        .map_err(|_| ApiError::BadRequest(format!("文件不是有效文本: {rel}")))?;

    let mime = guess_mime(&rel);
    let uri = path_to_file_uri(&canonical);

    Ok(Json(ReadFileResponse {
        rel_path: rel,
        uri,
        mime_type: mime,
        content,
        size: metadata.len(),
    }))
}

/// POST /api/workspace-files/batch-read
pub async fn batch_read_files(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchReadFilesRequest>,
) -> Result<Json<BatchReadFilesResponse>, ApiError> {
    if req.paths.len() > MAX_REFERENCES {
        return Err(ApiError::BadRequest(format!(
            "引用文件数量超限，最多 {} 个",
            MAX_REFERENCES
        )));
    }

    let root = state.executor_hub.workspace_root().to_path_buf();
    let canonical_root = tokio::fs::canonicalize(&root)
        .await
        .map_err(|e| ApiError::Internal(format!("根目录解析失败: {e}")))?;

    let mut results = Vec::new();
    let mut total_size: u64 = 0;

    for rel in &req.paths {
        let rel = rel.trim().to_string();
        if let Err(e) = validate_path_safe(&rel) {
            results.push(ReadFileResult {
                rel_path: rel,
                uri: String::new(),
                mime_type: String::new(),
                content: None,
                size: 0,
                error: Some(e.to_string()),
            });
            continue;
        }

        let abs_path = root.join(&rel);
        let canonical = match tokio::fs::canonicalize(&abs_path).await {
            Ok(p) => p,
            Err(_) => {
                results.push(ReadFileResult {
                    rel_path: rel,
                    uri: String::new(),
                    mime_type: String::new(),
                    content: None,
                    size: 0,
                    error: Some("文件不存在".into()),
                });
                continue;
            }
        };

        if !canonical.starts_with(&canonical_root) {
            results.push(ReadFileResult {
                rel_path: rel,
                uri: String::new(),
                mime_type: String::new(),
                content: None,
                size: 0,
                error: Some("禁止访问工作空间外文件".into()),
            });
            continue;
        }

        let metadata = match tokio::fs::metadata(&canonical).await {
            Ok(m) => m,
            Err(_) => {
                results.push(ReadFileResult {
                    rel_path: rel,
                    uri: String::new(),
                    mime_type: String::new(),
                    content: None,
                    size: 0,
                    error: Some("文件不存在".into()),
                });
                continue;
            }
        };

        if metadata.len() > MAX_FILE_SIZE {
            results.push(ReadFileResult {
                rel_path: rel.clone(),
                uri: path_to_file_uri(&canonical),
                mime_type: guess_mime(&rel),
                content: None,
                size: metadata.len(),
                error: Some(format!("文件过大 ({} bytes)", metadata.len())),
            });
            continue;
        }

        if total_size + metadata.len() > MAX_TOTAL_SIZE {
            results.push(ReadFileResult {
                rel_path: rel.clone(),
                uri: path_to_file_uri(&canonical),
                mime_type: guess_mime(&rel),
                content: None,
                size: metadata.len(),
                error: Some("总嵌入大小超限".into()),
            });
            continue;
        }

        match tokio::fs::read_to_string(&canonical).await {
            Ok(content) => {
                total_size += metadata.len();
                let uri = path_to_file_uri(&canonical);
                results.push(ReadFileResult {
                    rel_path: rel.clone(),
                    uri,
                    mime_type: guess_mime(&rel),
                    content: Some(content),
                    size: metadata.len(),
                    error: None,
                });
            }
            Err(_) => {
                results.push(ReadFileResult {
                    rel_path: rel.clone(),
                    uri: path_to_file_uri(&canonical),
                    mime_type: guess_mime(&rel),
                    content: None,
                    size: metadata.len(),
                    error: Some("文件不是有效文本".into()),
                });
            }
        }
    }

    Ok(Json(BatchReadFilesResponse {
        files: results,
        total_size,
    }))
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

pub(crate) fn walk_files(root: &Path, pattern: &str, max_results: usize) -> Vec<FileEntry> {
    let mut results = Vec::new();
    walk_dir_recursive(root, root, pattern, max_results, &mut results);
    results.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    results
}

fn walk_dir_recursive(
    base: &Path,
    dir: &Path,
    pattern: &str,
    max_results: usize,
    results: &mut Vec<FileEntry>,
) {
    if results.len() >= max_results {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if results.len() >= max_results {
            return;
        }

        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        if should_skip(&file_name) {
            continue;
        }

        if path.is_dir() {
            walk_dir_recursive(base, &path, pattern, max_results, results);
        } else if path.is_file() {
            let rel = match path.strip_prefix(base) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };

            if !pattern.is_empty() && !rel.to_lowercase().contains(pattern) {
                continue;
            }

            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let is_text = is_likely_text(&rel);

            results.push(FileEntry {
                rel_path: rel,
                size,
                is_text,
            });
        }
    }
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "__pycache__"
            | ".next"
            | ".agentdash"
            | "dist"
            | "build"
            | ".trellis"
            | ".venv"
            | ".mypy_cache"
            | ".pytest_cache"
            | ".ruff_cache"
            | "references"
            | "third_party"
            | ".cursor"
            | ".claude"
            | ".agents"
    ) || name.starts_with('.') && matches!(name, ".env" | ".env.local" | ".DS_Store")
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

pub(crate) fn path_to_file_uri(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    let cleaned = if cfg!(windows) {
        s.trim_start_matches(r"\\?\").replace('\\', "/")
    } else {
        s
    };
    let trimmed = cleaned.trim_start_matches('/');
    format!("file:///{trimmed}")
}

pub(crate) fn normalize_path_display(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    if cfg!(windows) {
        s.trim_start_matches(r"\\?\").to_string()
    } else {
        s
    }
}

async fn load_workspace_by_id(
    state: &Arc<AppState>,
    workspace_id: &str,
) -> Result<agentdash_domain::workspace::Workspace, ApiError> {
    let workspace_uuid = uuid::Uuid::parse_str(workspace_id)
        .map_err(|_| ApiError::BadRequest("无效的 workspace_id".into()))?;
    state
        .workspace_repo
        .get_by_id(workspace_uuid)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Workspace 不存在: {workspace_id}")))
}

async fn require_online_backend<'a>(
    state: &Arc<AppState>,
    backend_id: &'a str,
) -> Result<&'a str, ApiError> {
    let trimmed = backend_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(
            "Workspace.backend_id 不能为空".to_string(),
        ));
    }
    if !state.backend_registry.is_online(trimmed).await {
        return Err(ApiError::Conflict(format!(
            "Workspace 所属 Backend 当前不在线: {trimmed}"
        )));
    }
    Ok(trimmed)
}

// ─── Relay 辅助：workspace_files 转发到远程后端 ──────────────

async fn relay_list_files(
    state: &Arc<AppState>,
    backend_id: &str,
    workspace_id: &str,
    container_ref: &str,
    pattern: &str,
) -> Result<Json<ListFilesResponse>, ApiError> {
    use agentdash_relay::{CommandWorkspaceFilesListPayload, RelayMessage};

    let cmd = RelayMessage::CommandWorkspaceFilesList {
        id: RelayMessage::new_id("ws-files-list"),
        payload: CommandWorkspaceFilesListPayload {
            workspace_id: workspace_id.to_string(),
            root_path: Some(container_ref.to_string()),
            path: None,
            pattern: if pattern.is_empty() {
                None
            } else {
                Some(pattern.to_string())
            },
        },
    };

    let resp = state
        .backend_registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| ApiError::Internal(format!("relay workspace_files.list 失败: {e}")))?;

    match resp {
        RelayMessage::ResponseWorkspaceFilesList {
            payload: Some(p), ..
        } => {
            let files = p
                .files
                .into_iter()
                .map(|f| {
                    let text = is_likely_text(&f.path);
                    FileEntry {
                        rel_path: f.path,
                        size: f.size.unwrap_or(0),
                        is_text: text,
                    }
                })
                .collect();
            Ok(Json(ListFilesResponse {
                files,
                root: container_ref.to_string(),
            }))
        }
        RelayMessage::ResponseWorkspaceFilesList { error: Some(e), .. } => Err(ApiError::Internal(
            format!("远程文件列表错误: {}", e.message),
        )),
        _ => Err(ApiError::Internal("远程文件列表：意外响应类型".into())),
    }
}

async fn relay_read_file(
    state: &Arc<AppState>,
    backend_id: &str,
    workspace_id: &str,
    container_ref: &str,
    rel_path: &str,
) -> Result<Json<ReadFileResponse>, ApiError> {
    use agentdash_relay::{CommandWorkspaceFilesReadPayload, RelayMessage};

    let cmd = RelayMessage::CommandWorkspaceFilesRead {
        id: RelayMessage::new_id("ws-files-read"),
        payload: CommandWorkspaceFilesReadPayload {
            workspace_id: workspace_id.to_string(),
            root_path: Some(container_ref.to_string()),
            path: rel_path.to_string(),
        },
    };

    let resp = state
        .backend_registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| ApiError::Internal(format!("relay workspace_files.read 失败: {e}")))?;

    match resp {
        RelayMessage::ResponseWorkspaceFilesRead {
            payload: Some(p), ..
        } => {
            let mime = guess_mime(&p.path);
            let size = p.content.len() as u64;
            Ok(Json(ReadFileResponse {
                rel_path: p.path,
                uri: String::new(),
                mime_type: mime,
                content: p.content,
                size,
            }))
        }
        RelayMessage::ResponseWorkspaceFilesRead { error: Some(e), .. } => Err(ApiError::Internal(
            format!("远程文件读取错误: {}", e.message),
        )),
        _ => Err(ApiError::Internal("远程文件读取：意外响应类型".into())),
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::BadRequest(msg) => write!(f, "{}", msg),
            ApiError::NotFound(msg) => write!(f, "{}", msg),
            ApiError::Conflict(msg) => write!(f, "{}", msg),
            ApiError::UnprocessableEntity(msg) => write!(f, "{}", msg),
            ApiError::Internal(msg) => write!(f, "{}", msg),
        }
    }
}
