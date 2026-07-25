use crate::surface::{ResolvedMountOwnerKind, ResolvedMountPurpose};
use agentdash_domain::common::Mount;
use agentdash_domain::inline_file::InlineFileOwnerKind;

pub const PROJECT_VFS_MOUNT_CONTAINER_ID: &str = "files";
pub const PROJECT_AGENT_MEMORY_MOUNT_ID: &str = "agent";
pub const PROJECT_AGENT_KNOWLEDGE_CONTAINER_ID: &str = "knowledge";

pub const PROVIDER_RELAY_FS: &str = "relay_fs";
pub const PROVIDER_INLINE_FS: &str = "inline_fs";
pub const PROVIDER_LIFECYCLE_VFS: &str = "lifecycle_vfs";
pub const PROVIDER_ROUTINE_VFS: &str = "routine_vfs";
pub const PROVIDER_CANVAS_FS: &str = "canvas_fs";
pub const PROVIDER_SKILL_ASSET_FS: &str = "skill_asset_fs";
pub(crate) const CONTEXT_OWNER_KIND_METADATA_KEY: &str = "agentdash_context_owner_kind";
pub(crate) const CONTEXT_OWNER_ID_METADATA_KEY: &str = "agentdash_context_owner_id";
pub(crate) const CONTEXT_CONTAINER_ID_METADATA_KEY: &str = "agentdash_context_container_id";
pub(crate) const PROJECT_VFS_MOUNT_METADATA_KEY: &str = "agentdash_project_vfs_mount";
pub(crate) const SKILL_ASSET_PROJECT_ID_METADATA_KEY: &str = "skill_asset_project_id";
pub const SKILL_ASSET_KEYS_METADATA_KEY: &str = "skill_asset_keys";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMountTarget {
    Project,
    Story,
    Task,
}

impl From<agentdash_platform_spi::CapabilityScope> for SessionMountTarget {
    fn from(scope: agentdash_platform_spi::CapabilityScope) -> Self {
        match scope {
            agentdash_platform_spi::CapabilityScope::Project => Self::Project,
            agentdash_platform_spi::CapabilityScope::Story => Self::Story,
            agentdash_platform_spi::CapabilityScope::Task => Self::Task,
        }
    }
}

pub fn mount_container_id(mount: &Mount) -> Option<&str> {
    mount
        .metadata
        .get(CONTEXT_CONTAINER_ID_METADATA_KEY)
        .and_then(|value| value.as_str())
}

pub fn mount_owner_kind(mount: &Mount) -> ResolvedMountOwnerKind {
    if mount.provider == PROVIDER_RELAY_FS {
        return ResolvedMountOwnerKind::Workspace;
    }
    if mount.provider == PROVIDER_LIFECYCLE_VFS {
        return ResolvedMountOwnerKind::Session;
    }
    if mount.provider == PROVIDER_ROUTINE_VFS {
        return ResolvedMountOwnerKind::Session;
    }
    if mount.provider == PROVIDER_CANVAS_FS {
        return ResolvedMountOwnerKind::Canvas;
    }
    if mount.provider == PROVIDER_SKILL_ASSET_FS {
        return ResolvedMountOwnerKind::Project;
    }
    match mount
        .metadata
        .get(CONTEXT_OWNER_KIND_METADATA_KEY)
        .and_then(|value| value.as_str())
    {
        Some("project") => ResolvedMountOwnerKind::Project,
        Some("story") => ResolvedMountOwnerKind::Story,
        Some("task") => ResolvedMountOwnerKind::Task,
        Some("session") => ResolvedMountOwnerKind::Session,
        Some(value) if value == InlineFileOwnerKind::ProjectAgent.as_str() => {
            ResolvedMountOwnerKind::ProjectAgent
        }
        Some(value) if value == InlineFileOwnerKind::ProjectVfsMount.as_str() => {
            ResolvedMountOwnerKind::Project
        }
        Some("canvas") => ResolvedMountOwnerKind::Canvas,
        Some("workspace") => ResolvedMountOwnerKind::Workspace,
        _ => ResolvedMountOwnerKind::External,
    }
}

pub fn mount_owner_id(mount: &Mount) -> String {
    mount
        .metadata
        .get(CONTEXT_OWNER_ID_METADATA_KEY)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

pub fn mount_purpose(mount: &Mount) -> ResolvedMountPurpose {
    let owner_kind = mount_owner_kind(mount);
    if mount.id == PROJECT_AGENT_MEMORY_MOUNT_ID
        && owner_kind == ResolvedMountOwnerKind::ProjectAgent
    {
        return ResolvedMountPurpose::AgentKnowledge;
    }
    match mount.provider.as_str() {
        PROVIDER_RELAY_FS => ResolvedMountPurpose::Workspace,
        PROVIDER_LIFECYCLE_VFS => ResolvedMountPurpose::Lifecycle,
        PROVIDER_ROUTINE_VFS => ResolvedMountPurpose::ExternalService,
        PROVIDER_CANVAS_FS => ResolvedMountPurpose::Canvas,
        PROVIDER_SKILL_ASSET_FS => ResolvedMountPurpose::ProjectContainer,
        PROVIDER_INLINE_FS => match owner_kind {
            ResolvedMountOwnerKind::Project => {
                if is_project_vfs_mount(mount) {
                    ResolvedMountPurpose::VfsMount
                } else {
                    ResolvedMountPurpose::ProjectContainer
                }
            }
            ResolvedMountOwnerKind::Story => ResolvedMountPurpose::StoryContainer,
            ResolvedMountOwnerKind::ProjectAgent => ResolvedMountPurpose::AgentKnowledge,
            _ => ResolvedMountPurpose::ExternalService,
        },
        _ => ResolvedMountPurpose::ExternalService,
    }
}

pub(crate) fn is_project_vfs_mount(mount: &Mount) -> bool {
    mount
        .metadata
        .get(PROJECT_VFS_MOUNT_METADATA_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}
