use std::sync::Arc;

use agentdash_platform_spi::{
    AgentToolResult, ContentPart, RuntimeVfsAccessPolicy, RuntimeVfsAccessSource, Vfs,
};
use tokio::sync::RwLock;

use crate::{ResourceRef, compile_whole_mount_runtime_vfs_access_policy, parse_mount_uri};

/// Resolve a tool parameter path into a `ResourceRef`.
///
/// Rules:
/// 1. Contains `://` -> split into mount_id and relative path by URI syntax
/// 2. No `://` and the VFS has a default mount -> use that mount implicitly
/// 3. No `://` and the VFS has exactly one mount -> use that mount implicitly
/// 4. Otherwise -> error, require explicit mount prefix
pub fn resolve_uri_path(vfs: &Vfs, path: &str) -> Result<ResourceRef, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path must not be empty".to_string());
    }

    if trimmed.contains("://") {
        return parse_mount_uri(trimmed, vfs);
    }

    let has_default_mount = vfs
        .default_mount_id
        .as_ref()
        .is_some_and(|default_mount_id| {
            vfs.mounts.iter().any(|mount| &mount.id == default_mount_id)
        });
    if has_default_mount || vfs.mounts.len() == 1 {
        return parse_mount_uri(trimmed, vfs);
    }

    Err(format!(
        "path `{trimmed}` is missing a mount prefix (format: mount_id://path). \
        The current session has {} mount(s); call mounts_list to see available mounts, \
        then use a fully qualified URI.",
        vfs.mounts.len(),
    ))
}

#[derive(Clone)]
pub struct SharedRuntimeVfs {
    inner: Arc<RwLock<RuntimeVfsState>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVfsState {
    pub vfs: Vfs,
    pub access_policy: RuntimeVfsAccessPolicy,
}

impl RuntimeVfsState {
    pub fn new(vfs: Vfs, access_policy: RuntimeVfsAccessPolicy) -> Self {
        let access_policy = normalize_runtime_vfs_access_policy_for_vfs(&vfs, access_policy);
        Self { vfs, access_policy }
    }
}

fn normalize_runtime_vfs_access_policy_for_vfs(
    vfs: &Vfs,
    access_policy: RuntimeVfsAccessPolicy,
) -> RuntimeVfsAccessPolicy {
    let mut explicit_mounts = std::collections::BTreeSet::new();
    let mut rules = Vec::new();

    for rule in access_policy.rules {
        if rule.source == RuntimeVfsAccessSource::SystemRuntimeProjection {
            continue;
        }
        explicit_mounts.insert(rule.mount_id.clone());
        rules.push(rule);
    }

    rules.extend(
        compile_whole_mount_runtime_vfs_access_policy(vfs)
            .rules
            .into_iter()
            .filter(|rule| !explicit_mounts.contains(&rule.mount_id)),
    );

    RuntimeVfsAccessPolicy { rules }
}

impl SharedRuntimeVfs {
    pub fn new(vfs: Vfs) -> Self {
        let access_policy = compile_whole_mount_runtime_vfs_access_policy(&vfs);
        Self::new_with_policy(vfs, access_policy)
    }

    pub fn new_with_policy(vfs: Vfs, access_policy: RuntimeVfsAccessPolicy) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RuntimeVfsState::new(vfs, access_policy))),
        }
    }

    pub async fn snapshot(&self) -> Vfs {
        self.inner.read().await.vfs.clone()
    }

    pub async fn snapshot_state(&self) -> RuntimeVfsState {
        self.inner.read().await.clone()
    }

    pub async fn access_policy_snapshot(&self) -> RuntimeVfsAccessPolicy {
        self.inner.read().await.access_policy.clone()
    }

    pub async fn replace(&self, vfs: Vfs) {
        let access_policy = compile_whole_mount_runtime_vfs_access_policy(&vfs);
        self.replace_with_policy(vfs, access_policy).await;
    }

    pub async fn replace_with_policy(&self, vfs: Vfs, access_policy: RuntimeVfsAccessPolicy) {
        let mut guard = self.inner.write().await;
        *guard = RuntimeVfsState::new(vfs, access_policy);
    }
}

pub fn ok_text(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: false,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use agentdash_platform_spi::{
        Mount, MountCapability, RuntimeVfsAccessRule, RuntimeVfsOperation, RuntimeVfsPathPattern,
    };

    fn mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "memory".to_string(),
            backend_id: String::new(),
            root_ref: format!("memory://{id}"),
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn shared_runtime_vfs_carries_compiled_access_policy() {
        let vfs = Vfs {
            mounts: vec![mount("main")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let shared = SharedRuntimeVfs::new(vfs);

        let state = shared.snapshot_state().await;

        assert_eq!(state.vfs.mounts[0].id, "main");
        assert!(
            state
                .access_policy
                .admits("main", "README.md", RuntimeVfsOperation::Read)
        );
        assert!(
            !state
                .access_policy
                .admits("main", "README.md", RuntimeVfsOperation::Write)
        );
    }

    #[tokio::test]
    async fn shared_runtime_vfs_rebuilds_system_projection_from_current_vfs() {
        let vfs = Vfs {
            mounts: vec![mount("canvas")],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let stale_policy = RuntimeVfsAccessPolicy::default();

        let shared = SharedRuntimeVfs::new_with_policy(vfs, stale_policy);
        let state = shared.snapshot_state().await;

        assert!(
            state
                .access_policy
                .admits("canvas", "src/main.tsx", RuntimeVfsOperation::Read)
        );
        assert!(
            state
                .access_policy
                .admits("canvas", "src", RuntimeVfsOperation::List)
        );
        assert!(
            state
                .access_policy
                .admits("canvas", "src/main.tsx", RuntimeVfsOperation::Search)
        );
    }

    #[tokio::test]
    async fn shared_runtime_vfs_preserves_explicit_mount_restrictions_as_overrides() {
        let vfs = Vfs {
            mounts: vec![mount("main")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let restricted_policy = RuntimeVfsAccessPolicy {
            rules: vec![RuntimeVfsAccessRule {
                mount_id: "main".to_string(),
                path_pattern: RuntimeVfsPathPattern::Prefix("docs".to_string()),
                operations: BTreeSet::from([RuntimeVfsOperation::Read]),
                source: RuntimeVfsAccessSource::ProjectPreset,
            }],
        };

        let shared = SharedRuntimeVfs::new_with_policy(vfs, restricted_policy);
        let state = shared.snapshot_state().await;

        assert!(
            state
                .access_policy
                .admits("main", "docs/readme.md", RuntimeVfsOperation::Read)
        );
        assert!(
            !state
                .access_policy
                .admits("main", "src/lib.rs", RuntimeVfsOperation::Read)
        );
        assert!(
            !state
                .access_policy
                .admits("main", "docs", RuntimeVfsOperation::List)
        );
    }

    #[test]
    fn resolve_uri_path_unqualified_uses_default_mount_when_available() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let resolved = resolve_uri_path(&vfs, ".").expect("resolve");

        assert_eq!(resolved.mount_id, "main");
        assert_eq!(resolved.path, "");
    }

    #[test]
    fn resolve_uri_path_unqualified_normalizes_through_default_mount() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let resolved = resolve_uri_path(&vfs, "./src//lib.rs").expect("resolve");

        assert_eq!(resolved.mount_id, "main");
        assert_eq!(resolved.path, "src/lib.rs");
    }

    #[test]
    fn resolve_uri_path_unqualified_rejects_parent_escape() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let err = resolve_uri_path(&vfs, "../secret").expect_err("escape rejected");

        assert!(err.contains("越界"));
    }

    #[test]
    fn resolve_uri_path_unqualified_rejects_windows_absolute_path() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let err = resolve_uri_path(&vfs, "C:/repo/file.rs").expect_err("absolute path rejected");

        assert!(err.contains("相对于 mount 根目录"));
    }

    #[test]
    fn resolve_uri_path_unqualified_requires_prefix_without_default_or_single_mount() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let err = resolve_uri_path(&vfs, ".").expect_err("ambiguous path rejected");

        assert!(err.contains("missing a mount prefix"));
    }
}
