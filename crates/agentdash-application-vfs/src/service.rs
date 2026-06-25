use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::search::{TextSearchParams, format_search_matches, grep_inline};
use crate::types::runtime_text_file_attributes;
use crate::*;
use agentdash_domain::common::Mount;
use agentdash_spi::{MountCapability, Vfs};
use async_trait::async_trait;

use super::inline_persistence::InlineContentOverlay;

pub(crate) use crate::search::is_vcs_path;

// ─── Service ────────────────────────────────────────────────

#[derive(Clone)]
pub struct VfsService {
    mount_provider_registry: Arc<MountProviderRegistry>,
}

struct MountDispatch {
    mount: Mount,
    path: String,
    provider: Arc<dyn MountProvider>,
    ctx: MountOperationContext,
}

pub struct BasicTextSearchRequest<'a> {
    pub mount_id: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub max_results: usize,
    pub overlay: Option<&'a InlineContentOverlay>,
    pub identity: Option<&'a agentdash_spi::platform::auth::AuthIdentity>,
}

impl VfsService {
    pub fn new(mount_provider_registry: Arc<MountProviderRegistry>) -> Self {
        Self {
            mount_provider_registry,
        }
    }

    pub fn session_for_workspace(
        &self,
        workspace: &agentdash_domain::workspace::Workspace,
    ) -> Result<Vfs, String> {
        build_workspace_vfs(workspace)
    }

    pub fn build_vfs(
        &self,
        project: &agentdash_domain::project::Project,
        project_vfs_mounts: &[agentdash_domain::project_vfs_mount::ProjectVfsMount],
        story: Option<&agentdash_domain::story::Story>,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        target: SessionMountTarget,
        agent_type: Option<&str>,
    ) -> Result<Vfs, String> {
        build_derived_vfs(
            project,
            project_vfs_mounts,
            story,
            workspace,
            agent_type,
            target,
        )
    }

    pub fn list_mounts(&self, vfs: &Vfs) -> Vec<agentdash_spi::Mount> {
        vfs.mounts.clone()
    }

    fn resolve_provider_dispatch(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        capability: MountCapability,
        raw_path: &str,
        allow_empty: bool,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<MountDispatch, MountError> {
        let mount = resolve_mount(vfs, mount_id, capability)
            .map_err(MountError::OperationFailed)?
            .clone();
        let path = normalize_mount_relative_path(raw_path, allow_empty)
            .map_err(MountError::OperationFailed)?;
        let provider = self
            .mount_provider_registry
            .get(&mount.provider)
            .ok_or_else(|| MountError::ProviderNotRegistered(mount.provider.clone()))?;
        let ctx = MountOperationContext {
            identity: identity.cloned(),
            runtime_vfs: Some(Arc::new(vfs.clone())),
            runtime_text_resolver: Some(Arc::new(self.clone())),
        };

        Ok(MountDispatch {
            mount,
            path,
            provider,
            ctx,
        })
    }

    /// 按行号 range 读取文本文件。
    ///
    /// 与 `read_text` 的区别：
    /// - `offset` 是 0-based 行号（与 SPI `read_text_range` 对齐；tool 层 1-based 自行转换）。
    /// - `limit = None` 表示读到 EOF（受 SPI 默认实现的全文加载约束）。
    /// - 错误类型保留 `MountError` 而非 String，方便调用方区分 NotFound 等场景
    ///   （fs_read tool 用此区分 ENOENT 友好提示路径）。
    pub async fn read_text_range(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        offset: usize,
        limit: Option<usize>,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<ReadResult, agentdash_spi::platform::mount::MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &target.mount_id,
            MountCapability::Read,
            &target.path,
            false,
            identity,
        )?;

        if let Some(ov) = overlay
            && let Some(override_state) = ov.read_override(&dispatch.mount.id, &dispatch.path).await
        {
            return match override_state {
                Some(content) => {
                    let sliced = content
                        .lines()
                        .skip(offset)
                        .take(limit.unwrap_or(usize::MAX))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(ReadResult::new(dispatch.path, sliced))
                }
                None => Err(MountError::NotFound(target.path.clone())),
            };
        }

        let started_at = Instant::now();
        let result = dispatch
            .provider
            .read_text_range(
                &dispatch.mount,
                &dispatch.path,
                offset,
                limit,
                &dispatch.ctx,
            )
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "read_text_range",
            &dispatch.path,
            started_at,
            result.is_ok(),
        );
        result
    }

    /// 在 `target` 所属 mount 内按相似度查找候选路径。
    /// fs_read 的 ENOENT 友好提示用此接口，传 `limit ≤ 5`。
    pub async fn suggest_paths(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        limit: usize,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<Vec<String>, MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &target.mount_id,
            MountCapability::List,
            &target.path,
            true,
            identity,
        )?;
        let basename = std::path::Path::new(&target.path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.path.clone());
        dispatch
            .provider
            .suggest_paths(&dispatch.mount, &basename, limit, &dispatch.ctx)
            .await
    }

    pub async fn read_text(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<ReadResult, MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &target.mount_id,
            MountCapability::Read,
            &target.path,
            false,
            identity,
        )?;

        if let Some(ov) = overlay
            && let Some(override_state) = ov.read_override(&dispatch.mount.id, &dispatch.path).await
        {
            return match override_state {
                Some(content) => Ok(ReadResult::new(dispatch.path, content)),
                None => Err(MountError::NotFound(target.path.clone())),
            };
        }

        let started_at = Instant::now();
        let result = dispatch
            .provider
            .read_text(&dispatch.mount, &dispatch.path, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "read_text",
            &dispatch.path,
            started_at,
            result.is_ok(),
        );
        result
    }

    pub async fn read_binary(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<BinaryReadResult, MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &target.mount_id,
            MountCapability::Read,
            &target.path,
            false,
            identity,
        )?;

        if let Some(ov) = overlay
            && let Some(override_state) = ov.read_override(&dispatch.mount.id, &dispatch.path).await
        {
            return match override_state {
                Some(_) => Err(MountError::OperationFailed(format!(
                    "文件是文本 overlay，不能按二进制读取: {}",
                    dispatch.path
                ))),
                None => Err(MountError::NotFound(target.path.clone())),
            };
        }

        let started_at = Instant::now();
        let result = dispatch
            .provider
            .read_binary(&dispatch.mount, &dispatch.path, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "read_binary",
            &dispatch.path,
            started_at,
            result.is_ok(),
        );
        result
    }

    pub async fn write_text(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        content: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<(), MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &target.mount_id,
            MountCapability::Write,
            &target.path,
            false,
            identity,
        )?;

        if is_inline_mount(&dispatch.mount) {
            let ov = overlay.ok_or_else(|| {
                MountError::OperationFailed(format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能写入",
                    dispatch.mount.id
                ))
            })?;
            return ov
                .write(&dispatch.mount, &dispatch.path, content)
                .await
                .map_err(MountError::OperationFailed);
        }

        let started_at = Instant::now();
        let result = dispatch
            .provider
            .write_text(&dispatch.mount, &dispatch.path, content, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "write_text",
            &dispatch.path,
            started_at,
            result.is_ok(),
        );
        result
    }

    pub async fn create_text(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        content: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<(), MountError> {
        match self.read_text(vfs, target, overlay, identity).await {
            Ok(_) => {
                return Err(MountError::OperationFailed(format!(
                    "目标文件已存在: {}",
                    target.path
                )));
            }
            Err(MountError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }

        self.write_text(vfs, target, content, overlay, identity)
            .await
    }

    pub async fn delete_text(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<(), MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &target.mount_id,
            MountCapability::Write,
            &target.path,
            false,
            identity,
        )?;

        self.read_text(
            vfs,
            &ResourceRef {
                mount_id: target.mount_id.clone(),
                path: dispatch.path.clone(),
            },
            overlay,
            identity,
        )
        .await?;

        if is_inline_mount(&dispatch.mount) {
            let ov = overlay.ok_or_else(|| {
                MountError::OperationFailed(format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能删除",
                    dispatch.mount.id
                ))
            })?;
            return ov
                .delete(&dispatch.mount, &dispatch.path)
                .await
                .map_err(MountError::OperationFailed);
        }

        let started_at = Instant::now();
        let result = dispatch
            .provider
            .delete_text(&dispatch.mount, &dispatch.path, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "delete_text",
            &dispatch.path,
            started_at,
            result.is_ok(),
        );
        result
    }

    pub async fn rename_text(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        from_path: &str,
        to_path: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<(), MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            mount_id,
            MountCapability::Write,
            from_path,
            false,
            identity,
        )?;
        let from_path = dispatch.path.clone();
        let to_path =
            normalize_mount_relative_path(to_path, false).map_err(MountError::OperationFailed)?;
        if from_path == to_path {
            return Ok(());
        }

        let source = self
            .read_text(
                vfs,
                &ResourceRef {
                    mount_id: mount_id.to_string(),
                    path: from_path.clone(),
                },
                overlay,
                identity,
            )
            .await?;

        if self
            .read_text(
                vfs,
                &ResourceRef {
                    mount_id: mount_id.to_string(),
                    path: to_path.clone(),
                },
                overlay,
                identity,
            )
            .await
            .is_ok()
        {
            return Err(MountError::OperationFailed(format!(
                "目标文件已存在: {to_path}"
            )));
        }

        if is_inline_mount(&dispatch.mount) {
            let ov = overlay.ok_or_else(|| {
                MountError::OperationFailed(format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能重命名",
                    dispatch.mount.id
                ))
            })?;
            ov.write(&dispatch.mount, &to_path, &source.content)
                .await
                .map_err(MountError::OperationFailed)?;
            return ov
                .delete(&dispatch.mount, &from_path)
                .await
                .map_err(MountError::OperationFailed);
        }

        let started_at = Instant::now();
        let result = dispatch
            .provider
            .rename_text(&dispatch.mount, &from_path, &to_path, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "rename_text",
            &format!("{from_path} -> {to_path}"),
            started_at,
            result.is_ok(),
        );
        result
    }

    pub async fn stat(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<RuntimeFileEntry, MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &target.mount_id,
            MountCapability::List,
            &target.path,
            true,
            identity,
        )?;
        let path = dispatch.path.clone();

        if path.is_empty() || path == "." {
            return Ok(RuntimeFileEntry::dir("."));
        }

        if is_inline_mount(&dispatch.mount)
            && let Some(ov) = overlay
            && let Some(override_state) = ov.read_override(&dispatch.mount.id, &path).await
        {
            return match override_state {
                Some(content) => Ok(RuntimeFileEntry::file(path)
                    .with_size(content.len() as u64)
                    .with_attributes(runtime_text_file_attributes())),
                None => Err(MountError::NotFound(target.path.clone())),
            };
        }

        let started_at = Instant::now();
        let result = dispatch
            .provider
            .stat(&dispatch.mount, &path, &dispatch.ctx)
            .await;
        log_vfs_operation_result(&dispatch.mount, "stat", &path, started_at, result.is_ok());
        match result {
            Ok(entry) => return Ok(entry),
            Err(MountError::NotSupported(_)) => {}
            Err(error) => return Err(error),
        }

        let parent = path
            .rsplit_once('/')
            .map(|(parent, _)| {
                if parent.is_empty() {
                    ".".to_string()
                } else {
                    parent.to_string()
                }
            })
            .unwrap_or_else(|| ".".to_string());
        let listed = self
            .list(
                vfs,
                &target.mount_id,
                ListOptions {
                    path: parent,
                    pattern: None,
                    recursive: false,
                },
                overlay,
                identity,
            )
            .await?;
        listed
            .entries
            .into_iter()
            .find(|entry| entry.path == path)
            .ok_or(MountError::NotFound(path))
    }

    pub async fn apply_patch(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        patch: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<ApplyPatchResult, MountError> {
        let mount = resolve_mount(vfs, mount_id, MountCapability::Write)
            .map_err(MountError::OperationFailed)?;

        if is_inline_mount(mount) {
            let ov = overlay.ok_or_else(|| {
                MountError::OperationFailed(format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能应用 patch",
                    mount.id
                ))
            })?;
            let target = InlineOverlayPatchTarget {
                mount,
                overlay: ov,
                provider_registry: &self.mount_provider_registry,
            };
            let result = crate::apply_patch_to_target(&target, patch)
                .await
                .map_err(|e| MountError::OperationFailed(e.to_string()))?;
            return Ok(ApplyPatchResult {
                added: result.added,
                modified: result.modified,
                deleted: result.deleted,
            });
        }

        let provider = self
            .mount_provider_registry
            .get(&mount.provider)
            .ok_or_else(|| MountError::ProviderNotRegistered(mount.provider.clone()))?;
        let ctx = MountOperationContext {
            identity: identity.cloned(),
            runtime_vfs: Some(Arc::new(vfs.clone())),
            runtime_text_resolver: Some(Arc::new(self.clone())),
        };
        let target = ProviderPatchTarget {
            provider: provider.as_ref(),
            mount,
            ctx: &ctx,
        };
        match crate::apply_patch_to_target(&target, patch).await {
            Ok(result) => Ok(ApplyPatchResult {
                added: result.added,
                modified: result.modified,
                deleted: result.deleted,
            }),
            Err(crate::ApplyPatchError::Capabilities(cap_error)) => {
                let request = ApplyPatchRequest {
                    patch: patch.to_string(),
                };
                return provider
                    .apply_patch(mount, &request, &ctx)
                    .await
                    .map_err(|native_err| {
                        MountError::OperationFailed(format!(
                            "patch 组合执行不可用（{cap_error}），且 provider 原生 apply_patch 失败: {native_err}"
                        ))
                    });
            }
            Err(other) => Err(MountError::OperationFailed(other.to_string())),
        }
    }

    /// 跨 mount apply_patch —— 解析 patch 条目中的路径前缀，按 mount 分组独立执行。
    ///
    /// patch 中的文件路径必须使用 `mount_id://relative/path` 格式。
    /// 每个 mount 组独立执行，支持 partial success。
    pub async fn apply_patch_multi(
        &self,
        vfs: &Vfs,
        patch: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<MultiMountPatchResult, MountError> {
        let entries = parse_patch_text(patch)
            .map_err(|e| MountError::OperationFailed(format!("patch 解析失败: {e}")))?;
        if entries.is_empty() {
            return Err(MountError::OperationFailed(
                "没有检测到任何文件改动".to_string(),
            ));
        }

        // 按 mount 分组
        let mut grouped: BTreeMap<String, Vec<PatchEntry>> = BTreeMap::new();
        for mut entry in entries {
            let targets =
                normalize_patch_entry_targets(&mut entry).map_err(MountError::OperationFailed)?;
            grouped
                .entry(targets.primary.mount_id)
                .or_default()
                .push(entry);
        }

        let mut result = MultiMountPatchResult::default();

        for (mount_id, group) in &grouped {
            match self
                .apply_entry_group(vfs, mount_id, group, overlay, identity)
                .await
            {
                Ok(affected) => {
                    let prefix = if grouped.len() > 1 {
                        format!("{mount_id}://")
                    } else {
                        String::new()
                    };
                    result
                        .added
                        .extend(affected.added.iter().map(|p| format!("{prefix}{p}")));
                    result
                        .modified
                        .extend(affected.modified.iter().map(|p| format!("{prefix}{p}")));
                    result
                        .deleted
                        .extend(affected.deleted.iter().map(|p| format!("{prefix}{p}")));
                }
                Err(error) => {
                    for entry in group {
                        result.errors.push(PatchEntryError {
                            mount_id: mount_id.clone(),
                            path: entry.path().to_string_lossy().to_string(),
                            message: error.to_string(),
                        });
                    }
                }
            }
        }

        Ok(result)
    }

    /// 对单个 mount 的一组 PatchEntry 执行 apply。
    async fn apply_entry_group(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        entries: &[PatchEntry],
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<ApplyPatchAffectedPaths, MountError> {
        let mount = resolve_mount(vfs, mount_id, MountCapability::Write)
            .map_err(MountError::OperationFailed)?;

        if is_inline_mount(mount) {
            let ov = overlay.ok_or_else(|| {
                MountError::OperationFailed(format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能应用 patch",
                    mount.id
                ))
            })?;
            let target = InlineOverlayPatchTarget {
                mount,
                overlay: ov,
                provider_registry: &self.mount_provider_registry,
            };
            return apply_entries_to_target(&target, entries)
                .await
                .map_err(|e| MountError::OperationFailed(e.to_string()));
        }

        let provider = self
            .mount_provider_registry
            .get(&mount.provider)
            .ok_or_else(|| MountError::ProviderNotRegistered(mount.provider.clone()))?;
        let ctx = MountOperationContext {
            identity: identity.cloned(),
            runtime_vfs: Some(Arc::new(vfs.clone())),
            runtime_text_resolver: Some(Arc::new(self.clone())),
        };
        let target = ProviderPatchTarget {
            provider: provider.as_ref(),
            mount,
            ctx: &ctx,
        };
        apply_entries_to_target(&target, entries)
            .await
            .map_err(|e| MountError::OperationFailed(e.to_string()))
    }

    pub async fn list(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        options: ListOptions,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<ListResult, MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            mount_id,
            MountCapability::List,
            &options.path,
            true,
            identity,
        )?;

        if is_inline_mount(&dispatch.mount) {
            // 从 provider（DB）加载文件列表，再合并 overlay
            if overlay.is_none() {
                // 无 overlay 直接委托 provider
                let opts = ListOptions {
                    path: dispatch.path,
                    pattern: options.pattern,
                    recursive: options.recursive,
                };
                let started_at = Instant::now();
                let result = dispatch
                    .provider
                    .list(&dispatch.mount, &opts, &dispatch.ctx)
                    .await;
                log_vfs_operation_result(
                    &dispatch.mount,
                    "list",
                    &opts.path,
                    started_at,
                    result.is_ok(),
                );
                return result;
            }
            // 有 overlay：从 provider 读取完整文件映射，合并 overlay，再列出
            let full_opts = ListOptions {
                path: String::new(),
                pattern: None,
                recursive: true,
            };
            let started_at = Instant::now();
            let full_result = dispatch
                .provider
                .list(&dispatch.mount, &full_opts, &dispatch.ctx)
                .await;
            log_vfs_operation_result(
                &dispatch.mount,
                "list",
                &full_opts.path,
                started_at,
                full_result.is_ok(),
            );
            let full_result = full_result?;
            let mut files = BTreeMap::new();
            for entry in full_result.entries {
                if !entry.is_dir {
                    files.insert(entry.path, String::new());
                }
            }
            if let Some(ov) = overlay {
                ov.apply_to_files(&dispatch.mount.id, &mut files).await;
            }
            return Ok(ListResult {
                entries: list_inline_entries(
                    &files,
                    &dispatch.path,
                    options.pattern.as_deref(),
                    options.recursive,
                ),
            });
        }

        let opts = ListOptions {
            path: dispatch.path,
            pattern: options.pattern,
            recursive: options.recursive,
        };
        let started_at = Instant::now();
        let result = dispatch
            .provider
            .list(&dispatch.mount, &opts, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "list",
            &opts.path,
            started_at,
            result.is_ok(),
        );
        result
    }

    pub async fn exec(&self, vfs: &Vfs, request: &ExecRequest) -> Result<ExecResult, MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            &request.mount_id,
            MountCapability::Exec,
            &request.cwd,
            true,
            None,
        )?;
        let req = ExecRequest {
            mount_id: request.mount_id.clone(),
            cwd: dispatch.path,
            command: request.command.clone(),
            timeout_ms: request.timeout_ms,
            streaming_call_id: request.streaming_call_id.clone(),
        };
        let started_at = Instant::now();
        let result = dispatch
            .provider
            .exec(&dispatch.mount, &req, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "exec",
            &req.cwd,
            started_at,
            result.is_ok(),
        );
        result
    }

    pub async fn search_text(
        &self,
        vfs: &Vfs,
        request: BasicTextSearchRequest<'_>,
    ) -> Result<Vec<String>, MountError> {
        self.search_text_extended(
            vfs,
            &TextSearchParams {
                mount_id: request.mount_id,
                path: request.path,
                query: request.query,
                is_regex: false,
                include_glob: None,
                max_results: request.max_results,
                context_lines: 0,
                overlay: request.overlay,
                identity: request.identity,
                case_sensitive: true,
                before_lines: 0,
                after_lines: 0,
                multiline: false,
                output_mode: agentdash_spi::platform::mount::SearchOutputMode::Content,
            },
        )
        .await
        .map(|(hits, _truncated)| hits)
    }

    pub async fn search_text_extended(
        &self,
        vfs: &Vfs,
        params: &TextSearchParams<'_>,
    ) -> Result<(Vec<String>, bool), MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            params.mount_id,
            MountCapability::Search,
            params.path,
            true,
            params.identity,
        )?;
        let base_path = dispatch.path.clone();

        if is_inline_mount(&dispatch.mount) {
            // 通用 inline 搜索复用 grep_inline；当 params 的 grep 字段为空时
            // 行为与 substring 等价（is_regex=false → substring，include_glob/
            // context_lines/multiline 都默认零值）。
            return grep_inline(
                &self.mount_provider_registry,
                &dispatch.mount,
                &base_path,
                params,
            )
            .await;
        }

        // search_text_extended 仅承载通用搜索语义（substring）；grep 字段在
        // grep_text_extended 路径处理。这里只填 SearchQuery 的 4 个通用字段。
        let sq = SearchQuery {
            path: if base_path.is_empty() {
                None
            } else {
                Some(base_path)
            },
            pattern: params.query.to_string(),
            case_sensitive: params.case_sensitive,
            max_results: Some(params.max_results),
        };
        let started_at = Instant::now();
        let result = dispatch
            .provider
            .search_text(&dispatch.mount, &sq, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "search_text",
            sq.path.as_deref().unwrap_or("."),
            started_at,
            result.is_ok(),
        );
        let result = result?;
        let truncated = result.truncated;
        let hits = format_search_matches(&result.matches);
        Ok((hits, truncated))
    }

    /// grep 风格搜索（pattern 始终正则；支持 include_glob / context / multiline /
    /// output_mode）。fs_grep tool 调用此方法；通用搜索请用 [`search_text_extended`]。
    pub async fn grep_text_extended(
        &self,
        vfs: &Vfs,
        params: &TextSearchParams<'_>,
    ) -> Result<(Vec<String>, bool), MountError> {
        let dispatch = self.resolve_provider_dispatch(
            vfs,
            params.mount_id,
            MountCapability::Search,
            params.path,
            true,
            params.identity,
        )?;
        let base_path = dispatch.path.clone();

        if is_inline_mount(&dispatch.mount) {
            return grep_inline(
                &self.mount_provider_registry,
                &dispatch.mount,
                &base_path,
                params,
            )
            .await;
        }

        let gq = GrepQuery {
            base: SearchQuery {
                path: if base_path.is_empty() {
                    None
                } else {
                    Some(base_path)
                },
                pattern: params.query.to_string(),
                case_sensitive: params.case_sensitive,
                max_results: Some(params.max_results),
            },
            include_glob: params.include_glob.map(|s| s.to_string()),
            context_lines: params.context_lines,
            before_lines: params.before_lines,
            after_lines: params.after_lines,
            multiline: params.multiline,
            output_mode: params.output_mode,
        };
        let started_at = Instant::now();
        let result = dispatch
            .provider
            .grep_text(&dispatch.mount, &gq, &dispatch.ctx)
            .await;
        log_vfs_operation_result(
            &dispatch.mount,
            "grep_text",
            gq.base.path.as_deref().unwrap_or("."),
            started_at,
            result.is_ok(),
        );
        let result = result?;
        let truncated = result.truncated;
        let hits = format_search_matches(&result.matches);
        Ok((hits, truncated))
    }
}

#[async_trait]
impl agentdash_spi::platform::mount::MountRuntimeTextResolver for VfsService {
    async fn read_runtime_text(
        &self,
        vfs: &Vfs,
        uri: &str,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<ReadResult, MountError> {
        let target = parse_mount_uri(uri, vfs).map_err(MountError::OperationFailed)?;
        self.read_text(vfs, &target, None, identity).await
    }
}

fn log_vfs_operation_result(
    mount: &Mount,
    operation: &str,
    path: &str,
    started_at: Instant,
    success: bool,
) {
    tracing::debug!(
        provider = %mount.provider,
        mount_id = %mount.id,
        operation,
        path,
        duration_ms = started_at.elapsed().as_millis(),
        success,
        "VFS mount operation completed"
    );
}

fn is_inline_mount(mount: &Mount) -> bool {
    mount.provider == PROVIDER_INLINE_FS
}

struct ProviderPatchTarget<'a> {
    provider: &'a dyn MountProvider,
    mount: &'a Mount,
    ctx: &'a MountOperationContext,
}

#[async_trait]
impl ApplyPatchTarget for ProviderPatchTarget<'_> {
    fn edit_capabilities(&self) -> MountEditCapabilities {
        self.provider.edit_capabilities(self.mount)
    }

    async fn read_text(&self, path: &str) -> Result<String, ApplyPatchError> {
        self.provider
            .read_text(self.mount, path, self.ctx)
            .await
            .map(|result| result.content)
            .map_err(|e| ApplyPatchError::Apply(e.to_string()))
    }

    async fn write_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError> {
        self.provider
            .write_text(self.mount, path, content, self.ctx)
            .await
            .map_err(|e| ApplyPatchError::Apply(e.to_string()))
    }

    async fn create_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError> {
        let patch = build_add_file_patch(path, content);
        let request = ApplyPatchRequest { patch };
        match self
            .provider
            .apply_patch(self.mount, &request, self.ctx)
            .await
        {
            Ok(_) => Ok(()),
            Err(MountError::NotSupported(_)) => self.write_text(path, content).await,
            Err(e) => Err(ApplyPatchError::Apply(e.to_string())),
        }
    }

    async fn delete_text(&self, path: &str) -> Result<(), ApplyPatchError> {
        self.provider
            .delete_text(self.mount, path, self.ctx)
            .await
            .map_err(|e| ApplyPatchError::Apply(e.to_string()))
    }

    async fn rename_text(&self, from_path: &str, to_path: &str) -> Result<(), ApplyPatchError> {
        self.provider
            .rename_text(self.mount, from_path, to_path, self.ctx)
            .await
            .map_err(|e| ApplyPatchError::Apply(e.to_string()))
    }
}

fn build_add_file_patch(path: &str, content: &str) -> String {
    let mut patch = String::new();
    patch.push_str("*** Begin Patch\n");
    patch.push_str(&format!("*** Add File: {path}\n"));

    let lines: Vec<&str> = if let Some(stripped) = content.strip_suffix('\n') {
        stripped.split('\n').collect()
    } else {
        content.split('\n').collect()
    };
    for line in lines {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }

    patch.push_str("*** End Patch");
    patch
}

struct InlineOverlayPatchTarget<'a> {
    mount: &'a Mount,
    overlay: &'a InlineContentOverlay,
    provider_registry: &'a MountProviderRegistry,
}

#[async_trait]
impl ApplyPatchTarget for InlineOverlayPatchTarget<'_> {
    fn edit_capabilities(&self) -> MountEditCapabilities {
        MountEditCapabilities {
            create: true,
            delete: true,
            rename: true,
        }
    }

    async fn read_text(&self, path: &str) -> Result<String, ApplyPatchError> {
        let normalized =
            normalize_mount_relative_path(path, false).map_err(ApplyPatchError::InvalidPath)?;
        if let Some(override_state) = self
            .overlay
            .read_override(&self.mount.id, &normalized)
            .await
        {
            return match override_state {
                Some(content) => Ok(content),
                None => Err(ApplyPatchError::Apply(format!(
                    "目标文件不存在: {normalized}"
                ))),
            };
        }
        // 从 provider（DB）读取
        let provider = self
            .provider_registry
            .get(&self.mount.provider)
            .ok_or_else(|| ApplyPatchError::Apply("inline_fs provider 未注册".to_string()))?;
        let ctx = MountOperationContext::default();
        provider
            .read_text(self.mount, &normalized, &ctx)
            .await
            .map(|result| result.content)
            .map_err(|e| ApplyPatchError::Apply(e.to_string()))
    }

    async fn write_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError> {
        let normalized =
            normalize_mount_relative_path(path, false).map_err(ApplyPatchError::InvalidPath)?;
        self.overlay
            .write(self.mount, &normalized, content)
            .await
            .map_err(ApplyPatchError::Apply)
    }

    async fn delete_text(&self, path: &str) -> Result<(), ApplyPatchError> {
        let normalized =
            normalize_mount_relative_path(path, false).map_err(ApplyPatchError::InvalidPath)?;
        self.overlay
            .delete(self.mount, &normalized)
            .await
            .map_err(ApplyPatchError::Apply)
    }

    async fn rename_text(&self, from_path: &str, to_path: &str) -> Result<(), ApplyPatchError> {
        let source = self.read_text(from_path).await?;
        self.write_text(to_path, &source).await?;
        self.delete_text(from_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::platform::auth::AuthIdentity;
    use std::path::PathBuf;
    use tokio::sync::Mutex;

    struct IdentityCaptureProvider {
        provider_id: String,
        calls: Mutex<Vec<(String, Option<String>)>>,
    }

    impl IdentityCaptureProvider {
        fn new(provider_id: impl Into<String>) -> Self {
            Self {
                provider_id: provider_id.into(),
                calls: Mutex::new(Vec::new()),
            }
        }

        async fn record(&self, operation: &str, ctx: &MountOperationContext) {
            self.calls.lock().await.push((
                operation.to_string(),
                ctx.identity
                    .as_ref()
                    .map(|identity| identity.user_id.clone()),
            ));
        }

        async fn captured_calls(&self) -> Vec<(String, Option<String>)> {
            self.calls.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl MountProvider for IdentityCaptureProvider {
        fn provider_id(&self) -> &str {
            &self.provider_id
        }

        fn supported_capabilities(&self) -> Vec<&str> {
            vec!["read", "list", "search"]
        }

        async fn read_text(
            &self,
            _mount: &Mount,
            path: &str,
            ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            self.record("read_text", ctx).await;
            Ok(ReadResult::new(path, "inline search identity\nother"))
        }

        async fn write_text(
            &self,
            _mount: &Mount,
            _path: &str,
            _content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            Err(MountError::NotSupported(
                "identity test provider".to_string(),
            ))
        }

        async fn list(
            &self,
            _mount: &Mount,
            _options: &ListOptions,
            ctx: &MountOperationContext,
        ) -> Result<ListResult, MountError> {
            self.record("list", ctx).await;
            Ok(ListResult {
                entries: vec![RuntimeFileEntry::file("docs/inline.md")],
            })
        }

        async fn search_text(
            &self,
            _mount: &Mount,
            _query: &SearchQuery,
            ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            self.record("search_text", ctx).await;
            Ok(SearchResult {
                matches: vec![SearchMatch {
                    path: "docs/provider.md".to_string(),
                    line: Some(3),
                    content: "found provider".to_string(),
                }],
                truncated: false,
            })
        }

        async fn grep_text(
            &self,
            _mount: &Mount,
            _query: &GrepQuery,
            ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            self.record("grep_text", ctx).await;
            Ok(SearchResult {
                matches: vec![SearchMatch {
                    path: "docs/provider.md".to_string(),
                    line: Some(5),
                    content: "grep provider".to_string(),
                }],
                truncated: false,
            })
        }
    }

    fn search_identity_fixture(
        provider_id: &str,
    ) -> (VfsService, Vfs, Arc<IdentityCaptureProvider>) {
        let provider = Arc::new(IdentityCaptureProvider::new(provider_id));
        let mut registry = MountProviderRegistry::new();
        registry.register(provider.clone());
        let service = VfsService::new(Arc::new(registry));
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: provider_id.to_string(),
                backend_id: "backend".to_string(),
                root_ref: "capture://root".to_string(),
                capabilities: vec![
                    MountCapability::Read,
                    MountCapability::List,
                    MountCapability::Search,
                ],
                default_write: false,
                display_name: "Capture".to_string(),
                metadata: serde_json::json!({}),
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        (service, vfs, provider)
    }

    fn search_identity() -> AuthIdentity {
        AuthIdentity::system_routine("search-identity")
    }

    fn search_identity_params<'a>(
        identity: &'a AuthIdentity,
        query: &'a str,
    ) -> TextSearchParams<'a> {
        TextSearchParams {
            mount_id: "main",
            path: ".",
            query,
            is_regex: true,
            include_glob: None,
            max_results: 10,
            context_lines: 0,
            overlay: None,
            identity: Some(identity),
            case_sensitive: true,
            before_lines: 0,
            after_lines: 0,
            multiline: false,
            output_mode: SearchOutputMode::Content,
        }
    }

    #[tokio::test]
    async fn search_identity_provider_search_receives_identity() {
        let (service, vfs, provider) = search_identity_fixture("search_identity_provider");
        let identity = search_identity();

        let (hits, truncated) = service
            .search_text_extended(&vfs, &search_identity_params(&identity, "provider"))
            .await
            .expect("search text");

        assert!(!truncated);
        assert_eq!(hits, vec!["docs/provider.md:3: found provider"]);
        assert_eq!(
            provider.captured_calls().await,
            vec![("search_text".to_string(), Some(identity.user_id.clone()))]
        );
    }

    #[tokio::test]
    async fn search_identity_provider_grep_receives_identity() {
        let (service, vfs, provider) = search_identity_fixture("search_identity_provider");
        let identity = search_identity();

        let (hits, truncated) = service
            .grep_text_extended(&vfs, &search_identity_params(&identity, "provider"))
            .await
            .expect("grep text");

        assert!(!truncated);
        assert_eq!(hits, vec!["docs/provider.md:5: grep provider"]);
        assert_eq!(
            provider.captured_calls().await,
            vec![("grep_text".to_string(), Some(identity.user_id.clone()))]
        );
    }

    #[tokio::test]
    async fn search_identity_inline_grep_receives_identity() {
        let (service, vfs, provider) = search_identity_fixture(PROVIDER_INLINE_FS);
        let identity = search_identity();

        let (hits, truncated) = service
            .grep_text_extended(
                &vfs,
                &search_identity_params(&identity, "inline search identity"),
            )
            .await
            .expect("inline grep");

        assert!(!truncated);
        assert_eq!(hits, vec!["docs/inline.md:1: inline search identity"]);
        assert_eq!(
            provider.captured_calls().await,
            vec![
                ("list".to_string(), Some(identity.user_id.clone())),
                ("read_text".to_string(), Some(identity.user_id.clone())),
            ]
        );
    }

    #[test]
    fn patch_entry_normalizes_same_mount_move_target() {
        let mut entry = PatchEntry::UpdateFile {
            path: PathBuf::from("main://src//old.rs"),
            move_path: Some(PathBuf::from("main://src/./new.rs")),
            chunks: Vec::new(),
        };

        let targets = normalize_patch_entry_targets(&mut entry).expect("normalize");

        assert_eq!(targets.primary.mount_id, "main");
        assert_eq!(targets.primary.relative_path, "src/old.rs");
        assert_eq!(
            targets.move_target,
            Some(PatchPathTarget {
                mount_id: "main".to_string(),
                relative_path: "src/new.rs".to_string(),
            })
        );
        assert_eq!(entry.path(), PathBuf::from("src/old.rs").as_path());
        match entry {
            PatchEntry::UpdateFile { move_path, .. } => {
                assert_eq!(move_path, Some(PathBuf::from("src/new.rs")));
            }
            _ => panic!("expected update entry"),
        }
    }

    #[test]
    fn patch_entry_rejects_cross_mount_move_target() {
        let mut entry = PatchEntry::UpdateFile {
            path: PathBuf::from("main://src/old.rs"),
            move_path: Some(PathBuf::from("other://src/new.rs")),
            chunks: Vec::new(),
        };

        let err = normalize_patch_entry_targets(&mut entry).expect_err("cross mount");

        assert!(err.contains("跨 mount move"));
    }

    #[test]
    fn patch_entry_rejects_escaping_move_target() {
        let mut entry = PatchEntry::UpdateFile {
            path: PathBuf::from("main://src/old.rs"),
            move_path: Some(PathBuf::from("main://../new.rs")),
            chunks: Vec::new(),
        };

        let err = normalize_patch_entry_targets(&mut entry).expect_err("escaping move");

        assert!(err.contains("路径越界"));
    }
}
