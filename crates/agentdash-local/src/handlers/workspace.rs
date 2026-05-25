//! Workspace 探测 + 目录浏览命令处理

use agentdash_relay::*;

use super::CommandHandler;

impl CommandHandler {
    pub(super) async fn handle_workspace_detect(
        &self,
        id: String,
        payload: CommandWorkspaceDetectPayload,
    ) -> RelayMessage {
        let workspace_root =
            match crate::tool_executor::resolve_detect_workspace_root(&payload.path) {
                Ok(path) => path,
                Err(error) => {
                    return RelayMessage::ResponseWorkspaceDetect {
                        id,
                        payload: None,
                        error: Some(RelayError::runtime_error(format!(
                            "workspace_detect 路径校验失败: {error}"
                        ))),
                    };
                }
            };

        tracing::debug!(path = %workspace_root.display(), "workspace_detect");
        let detected = match tokio::task::spawn_blocking(move || {
            crate::workspace_probe::detect_workspace(&workspace_root)
        })
        .await
        {
            Ok(result) => result,
            Err(err) => {
                return RelayMessage::ResponseWorkspaceDetect {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "workspace_detect 任务失败: {err}"
                    ))),
                };
            }
        };

        RelayMessage::ResponseWorkspaceDetect {
            id,
            payload: Some(detected),
            error: None,
        }
    }

    pub(super) async fn handle_workspace_detect_git(
        &self,
        id: String,
        payload: CommandWorkspaceDetectGitPayload,
    ) -> RelayMessage {
        let detected = match self
            .handle_workspace_detect(
                id.clone(),
                CommandWorkspaceDetectPayload { path: payload.path },
            )
            .await
        {
            RelayMessage::ResponseWorkspaceDetect {
                payload: Some(payload),
                error: None,
                ..
            } => payload,
            RelayMessage::ResponseWorkspaceDetect {
                error: Some(err), ..
            } => {
                return RelayMessage::ResponseWorkspaceDetectGit {
                    id,
                    payload: None,
                    error: Some(err),
                };
            }
            _ => {
                return RelayMessage::ResponseWorkspaceDetectGit {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("workspace_detect 未返回可用结果")),
                };
            }
        };

        let git = detected.git;
        RelayMessage::ResponseWorkspaceDetectGit {
            id,
            payload: Some(ResponseWorkspaceDetectGitPayload {
                is_git: git.is_some(),
                default_branch: git.as_ref().and_then(|item| item.default_branch.clone()),
                current_branch: git.as_ref().and_then(|item| item.current_branch.clone()),
                remote_url: git.as_ref().and_then(|item| item.remote_url.clone()),
            }),
            error: None,
        }
    }

    pub(super) async fn handle_browse_directory(
        &self,
        id: String,
        payload: CommandBrowseDirectoryPayload,
    ) -> RelayMessage {
        let result =
            tokio::task::spawn_blocking(move || browse_directory(payload.path.as_deref())).await;

        match result {
            Ok(Ok((current_path, entries))) => RelayMessage::ResponseBrowseDirectory {
                id,
                payload: Some(ResponseBrowseDirectoryPayload {
                    current_path,
                    entries,
                }),
                error: None,
            },
            Ok(Err(e)) => RelayMessage::ResponseBrowseDirectory {
                id,
                payload: None,
                error: Some(RelayError::io_error(e)),
            },
            Err(e) => RelayMessage::ResponseBrowseDirectory {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(format!("目录浏览任务失败: {e}"))),
            },
        }
    }
}

// ─── 目录浏览实现 ─────────────────────────────────────────

pub fn browse_directory(path: Option<&str>) -> Result<(String, Vec<BrowseDirectoryEntry>), String> {
    let path = path.map(|p| p.trim()).filter(|p| !p.is_empty());

    match path {
        None | Some("") => list_root_entries(),
        Some(dir_path) => list_directory_children(dir_path),
    }
}

/// 列出根级入口点：Windows 上返回可用盘符，其他平台返回 "/" 下的目录
fn list_root_entries() -> Result<(String, Vec<BrowseDirectoryEntry>), String> {
    #[cfg(windows)]
    {
        let mut entries = Vec::new();
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            let drive_path = std::path::Path::new(&drive);
            if drive_path.exists() {
                entries.push(BrowseDirectoryEntry {
                    name: format!("{}: 盘", letter as char),
                    path: drive.clone(),
                    is_dir: true,
                });
            }
        }
        Ok(("".to_string(), entries))
    }

    #[cfg(not(windows))]
    {
        list_directory_children("/")
    }
}

/// 列出指定目录下的子目录（仅目录，不递归）
fn list_directory_children(dir_path: &str) -> Result<(String, Vec<BrowseDirectoryEntry>), String> {
    let path = std::path::Path::new(dir_path);
    if !path.exists() {
        return Err(format!("路径不存在: {dir_path}"));
    }
    if !path.is_dir() {
        return Err(format!("不是目录: {dir_path}"));
    }

    let canonical = std::fs::canonicalize(path).map_err(|e| format!("路径规范化失败: {e}"))?;
    let current_path = normalize_display_path(&canonical);

    let read_dir = std::fs::read_dir(&canonical).map_err(|e| format!("无法读取目录: {e}"))?;

    let mut entries: Vec<BrowseDirectoryEntry> = Vec::new();

    for entry in read_dir.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if !ft.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if should_skip_directory(&name, &metadata) {
            continue;
        }

        let full_path = normalize_display_path(&entry.path());
        entries.push(BrowseDirectoryEntry {
            name,
            path: full_path,
            is_dir: true,
        });
    }

    entries.sort_by_key(|e| e.name.to_lowercase());
    Ok((current_path, entries))
}

fn should_skip_directory(name: &str, metadata: &std::fs::Metadata) -> bool {
    if name.starts_with('.') || name.starts_with('$') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        let attrs = metadata.file_attributes();
        if (attrs & FILE_ATTRIBUTE_HIDDEN) != 0 || (attrs & FILE_ATTRIBUTE_SYSTEM) != 0 {
            return true;
        }
    }
    false
}

fn normalize_display_path(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", rest);
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    raw.to_string()
}
