use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::runtime::Mount;
use crate::vfs::*;
use agentdash_spi::{MountCapability, Vfs};
use async_trait::async_trait;

use super::inline_persistence::InlineContentOverlay;

pub struct TextSearchParams<'a> {
    pub mount_id: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub is_regex: bool,
    pub include_glob: Option<&'a str>,
    pub max_results: usize,
    pub context_lines: usize,
    pub overlay: Option<&'a InlineContentOverlay>,
}

// ─── Service ────────────────────────────────────────────────

#[derive(Clone)]
pub struct RelayVfsService {
    mount_provider_registry: Arc<MountProviderRegistry>,
}

impl RelayVfsService {
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
        story: Option<&agentdash_domain::story::Story>,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        target: SessionMountTarget,
        agent_type: Option<&str>,
    ) -> Result<Vfs, String> {
        build_derived_vfs(project, story, workspace, agent_type, target)
    }

    pub fn list_mounts(&self, vfs: &Vfs) -> Vec<agentdash_spi::Mount> {
        vfs.mounts.clone()
    }

    pub async fn read_text(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<ReadResult, String> {
        let runtime_vfs = vfs.clone();
        let mount = resolve_mount(&runtime_vfs, &target.mount_id, MountCapability::Read)?;
        let path = normalize_mount_relative_path(&target.path, false)?;

        if let Some(ov) = overlay
            && let Some(override_state) = ov.read_override(&mount.id, &path).await
        {
            return match override_state {
                Some(content) => Ok(ReadResult::new(path, content)),
                None => Err(format!("文件不存在: {}", target.path)),
            };
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext {
                identity: identity.cloned(),
            };
            return provider
                .read_text(mount, &path, &ctx)
                .await
                .map_err(|e| e.to_string());
        }

        Err(format!("unregistered mount provider: {}", mount.provider))
    }

    pub async fn write_text(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        content: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<(), String> {
        let runtime_vfs = vfs.clone();
        let mount = resolve_mount(&runtime_vfs, &target.mount_id, MountCapability::Write)?;
        let path = normalize_mount_relative_path(&target.path, false)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let ov = overlay.ok_or_else(|| {
                format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能写入",
                    mount.id
                )
            })?;
            return ov.write(mount, &path, content).await;
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext {
                identity: identity.cloned(),
            };
            return provider
                .write_text(mount, &path, content, &ctx)
                .await
                .map_err(|e| e.to_string());
        }

        Err(format!("unregistered mount provider: {}", mount.provider))
    }

    pub async fn apply_patch(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        patch: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<ApplyPatchResult, String> {
        let runtime_vfs = vfs.clone();
        let mount = resolve_mount(&runtime_vfs, mount_id, MountCapability::Write)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let ov = overlay.ok_or_else(|| {
                format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能应用 patch",
                    mount.id
                )
            })?;
            let target = InlineOverlayPatchTarget {
                mount,
                overlay: ov,
                provider_registry: &self.mount_provider_registry,
            };
            let result = crate::vfs::apply_patch_to_target(&target, patch)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(ApplyPatchResult {
                added: result.added,
                modified: result.modified,
                deleted: result.deleted,
            });
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext {
                identity: identity.cloned(),
            };
            let target = ProviderPatchTarget {
                provider: provider.as_ref(),
                mount,
                ctx: &ctx,
            };
            match crate::vfs::apply_patch_to_target(&target, patch).await {
                Ok(result) => {
                    return Ok(ApplyPatchResult {
                        added: result.added,
                        modified: result.modified,
                        deleted: result.deleted,
                    });
                }
                Err(crate::vfs::ApplyPatchError::Capabilities(cap_error)) => {
                    let request = ApplyPatchRequest {
                        patch: patch.to_string(),
                    };
                    return provider
                        .apply_patch(mount, &request, &ctx)
                        .await
                        .map_err(|native_err| {
                            format!(
                                "patch 组合执行不可用（{cap_error}），且 provider 原生 apply_patch 失败: {native_err}"
                            )
                        });
                }
                Err(other) => return Err(other.to_string()),
            }
        }

        Err(format!("unregistered mount provider: {}", mount.provider))
    }

    /// 跨 mount apply_patch —— 解析 patch 条目中的路径前缀，按 mount 分组独立执行。
    ///
    /// patch 中的文件路径支持 `mount_id://relative/path` 格式；
    /// 不含前缀的路径使用 `default_mount_id`（或 VFS 默认 mount）。
    /// 每个 mount 组独立执行，支持 partial success。
    pub async fn apply_patch_multi(
        &self,
        vfs: &Vfs,
        default_mount_id: Option<&str>,
        patch: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<MultiMountPatchResult, String> {
        let entries = parse_patch_text(patch).map_err(|e| format!("patch 解析失败: {e}"))?;
        if entries.is_empty() {
            return Err("没有检测到任何文件改动".to_string());
        }

        let fallback_mount_id = match default_mount_id {
            Some(id) if !id.trim().is_empty() => id.to_string(),
            _ => resolve_mount_id(vfs, None)?,
        };

        // 按 mount 分组
        let mut grouped: BTreeMap<String, Vec<PatchEntry>> = BTreeMap::new();
        for mut entry in entries {
            let raw_path = entry.path().to_string_lossy().to_string();
            let (mount_id, relative) = split_mount_prefix(&raw_path, &fallback_mount_id);
            entry.set_path(PathBuf::from(&relative));
            grouped.entry(mount_id).or_default().push(entry);
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
                            message: error.clone(),
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
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<ApplyPatchAffectedPaths, String> {
        let mount = resolve_mount(vfs, mount_id, MountCapability::Write)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let ov = overlay.ok_or_else(|| {
                format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能应用 patch",
                    mount.id
                )
            })?;
            let target = InlineOverlayPatchTarget {
                mount,
                overlay: ov,
                provider_registry: &self.mount_provider_registry,
            };
            return apply_entries_to_target(&target, entries)
                .await
                .map_err(|e| e.to_string());
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext {
                identity: identity.cloned(),
            };
            let target = ProviderPatchTarget {
                provider: provider.as_ref(),
                mount,
                ctx: &ctx,
            };
            return apply_entries_to_target(&target, entries)
                .await
                .map_err(|e| e.to_string());
        }

        Err(format!("unregistered mount provider: {}", mount.provider))
    }

    pub async fn list(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        options: ListOptions,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<ListResult, String> {
        let runtime_vfs = vfs.clone();
        let mount = resolve_mount(&runtime_vfs, mount_id, MountCapability::List)?;
        let path = normalize_mount_relative_path(&options.path, true)?;

        if mount.provider == PROVIDER_INLINE_FS {
            // 从 provider（DB）加载文件列表，再合并 overlay
            let provider = self
                .mount_provider_registry
                .get(&mount.provider)
                .ok_or_else(|| "inline_fs provider 未注册".to_string())?;
            let ctx = MountOperationContext::default();
            if overlay.is_none() {
                // 无 overlay 直接委托 provider
                let opts = ListOptions {
                    path,
                    pattern: options.pattern,
                    recursive: options.recursive,
                };
                return provider
                    .list(mount, &opts, &ctx)
                    .await
                    .map_err(|e| e.to_string());
            }
            // 有 overlay：从 provider 读取完整文件映射，合并 overlay，再列出
            let full_opts = ListOptions {
                path: String::new(),
                pattern: None,
                recursive: true,
            };
            let full_result = provider
                .list(mount, &full_opts, &ctx)
                .await
                .map_err(|e| e.to_string())?;
            let mut files = BTreeMap::new();
            for entry in full_result.entries {
                if !entry.is_dir {
                    files.insert(entry.path, String::new());
                }
            }
            if let Some(ov) = overlay {
                ov.apply_to_files(&mount.id, &mut files).await;
            }
            return Ok(ListResult {
                entries: list_inline_entries(
                    &files,
                    &path,
                    options.pattern.as_deref(),
                    options.recursive,
                ),
            });
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext {
                identity: identity.cloned(),
            };
            let opts = ListOptions {
                path,
                pattern: options.pattern,
                recursive: options.recursive,
            };
            return provider
                .list(mount, &opts, &ctx)
                .await
                .map_err(|e| e.to_string());
        }

        Err(format!("unregistered mount provider: {}", mount.provider))
    }

    pub async fn exec(&self, vfs: &Vfs, request: &ExecRequest) -> Result<ExecResult, String> {
        let runtime_vfs = vfs.clone();
        let mount = resolve_mount(&runtime_vfs, &request.mount_id, MountCapability::Exec)?;
        let cwd = normalize_mount_relative_path(&request.cwd, true)?;

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext::default();
            let req = ExecRequest {
                mount_id: request.mount_id.clone(),
                cwd,
                command: request.command.clone(),
                timeout_ms: request.timeout_ms,
            };
            return provider
                .exec(mount, &req, &ctx)
                .await
                .map_err(|e| e.to_string());
        }

        Err(format!("unregistered mount provider: {}", mount.provider))
    }

    pub async fn search_text(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        path: &str,
        query: &str,
        max_results: usize,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<Vec<String>, String> {
        self.search_text_extended(
            vfs,
            &TextSearchParams {
                mount_id,
                path,
                query,
                is_regex: false,
                include_glob: None,
                max_results,
                context_lines: 0,
                overlay,
            },
        )
        .await
        .map(|(hits, _truncated)| hits)
    }

    pub async fn search_text_extended(
        &self,
        vfs: &Vfs,
        params: &TextSearchParams<'_>,
    ) -> Result<(Vec<String>, bool), String> {
        let runtime_vfs = vfs.clone();
        let mount = resolve_mount(&runtime_vfs, params.mount_id, MountCapability::Search)?;
        let base_path = normalize_mount_relative_path(params.path, true)?;

        if mount.provider == PROVIDER_INLINE_FS {
            return self.search_inline(mount, &base_path, params).await;
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext::default();
            let sq = SearchQuery {
                path: if base_path.is_empty() {
                    None
                } else {
                    Some(base_path)
                },
                pattern: params.query.to_string(),
                case_sensitive: true,
                max_results: Some(params.max_results),
            };
            let result = provider
                .search_text(mount, &sq, &ctx)
                .await
                .map_err(|e| e.to_string())?;
            let hits: Vec<String> = result
                .matches
                .iter()
                .map(|m| {
                    if let Some(line) = m.line {
                        format!("{}:{}: {}", m.path, line, m.content)
                    } else {
                        format!("{}: {}", m.path, m.content)
                    }
                })
                .collect();
            return Ok((hits, false));
        }

        Err(format!("unregistered mount provider: {}", mount.provider))
    }

    async fn search_inline(
        &self,
        mount: &Mount,
        base_path: &str,
        params: &TextSearchParams<'_>,
    ) -> Result<(Vec<String>, bool), String> {
        // 从 provider（DB）加载全部文件内容，再合并 overlay 后搜索
        let provider = self
            .mount_provider_registry
            .get(&mount.provider)
            .ok_or_else(|| "inline_fs provider 未注册".to_string())?;
        let ctx = MountOperationContext::default();
        let full_opts = ListOptions {
            path: String::new(),
            pattern: None,
            recursive: true,
        };
        let full_result = provider
            .list(mount, &full_opts, &ctx)
            .await
            .map_err(|e| e.to_string())?;
        let mut files = BTreeMap::new();
        for entry in full_result.entries {
            if !entry.is_dir {
                let read_result = provider
                    .read_text(mount, &entry.path, &ctx)
                    .await
                    .map_err(|e| e.to_string())?;
                files.insert(entry.path, read_result.content);
            }
        }
        if let Some(ov) = params.overlay {
            ov.apply_to_files(&mount.id, &mut files).await;
        }

        let re = if params.is_regex {
            Some(regex::Regex::new(params.query).map_err(|e| format!("无效正则: {e}"))?)
        } else {
            None
        };

        let mut hits = Vec::new();
        let mut truncated = false;

        for (file_path, content) in &files {
            if !file_path.starts_with(base_path.trim_start_matches("./").trim_start_matches('/'))
                && !base_path.is_empty()
                && base_path != "."
            {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            for (idx, line) in lines.iter().enumerate() {
                let matched = match &re {
                    Some(re) => re.is_match(line),
                    None => line.contains(params.query),
                };
                if matched {
                    let mut formatted = format!("{}:{}: {}", file_path, idx + 1, line.trim());
                    if params.context_lines > 0 {
                        let start = idx.saturating_sub(params.context_lines);
                        let end = (idx + 1 + params.context_lines).min(lines.len());
                        if start < idx {
                            let before: Vec<String> = (start..idx)
                                .map(|i| format!("{}:{}- {}", file_path, i + 1, lines[i].trim()))
                                .collect();
                            formatted = format!("{}\n{}", before.join("\n"), formatted);
                        }
                        if idx + 1 < end {
                            let after: Vec<String> = (idx + 1..end)
                                .map(|i| format!("{}:{}- {}", file_path, i + 1, lines[i].trim()))
                                .collect();
                            formatted = format!("{}\n{}", formatted, after.join("\n"));
                        }
                    }
                    hits.push(formatted);
                    if hits.len() >= params.max_results {
                        truncated = true;
                        return Ok((hits, truncated));
                    }
                }
            }
        }

        Ok((hits, truncated))
    }
}

/// 从 patch 内的路径拆出 mount 前缀。
/// `"main://src/lib.rs"` → `("main", "src/lib.rs")`
/// `"src/lib.rs"` → `(fallback, "src/lib.rs")`
fn split_mount_prefix(raw: &str, fallback: &str) -> (String, String) {
    if let Some(pos) = raw.find("://") {
        let mount_id = &raw[..pos];
        let relative = &raw[pos + 3..];
        let relative = relative.trim_start_matches('/');
        (mount_id.to_string(), relative.to_string())
    } else {
        (fallback.to_string(), raw.to_string())
    }
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
