use std::sync::Arc;

use crate::address_space::*;
use crate::runtime::Mount;
use agentdash_connector_contract::{AddressSpace, MountCapability};

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

    pub fn list_mounts(
        &self,
        address_space: &AddressSpace,
    ) -> Vec<agentdash_connector_contract::Mount> {
        address_space.mounts.clone()
    }

    pub async fn read_text(
        &self,
        address_space: &AddressSpace,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<ReadResult, String> {
        let runtime_address_space = address_space.clone();
        let mount =
            resolve_mount(&runtime_address_space, &target.mount_id, MountCapability::Read)?;
        let path = normalize_mount_relative_path(&target.path, false)?;

        if let Some(ov) = overlay
            && let Some(content) = ov.read(&mount.id, &path).await {
                return Ok(ReadResult { path, content });
            }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext;
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
            return ov.write(&runtime_address_space, mount, &path, content).await;
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext;
            return provider
                .write_text(mount, &path, content, &ctx)
                .await
                .map_err(|e| e.to_string());
        }

        Err(format!("未注册的 mount provider: {}", mount.provider))
    }

    pub async fn list(
        &self,
        address_space: &AddressSpace,
        mount_id: &str,
        options: ListOptions,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<ListResult, String> {
        let runtime_address_space = address_space.clone();
        let mount =
            resolve_mount(&runtime_address_space, mount_id, MountCapability::List)?;
        let path = normalize_mount_relative_path(&options.path, true)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let mut files = inline_files_from_mount(mount)?;
            if let Some(ov) = overlay {
                for (p, c) in ov.overridden_files(&mount.id).await {
                    files.insert(p, c);
                }
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
            let ctx = MountOperationContext;
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
            let ctx = MountOperationContext;
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
        self.search_text_extended(address_space, &TextSearchParams {
            mount_id,
            path,
            query,
            is_regex: false,
            include_glob: None,
            max_results,
            context_lines: 0,
            overlay,
        })
        .await
        .map(|(hits, _truncated)| hits)
    }

    pub async fn search_text_extended(
        &self,
        address_space: &AddressSpace,
        params: &TextSearchParams<'_>,
    ) -> Result<(Vec<String>, bool), String> {
        let runtime_address_space = address_space.clone();
        let mount =
            resolve_mount(&runtime_address_space, params.mount_id, MountCapability::Search)?;
        let base_path = normalize_mount_relative_path(params.path, true)?;

        if mount.provider == PROVIDER_INLINE_FS {
            return self
                .search_inline(mount, &base_path, params)
                .await;
        }

        if let Some(provider) = self.mount_provider_registry.get(&mount.provider) {
            let ctx = MountOperationContext;
            let sq = SearchQuery {
                path: if base_path.is_empty() { None } else { Some(base_path) },
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
            for (p, c) in ov.overridden_files(&mount.id).await {
                files.insert(p, c);
            }
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
