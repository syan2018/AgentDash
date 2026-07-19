use agentdash_domain::backend::RuntimeBackendAnchor;
use agentdash_domain::common::Vfs;

use super::extension_actions::ExtensionInvocationWorkspaceContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionInvocationWorkspaceResolution {
    Workspace(ExtensionInvocationWorkspaceContext),
    NoWorkspace(ExtensionInvocationWorkspaceUnavailableReason),
}

impl ExtensionInvocationWorkspaceResolution {
    pub fn into_workspace(self) -> Option<ExtensionInvocationWorkspaceContext> {
        match self {
            Self::Workspace(workspace) => Some(workspace),
            Self::NoWorkspace(_) => None,
        }
    }

    pub fn workspace(&self) -> Option<&ExtensionInvocationWorkspaceContext> {
        match self {
            Self::Workspace(workspace) => Some(workspace),
            Self::NoWorkspace(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionInvocationWorkspaceUnavailableReason {
    AnchorRootMountNotFound,
    AnchorRootMissingAndDefaultMountNotLocalWorkspace,
}

pub fn resolve_extension_invocation_workspace(
    vfs: &Vfs,
    anchor: &RuntimeBackendAnchor,
) -> ExtensionInvocationWorkspaceResolution {
    let backend_id = anchor.backend_id();
    if let Some(root_ref) = anchor
        .root_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return vfs
            .mounts
            .iter()
            .find(|mount| {
                mount.backend_id == backend_id
                    && is_local_workspace_mount(mount.provider.as_str())
                    && mount.root_ref.trim() == root_ref
                    && !mount.root_ref.trim().is_empty()
            })
            .map(|mount| {
                ExtensionInvocationWorkspaceResolution::Workspace(
                    ExtensionInvocationWorkspaceContext::new(
                        mount.id.clone(),
                        mount.root_ref.trim().to_string(),
                    ),
                )
            })
            .unwrap_or(ExtensionInvocationWorkspaceResolution::NoWorkspace(
                ExtensionInvocationWorkspaceUnavailableReason::AnchorRootMountNotFound,
            ));
    }

    vfs.default_mount()
        .filter(|mount| {
            mount.backend_id == backend_id
                && is_local_workspace_mount(mount.provider.as_str())
                && !mount.root_ref.trim().is_empty()
        })
        .map(|mount| {
            ExtensionInvocationWorkspaceResolution::Workspace(
                ExtensionInvocationWorkspaceContext::new(
                    mount.id.clone(),
                    mount.root_ref.trim().to_string(),
                ),
            )
        })
        .unwrap_or(ExtensionInvocationWorkspaceResolution::NoWorkspace(
            ExtensionInvocationWorkspaceUnavailableReason::AnchorRootMissingAndDefaultMountNotLocalWorkspace,
        ))
}

fn is_local_workspace_mount(provider: &str) -> bool {
    provider == "relay_fs"
}

#[cfg(test)]
mod tests {
    use agentdash_domain::common::{Mount, MountCapability, Vfs};
    use agentdash_platform_spi::RuntimeBackendAnchorSource;

    use super::*;

    #[test]
    fn uses_anchor_root_only_when_mount_matches_backend_and_provider() {
        let vfs = Vfs {
            mounts: vec![
                mount("inline", "backend-1", "D:/Workspaces/main", "inline_fs"),
                mount("local", "backend-1", "D:/Workspaces/main", "relay_fs"),
            ],
            default_mount_id: Some("inline".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let workspace = resolve_extension_invocation_workspace(
            &vfs,
            &anchor("backend-1", Some("D:/Workspaces/main")),
        )
        .workspace()
        .expect("workspace")
        .clone();

        assert_eq!(workspace.mount_id, "local");
        assert_eq!(workspace.root_ref, "D:/Workspaces/main");
    }

    #[test]
    fn does_not_fallback_to_arbitrary_default_mount_without_anchor_root() {
        let vfs = Vfs {
            mounts: vec![mount(
                "main",
                "backend-2",
                "D:/Workspaces/other",
                "relay_fs",
            )],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        assert_eq!(
            resolve_extension_invocation_workspace(&vfs, &anchor("backend-1", None)),
            ExtensionInvocationWorkspaceResolution::NoWorkspace(
                ExtensionInvocationWorkspaceUnavailableReason::AnchorRootMissingAndDefaultMountNotLocalWorkspace
            )
        );
    }

    #[test]
    fn uses_default_mount_when_it_matches_anchor_backend_and_local_provider() {
        let vfs = Vfs {
            mounts: vec![mount("main", "backend-1", "D:/Workspaces/main", "relay_fs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let workspace = resolve_extension_invocation_workspace(&vfs, &anchor("backend-1", None))
            .workspace()
            .expect("workspace")
            .clone();

        assert_eq!(workspace.mount_id, "main");
        assert_eq!(workspace.root_ref, "D:/Workspaces/main");
    }

    fn anchor(backend_id: &str, root_ref: Option<&str>) -> RuntimeBackendAnchor {
        RuntimeBackendAnchor::new(backend_id, RuntimeBackendAnchorSource::System)
            .expect("anchor")
            .with_root_ref(root_ref.map(ToString::to_string))
    }

    fn mount(id: &str, backend_id: &str, root_ref: &str, provider: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: provider.to_string(),
            backend_id: backend_id.to_string(),
            root_ref: root_ref.to_string(),
            capabilities: vec![MountCapability::Read, MountCapability::Write],
            default_write: true,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }
}
