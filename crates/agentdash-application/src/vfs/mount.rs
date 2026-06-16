use std::collections::{BTreeMap, BTreeSet};

use super::mount_inline::{annotate_context_mount_owner, build_context_container_mount};
use super::mount_workspace::workspace_mount;
use super::path::validate_vfs;
use crate::runtime::{Mount, MountCapability, Vfs};
use crate::vfs::surface::{ResolvedMountOwnerKind, ResolvedMountPurpose};
use agentdash_domain::common::AgentVfsAccessGrant;
use agentdash_domain::context_container::{ContextContainerDefinition, ContextContainerProvider};
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::project_vfs_mount::{ProjectVfsMount, ProjectVfsMountContent};

pub const PROJECT_VFS_MOUNT_CONTAINER_ID: &str = "files";
use agentdash_domain::{
    agent::ProjectAgent,
    project::Project,
    story::Story,
    workspace::Workspace,
};

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
pub(crate) const SKILL_ASSET_KEYS_METADATA_KEY: &str = "skill_asset_keys";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextContainerOwnerScope {
    Project,
    Story,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMountTarget {
    Project,
    Story,
    Task,
}

impl From<agentdash_spi::CapabilityScope> for SessionMountTarget {
    fn from(scope: agentdash_spi::CapabilityScope) -> Self {
        match scope {
            agentdash_spi::CapabilityScope::Project => Self::Project,
            agentdash_spi::CapabilityScope::Story => Self::Story,
            agentdash_spi::CapabilityScope::Task => Self::Task,
        }
    }
}

/// 从 Project / Story / Workspace 策略构建最终 VFS
pub fn build_derived_vfs(
    project: &Project,
    project_vfs_mounts: &[ProjectVfsMount],
    story: Option<&Story>,
    workspace: Option<&Workspace>,
    _agent_type: Option<&str>,
    target: SessionMountTarget,
) -> Result<Vfs, String> {
    let mut mounts = Vec::new();

    if let Some(workspace) = workspace {
        mounts.push(workspace_mount(workspace)?);
    }

    let mut project_mounts = project_vfs_mounts
        .iter()
        .map(build_project_vfs_mount_mount)
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(story) = story {
        let disabled = story
            .context
            .disabled_container_ids
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(|item| item.to_string())
            .collect::<BTreeSet<_>>();
        if !disabled.is_empty() {
            project_mounts.retain(|mount| !disabled.contains(mount.id.trim()));
        }

        for container in &story.context.context_containers {
            project_mounts.retain(|mount| mount.id.trim() != container.mount_id.trim());
        }
    }

    if matches!(target, SessionMountTarget::Story | SessionMountTarget::Task) {
        project_mounts.retain(|mount| !is_external_project_vfs_mount(mount));
    }

    mounts.extend(project_mounts);

    if let Some(story) = story {
        for container in &story.context.context_containers {
            let mut mount = build_context_container_mount(container)?;
            annotate_context_mount_owner(&mut mount, "story", story.id);
            mounts.push(mount);
        }
    }

    let default_mount_id = if mounts.iter().any(|mount| mount.id == "main") {
        Some("main".to_string())
    } else {
        mounts.first().map(|mount| mount.id.clone())
    };

    let vfs = Vfs {
        mounts,
        default_mount_id,
        source_project_id: Some(project.id.to_string()),
        source_story_id: story.map(|s| s.id.to_string()),
        links: Vec::new(),
    };
    validate_vfs(&vfs)?;
    Ok(vfs)
}

/// 为 Agent 知识容器构建单个 mount（knowledge_enabled=true 时调用）
///
/// 知识容器定义由系统自动派生：mount_id = "agent-knowledge"，
/// container_id = "knowledge"，按 ProjectAgent 隔离。
fn build_agent_knowledge_mount(agent: &ProjectAgent) -> Result<Mount, String> {
    let container = ContextContainerDefinition {
        mount_id: "agent-knowledge".to_string(),
        display_name: "Agent 知识库".to_string(),
        provider: ContextContainerProvider::InlineFiles { files: vec![] },
        capabilities: vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: false,
    };
    let mut mount = build_context_container_mount(&container)?;
    annotate_context_mount_owner(
        &mut mount,
        InlineFileOwnerKind::ProjectAgent.as_str(),
        agent.id,
    );
    Ok(mount)
}

/// 将 Agent 知识 mount 追加到已有 VFS（仅当 knowledge_enabled=true）
pub fn append_agent_knowledge_mounts(vfs: &mut Vfs, agent: &ProjectAgent) -> Result<(), String> {
    if !agent.knowledge_enabled {
        return Ok(());
    }
    let mount = build_agent_knowledge_mount(agent)?;
    if !vfs.mounts.iter().any(|m| m.id == mount.id) {
        vfs.mounts.push(mount);
    }
    Ok(())
}

/// 构建仅包含 Agent 私有知识库的 VFS。
///
/// 该 surface 只用于 Agent 页知识库浏览，不混入 project/workspace/lifecycle/canvas mounts。
pub fn build_project_agent_knowledge_vfs(agent: &ProjectAgent) -> Result<Vfs, String> {
    let mut vfs = Vfs {
        mounts: Vec::new(),
        default_mount_id: None,
        source_project_id: Some(agent.project_id.to_string()),
        source_story_id: None,
        links: Vec::new(),
    };
    append_agent_knowledge_mounts(&mut vfs, agent)?;
    vfs.default_mount_id = vfs.mounts.first().map(|mount| mount.id.clone());
    validate_vfs(&vfs)?;
    Ok(vfs)
}

pub fn apply_agent_vfs_access_grants(vfs: &mut Vfs, grants: Option<&[AgentVfsAccessGrant]>) {
    let grants_by_mount = grants
        .unwrap_or_default()
        .iter()
        .map(|grant| (grant.mount_id.trim(), grant))
        .filter(|(mount_id, _)| !mount_id.is_empty())
        .collect::<BTreeMap<_, _>>();

    for mount in &mut vfs.mounts {
        if !is_project_vfs_mount(mount) {
            continue;
        }
        let Some(grant) = grants_by_mount.get(mount.id.as_str()) else {
            mount.capabilities.clear();
            continue;
        };
        let allowed = grant.capabilities.as_slice();
        mount
            .capabilities
            .retain(|capability| allowed.contains(capability));
        if !mount.capabilities.contains(&MountCapability::Write) {
            mount.default_write = false;
        }
    }

    vfs.mounts
        .retain(|mount| !is_project_vfs_mount(mount) || !mount.capabilities.is_empty());
    reset_default_mount(vfs);
}

pub fn effective_context_containers(
    project: &Project,
    story: Option<&Story>,
) -> Vec<ContextContainerDefinition> {
    effective_context_containers_with_origin(project, story)
        .into_iter()
        .map(|(container, _)| container)
        .collect()
}

pub fn build_project_vfs_mount_mount(mount: &ProjectVfsMount) -> Result<Mount, String> {
    let id = non_empty_trimmed(&mount.mount_id, "mount_id")?.to_string();
    let display_name = if mount.display_name.trim().is_empty() {
        id.clone()
    } else {
        mount.display_name.trim().to_string()
    };
    let capabilities = if mount.capabilities.is_empty() {
        vec![
            MountCapability::Read,
            MountCapability::List,
            MountCapability::Search,
        ]
    } else {
        mount.capabilities.to_vec()
    };

    let (provider, root_ref, owner_kind, owner_id, metadata) = match &mount.content {
        ProjectVfsMountContent::Inline => (
            PROVIDER_INLINE_FS.to_string(),
            format!("project-vfs-mount://{}", mount.id),
            InlineFileOwnerKind::ProjectVfsMount.as_str(),
            mount.id,
            serde_json::json!({
                "container_id": PROJECT_VFS_MOUNT_CONTAINER_ID,
                CONTEXT_CONTAINER_ID_METADATA_KEY: id,
                "project_vfs_mount_id": mount.id.to_string(),
                PROJECT_VFS_MOUNT_METADATA_KEY: true,
            }),
        ),
        ProjectVfsMountContent::ExternalService {
            service_id,
            root_ref,
        } => (
            non_empty_trimmed(service_id, "external_service.service_id")?.to_string(),
            non_empty_trimmed(root_ref, "external_service.root_ref")?.to_string(),
            "project",
            mount.project_id,
            serde_json::json!({
                "service_id": service_id.trim(),
                "root_ref": root_ref.trim(),
                CONTEXT_CONTAINER_ID_METADATA_KEY: id,
                PROJECT_VFS_MOUNT_METADATA_KEY: true,
            }),
        ),
    };
    let mut runtime_mount = Mount {
        id,
        provider,
        backend_id: String::new(),
        root_ref,
        capabilities,
        default_write: false,
        display_name,
        metadata,
    };
    annotate_context_mount_owner(&mut runtime_mount, owner_kind, owner_id);
    Ok(runtime_mount)
}

fn effective_context_containers_with_origin(
    project: &Project,
    story: Option<&Story>,
) -> Vec<(ContextContainerDefinition, ContextContainerOwnerScope)> {
    let mut containers = project.config.context_containers.clone();
    let mut owned = containers
        .drain(..)
        .map(|container| (container, ContextContainerOwnerScope::Project))
        .collect::<Vec<_>>();
    if let Some(story) = story {
        let disabled = story
            .context
            .disabled_container_ids
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(|item| item.to_string())
            .collect::<BTreeSet<_>>();
        if !disabled.is_empty() {
            owned.retain(|(container, _)| !disabled.contains(container.mount_id.trim()));
        }

        for container in &story.context.context_containers {
            owned.retain(|(item, _)| item.mount_id.trim() != container.mount_id.trim());
            owned.push((container.clone(), ContextContainerOwnerScope::Story));
        }
    }
    owned
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
    if mount.id == "agent-knowledge" {
        return ResolvedMountPurpose::AgentKnowledge;
    }
    match mount.provider.as_str() {
        PROVIDER_RELAY_FS => ResolvedMountPurpose::Workspace,
        PROVIDER_LIFECYCLE_VFS => ResolvedMountPurpose::Lifecycle,
        PROVIDER_ROUTINE_VFS => ResolvedMountPurpose::ExternalService,
        PROVIDER_CANVAS_FS => ResolvedMountPurpose::Canvas,
        PROVIDER_SKILL_ASSET_FS => ResolvedMountPurpose::ProjectContainer,
        PROVIDER_INLINE_FS => match mount_owner_kind(mount) {
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

fn is_project_vfs_mount(mount: &Mount) -> bool {
    mount
        .metadata
        .get(PROJECT_VFS_MOUNT_METADATA_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn is_external_project_vfs_mount(mount: &Mount) -> bool {
    is_project_vfs_mount(mount) && mount.provider != PROVIDER_INLINE_FS
}

fn reset_default_mount(vfs: &mut Vfs) {
    if let Some(default_mount_id) = vfs.default_mount_id.as_deref()
        && vfs.mounts.iter().any(|mount| mount.id == default_mount_id)
    {
        return;
    }
    vfs.default_mount_id = if vfs.mounts.iter().any(|mount| mount.id == "main") {
        Some("main".to_string())
    } else {
        vfs.mounts.first().map(|mount| mount.id.clone())
    };
}

fn non_empty_trimmed<'a>(value: &'a str, field_name: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field_name} 不能为空"))
    } else {
        Ok(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::context_container::{
        ContextContainerDefinition, ContextContainerFile, ContextContainerProvider,
    };
    use uuid::Uuid;

    fn inline_container(mount_id: &str, path: &str) -> ContextContainerDefinition {
        ContextContainerDefinition {
            mount_id: mount_id.to_string(),
            display_name: mount_id.to_string(),
            provider: ContextContainerProvider::InlineFiles {
                files: vec![ContextContainerFile {
                    path: path.to_string(),
                    content: "content".to_string(),
                }],
            },
            capabilities: vec![],
            default_write: false,
        }
    }

    fn inline_mount(project_id: Uuid, mount_id: &str) -> ProjectVfsMount {
        ProjectVfsMount::new_inline(project_id, mount_id.to_string(), mount_id.to_string())
    }

    fn external_mount(project_id: Uuid, mount_id: &str) -> ProjectVfsMount {
        ProjectVfsMount::new_external_service(
            project_id,
            mount_id.to_string(),
            mount_id.to_string(),
            "external_docs".to_string(),
            "knowledge".to_string(),
        )
    }

    #[test]
    fn workspace_mount_metadata_includes_selected_binding_detected_facts() {
        let project_id = Uuid::new_v4();
        let mut workspace = Workspace::new(
            project_id,
            "p4 workspace".to_string(),
            agentdash_domain::workspace::WorkspaceIdentityKind::P4Workspace,
            serde_json::json!({ "client_name": "p4-client-main" }),
            agentdash_domain::workspace::WorkspaceResolutionPolicy::PreferDefaultBinding,
        );
        let mut binding = WorkspaceBinding::new(
            workspace.id,
            "backend-1".to_string(),
            "main://workspace".to_string(),
            serde_json::json!({
                "p4": {
                    "client_name": "p4-client-main",
                    "workspace_root": "F:/work/main"
                }
            }),
        );
        binding.status = agentdash_domain::workspace::WorkspaceBindingStatus::Ready;
        workspace.set_bindings(vec![binding]);

        let mount = workspace_mount(&workspace).expect("workspace mount");

        assert_eq!(
            mount
                .metadata
                .pointer("/workspace_detected_facts/p4/client_name")
                .and_then(|value| value.as_str()),
            Some("p4-client-main")
        );
        assert_eq!(
            mount
                .metadata
                .pointer("/workspace_detected_facts/p4/workspace_root")
                .and_then(|value| value.as_str()),
            Some("F:/work/main")
        );
    }

    #[test]
    fn story_override_container_is_marked_as_story_owned() {
        let project = Project::new("proj".to_string(), "desc".to_string());
        let mounts = vec![inline_mount(project.id, "brief")];

        let mut story = Story::new(project.id, "story".to_string(), "desc".to_string());
        story.context.context_containers = vec![inline_container("brief", "story.md")];

        let vfs = build_derived_vfs(
            &project,
            &mounts,
            Some(&story),
            None,
            None,
            SessionMountTarget::Story,
        )
        .expect("VFS should build");

        let mount = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "brief")
            .expect("brief mount should exist");
        assert_eq!(
            mount.metadata.get(CONTEXT_OWNER_KIND_METADATA_KEY),
            Some(&serde_json::Value::String("story".to_string()))
        );
        assert_eq!(
            mount
                .metadata
                .get(CONTEXT_OWNER_ID_METADATA_KEY)
                .and_then(|v| v.as_str()),
            Some(story.id.to_string()).as_deref()
        );
        assert_eq!(
            mount.metadata.get("container_id").and_then(|v| v.as_str()),
            Some("brief")
        );
    }

    #[test]
    fn inherited_project_container_is_marked_as_project_owned() {
        let project = Project::new("proj".to_string(), "desc".to_string());
        let mounts = vec![inline_mount(project.id, "spec")];

        let story = Story::new(project.id, "story".to_string(), "desc".to_string());

        let vfs = build_derived_vfs(
            &project,
            &mounts,
            Some(&story),
            None,
            None,
            SessionMountTarget::Story,
        )
        .expect("VFS should build");

        let mount = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "spec")
            .expect("spec mount should exist");
        assert_eq!(
            mount.metadata.get(CONTEXT_OWNER_KIND_METADATA_KEY),
            Some(&serde_json::Value::String(
                InlineFileOwnerKind::ProjectVfsMount.as_str().to_string()
            ))
        );
        assert_eq!(mount_owner_kind(mount), ResolvedMountOwnerKind::Project);
    }

    #[test]
    fn story_target_excludes_external_project_vfs_mounts() {
        let project = Project::new("proj".to_string(), "desc".to_string());
        let mounts = vec![
            inline_mount(project.id, "local"),
            external_mount(project.id, "knowledge"),
        ];
        let story = Story::new(project.id, "story".to_string(), "desc".to_string());

        let vfs = build_derived_vfs(
            &project,
            &mounts,
            Some(&story),
            None,
            None,
            SessionMountTarget::Story,
        )
        .expect("VFS should build");

        assert!(vfs.mounts.iter().any(|mount| mount.id == "local"));
        assert!(!vfs.mounts.iter().any(|mount| mount.id == "knowledge"));
    }

    #[test]
    fn task_target_excludes_external_project_vfs_mounts() {
        let project = Project::new("proj".to_string(), "desc".to_string());
        let mounts = vec![
            inline_mount(project.id, "local"),
            external_mount(project.id, "knowledge"),
        ];

        let vfs = build_derived_vfs(
            &project,
            &mounts,
            None,
            None,
            None,
            SessionMountTarget::Task,
        )
        .expect("VFS should build");

        assert!(vfs.mounts.iter().any(|mount| mount.id == "local"));
        assert!(!vfs.mounts.iter().any(|mount| mount.id == "knowledge"));
    }

    #[test]
    fn project_target_keeps_external_project_vfs_mounts_for_preview_and_grants() {
        let project = Project::new("proj".to_string(), "desc".to_string());
        let mounts = vec![external_mount(project.id, "knowledge")];

        let vfs = build_derived_vfs(
            &project,
            &mounts,
            None,
            None,
            None,
            SessionMountTarget::Project,
        )
        .expect("VFS should build");

        let mount = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "knowledge")
            .expect("project preview should keep external project VFS mount");
        assert_eq!(mount.provider, "external_docs");
        assert!(is_external_project_vfs_mount(mount));
    }
}
