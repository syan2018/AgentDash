use std::sync::Arc;

use crate::address_space::*;
use crate::runtime::Mount;
use agentdash_spi::{AddressSpace, MountCapability};
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
pub struct RelayAddressSpaceService {
    mount_provider_registry: Arc<MountProviderRegistry>,
}

impl RelayAddressSpaceService {
    pub fn new(mount_provider_registry: Arc<MountProviderRegistry>) -> Self {
        Self {
            mount_provider_registry,
        }
    }

    pub fn session_for_workspace(
        &self,
        workspace: &agentdash_domain::workspace::Workspace,
    ) -> Result<AddressSpace, String> {
        build_workspace_address_space(workspace)
    }

    pub fn build_address_space(
        &self,
        project: &agentdash_domain::project::Project,
        story: Option<&agentdash_domain::story::Story>,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        target: SessionMountTarget,
        agent_type: Option<&str>,
    ) -> Result<AddressSpace, String> {
        build_derived_address_space(project, story, workspace, agent_type, target)
    }

    pub fn list_mounts(&self, address_space: &AddressSpace) -> Vec<agentdash_spi::Mount> {
        address_space.mounts.clone()
    }

    pub async fn read_text(
        &self,
        address_space: &AddressSpace,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<ReadResult, String> {
        let runtime_address_space = address_space.clone();
        let mount = resolve_mount(
            &runtime_address_space,
            &target.mount_id,
            MountCapability::Read,
        )?;
        let path = normalize_mount_relative_path(&target.path, false)?;

        if let Some(ov) = overlay
            && let Some(override_state) = ov.read_override(&mount.id, &path).await
        {
            return match override_state {
                Some(content) => Ok(ReadResult { path, content }),
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

        Err(format!("未注册的 mount provider: {}", mount.provider))
    }

    pub async fn write_text(
        &self,
        address_space: &AddressSpace,
        target: &ResourceRef,
        content: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<(), String> {
        let runtime_address_space = address_space.clone();
        let mount = resolve_mount(
            &runtime_address_space,
            &target.mount_id,
            MountCapability::Write,
        )?;
        let path = normalize_mount_relative_path(&target.path, false)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let ov = overlay.ok_or_else(|| {
                format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能写入",
                    mount.id
                )
            })?;
            return ov
                .write(&runtime_address_space, mount, &path, content)
                .await;
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

        Err(format!("未注册的 mount provider: {}", mount.provider))
    }

    pub async fn apply_patch(
        &self,
        address_space: &AddressSpace,
        mount_id: &str,
        patch: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<ApplyPatchResult, String> {
        let runtime_address_space = address_space.clone();
        let mount = resolve_mount(&runtime_address_space, mount_id, MountCapability::Write)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let ov = overlay.ok_or_else(|| {
                format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能应用 patch",
                    mount.id
                )
            })?;
            let target = InlineOverlayPatchTarget {
                address_space: &runtime_address_space,
                mount,
                overlay: ov,
            };
            let result = crate::address_space::apply_patch_to_target(&target, patch)
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
            match crate::address_space::apply_patch_to_target(&target, patch).await {
                Ok(result) => {
                    return Ok(ApplyPatchResult {
                        added: result.added,
                        modified: result.modified,
                        deleted: result.deleted,
                    });
                }
                Err(crate::address_space::ApplyPatchError::Capabilities(cap_error)) => {
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

        Err(format!("未注册的 mount provider: {}", mount.provider))
    }

    pub async fn list(
        &self,
        address_space: &AddressSpace,
        mount_id: &str,
        options: ListOptions,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::auth::AuthIdentity>,
    ) -> Result<ListResult, String> {
        let runtime_address_space = address_space.clone();
        let mount = resolve_mount(&runtime_address_space, mount_id, MountCapability::List)?;
        let path = normalize_mount_relative_path(&options.path, true)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let mut files = inline_files_from_mount(mount)?;
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

        Err(format!("未注册的 mount provider: {}", mount.provider))
    }

    pub async fn exec(
        &self,
        address_space: &AddressSpace,
        request: &ExecRequest,
    ) -> Result<ExecResult, String> {
        let runtime_address_space = address_space.clone();
        let mount = resolve_mount(
            &runtime_address_space,
            &request.mount_id,
            MountCapability::Exec,
        )?;
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

        Err(format!("未注册的 mount provider: {}", mount.provider))
    }

    pub async fn search_text(
        &self,
        address_space: &AddressSpace,
        mount_id: &str,
        path: &str,
        query: &str,
        max_results: usize,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<Vec<String>, String> {
        self.search_text_extended(
            address_space,
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
        address_space: &AddressSpace,
        params: &TextSearchParams<'_>,
    ) -> Result<(Vec<String>, bool), String> {
        let runtime_address_space = address_space.clone();
        let mount = resolve_mount(
            &runtime_address_space,
            params.mount_id,
            MountCapability::Search,
        )?;
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

        Err(format!("未注册的 mount provider: {}", mount.provider))
    }

    async fn search_inline(
        &self,
        mount: &Mount,
        base_path: &str,
        params: &TextSearchParams<'_>,
    ) -> Result<(Vec<String>, bool), String> {
        let mut files = inline_files_from_mount(mount)?;
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

struct InlineOverlayPatchTarget<'a> {
    address_space: &'a AddressSpace,
    mount: &'a Mount,
    overlay: &'a InlineContentOverlay,
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
        let files = inline_files_from_mount(self.mount).map_err(ApplyPatchError::Apply)?;
        files
            .get(&normalized)
            .cloned()
            .ok_or_else(|| ApplyPatchError::Apply(format!("目标文件不存在: {normalized}")))
    }

    async fn write_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError> {
        let normalized =
            normalize_mount_relative_path(path, false).map_err(ApplyPatchError::InvalidPath)?;
        self.overlay
            .write(self.address_space, self.mount, &normalized, content)
            .await
            .map_err(ApplyPatchError::Apply)
    }

    async fn delete_text(&self, path: &str) -> Result<(), ApplyPatchError> {
        let normalized =
            normalize_mount_relative_path(path, false).map_err(ApplyPatchError::InvalidPath)?;
        self.overlay
            .delete(self.address_space, self.mount, &normalized)
            .await
            .map_err(ApplyPatchError::Apply)
    }

    async fn rename_text(&self, from_path: &str, to_path: &str) -> Result<(), ApplyPatchError> {
        let source = self.read_text(from_path).await?;
        self.write_text(to_path, &source).await?;
        self.delete_text(from_path).await
    }
}
