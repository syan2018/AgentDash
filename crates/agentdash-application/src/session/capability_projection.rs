use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use agentdash_spi::context::capability::{
    SessionBaselineCapabilities, SkillCapabilityEntry, SkillEntry, SkillProviderCluster,
};
use agentdash_spi::{
    AuthIdentity, DiscoveredGuideline, RuntimeMcpServer, SkillContextExposure,
    SkillDiscoveryCluster, SkillDiscoveryContext, SkillDiscoveryDiagnostic, SkillDiscoveryOutput,
    SkillDiscoveryProvider, SkillDiscoveryUserContext, Vfs, skill_capability_key,
};

use crate::context::mount_file_discovery::{
    BUILTIN_GUIDELINE_RULES, DiscoveredMountFile, discover_mount_files,
};
use crate::vfs::VfsService;

use super::baseline_capabilities::{
    INTEGRATION_STATIC_SKILL_PROVIDER_KEY, WORKSPACE_SKILL_PROVIDER_KEY,
    build_session_baseline_capabilities_from_clusters, skills_to_provider_clusters,
};
use super::types::CapabilityState;

#[derive(Clone, Copy)]
pub struct SessionCapabilityProjectionInput<'a> {
    pub vfs_service: Option<&'a VfsService>,
    pub active_vfs: Option<&'a Vfs>,
    pub identity: Option<&'a AuthIdentity>,
    pub extra_skill_dirs: &'a [PathBuf],
    pub skill_discovery_providers: &'a [Arc<dyn SkillDiscoveryProvider>],
    pub diagnostics_label: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct SessionCapabilityProjection {
    pub session_capabilities: SessionBaselineCapabilities,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
}

pub async fn derive_session_capability_projection(
    input: SessionCapabilityProjectionInput<'_>,
) -> SessionCapabilityProjection {
    let session_capabilities = derive_session_skill_baseline(input)
        .await
        .unwrap_or_default();
    let discovered_guidelines = match (input.vfs_service, input.active_vfs) {
        (Some(vfs_service), Some(active_vfs)) => {
            derive_session_guidelines(vfs_service, active_vfs, input.diagnostics_label).await
        }
        _ => Vec::new(),
    };

    SessionCapabilityProjection {
        session_capabilities,
        discovered_guidelines,
    }
}

pub async fn derive_session_skill_baseline(
    input: SessionCapabilityProjectionInput<'_>,
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
        match provider.discover(discovery_context.clone()).await {
            Ok(output) => {
                let (provider_clusters, provider_diagnostics) =
                    provider_output_to_surface(output, provider.provider_key());
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

pub async fn derive_session_guidelines(
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
    refreshed_vfs_skills: Vec<SkillEntry>,
) -> Vec<SkillEntry> {
    let mut merged = refreshed_vfs_skills;
    for skill in existing {
        if skill.file_path.contains("://") {
            continue;
        }
        if !merged
            .iter()
            .any(|item| item.capability_key_or_name() == skill.capability_key_or_name())
        {
            merged.push(skill.clone());
        }
    }
    merged
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
    use async_trait::async_trait;
    use std::sync::Mutex;

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

    #[derive(Default)]
    struct CapturingSkillDiscoveryProvider {
        context: Mutex<Option<SkillDiscoveryContext>>,
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
    fn live_vfs_skill_merge_replaces_uri_skills_and_preserves_local_skills() {
        let existing = vec![
            skill("workspace/old-vfs", "main://skills/old-vfs/SKILL.md"),
            skill(
                "integration-static/plugin-skill",
                "/plugins/plugin-skill/SKILL.md",
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
                .any(|item| item.capability_key == "integration-static/plugin-skill")
        );
        assert!(
            !merged
                .iter()
                .any(|item| item.capability_key == "workspace/old-vfs")
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

        let caps = derive_session_skill_baseline(SessionCapabilityProjectionInput {
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

        let _ = derive_session_skill_baseline(SessionCapabilityProjectionInput {
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
}
