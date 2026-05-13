use std::sync::Arc;

use agentdash_relay::{
    MaterializationAccessMode, MaterializationCacheScope, MaterializationPlanKind,
    MaterializationTargetKind, VfsMaterializeContent, VfsMaterializeEntry, VfsMaterializePayload,
    VfsMaterializeResponse,
};
use agentdash_spi::{Mount, MountCapability, Vfs};
use async_trait::async_trait;
use futures::future::BoxFuture;
use sha2::{Digest, Sha256};

use super::inline_persistence::InlineContentOverlay;
use super::relay_service::RelayVfsService;
use super::rewrite::{
    RewriteReplacement, apply_replacements, find_mount_uri_candidates, quote_for_shell_path,
};
use super::{
    ListOptions, PROVIDER_RELAY_FS, ResourceRef, format_mount_uri, join_root_ref,
    normalize_mount_relative_path, parse_mount_uri, resolve_mount,
};

#[async_trait]
pub trait VfsMaterializationTransport: Send + Sync {
    async fn materialize(
        &self,
        backend_id: &str,
        payload: VfsMaterializePayload,
    ) -> Result<VfsMaterializeResponse, String>;
}

#[derive(Clone)]
pub struct VfsMaterializationService {
    vfs_service: Arc<RelayVfsService>,
    transport: Arc<dyn VfsMaterializationTransport>,
}

impl VfsMaterializationService {
    pub fn new(
        vfs_service: Arc<RelayVfsService>,
        transport: Arc<dyn VfsMaterializationTransport>,
    ) -> Self {
        Self {
            vfs_service,
            transport,
        }
    }

    pub async fn rewrite_shell_command(
        &self,
        input: RewriteShellCommandInput<'_>,
    ) -> Result<RewriteShellCommandOutput, String> {
        let mount_ids = input
            .vfs
            .mounts
            .iter()
            .map(|mount| mount.id.clone())
            .collect::<Vec<_>>();
        let candidates = find_mount_uri_candidates(input.command, &mount_ids);
        if candidates.is_empty() {
            return Ok(RewriteShellCommandOutput {
                command: input.command.to_string(),
                rewrites: Vec::new(),
            });
        }

        let exec_mount = resolve_mount(input.vfs, input.exec_mount_id, MountCapability::Exec)?;
        let mut replacements = Vec::new();
        let mut rewrites = Vec::new();

        for candidate in candidates {
            let target = parse_mount_uri(&candidate.value, input.vfs)?;
            let source_mount = resolve_mount(input.vfs, &target.mount_id, MountCapability::Read)?;
            let local_path = if can_directly_reference_local_path(source_mount, exec_mount) {
                let path = normalize_mount_relative_path(&target.path, true)?;
                join_root_ref(&source_mount.root_ref, &path)
            } else {
                let payload = self
                    .build_payload(&input, &candidate.value, &target, source_mount)
                    .await?;
                let response = self
                    .transport
                    .materialize(&exec_mount.backend_id, payload)
                    .await?;
                response.primary_local_path
            };

            let replacement_value = if candidate.quoted {
                local_path.clone()
            } else {
                quote_for_shell_path(&local_path)
            };
            replacements.push(RewriteReplacement {
                start: candidate.start,
                end: candidate.end,
                value: replacement_value,
            });
            rewrites.push(MaterializationRewrite {
                source_uri: candidate.value,
                local_path,
            });
        }

        Ok(RewriteShellCommandOutput {
            command: apply_replacements(input.command, &replacements),
            rewrites,
        })
    }

    pub async fn rewrite_json_arguments(
        &self,
        input: RewriteJsonArgumentsInput<'_>,
    ) -> Result<RewriteJsonArgumentsOutput, String> {
        let mut rewrites = Vec::new();
        let value = rewrite_json_value(
            self,
            input,
            serde_json::Value::Object(input.arguments.clone()),
            &mut rewrites,
        )
        .await?;
        let arguments = match value {
            serde_json::Value::Object(map) => map,
            _ => unreachable!("object input remains object"),
        };
        Ok(RewriteJsonArgumentsOutput {
            arguments,
            rewrites,
        })
    }

    async fn build_payload(
        &self,
        input: &RewriteShellCommandInput<'_>,
        source_uri: &str,
        target: &ResourceRef,
        source_mount: &Mount,
    ) -> Result<VfsMaterializePayload, String> {
        let plan = self
            .plan_entries(
                input.vfs,
                target,
                source_mount,
                input.overlay,
                input.identity,
            )
            .await?;
        let entries = self
            .read_plan_entries(input.vfs, target, &plan, input.overlay, input.identity)
            .await?;

        Ok(VfsMaterializePayload {
            session_id: input.session_id.to_string(),
            turn_id: input.turn_id.map(str::to_string),
            tool_call_id: input.tool_call_id.map(str::to_string),
            plan_id: uuid::Uuid::new_v4().to_string(),
            plan_kind: plan.kind,
            source_uri: source_uri.to_string(),
            root_uri: format_mount_uri(&target.mount_id, &plan.root_path),
            mount_id: target.mount_id.clone(),
            provider: source_mount.provider.clone(),
            primary_relative_path: plan.primary_relative_path,
            target_kind: plan.target_kind,
            access_mode: plan.access_mode,
            entries,
            cache_scope: plan.cache_scope,
            ttl_ms: Some(30 * 60 * 1000),
        })
    }

    async fn build_payload_for_context(
        &self,
        input: MaterializationBuildInput<'_>,
    ) -> Result<VfsMaterializePayload, String> {
        let plan = self
            .plan_entries(
                input.vfs,
                input.target,
                input.source_mount,
                input.overlay,
                input.identity,
            )
            .await?;
        let entries = self
            .read_plan_entries(
                input.vfs,
                input.target,
                &plan,
                input.overlay,
                input.identity,
            )
            .await?;

        Ok(VfsMaterializePayload {
            session_id: input.session_id.to_string(),
            turn_id: input.turn_id.map(str::to_string),
            tool_call_id: input.tool_call_id.map(str::to_string),
            plan_id: uuid::Uuid::new_v4().to_string(),
            plan_kind: plan.kind,
            source_uri: input.source_uri.to_string(),
            root_uri: format_mount_uri(&input.target.mount_id, &plan.root_path),
            mount_id: input.target.mount_id.clone(),
            provider: input.source_mount.provider.clone(),
            primary_relative_path: plan.primary_relative_path,
            target_kind: plan.target_kind,
            access_mode: plan.access_mode,
            entries,
            cache_scope: plan.cache_scope,
            ttl_ms: Some(30 * 60 * 1000),
        })
    }

    async fn local_path_for_uri(&self, input: LocalPathForUriInput<'_>) -> Result<String, String> {
        let target = parse_mount_uri(input.source_uri, input.vfs)?;
        let source_mount = resolve_mount(input.vfs, &target.mount_id, MountCapability::Read)?;
        if can_directly_reference_backend_path(source_mount, input.target_backend_id) {
            let path = normalize_mount_relative_path(&target.path, true)?;
            return Ok(join_root_ref(&source_mount.root_ref, &path));
        }

        let payload = self
            .build_payload_for_context(MaterializationBuildInput {
                vfs: input.vfs,
                source_uri: input.source_uri,
                target: &target,
                source_mount,
                session_id: input.session_id,
                turn_id: input.turn_id,
                tool_call_id: input.tool_call_id,
                overlay: input.overlay,
                identity: input.identity,
            })
            .await?;
        let response = self
            .transport
            .materialize(input.target_backend_id, payload)
            .await?;
        Ok(response.primary_local_path)
    }

    async fn plan_entries(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        _mount: &Mount,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<MaterializationPlan, String> {
        let normalized_path = normalize_mount_relative_path(&target.path, true)?;
        let stat = self
            .vfs_service
            .stat(vfs, target, overlay, identity)
            .await
            .ok();

        if is_skill_script_path(&normalized_path) {
            let root_path = skill_root_path(&normalized_path).expect("skill root path");
            let primary_relative_path = strip_root_prefix(&normalized_path, &root_path);
            let entries = self
                .list_files_under(vfs, &target.mount_id, &root_path, overlay, identity)
                .await?
                .into_iter()
                .filter(|path| is_skill_resource_path(&strip_root_prefix(path, &root_path)))
                .collect::<Vec<_>>();
            return Ok(MaterializationPlan {
                kind: MaterializationPlanKind::SkillResourceSet,
                root_path,
                primary_relative_path,
                entry_paths: entries,
                target_kind: MaterializationTargetKind::File,
                access_mode: MaterializationAccessMode::ReadOnly,
                cache_scope: MaterializationCacheScope::Session,
            });
        }

        let is_dir = stat.as_ref().is_some_and(|entry| entry.is_dir)
            || target.path.trim_end().ends_with('/')
            || is_skill_root_path(&normalized_path);
        if is_dir {
            let root_path = normalized_path.clone();
            let entries = self
                .list_files_under(vfs, &target.mount_id, &root_path, overlay, identity)
                .await?;
            return Ok(MaterializationPlan {
                kind: MaterializationPlanKind::WritableWorkingCopy,
                root_path,
                primary_relative_path: ".".to_string(),
                entry_paths: entries,
                target_kind: MaterializationTargetKind::Directory,
                access_mode: MaterializationAccessMode::WritableLocalCopy,
                cache_scope: MaterializationCacheScope::PersistentWorkingCopy,
            });
        }

        let root_path = parent_path(&normalized_path);
        let primary_relative_path = file_name(&normalized_path)?;
        Ok(MaterializationPlan {
            kind: MaterializationPlanKind::SingleFile,
            root_path,
            primary_relative_path,
            entry_paths: vec![normalized_path],
            target_kind: MaterializationTargetKind::File,
            access_mode: MaterializationAccessMode::ReadOnly,
            cache_scope: MaterializationCacheScope::Session,
        })
    }

    async fn list_files_under(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        root_path: &str,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<Vec<String>, String> {
        let result = self
            .vfs_service
            .list(
                vfs,
                mount_id,
                ListOptions {
                    path: root_path.to_string(),
                    pattern: None,
                    recursive: true,
                },
                overlay,
                identity,
            )
            .await?;
        Ok(result
            .entries
            .into_iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path)
            .collect())
    }

    async fn read_plan_entries(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        plan: &MaterializationPlan,
        overlay: Option<&InlineContentOverlay>,
        identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<Vec<VfsMaterializeEntry>, String> {
        let mut entries = Vec::with_capacity(plan.entry_paths.len());
        for path in &plan.entry_paths {
            let read = self
                .vfs_service
                .read_text(
                    vfs,
                    &ResourceRef {
                        mount_id: target.mount_id.clone(),
                        path: path.clone(),
                    },
                    overlay,
                    identity,
                )
                .await?;
            let digest = format!("sha256:{}", sha256_hex(read.content.as_bytes()));
            let size_bytes = read.content.len() as u64;
            entries.push(VfsMaterializeEntry {
                relative_path: strip_root_prefix(path, &plan.root_path),
                content: VfsMaterializeContent::Utf8Text { text: read.content },
                digest,
                size_bytes,
                mime_hint: mime_hint(path),
                executable_hint: executable_hint(path),
            });
        }
        Ok(entries)
    }
}

pub struct RewriteShellCommandInput<'a> {
    pub vfs: &'a Vfs,
    pub exec_mount_id: &'a str,
    pub command: &'a str,
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub tool_call_id: Option<&'a str>,
    pub overlay: Option<&'a InlineContentOverlay>,
    pub identity: Option<&'a agentdash_spi::platform::auth::AuthIdentity>,
}

#[derive(Clone, Copy)]
pub struct RewriteJsonArgumentsInput<'a> {
    pub vfs: &'a Vfs,
    pub target_backend_id: &'a str,
    pub arguments: &'a serde_json::Map<String, serde_json::Value>,
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub tool_call_id: Option<&'a str>,
    pub overlay: Option<&'a InlineContentOverlay>,
    pub identity: Option<&'a agentdash_spi::platform::auth::AuthIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteShellCommandOutput {
    pub command: String,
    pub rewrites: Vec<MaterializationRewrite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteJsonArgumentsOutput {
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub rewrites: Vec<MaterializationRewrite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializationRewrite {
    pub source_uri: String,
    pub local_path: String,
}

struct MaterializationBuildInput<'a> {
    vfs: &'a Vfs,
    source_uri: &'a str,
    target: &'a ResourceRef,
    source_mount: &'a Mount,
    session_id: &'a str,
    turn_id: Option<&'a str>,
    tool_call_id: Option<&'a str>,
    overlay: Option<&'a InlineContentOverlay>,
    identity: Option<&'a agentdash_spi::platform::auth::AuthIdentity>,
}

struct LocalPathForUriInput<'a> {
    vfs: &'a Vfs,
    target_backend_id: &'a str,
    source_uri: &'a str,
    session_id: &'a str,
    turn_id: Option<&'a str>,
    tool_call_id: Option<&'a str>,
    overlay: Option<&'a InlineContentOverlay>,
    identity: Option<&'a agentdash_spi::platform::auth::AuthIdentity>,
}

struct MaterializationPlan {
    kind: MaterializationPlanKind,
    root_path: String,
    primary_relative_path: String,
    entry_paths: Vec<String>,
    target_kind: MaterializationTargetKind,
    access_mode: MaterializationAccessMode,
    cache_scope: MaterializationCacheScope,
}

fn can_directly_reference_local_path(source_mount: &Mount, exec_mount: &Mount) -> bool {
    exec_mount.provider == PROVIDER_RELAY_FS
        && can_directly_reference_backend_path(source_mount, &exec_mount.backend_id)
}

fn can_directly_reference_backend_path(source_mount: &Mount, target_backend_id: &str) -> bool {
    source_mount.provider == PROVIDER_RELAY_FS
        && !source_mount.backend_id.is_empty()
        && source_mount.backend_id == target_backend_id
}

fn rewrite_json_value<'a>(
    service: &'a VfsMaterializationService,
    input: RewriteJsonArgumentsInput<'a>,
    value: serde_json::Value,
    rewrites: &'a mut Vec<MaterializationRewrite>,
) -> BoxFuture<'a, Result<serde_json::Value, String>> {
    Box::pin(async move {
        match value {
            serde_json::Value::String(text) => {
                let rewritten = rewrite_json_string(service, input, &text, rewrites).await?;
                Ok(serde_json::Value::String(rewritten))
            }
            serde_json::Value::Array(values) => {
                let mut next = Vec::with_capacity(values.len());
                for value in values {
                    next.push(rewrite_json_value(service, input, value, rewrites).await?);
                }
                Ok(serde_json::Value::Array(next))
            }
            serde_json::Value::Object(map) => {
                let mut next = serde_json::Map::new();
                for (key, value) in map {
                    next.insert(
                        key,
                        rewrite_json_value(service, input, value, rewrites).await?,
                    );
                }
                Ok(serde_json::Value::Object(next))
            }
            other => Ok(other),
        }
    })
}

async fn rewrite_json_string(
    service: &VfsMaterializationService,
    input: RewriteJsonArgumentsInput<'_>,
    text: &str,
    rewrites: &mut Vec<MaterializationRewrite>,
) -> Result<String, String> {
    let mount_ids = input
        .vfs
        .mounts
        .iter()
        .map(|mount| mount.id.clone())
        .collect::<Vec<_>>();
    let candidates = find_mount_uri_candidates(text, &mount_ids);
    if candidates.is_empty() {
        return Ok(text.to_string());
    }

    let mut replacements = Vec::new();
    for candidate in candidates {
        let local_path = service
            .local_path_for_uri(LocalPathForUriInput {
                vfs: input.vfs,
                target_backend_id: input.target_backend_id,
                source_uri: &candidate.value,
                session_id: input.session_id,
                turn_id: input.turn_id,
                tool_call_id: input.tool_call_id,
                overlay: input.overlay,
                identity: input.identity,
            })
            .await?;
        replacements.push(RewriteReplacement {
            start: candidate.start,
            end: candidate.end,
            value: local_path.clone(),
        });
        rewrites.push(MaterializationRewrite {
            source_uri: candidate.value,
            local_path,
        });
    }

    Ok(apply_replacements(text, &replacements))
}

fn is_skill_script_path(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() >= 4 && parts[0] == "skills" && parts[2] == "scripts"
}

fn is_skill_root_path(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() == 2 && parts[0] == "skills" && !parts[1].is_empty()
}

fn skill_root_path(path: &str) -> Option<String> {
    let parts = path.split('/').collect::<Vec<_>>();
    (parts.len() >= 2 && parts[0] == "skills" && !parts[1].is_empty())
        .then(|| format!("skills/{}", parts[1]))
}

fn is_skill_resource_path(relative: &str) -> bool {
    relative == "SKILL.md"
        || relative.starts_with("scripts/")
        || relative.starts_with("references/")
        || relative.starts_with("assets/")
}

fn parent_path(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default()
}

fn file_name(path: &str) -> Result<String, String> {
    path.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("无法从路径解析文件名: {path}"))
}

fn strip_root_prefix(path: &str, root: &str) -> String {
    if root.is_empty() {
        return path.to_string();
    }
    if path == root {
        return ".".to_string();
    }
    path.strip_prefix(&format!("{}/", root))
        .unwrap_or(path)
        .to_string()
}

fn executable_hint(path: &str) -> bool {
    path.ends_with(".sh")
        || path.ends_with(".bash")
        || path.ends_with(".zsh")
        || path.ends_with(".ps1")
        || path.ends_with(".cmd")
        || path.ends_with(".bat")
}

fn mime_hint(path: &str) -> Option<String> {
    if path.ends_with(".md") {
        Some("text/markdown".to_string())
    } else if path.ends_with(".json") {
        Some("application/json".to_string())
    } else if path.ends_with(".sh") {
        Some("text/x-shellscript".to_string())
    } else {
        Some("text/plain".to_string())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::MountCapability;

    fn mount(id: &str, provider: &str, backend_id: &str, root_ref: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: provider.to_string(),
            backend_id: backend_id.to_string(),
            root_ref: root_ref.to_string(),
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Exec,
            ],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn direct_relay_mount_reference_uses_root_ref_without_materialization() {
        let source = mount("main", PROVIDER_RELAY_FS, "local-a", "/workspace/repo");
        let exec = mount("main", PROVIDER_RELAY_FS, "local-a", "/workspace/repo");
        assert!(can_directly_reference_local_path(&source, &exec));
        assert_eq!(
            join_root_ref(&source.root_ref, "src/main.rs"),
            "/workspace/repo/src/main.rs"
        );
    }

    #[test]
    fn skill_script_paths_expand_to_skill_root_resources() {
        assert!(is_skill_script_path("skills/reviewer/scripts/check.sh"));
        assert_eq!(
            skill_root_path("skills/reviewer/scripts/check.sh"),
            Some("skills/reviewer".to_string())
        );
        assert!(is_skill_resource_path("references/rules.md"));
        assert!(!is_skill_resource_path("tmp/cache.txt"));
    }
}
