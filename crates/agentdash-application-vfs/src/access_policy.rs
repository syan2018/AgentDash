use agentdash_spi::{
    RuntimeVfsAccessPolicy, RuntimeVfsAccessSource, RuntimeVfsOperation, RuntimeVfsPathPattern, Vfs,
};

pub fn compile_whole_mount_runtime_vfs_access_policy(vfs: &Vfs) -> RuntimeVfsAccessPolicy {
    RuntimeVfsAccessPolicy::whole_mounts_from_vfs(vfs)
}

pub fn compile_whole_mount_runtime_vfs_access_policy_with_source(
    vfs: &Vfs,
    source: RuntimeVfsAccessSource,
) -> RuntimeVfsAccessPolicy {
    RuntimeVfsAccessPolicy::whole_mounts_from_vfs_with_source(vfs, source)
}

pub fn runtime_vfs_policy_admits(
    policy: &RuntimeVfsAccessPolicy,
    mount_id: &str,
    normalized_path: &str,
    operation: RuntimeVfsOperation,
) -> bool {
    policy.admits(mount_id, normalized_path, operation)
}

pub fn runtime_vfs_path_pattern_matches(
    pattern: &RuntimeVfsPathPattern,
    normalized_path: &str,
) -> bool {
    pattern.matches_normalized_path(normalized_path)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_spi::{Mount, MountCapability, RuntimeVfsAccessRule};

    use super::*;

    fn mount(id: &str, capabilities: Vec<MountCapability>) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "relay_fs".to_string(),
            backend_id: "backend".to_string(),
            root_ref: format!("/workspace/{id}"),
            capabilities,
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn vfs(mounts: Vec<Mount>) -> Vfs {
        Vfs {
            default_mount_id: mounts.first().map(|mount| mount.id.clone()),
            mounts,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn prefix_pattern_matches_only_normalized_prefix_boundary() {
        let pattern = RuntimeVfsPathPattern::Prefix("docs".to_string());

        assert!(runtime_vfs_path_pattern_matches(&pattern, "docs"));
        assert!(runtime_vfs_path_pattern_matches(&pattern, "docs/readme.md"));
        assert!(!runtime_vfs_path_pattern_matches(
            &pattern,
            "docs2/readme.md"
        ));
        assert!(!runtime_vfs_path_pattern_matches(&pattern, "src/docs"));
    }

    #[test]
    fn empty_prefix_pattern_matches_mount_root() {
        let pattern = RuntimeVfsPathPattern::Prefix(String::new());

        assert!(runtime_vfs_path_pattern_matches(&pattern, ""));
        assert!(runtime_vfs_path_pattern_matches(&pattern, "src/lib.rs"));
    }

    #[test]
    fn matcher_requires_mount_operation_and_path_scope() {
        let policy = RuntimeVfsAccessPolicy {
            rules: vec![RuntimeVfsAccessRule {
                mount_id: "main".to_string(),
                path_pattern: RuntimeVfsPathPattern::Prefix("src".to_string()),
                operations: BTreeSet::from([
                    RuntimeVfsOperation::Read,
                    RuntimeVfsOperation::Search,
                ]),
                source: RuntimeVfsAccessSource::PermissionGrant,
            }],
        };

        assert!(runtime_vfs_policy_admits(
            &policy,
            "main",
            "src/lib.rs",
            RuntimeVfsOperation::Read
        ));
        assert!(!runtime_vfs_policy_admits(
            &policy,
            "main",
            "target/lib.rs",
            RuntimeVfsOperation::Read
        ));
        assert!(!runtime_vfs_policy_admits(
            &policy,
            "docs",
            "src/lib.rs",
            RuntimeVfsOperation::Read
        ));
        assert!(!runtime_vfs_policy_admits(
            &policy,
            "main",
            "src/lib.rs",
            RuntimeVfsOperation::Write
        ));
    }

    #[test]
    fn whole_mount_compiler_preserves_provider_capability_operations() {
        let policy = compile_whole_mount_runtime_vfs_access_policy(&vfs(vec![
            mount(
                "main",
                vec![
                    MountCapability::Read,
                    MountCapability::Write,
                    MountCapability::List,
                    MountCapability::Search,
                    MountCapability::Exec,
                ],
            ),
            mount(
                "readonly",
                vec![MountCapability::Read, MountCapability::Watch],
            ),
        ]));

        assert!(runtime_vfs_policy_admits(
            &policy,
            "main",
            "any/path.rs",
            RuntimeVfsOperation::Read
        ));
        assert!(runtime_vfs_policy_admits(
            &policy,
            "main",
            "any/path.rs",
            RuntimeVfsOperation::Write
        ));
        assert!(runtime_vfs_policy_admits(
            &policy,
            "main",
            "any/path.rs",
            RuntimeVfsOperation::ApplyPatch
        ));
        assert!(runtime_vfs_policy_admits(
            &policy,
            "main",
            "scripts/build.ps1",
            RuntimeVfsOperation::Exec
        ));
        assert!(runtime_vfs_policy_admits(
            &policy,
            "readonly",
            "notes.md",
            RuntimeVfsOperation::Read
        ));
        assert!(!runtime_vfs_policy_admits(
            &policy,
            "readonly",
            "notes.md",
            RuntimeVfsOperation::Write
        ));
        assert!(!runtime_vfs_policy_admits(
            &policy,
            "readonly",
            "notes.md",
            RuntimeVfsOperation::ApplyPatch
        ));
        assert!(!runtime_vfs_policy_admits(
            &policy,
            "readonly",
            "notes.md",
            RuntimeVfsOperation::Exec
        ));
    }

    #[test]
    fn whole_mount_compiler_can_record_source() {
        let policy = compile_whole_mount_runtime_vfs_access_policy_with_source(
            &vfs(vec![mount("main", vec![MountCapability::Read])]),
            RuntimeVfsAccessSource::ProjectPreset,
        );

        assert_eq!(policy.rules.len(), 1);
        assert_eq!(
            policy.rules[0].source,
            RuntimeVfsAccessSource::ProjectPreset
        );
    }
}
