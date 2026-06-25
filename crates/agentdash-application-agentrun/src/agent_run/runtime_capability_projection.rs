use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use agentdash_spi::context::capability::{
    SessionBaselineCapabilities, SkillCapabilityEntry, SkillEntry, SkillProviderCluster,
};
use agentdash_spi::{
    AuthIdentity, DiscoveredGuideline, DiscoveredSkill, MemoryDiscoveryContext,
    MemoryDiscoveryDiagnostic, MemoryDiscoveryMount, MemoryDiscoveryOutput,
    MemoryDiscoveryOwnerKind, MemoryDiscoveryProvider, MemoryDiscoveryUserContext,
    MemoryIndexStatus, RuntimeMcpServer, SkillContextExposure, SkillDiscoveryCluster,
    SkillDiscoveryContext, SkillDiscoveryDiagnostic, SkillDiscoveryOutput, SkillDiscoveryProvider,
    SkillDiscoveryUserContext, Vfs, skill_capability_key,
};

use crate::context::mount_file_discovery::{
    BUILTIN_GUIDELINE_RULES, DiscoveredMountFile, MountFileDiscoveryDiagnostic,
    discover_memory_vfs_files, discover_mount_files, discover_skill_vfs_files,
};
use agentdash_application_vfs::VfsService;
use agentdash_application_vfs::mount::{
    CONTEXT_CONTAINER_ID_METADATA_KEY, CONTEXT_OWNER_ID_METADATA_KEY,
    CONTEXT_OWNER_KIND_METADATA_KEY, PROJECT_AGENT_MEMORY_MOUNT_ID, PROJECT_VFS_MOUNT_METADATA_KEY,
    mount_owner_kind,
};
use agentdash_application_vfs::mount_purpose;

use crate::session::baseline_capabilities::{
    INTEGRATION_STATIC_SKILL_PROVIDER_KEY, WORKSPACE_SKILL_PROVIDER_KEY,
    build_session_baseline_capabilities_from_clusters, skills_to_provider_clusters,
};
use crate::session::types::CapabilityState;

#[derive(Clone, Copy)]
pub struct RuntimeCapabilityProjectionInput<'a> {
    pub vfs_service: Option<&'a VfsService>,
    pub active_vfs: Option<&'a Vfs>,
    pub identity: Option<&'a AuthIdentity>,
    pub extra_skill_dirs: &'a [PathBuf],
    pub skill_discovery_providers: &'a [Arc<dyn SkillDiscoveryProvider>],
    pub diagnostics_label: &'static str,
}

#[derive(Clone, Copy)]
pub struct RuntimeMemoryProjectionInput<'a> {
    pub vfs_service: Option<&'a VfsService>,
    pub active_vfs: Option<&'a Vfs>,
    pub identity: Option<&'a AuthIdentity>,
    pub memory_discovery_providers: &'a [Arc<dyn MemoryDiscoveryProvider>],
    pub diagnostics_label: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeCapabilityProjection {
    pub session_capabilities: SessionBaselineCapabilities,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub discovered_memory: MemoryDiscoveryOutput,
}

pub async fn derive_runtime_capability_projection(
    input: RuntimeCapabilityProjectionInput<'_>,
) -> RuntimeCapabilityProjection {
    let session_capabilities = derive_runtime_skill_baseline(input)
        .await
        .unwrap_or_default();
    let discovered_guidelines = match (input.vfs_service, input.active_vfs) {
        (Some(vfs_service), Some(active_vfs)) => {
            derive_runtime_guidelines(vfs_service, active_vfs, input.diagnostics_label).await
        }
        _ => Vec::new(),
    };

    RuntimeCapabilityProjection {
        session_capabilities,
        discovered_guidelines,
        discovered_memory: MemoryDiscoveryOutput::default(),
    }
}

pub async fn derive_runtime_skill_baseline(
    input: RuntimeCapabilityProjectionInput<'_>,
) -> Option<SessionBaselineCapabilities> {
    let mut clusters = Vec::new();
    let mut diagnostics = Vec::new();

    if let (Some(vfs_service), Some(active_vfs)) = (input.vfs_service, input.active_vfs) {
        let result = crate::skill::load_skills_from_vfs(vfs_service, active_vfs).await;
        log_skill_diagnostics(input.diagnostics_label, "vfs", &result.diagnostics);
        diagnostics.extend(loader_diagnostics_to_discovery(
            WORKSPACE_SKILL_PROVIDER_KEY,
            result.diagnostics,
        ));
        clusters.extend(skills_to_provider_clusters(
            WORKSPACE_SKILL_PROVIDER_KEY,
            "Workspace Skills",
            Some("Skills discovered from the active workspace.".to_string()),
            Some("当前 workspace 中声明的 skills。".to_string()),
            None,
            &result.skills,
        ));
    }

    if !input.extra_skill_dirs.is_empty() {
        let existing_names = HashMap::new();
        let result =
            crate::skill::load_skills_from_local_dirs(input.extra_skill_dirs, &existing_names);
        log_skill_diagnostics(
            input.diagnostics_label,
            "integration-static",
            &result.diagnostics,
        );
        diagnostics.extend(loader_diagnostics_to_discovery(
            INTEGRATION_STATIC_SKILL_PROVIDER_KEY,
            result.diagnostics,
        ));
        clusters.extend(skills_to_provider_clusters(
            INTEGRATION_STATIC_SKILL_PROVIDER_KEY,
            "Integration Skills",
            Some("Static skill directories contributed by Host Integrations.".to_string()),
            Some("Host Integration 提供的静态 skill 目录。".to_string()),
            None,
            &result.skills,
        ));
    }

    let discovery_context =
        skill_discovery_context_from_vfs_and_identity(input.active_vfs, input.identity);
    for provider in input.skill_discovery_providers {
        let rules = provider.vfs_discovery_rules();
        let vfs_first = !rules.is_empty();
        let output = if vfs_first {
            match (input.vfs_service, input.active_vfs) {
                (Some(vfs_service), Some(active_vfs)) => {
                    let (files, scan_diagnostics) =
                        discover_skill_vfs_files(vfs_service, active_vfs, &rules).await;
                    diagnostics.extend(mount_diagnostics_to_discovery(
                        provider.provider_key(),
                        scan_diagnostics,
                    ));
                    provider
                        .discover_from_vfs(discovery_context.clone(), files)
                        .await
                }
                _ => {
                    diagnostics.push(SkillDiscoveryDiagnostic {
                        provider_key: provider.provider_key().to_string(),
                        code: "vfs_context_missing".to_string(),
                        message: "provider 声明了 VFS discovery rules，但当前 session 缺少 active VFS 或 VfsService，已跳过".to_string(),
                        local_name: None,
                        file_path: None,
                    });
                    continue;
                }
            }
        } else {
            provider.discover(discovery_context.clone()).await
        };

        match output {
            Ok(output) => {
                let (provider_clusters, provider_diagnostics) =
                    provider_output_to_surface(output, provider.provider_key(), vfs_first);
                diagnostics.extend(provider_diagnostics);
                clusters.extend(provider_clusters);
            }
            Err(error) => {
                diagnostics.push(SkillDiscoveryDiagnostic {
                    provider_key: provider.provider_key().to_string(),
                    code: "provider_failed".to_string(),
                    message: error.to_string(),
                    local_name: None,
                    file_path: None,
                });
            }
        }
    }

    let (clusters, duplicate_diagnostics) = normalize_provider_clusters(clusters);
    diagnostics.extend(duplicate_diagnostics);
    log_discovery_diagnostics(input.diagnostics_label, &diagnostics);

    if input.vfs_service.is_none()
        && input.extra_skill_dirs.is_empty()
        && input.skill_discovery_providers.is_empty()
    {
        return None;
    }

    Some(build_session_baseline_capabilities_from_clusters(
        clusters,
        diagnostics,
    ))
}

pub async fn derive_runtime_guidelines(
    vfs_service: &VfsService,
    active_vfs: &Vfs,
    diagnostics_label: &'static str,
) -> Vec<DiscoveredGuideline> {
    let guideline_result =
        discover_mount_files(vfs_service, active_vfs, BUILTIN_GUIDELINE_RULES).await;
    for diag in &guideline_result.diagnostics {
        tracing::warn!(
            label = diagnostics_label,
            rule_key = %diag.rule_key,
            mount_id = %diag.mount_id,
            path = %diag.path,
            "guideline 发现诊断: {}",
            diag.message
        );
    }

    merge_discovered_guideline_files(guideline_result.files)
}

pub async fn derive_runtime_memory_inventory(
    input: RuntimeMemoryProjectionInput<'_>,
) -> MemoryDiscoveryOutput {
    if input.memory_discovery_providers.is_empty() {
        return MemoryDiscoveryOutput::default();
    }

    let context = memory_discovery_context_from_vfs_and_identity(input.active_vfs, input.identity);
    let mounts = input
        .active_vfs
        .map(memory_discovery_mounts_from_vfs)
        .unwrap_or_default();
    let mut clusters = Vec::new();
    let mut diagnostics = Vec::new();

    for provider in input.memory_discovery_providers {
        let rules = provider.vfs_discovery_rules();
        let output = if rules.is_empty() {
            provider.discover(context.clone()).await
        } else {
            match (input.vfs_service, input.active_vfs) {
                (Some(vfs_service), Some(active_vfs)) => {
                    let (files, scan_diagnostics) =
                        discover_memory_vfs_files(vfs_service, active_vfs, &rules).await;
                    let oversized_indexes =
                        oversized_memory_index_paths(&scan_diagnostics, provider.provider_key());
                    diagnostics.extend(memory_mount_diagnostics_to_discovery(
                        provider.provider_key(),
                        scan_diagnostics,
                    ));
                    provider
                        .discover_from_vfs(context.clone(), mounts.clone(), files)
                        .await
                        .map(|output| mark_oversized_memory_indexes(output, &oversized_indexes))
                }
                _ => {
                    diagnostics.push(MemoryDiscoveryDiagnostic {
                        provider_key: provider.provider_key().to_string(),
                        code: "vfs_context_missing".to_string(),
                        message: "provider 声明了 VFS discovery rules，但当前 session 缺少 active VFS 或 VfsService，已跳过".to_string(),
                        source_key: None,
                        uri: None,
                    });
                    continue;
                }
            }
        };

        match output {
            Ok(output) => {
                let normalized = output.normalized(provider.provider_key());
                diagnostics.extend(normalized.diagnostics);
                clusters.extend(normalized.clusters);
            }
            Err(error) => {
                diagnostics.push(MemoryDiscoveryDiagnostic {
                    provider_key: provider.provider_key().to_string(),
                    code: "provider_failed".to_string(),
                    message: error.to_string(),
                    source_key: None,
                    uri: None,
                });
            }
        }
    }

    log_memory_discovery_diagnostics(input.diagnostics_label, &diagnostics);

    MemoryDiscoveryOutput {
        clusters,
        diagnostics,
    }
}

fn memory_discovery_context_from_vfs_and_identity(
    active_vfs: Option<&Vfs>,
    identity: Option<&AuthIdentity>,
) -> MemoryDiscoveryContext {
    let agent_id = active_vfs
        .and_then(|vfs| {
            vfs.mounts
                .iter()
                .find(|mount| mount.id == PROJECT_AGENT_MEMORY_MOUNT_ID)
        })
        .and_then(|mount| {
            mount
                .metadata
                .get(CONTEXT_OWNER_ID_METADATA_KEY)
                .and_then(serde_json::Value::as_str)
        })
        .and_then(|value| uuid::Uuid::parse_str(value).ok());

    MemoryDiscoveryContext {
        project_id: active_vfs
            .and_then(|vfs| vfs.source_project_id.as_deref())
            .and_then(|value| uuid::Uuid::parse_str(value).ok()),
        story_id: active_vfs
            .and_then(|vfs| vfs.source_story_id.as_deref())
            .and_then(|value| uuid::Uuid::parse_str(value).ok()),
        agent_id,
        owner_kind: if agent_id.is_some() {
            MemoryDiscoveryOwnerKind::Agent
        } else {
            MemoryDiscoveryOwnerKind::Project
        },
        user: identity.map(memory_discovery_user_context_from_identity),
        ..MemoryDiscoveryContext::default()
    }
}

fn memory_discovery_user_context_from_identity(
    identity: &AuthIdentity,
) -> MemoryDiscoveryUserContext {
    MemoryDiscoveryUserContext {
        user_id: identity.user_id.clone(),
        display_name: identity.display_name.clone(),
        email: identity.email.clone(),
        groups: identity
            .groups
            .iter()
            .map(|group| group.group_id.clone())
            .collect(),
    }
}

fn memory_discovery_mounts_from_vfs(vfs: &Vfs) -> Vec<MemoryDiscoveryMount> {
    vfs.mounts
        .iter()
        .map(|mount| {
            let mut summary = MemoryDiscoveryMount::new(
                mount.id.clone(),
                mount.provider.clone(),
                mount.display_name.clone(),
                mount.capabilities.clone(),
            );
            summary.purpose = serde_json::to_value(mount_purpose(mount))
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string));
            summary.owner_kind = serde_json::to_value(mount_owner_kind(mount))
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string));
            summary.metadata_summary = sanitized_memory_mount_metadata_summary(mount);
            summary
        })
        .collect()
}

fn sanitized_memory_mount_metadata_summary(
    mount: &agentdash_spi::Mount,
) -> Option<serde_json::Value> {
    let metadata = mount.metadata.as_object()?;
    let mut summary = serde_json::Map::new();
    for key in [
        "container_id",
        CONTEXT_CONTAINER_ID_METADATA_KEY,
        CONTEXT_OWNER_KIND_METADATA_KEY,
        CONTEXT_OWNER_ID_METADATA_KEY,
        PROJECT_VFS_MOUNT_METADATA_KEY,
        "project_vfs_mount_id",
        "scope",
    ] {
        if let Some(value) = metadata.get(key) {
            summary.insert(key.to_string(), value.clone());
        }
    }
    (!summary.is_empty()).then(|| serde_json::Value::Object(summary))
}

fn oversized_memory_index_paths(
    diagnostics: &[MountFileDiscoveryDiagnostic],
    provider_key: &str,
) -> Vec<(String, String, String)> {
    diagnostics
        .iter()
        .filter(|diag| diag.message.contains("文件过大"))
        .map(|diag| {
            (
                diag.mount_id.clone(),
                diag.path.clone(),
                provider_key.to_string(),
            )
        })
        .collect()
}

fn mark_oversized_memory_indexes(
    mut output: MemoryDiscoveryOutput,
    oversized_indexes: &[(String, String, String)],
) -> MemoryDiscoveryOutput {
    if oversized_indexes.is_empty() {
        return output;
    }

    for cluster in &mut output.clusters {
        for source in &mut cluster.sources {
            let index_path = source
                .index_uri
                .strip_prefix(&format!("{}://", source.mount_id));
            if oversized_indexes.iter().any(|(mount_id, path, _)| {
                mount_id == &source.mount_id && Some(path.as_str()) == index_path
            }) {
                source.index_status = MemoryIndexStatus::TooLarge;
                source.bounded_index_content = None;
            }
        }
    }
    output
}

fn memory_mount_diagnostics_to_discovery(
    provider_key: &str,
    diagnostics: Vec<MountFileDiscoveryDiagnostic>,
) -> Vec<MemoryDiscoveryDiagnostic> {
    diagnostics
        .into_iter()
        .map(|diag| {
            let code = if diag.message.contains("文件过大") {
                "memory_index_too_large"
            } else {
                "vfs_file_diagnostic"
            };
            MemoryDiscoveryDiagnostic {
                provider_key: provider_key.to_string(),
                code: code.to_string(),
                message: diag.message,
                source_key: None,
                uri: Some(format!("{}://{}", diag.mount_id, diag.path)),
            }
        })
        .collect()
}

/// 把发现到的原始文件合并为确定性的指引列表。
///
/// 合并语义：
/// - 稳定排序：按 (mount_id, 路径深度, 路径字典序)，保证多次发现顺序可复现；
///   更深目录（更靠近被操作文件）的指引排在更后，视为更具体/优先，由模型裁决冲突。
/// - 去重：按 (mount_id, 规范化路径) 去重，避免同一文件重复注入。
///
/// 注意：当前发现机制仅扫描 mount 根 + 一级子目录（见 mount_file_discovery
/// `BUILTIN_GUIDELINE_RULES`），不递归更深层级；亦不做"深层覆盖同名段"——
/// 后者需要"相对某个被编辑文件逐级解析"的锚点，本架构不具备，属未来独立增强。
fn merge_discovered_guideline_files(
    mut files: Vec<DiscoveredMountFile>,
) -> Vec<DiscoveredGuideline> {
    // 排序键与去重键一致（均基于规范化路径），且 sort_by 稳定：规范化后相同的
    // 重复项保持输入顺序，去重时保留首个（surface 出规范形态的路径）。
    files.sort_by(|a, b| {
        let (na, nb) = (
            normalize_guideline_path(&a.path),
            normalize_guideline_path(&b.path),
        );
        a.mount_id
            .cmp(&b.mount_id)
            .then_with(|| path_depth(&a.path).cmp(&path_depth(&b.path)))
            .then_with(|| na.cmp(&nb))
    });

    let mut seen = HashSet::new();
    files
        .into_iter()
        .filter(|file| seen.insert((file.mount_id.clone(), normalize_guideline_path(&file.path))))
        .map(|file| DiscoveredGuideline {
            file_name: file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_string(),
            mount_id: file.mount_id,
            path: file.path,
            content: file.content,
        })
        .collect()
}

/// 路径深度（`/` 分隔的层级数），用于就近排序。
fn path_depth(path: &str) -> usize {
    normalize_guideline_path(path)
        .split('/')
        .filter(|seg| !seg.is_empty())
        .count()
}

/// 规范化指引路径用于去重：统一分隔符、去掉前导 `./` 与首尾 `/`。
fn normalize_guideline_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_matches('/')
        .to_string()
}

pub fn normalize_capability_state_dimensions(
    state: &mut CapabilityState,
    active_vfs: Option<Vfs>,
    mcp_servers: Vec<RuntimeMcpServer>,
    session_capabilities: &SessionBaselineCapabilities,
) {
    state.vfs.active = active_vfs;
    state.tool.mcp_servers = mcp_servers;
    state.skill.skills = session_capabilities.skills.clone();
}

pub fn merge_live_vfs_skill_entries(
    existing: &[SkillEntry],
    refreshed_skills: Vec<SkillEntry>,
) -> Vec<SkillEntry> {
    let mut merged = Vec::new();
    let mut refreshed_identities = HashSet::new();
    for skill in refreshed_skills {
        refreshed_identities.insert(skill_entry_merge_identity(&skill));
        merged.push(skill);
    }
    let mut merged_identities = refreshed_identities.clone();
    for skill in existing {
        let identity = skill_entry_merge_identity(skill);
        if refreshed_identities.contains(&identity) {
            continue;
        }
        if skill.provider_key == WORKSPACE_SKILL_PROVIDER_KEY {
            continue;
        }
        if merged_identities.insert(identity) {
            merged.push(skill.clone());
        }
    }
    merged
}

fn skill_entry_merge_identity(skill: &SkillEntry) -> (String, String) {
    let provider_key = skill.provider_key.trim();
    let capability_key = skill.capability_key.trim();
    let local_name = skill.local_name.trim();
    let name = skill.name.trim();
    (
        provider_key.to_string(),
        if capability_key.is_empty() {
            if local_name.is_empty() {
                name.to_string()
            } else {
                local_name.to_string()
            }
        } else {
            capability_key.to_string()
        },
    )
}

fn skill_discovery_context_from_vfs_and_identity(
    active_vfs: Option<&Vfs>,
    identity: Option<&AuthIdentity>,
) -> SkillDiscoveryContext {
    SkillDiscoveryContext {
        workspace_root_ref: active_vfs
            .and_then(|vfs| vfs.default_mount())
            .map(|mount| mount.root_ref.clone()),
        user: identity.map(skill_discovery_user_context_from_identity),
        ..SkillDiscoveryContext::default()
    }
}

fn skill_discovery_user_context_from_identity(
    identity: &AuthIdentity,
) -> SkillDiscoveryUserContext {
    SkillDiscoveryUserContext {
        user_id: identity.user_id.clone(),
        display_name: identity.display_name.clone(),
        email: identity.email.clone(),
        groups: identity
            .groups
            .iter()
            .map(|group| group.group_id.clone())
            .collect(),
    }
}

fn provider_output_to_surface(
    output: SkillDiscoveryOutput,
    fallback_provider_key: &str,
    require_vfs_paths: bool,
) -> (Vec<SkillProviderCluster>, Vec<SkillDiscoveryDiagnostic>) {
    let mut diagnostics = output.diagnostics;
    let mut seen_by_provider: HashMap<String, HashSet<String>> = HashMap::new();
    let clusters = output
        .clusters
        .into_iter()
        .map(|cluster| {
            discovery_cluster_to_provider_cluster(
                cluster,
                fallback_provider_key,
                require_vfs_paths,
                &mut diagnostics,
                &mut seen_by_provider,
            )
        })
        .collect();
    (clusters, diagnostics)
}

fn discovery_cluster_to_provider_cluster(
    cluster: SkillDiscoveryCluster,
    fallback_provider_key: &str,
    require_vfs_paths: bool,
    diagnostics: &mut Vec<SkillDiscoveryDiagnostic>,
    seen_by_provider: &mut HashMap<String, HashSet<String>>,
) -> SkillProviderCluster {
    let provider_key = if cluster.provider_key.trim().is_empty() {
        fallback_provider_key.to_string()
    } else {
        cluster.provider_key
    };
    let seen = seen_by_provider.entry(provider_key.clone()).or_default();
    let mut default_exposed_skills = Vec::new();

    for skill in cluster.skills {
        if require_vfs_paths && !validate_vfs_first_skill_paths(&provider_key, &skill, diagnostics)
        {
            continue;
        }

        if !seen.insert(skill.local_name.clone()) {
            diagnostics.push(SkillDiscoveryDiagnostic {
                provider_key: provider_key.clone(),
                code: "duplicate_local_name".to_string(),
                message: format!(
                    "skill `{}` 在 provider `{}` 内重复声明，已保留首次发现项",
                    skill.local_name, provider_key
                ),
                local_name: Some(skill.local_name),
                file_path: Some(skill.file_path),
            });
            continue;
        }

        if skill.exposure.is_default_exposed() {
            default_exposed_skills.push(SkillCapabilityEntry {
                capability_key: skill_capability_key(&provider_key, &skill.local_name),
                provider_key: provider_key.clone(),
                local_name: skill.local_name,
                display_name: skill.display_name,
                description: skill.description,
                file_path: skill.file_path,
                base_dir: skill.base_dir,
                exposure: SkillContextExposure::DefaultExposed,
                disable_model_invocation: skill.disable_model_invocation,
            });
        }
    }

    SkillProviderCluster {
        provider_key,
        display_name: cluster.display_name,
        model_summary: cluster.model_summary,
        ui_summary: cluster.ui_summary,
        inventory_hint: cluster.inventory_hint,
        inventory_count: cluster.inventory_count,
        default_exposed_skills,
    }
}

fn validate_vfs_first_skill_paths(
    provider_key: &str,
    skill: &DiscoveredSkill,
    diagnostics: &mut Vec<SkillDiscoveryDiagnostic>,
) -> bool {
    let file_path_ok = is_controlled_vfs_skill_path(&skill.file_path, false);
    let base_dir_ok = skill
        .base_dir
        .as_deref()
        .map(|base_dir| is_controlled_vfs_skill_path(base_dir, true))
        .unwrap_or(true);

    if file_path_ok && base_dir_ok {
        return true;
    }

    diagnostics.push(SkillDiscoveryDiagnostic {
        provider_key: provider_key.to_string(),
        code: "invalid_vfs_skill_path".to_string(),
        message: format!(
            "VFS-first provider 返回了非受控 mount URI 路径，skill `{}` 已跳过",
            skill.local_name
        ),
        local_name: Some(skill.local_name.clone()),
        file_path: Some(skill.file_path.clone()),
    });
    false
}

fn is_controlled_vfs_skill_path(path: &str, allow_empty_tail: bool) -> bool {
    let Some((scheme, tail)) = path.split_once("://") else {
        return false;
    };
    if scheme.trim().is_empty()
        || scheme.eq_ignore_ascii_case("file")
        || (scheme.len() == 1 && scheme.chars().all(|ch| ch.is_ascii_alphabetic()))
    {
        return false;
    }

    if tail.is_empty() {
        return allow_empty_tail;
    }
    if tail.starts_with('/') || tail.starts_with('\\') || tail.contains('\\') {
        return false;
    }
    if tail.len() >= 2 && tail.as_bytes()[1] == b':' && tail.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }

    !tail.split('/').any(|segment| segment == "..")
}

fn normalize_provider_clusters(
    clusters: Vec<SkillProviderCluster>,
) -> (Vec<SkillProviderCluster>, Vec<SkillDiscoveryDiagnostic>) {
    let mut diagnostics = Vec::new();
    let mut seen_by_provider: HashMap<String, HashSet<String>> = HashMap::new();
    let clusters = clusters
        .into_iter()
        .map(|mut cluster| {
            let seen = seen_by_provider
                .entry(cluster.provider_key.clone())
                .or_default();
            let mut kept = Vec::new();
            for skill in cluster.default_exposed_skills {
                if seen.insert(skill.local_name.clone()) {
                    kept.push(skill);
                } else {
                    diagnostics.push(SkillDiscoveryDiagnostic {
                        provider_key: cluster.provider_key.clone(),
                        code: "duplicate_local_name".to_string(),
                        message: format!(
                            "skill `{}` 在 provider `{}` 内重复声明，已保留首次发现项",
                            skill.local_name, cluster.provider_key
                        ),
                        local_name: Some(skill.local_name),
                        file_path: Some(skill.file_path),
                    });
                }
            }
            cluster.default_exposed_skills = kept;
            cluster
        })
        .collect();
    (clusters, diagnostics)
}

fn loader_diagnostics_to_discovery(
    provider_key: &str,
    diagnostics: Vec<crate::skill::SkillDiagnostic>,
) -> Vec<SkillDiscoveryDiagnostic> {
    diagnostics
        .into_iter()
        .map(|diag| SkillDiscoveryDiagnostic {
            provider_key: provider_key.to_string(),
            code: "skill_file_diagnostic".to_string(),
            message: diag.message,
            local_name: Some(diag.name),
            file_path: Some(diag.file_path.to_string_lossy().to_string()),
        })
        .collect()
}

fn mount_diagnostics_to_discovery(
    provider_key: &str,
    diagnostics: Vec<MountFileDiscoveryDiagnostic>,
) -> Vec<SkillDiscoveryDiagnostic> {
    diagnostics
        .into_iter()
        .map(|diag| SkillDiscoveryDiagnostic {
            provider_key: provider_key.to_string(),
            code: "vfs_file_diagnostic".to_string(),
            message: diag.message,
            local_name: None,
            file_path: Some(format!("{}://{}", diag.mount_id, diag.path)),
        })
        .collect()
}

fn log_discovery_diagnostics(
    diagnostics_label: &'static str,
    diagnostics: &[SkillDiscoveryDiagnostic],
) {
    for diag in diagnostics {
        tracing::warn!(
            label = diagnostics_label,
            provider_key = %diag.provider_key,
            code = %diag.code,
            local_name = diag.local_name.as_deref().unwrap_or(""),
            "skill discovery 诊断: {}",
            diag.message
        );
    }
}

fn log_memory_discovery_diagnostics(
    diagnostics_label: &'static str,
    diagnostics: &[MemoryDiscoveryDiagnostic],
) {
    for diag in diagnostics {
        tracing::warn!(
            label = diagnostics_label,
            provider_key = %diag.provider_key,
            code = %diag.code,
            source_key = diag.source_key.as_deref().unwrap_or(""),
            "memory discovery 诊断: {}",
            diag.message
        );
    }
}

fn log_skill_diagnostics(
    diagnostics_label: &'static str,
    source: &'static str,
    diagnostics: &[crate::skill::SkillDiagnostic],
) {
    for diag in diagnostics {
        tracing::warn!(
            label = diagnostics_label,
            source,
            skill_name = %diag.name,
            path = %diag.file_path.display(),
            "skill 诊断: {}",
            diag.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_vfs::{
        ListOptions, ListResult, MountError, MountOperationContext, MountProvider,
        MountProviderRegistry, PROVIDER_INLINE_FS, ReadResult, RuntimeFileEntry, SearchQuery,
        SearchResult,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn mount_file(mount_id: &str, path: &str, content: &str) -> DiscoveredMountFile {
        DiscoveredMountFile {
            rule_key: "agents_md".to_string(),
            mount_id: mount_id.to_string(),
            path: path.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn merge_guidelines_sorts_by_mount_depth_path_and_dedupes() {
        let merged = merge_discovered_guideline_files(vec![
            mount_file("workspace", "packages/b/AGENTS.md", "B"),
            mount_file("workspace", "AGENTS.md", "ROOT"),
            mount_file("workspace", "packages/a/AGENTS.md", "A"),
            // 重复路径（规范化后相同），应去重保留首个。
            mount_file("workspace", "./AGENTS.md", "ROOT-DUP"),
            // 不同 mount 即便同路径也保留。
            mount_file("docs", "AGENTS.md", "DOCS"),
        ]);

        let paths: Vec<(&str, &str)> = merged
            .iter()
            .map(|g| (g.mount_id.as_str(), g.path.as_str()))
            .collect();

        // docs 在 workspace 之前（mount_id 字典序）；workspace 内根在前、深层在后。
        assert_eq!(
            paths,
            vec![
                ("docs", "AGENTS.md"),
                ("workspace", "AGENTS.md"),
                ("workspace", "packages/a/AGENTS.md"),
                ("workspace", "packages/b/AGENTS.md"),
            ]
        );
        // 去重：./AGENTS.md 与 AGENTS.md 视为同一文件，仅保留首个 ROOT。
        let root = merged
            .iter()
            .find(|g| g.mount_id == "workspace" && g.path == "AGENTS.md")
            .expect("root guideline");
        assert_eq!(root.content, "ROOT");
        assert_eq!(merged.len(), 4);
    }

    fn identity_for_projection() -> AuthIdentity {
        AuthIdentity {
            auth_mode: agentdash_spi::AuthMode::Enterprise,
            user_id: "user-123".to_string(),
            subject: "subject-123".to_string(),
            display_name: Some("Ada Lovelace".to_string()),
            email: Some("ada@example.com".to_string()),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
            groups: vec![
                agentdash_spi::AuthGroup {
                    group_id: "gameplay".to_string(),
                    display_name: Some("Gameplay".to_string()),
                },
                agentdash_spi::AuthGroup {
                    group_id: "tools".to_string(),
                    display_name: None,
                },
            ],
            is_admin: true,
            provider: Some("test".to_string()),
            extra: serde_json::json!({
                "project_id": "should-not-be-projected",
                "agent_type": "should-not-be-projected"
            }),
        }
    }

    fn vfs_with_default_root(root_ref: &str) -> Vfs {
        Vfs {
            mounts: vec![agentdash_domain::common::Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend".to_string(),
                root_ref: root_ref.to_string(),
                capabilities: vec![agentdash_domain::common::MountCapability::Read],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    fn memory_vfs(files: HashMap<String, String>) -> (VfsService, Vfs) {
        let provider = Arc::new(StaticMemoryMountProvider { files });
        let mut registry = MountProviderRegistry::new();
        registry.register(provider);
        let service = VfsService::new(Arc::new(registry));
        let vfs = Vfs {
            mounts: vec![agentdash_domain::common::Mount {
                id: "agent".to_string(),
                provider: PROVIDER_INLINE_FS.to_string(),
                backend_id: "backend".to_string(),
                root_ref: "F:/raw/workspace/root".to_string(),
                capabilities: vec![
                    agentdash_domain::common::MountCapability::Read,
                    agentdash_domain::common::MountCapability::Write,
                    agentdash_domain::common::MountCapability::List,
                    agentdash_domain::common::MountCapability::Search,
                ],
                default_write: false,
                display_name: "Agent Memory".to_string(),
                metadata: serde_json::json!({
                    "container_id": "knowledge",
                    CONTEXT_CONTAINER_ID_METADATA_KEY: "knowledge",
                    CONTEXT_OWNER_KIND_METADATA_KEY: "project_agent",
                    CONTEXT_OWNER_ID_METADATA_KEY: uuid::Uuid::new_v4().to_string(),
                    "workspace_detected_facts": {
                        "p4": {
                            "workspace_root": "F:/raw/workspace/root"
                        }
                    },
                }),
            }],
            default_mount_id: Some("agent".to_string()),
            source_project_id: Some(uuid::Uuid::new_v4().to_string()),
            source_story_id: None,
            links: Vec::new(),
        };
        (service, vfs)
    }

    #[derive(Default)]
    struct CapturingSkillDiscoveryProvider {
        context: Mutex<Option<SkillDiscoveryContext>>,
    }

    struct StaticMemoryMountProvider {
        files: HashMap<String, String>,
    }

    #[async_trait]
    impl MountProvider for StaticMemoryMountProvider {
        fn provider_id(&self) -> &str {
            PROVIDER_INLINE_FS
        }

        fn supported_capabilities(&self) -> Vec<&str> {
            vec!["read", "write", "list", "search"]
        }

        async fn read_text(
            &self,
            _mount: &agentdash_domain::common::Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            self.files
                .get(path)
                .cloned()
                .map(|content| ReadResult::new(path, content))
                .ok_or_else(|| MountError::NotFound(path.to_string()))
        }

        async fn write_text(
            &self,
            _mount: &agentdash_domain::common::Mount,
            _path: &str,
            _content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            Err(MountError::NotSupported("test memory provider".to_string()))
        }

        async fn list(
            &self,
            _mount: &agentdash_domain::common::Mount,
            _options: &ListOptions,
            _ctx: &MountOperationContext,
        ) -> Result<ListResult, MountError> {
            Ok(ListResult {
                entries: self
                    .files
                    .keys()
                    .map(|path| RuntimeFileEntry::file(path.clone()))
                    .collect(),
            })
        }

        async fn search_text(
            &self,
            _mount: &agentdash_domain::common::Mount,
            _query: &SearchQuery,
            _ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            Err(MountError::NotSupported("test memory provider".to_string()))
        }
    }

    struct ProjectionMemoryProvider {
        max_size_bytes: u64,
    }

    #[async_trait]
    impl MemoryDiscoveryProvider for ProjectionMemoryProvider {
        fn provider_key(&self) -> &str {
            "test.memory"
        }

        fn vfs_discovery_rules(&self) -> Vec<agentdash_spi::MemoryDiscoveryVfsRule> {
            let mut rule = agentdash_spi::MemoryDiscoveryVfsRule::new("memory-index");
            rule.exact_paths = vec!["MEMORY.md".to_string()];
            rule.max_size_bytes = self.max_size_bytes;
            vec![rule]
        }

        async fn discover_from_vfs(
            &self,
            _context: MemoryDiscoveryContext,
            mounts: Vec<MemoryDiscoveryMount>,
            files: Vec<agentdash_spi::MemoryDiscoveryVfsFile>,
        ) -> Result<MemoryDiscoveryOutput, agentdash_spi::MemoryDiscoveryError> {
            let Some(agent_mount) = mounts.into_iter().find(|mount| mount.mount_id == "agent")
            else {
                return Ok(MemoryDiscoveryOutput::default());
            };
            let index_file = files
                .into_iter()
                .find(|file| file.mount_id == "agent" && file.path == "MEMORY.md");
            Ok(MemoryDiscoveryOutput {
                clusters: vec![agentdash_spi::MemoryDiscoveryCluster {
                    provider_key: "test.memory".to_string(),
                    display_name: "Test Memory".to_string(),
                    sources: vec![agentdash_spi::DiscoveredMemorySource {
                        provider_key: "test.memory".to_string(),
                        source_key: "agent".to_string(),
                        display_name: agent_mount.display_name,
                        source_uri: "agent://".to_string(),
                        index_uri: "agent://MEMORY.md".to_string(),
                        mount_id: "agent".to_string(),
                        scope: agentdash_spi::MemorySourceScope::Agent,
                        capabilities: agent_mount.capabilities,
                        format: agentdash_spi::MemorySourceFormat::AgentDash,
                        index_status: if index_file.is_some() {
                            MemoryIndexStatus::Present
                        } else {
                            MemoryIndexStatus::Missing
                        },
                        trust_level: agentdash_spi::MemorySourceTrustLevel::FirstParty,
                        summary: None,
                        bounded_index_content: index_file.map(|file| file.content),
                    }],
                    ..Default::default()
                }],
                diagnostics: Vec::new(),
            })
        }
    }

    #[async_trait]
    impl SkillDiscoveryProvider for CapturingSkillDiscoveryProvider {
        fn provider_key(&self) -> &str {
            "test.capture"
        }

        async fn discover(
            &self,
            context: SkillDiscoveryContext,
        ) -> Result<SkillDiscoveryOutput, agentdash_spi::SkillDiscoveryError> {
            *self.context.lock().expect("context lock") = Some(context);
            Ok(SkillDiscoveryOutput::default())
        }
    }

    fn skill(name: &str, file_path: &str) -> SkillEntry {
        let (provider_key, local_name) = name
            .split_once('/')
            .map(|(provider, local)| (provider.to_string(), local.to_string()))
            .unwrap_or_else(|| ("".to_string(), name.to_string()));
        SkillEntry {
            name: local_name.clone(),
            capability_key: name.to_string(),
            provider_key,
            local_name,
            display_name: None,
            description: String::new(),
            file_path: file_path.to_string(),
            base_dir: None,
            exposure: SkillContextExposure::DefaultExposed,
            disable_model_invocation: false,
        }
    }

    #[test]
    fn live_vfs_skill_merge_uses_provider_identity_instead_of_uri_shape() {
        let existing = vec![
            skill("workspace/old-vfs", "main://skills/old-vfs/SKILL.md"),
            skill(
                "external-integration/plugin-skill",
                "external-integration://skills/plugin-skill/SKILL.md",
            ),
        ];
        let refreshed = vec![skill(
            "workspace/new-vfs",
            "cvs-demo://skills/new-vfs/SKILL.md",
        )];

        let merged = merge_live_vfs_skill_entries(&existing, refreshed);

        assert_eq!(merged.len(), 2);
        assert!(
            merged
                .iter()
                .any(|item| item.capability_key == "workspace/new-vfs")
        );
        assert!(
            merged
                .iter()
                .any(|item| item.capability_key == "external-integration/plugin-skill")
        );
        assert!(
            !merged
                .iter()
                .any(|item| item.capability_key == "workspace/old-vfs")
        );
    }

    #[test]
    fn live_skill_merge_replaces_same_provider_capability_identity() {
        let existing = vec![
            skill("workspace/review", "main://skills/review/SKILL.md"),
            skill("external/review", "external://skills/review-old/SKILL.md"),
        ];
        let refreshed = vec![
            skill("workspace/review", "main://skills/review-new/SKILL.md"),
            skill("external/review", "external://skills/review-new/SKILL.md"),
        ];

        let merged = merge_live_vfs_skill_entries(&existing, refreshed);

        assert_eq!(merged.len(), 2);
        assert!(
            merged
                .iter()
                .any(|item| item.file_path == "main://skills/review-new/SKILL.md")
        );
        assert!(
            merged
                .iter()
                .any(|item| item.file_path == "external://skills/review-new/SKILL.md")
        );
        assert!(
            !merged
                .iter()
                .any(|item| item.file_path.ends_with("review-old/SKILL.md"))
        );
    }

    #[test]
    fn provider_cluster_allows_same_local_name_across_providers() {
        let (clusters, diagnostics) = normalize_provider_clusters(vec![
            SkillProviderCluster {
                provider_key: "a".to_string(),
                display_name: "A".to_string(),
                default_exposed_skills: vec![SkillCapabilityEntry::new(
                    "a",
                    "config-edit",
                    "desc",
                    "/a/SKILL.md",
                )],
                ..Default::default()
            },
            SkillProviderCluster {
                provider_key: "b".to_string(),
                display_name: "B".to_string(),
                default_exposed_skills: vec![SkillCapabilityEntry::new(
                    "b",
                    "config-edit",
                    "desc",
                    "/b/SKILL.md",
                )],
                ..Default::default()
            },
        ]);

        assert!(diagnostics.is_empty());
        let caps = build_session_baseline_capabilities_from_clusters(clusters, diagnostics);
        let keys = caps
            .skills
            .iter()
            .map(|skill| skill.capability_key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["a/config-edit", "b/config-edit"]);
    }

    #[test]
    fn duplicate_local_name_within_provider_produces_diagnostic() {
        let (clusters, diagnostics) = normalize_provider_clusters(vec![SkillProviderCluster {
            provider_key: "a".to_string(),
            display_name: "A".to_string(),
            default_exposed_skills: vec![
                SkillCapabilityEntry::new("a", "config-edit", "desc", "/a/one/SKILL.md"),
                SkillCapabilityEntry::new("a", "config-edit", "desc", "/a/two/SKILL.md"),
            ],
            ..Default::default()
        }]);

        assert_eq!(clusters[0].default_exposed_skills.len(), 1);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "duplicate_local_name");
    }

    fn discovered_skill(local_name: &str, file_path: &str) -> DiscoveredSkill {
        DiscoveredSkill {
            local_name: local_name.to_string(),
            display_name: None,
            description: "desc".to_string(),
            file_path: file_path.to_string(),
            base_dir: None,
            exposure: SkillContextExposure::DefaultExposed,
            disable_model_invocation: false,
        }
    }

    #[test]
    fn vfs_first_provider_rejects_absolute_skill_paths() {
        let output = SkillDiscoveryOutput {
            clusters: vec![SkillDiscoveryCluster {
                provider_key: "dynamic".to_string(),
                display_name: "Dynamic".to_string(),
                skills: vec![discovered_skill("bad", "C:\\workspace\\SKILL.md")],
                ..Default::default()
            }],
            diagnostics: Vec::new(),
        };

        let (clusters, diagnostics) = provider_output_to_surface(output, "fallback", true);

        assert!(clusters[0].default_exposed_skills.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "invalid_vfs_skill_path");
        assert_eq!(diagnostics[0].local_name.as_deref(), Some("bad"));
    }

    #[test]
    fn legacy_provider_keeps_absolute_skill_paths_for_compatibility() {
        let output = SkillDiscoveryOutput {
            clusters: vec![SkillDiscoveryCluster {
                provider_key: "legacy".to_string(),
                display_name: "Legacy".to_string(),
                skills: vec![discovered_skill("old", "/workspace/SKILL.md")],
                ..Default::default()
            }],
            diagnostics: Vec::new(),
        };

        let (clusters, diagnostics) = provider_output_to_surface(output, "fallback", false);

        assert!(diagnostics.is_empty());
        assert_eq!(clusters[0].default_exposed_skills.len(), 1);
        assert_eq!(
            clusters[0].default_exposed_skills[0].file_path,
            "/workspace/SKILL.md"
        );
    }

    #[test]
    fn controlled_vfs_skill_path_validation_rejects_local_path_shapes() {
        assert!(is_controlled_vfs_skill_path(
            "main://skills/review/SKILL.md",
            false
        ));
        assert!(is_controlled_vfs_skill_path("main://", true));
        assert!(!is_controlled_vfs_skill_path("main://", false));
        assert!(!is_controlled_vfs_skill_path("file:///tmp/SKILL.md", false));
        assert!(!is_controlled_vfs_skill_path("main:///tmp/SKILL.md", false));
        assert!(!is_controlled_vfs_skill_path(
            "main://C:/tmp/SKILL.md",
            false
        ));
        assert!(!is_controlled_vfs_skill_path(
            "main://skills\\review\\SKILL.md",
            false
        ));
        assert!(!is_controlled_vfs_skill_path(
            "main://skills/../SKILL.md",
            false
        ));
    }

    #[tokio::test]
    async fn extra_skill_dirs_are_wrapped_as_integration_static_cluster() {
        let root = tempfile::tempdir().expect("tempdir");
        let skill_dir = root.path().join("review");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: review\ndescription: Review changes\n---\n",
        )
        .expect("skill file");

        let caps = derive_runtime_skill_baseline(RuntimeCapabilityProjectionInput {
            vfs_service: None,
            active_vfs: None,
            identity: None,
            extra_skill_dirs: &[root.path().to_path_buf()],
            skill_discovery_providers: &[],
            diagnostics_label: "test",
        })
        .await
        .expect("capabilities");

        assert_eq!(caps.skills.len(), 1);
        assert_eq!(caps.skills[0].capability_key, "integration-static/review");
        assert_eq!(caps.skill_clusters.len(), 1);
        assert_eq!(
            caps.skill_clusters[0].provider_key,
            INTEGRATION_STATIC_SKILL_PROVIDER_KEY
        );
    }

    #[tokio::test]
    async fn dynamic_skill_discovery_provider_receives_user_context_projection() {
        let vfs = vfs_with_default_root("/workspace/project");
        let identity = identity_for_projection();
        let provider = Arc::new(CapturingSkillDiscoveryProvider::default());
        let providers: Vec<Arc<dyn SkillDiscoveryProvider>> = vec![provider.clone()];

        let _ = derive_runtime_skill_baseline(RuntimeCapabilityProjectionInput {
            vfs_service: None,
            active_vfs: Some(&vfs),
            identity: Some(&identity),
            extra_skill_dirs: &[],
            skill_discovery_providers: &providers,
            diagnostics_label: "test",
        })
        .await;

        let captured = provider
            .context
            .lock()
            .expect("context lock")
            .clone()
            .expect("provider context");
        assert_eq!(
            captured.workspace_root_ref.as_deref(),
            Some("/workspace/project")
        );
        let user = captured.user.expect("user context");
        assert_eq!(user.user_id, "user-123");
        assert_eq!(user.display_name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(user.email.as_deref(), Some("ada@example.com"));
        assert_eq!(user.groups, vec!["gameplay", "tools"]);
        assert!(captured.project_id.is_none());
        assert!(captured.agent_type.is_none());
        assert!(captured.detected_facts.is_none());
    }

    #[test]
    fn skill_discovery_context_without_identity_keeps_user_absent() {
        let vfs = vfs_with_default_root("/workspace/project");

        let context = skill_discovery_context_from_vfs_and_identity(Some(&vfs), None);

        assert_eq!(
            context.workspace_root_ref.as_deref(),
            Some("/workspace/project")
        );
        assert!(context.user.is_none());
    }

    #[test]
    fn memory_mount_summary_omits_raw_root_ref_and_detected_workspace_root() {
        let (_service, vfs) = memory_vfs(HashMap::new());

        let summaries = memory_discovery_mounts_from_vfs(&vfs);

        assert_eq!(summaries.len(), 1);
        let summary = &summaries[0];
        assert_eq!(summary.mount_id, "agent");
        let encoded = serde_json::to_string(summary).expect("summary json");
        assert!(!encoded.contains("F:/raw/workspace/root"));
        assert!(!encoded.contains("root_ref"));
        assert!(!encoded.contains("backend"));
        assert_eq!(summary.purpose.as_deref(), Some("agent_knowledge"));
        assert_eq!(summary.owner_kind.as_deref(), Some("project_agent"));
    }

    #[tokio::test]
    async fn runtime_memory_projection_attaches_bounded_index_from_vfs() {
        let (service, vfs) = memory_vfs(HashMap::from([(
            "MEMORY.md".to_string(),
            "- [Workflow notes](topics/workflow.md)".to_string(),
        )]));
        let provider = Arc::new(ProjectionMemoryProvider {
            max_size_bytes: 1024,
        });
        let providers: Vec<Arc<dyn MemoryDiscoveryProvider>> = vec![provider];

        let output = derive_runtime_memory_inventory(RuntimeMemoryProjectionInput {
            vfs_service: Some(&service),
            active_vfs: Some(&vfs),
            identity: None,
            memory_discovery_providers: &providers,
            diagnostics_label: "test",
        })
        .await;

        let source = &output.clusters[0].sources[0];
        assert_eq!(source.index_status, MemoryIndexStatus::Present);
        assert_eq!(
            source.bounded_index_content.as_deref(),
            Some("- [Workflow notes](topics/workflow.md)")
        );
    }

    #[tokio::test]
    async fn runtime_memory_projection_marks_oversized_index_without_body() {
        let (service, vfs) = memory_vfs(HashMap::from([(
            "MEMORY.md".to_string(),
            "oversized index".to_string(),
        )]));
        let provider = Arc::new(ProjectionMemoryProvider { max_size_bytes: 4 });
        let providers: Vec<Arc<dyn MemoryDiscoveryProvider>> = vec![provider];

        let output = derive_runtime_memory_inventory(RuntimeMemoryProjectionInput {
            vfs_service: Some(&service),
            active_vfs: Some(&vfs),
            identity: None,
            memory_discovery_providers: &providers,
            diagnostics_label: "test",
        })
        .await;

        let source = &output.clusters[0].sources[0];
        assert_eq!(source.index_status, MemoryIndexStatus::TooLarge);
        assert!(source.bounded_index_content.is_none());
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diag| diag.code == "memory_index_too_large"
                    && diag.uri.as_deref() == Some("agent://MEMORY.md"))
        );
    }
}
