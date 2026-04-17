use std::collections::{BTreeMap, BTreeSet};

use super::path::normalize_mount_relative_path;
use crate::runtime::{Vfs, Mount, MountCapability, RuntimeFileEntry};
use crate::vfs::surface::{ResolvedMountOwnerKind, ResolvedMountPurpose};
use agentdash_domain::context_container::{
    ContextContainerDefinition, ContextContainerExposure, ContextContainerProvider,
};
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::{
    agent::ProjectAgentLink,
    canvas::Canvas,
    project::Project,
    story::Story,
    workspace::{Workspace, WorkspaceBinding},
};
use uuid::Uuid;

pub const PROVIDER_RELAY_FS: &str = "relay_fs";
pub const PROVIDER_INLINE_FS: &str = "inline_fs";
pub const PROVIDER_LIFECYCLE_VFS: &str = "lifecycle_vfs";
pub const PROVIDER_CANVAS_FS: &str = "canvas_fs";
pub(crate) const CONTEXT_OWNER_KIND_METADATA_KEY: &str = "agentdash_context_owner_kind";
pub(crate) const CONTEXT_OWNER_ID_METADATA_KEY: &str = "agentdash_context_owner_id";
pub(crate) const CONTEXT_CONTAINER_ID_METADATA_KEY: &str = "agentdash_context_container_id";

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

impl From<agentdash_domain::session_binding::SessionOwnerType> for SessionMountTarget {
    fn from(owner: agentdash_domain::session_binding::SessionOwnerType) -> Self {
        match owner {
            agentdash_domain::session_binding::SessionOwnerType::Project => Self::Project,
            agentdash_domain::session_binding::SessionOwnerType::Story => Self::Story,
            agentdash_domain::session_binding::SessionOwnerType::Task => Self::Task,
        }
    }
}

/// 从 Project / Story / Workspace 策略构建最终 VFS
pub fn build_derived_vfs(
    project: &Project,
    story: Option<&Story>,
    workspace: Option<&Workspace>,
    agent_type: Option<&str>,
    target: SessionMountTarget,
) -> Result<Vfs, String> {
    let mut mounts = Vec::new();

    if let Some(workspace) = workspace {
        mounts.push(workspace_mount(workspace)?);
    }

    for (container, owner_scope) in effective_context_containers_with_origin(project, story) {
        if !container_visible_for_target(&container, target, agent_type) {
            continue;
        }
        let mut mount = build_context_container_mount(&container)?;
        let (owner_kind_str, owner_id) = match owner_scope {
            ContextContainerOwnerScope::Project => ("project", project.id),
            ContextContainerOwnerScope::Story => ("story", story.expect("story scope 但 story 为 None").id),
        };
        annotate_context_mount_owner(&mut mount, owner_kind_str, owner_id);
        mounts.push(mount);
    }

    let default_mount_id = if mounts.iter().any(|mount| mount.id == "main") {
        Some("main".to_string())
    } else {
        mounts.first().map(|mount| mount.id.clone())
    };

    Ok(Vfs {
        mounts,
        default_mount_id,
        source_project_id: Some(project.id.to_string()),
        source_story_id: story.map(|s| s.id.to_string()),
        links: Vec::new(),
    })
}

/// 为 Agent 知识容器构建单个 mount（knowledge_enabled=true 时调用）
///
/// 知识容器定义由系统自动派生：mount_id = "agent-knowledge"，
/// container_id = "knowledge"，按 ProjectAgentLink 隔离。
fn build_agent_knowledge_mount(link: &ProjectAgentLink) -> Result<Mount, String> {
    let container = ContextContainerDefinition {
        id: "knowledge".to_string(),
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
        exposure: ContextContainerExposure::default(),
    };
    let mut mount = build_context_container_mount(&container)?;
    annotate_context_mount_owner(
        &mut mount,
        InlineFileOwnerKind::ProjectAgentLink.as_str(),
        link.id,
    );
    Ok(mount)
}

/// 将 Agent 知识 mount 追加到已有 VFS（仅当 knowledge_enabled=true）
pub fn append_agent_knowledge_mounts(
    vfs: &mut Vfs,
    link: &ProjectAgentLink,
) -> Result<(), String> {
    if !link.knowledge_enabled {
        return Ok(());
    }
    let mount = build_agent_knowledge_mount(link)?;
    if !vfs.mounts.iter().any(|m| m.id == mount.id) {
        vfs.mounts.push(mount);
    }
    Ok(())
}

/// 构建仅包含 Agent 私有知识库的 VFS。
///
/// 该 surface 只用于 Agent 页知识库浏览，不混入 project/workspace/lifecycle/canvas mounts。
pub fn build_project_agent_knowledge_vfs(
    link: &ProjectAgentLink,
) -> Result<Vfs, String> {
    let mut vfs = Vfs {
        mounts: Vec::new(),
        default_mount_id: None,
        source_project_id: Some(link.project_id.to_string()),
        source_story_id: None,
        links: Vec::new(),
    };
    append_agent_knowledge_mounts(&mut vfs, link)?;
    vfs.default_mount_id = vfs.mounts.first().map(|mount| mount.id.clone());
    Ok(vfs)
}

/// 按白名单过滤 VFS 中的项目级容器
///
/// 仅保留 `link.project_container_ids` 中列出的项目容器 mount。
/// 非项目级容器（workspace / canvas / lifecycle 等）不受影响。
pub fn filter_project_containers_by_whitelist(
    vfs: &mut Vfs,
    link: &ProjectAgentLink,
) {
    if link.project_container_ids.is_empty() {
        // 空白名单 = 移除所有项目级容器
        vfs.mounts.retain(|mount| {
            mount
                .metadata
                .get("agentdash_context_owner_kind")
                .map_or(true, |kind| kind != "project")
        });
    } else {
        let allowed: BTreeSet<&str> = link
            .project_container_ids
            .iter()
            .map(|s| s.as_str())
            .collect();
        vfs.mounts.retain(|mount| {
            let is_project_container = mount
                .metadata
                .get("agentdash_context_owner_kind")
                .map_or(false, |kind| kind == "project");
            if !is_project_container {
                return true;
            }
            // 检查 container_id metadata 是否在白名单中
            mount
                .metadata
                .get(CONTEXT_CONTAINER_ID_METADATA_KEY)
                .and_then(|v| v.as_str())
                .map_or(false, |cid| allowed.contains(cid))
        });
    }
}

/// 为 Workspace 创建简易单 mount VFS
pub fn build_workspace_vfs(workspace: &Workspace) -> Result<Vfs, String> {
    Ok(Vfs {
        mounts: vec![workspace_mount(workspace)?],
        default_mount_id: Some("main".to_string()),
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    })
}

pub fn workspace_mount(workspace: &Workspace) -> Result<Mount, String> {
    let binding = selected_workspace_binding(workspace)
        .ok_or_else(|| "Workspace 当前没有可用 binding".to_string())?;
    let backend_id = binding.backend_id.trim();
    if backend_id.is_empty() {
        return Err("Workspace binding.backend_id 不能为空".to_string());
    }
    if binding.root_ref.trim().is_empty() {
        return Err("Workspace binding.root_ref 不能为空".to_string());
    }

    let capabilities = workspace.mount_capabilities.to_vec();

    Ok(Mount {
        id: "main".to_string(),
        provider: PROVIDER_RELAY_FS.to_string(),
        backend_id: backend_id.to_string(),
        root_ref: binding.root_ref.clone(),
        capabilities,
        default_write: true,
        display_name: if workspace.name.trim().is_empty() {
            "主工作空间".to_string()
        } else {
            workspace.name.clone()
        },
        metadata: serde_json::Value::Null,
    })
}

pub fn selected_workspace_binding(workspace: &Workspace) -> Option<&WorkspaceBinding> {
    if let Some(default_binding_id) = workspace.default_binding_id {
        return workspace
            .bindings
            .iter()
            .find(|binding| binding.id == default_binding_id);
    }
    if workspace.bindings.len() == 1 {
        return workspace.bindings.first();
    }
    None
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
            owned.retain(|(container, _)| !disabled.contains(container.id.trim()));
        }

        for container in &story.context.context_containers {
            owned.retain(|(item, _)| {
                item.id.trim() != container.id.trim()
                    && item.mount_id.trim() != container.mount_id.trim()
            });
            owned.push((container.clone(), ContextContainerOwnerScope::Story));
        }
    }
    owned
}

pub fn container_visible_for_target(
    container: &ContextContainerDefinition,
    target: SessionMountTarget,
    agent_type: Option<&str>,
) -> bool {
    let exposure = &container.exposure;
    let target_enabled = match target {
        SessionMountTarget::Project => exposure.include_in_project_sessions,
        SessionMountTarget::Story => exposure.include_in_story_sessions,
        SessionMountTarget::Task => exposure.include_in_task_sessions,
    };
    if !target_enabled {
        return false;
    }

    if exposure.allowed_agent_types.is_empty() {
        return true;
    }

    let Some(agent_type) = agent_type.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    exposure
        .allowed_agent_types
        .iter()
        .any(|item| item.trim().eq_ignore_ascii_case(agent_type))
}

pub fn build_context_container_mount(
    container: &ContextContainerDefinition,
) -> Result<Mount, String> {
    let id = non_empty_trimmed(&container.mount_id, "mount_id")?.to_string();
    let display_name = if container.display_name.trim().is_empty() {
        container.id.trim().to_string()
    } else {
        container.display_name.trim().to_string()
    };
    let capabilities = if container.capabilities.is_empty() {
        vec![
            MountCapability::Read,
            MountCapability::List,
            MountCapability::Search,
        ]
    } else {
        container.capabilities.to_vec()
    };

    let container_id_trimmed = container.id.trim();
    let (provider, root_ref, metadata) = match &container.provider {
        ContextContainerProvider::InlineFiles { .. } => (
            PROVIDER_INLINE_FS.to_string(),
            format!("context://inline/{container_id_trimmed}"),
            serde_json::json!({
                "container_id": container_id_trimmed,
                CONTEXT_CONTAINER_ID_METADATA_KEY: container_id_trimmed,
            }),
        ),
        ContextContainerProvider::ExternalService {
            service_id,
            root_ref,
        } => (
            service_id.trim().to_string(),
            root_ref.trim().to_string(),
            serde_json::json!({
                "service_id": service_id.trim(),
                "root_ref": root_ref.trim(),
                CONTEXT_CONTAINER_ID_METADATA_KEY: container_id_trimmed,
            }),
        ),
    };

    Ok(Mount {
        id,
        provider,
        backend_id: String::new(),
        root_ref,
        capabilities,
        default_write: container.default_write,
        display_name,
        metadata,
    })
}



/// 为 context container mount 写入 owner_kind + owner_id metadata（新 API）
pub(crate) fn annotate_context_mount_owner(mount: &mut Mount, owner_kind: &str, owner_id: Uuid) {
    let mut metadata = match std::mem::take(&mut mount.metadata) {
        serde_json::Value::Object(object) => object,
        serde_json::Value::Null => serde_json::Map::new(),
        other => {
            let mut object = serde_json::Map::new();
            object.insert("raw_metadata".to_string(), other);
            object
        }
    };
    metadata.insert(
        CONTEXT_OWNER_KIND_METADATA_KEY.to_string(),
        serde_json::Value::String(owner_kind.to_string()),
    );
    metadata.insert(
        CONTEXT_OWNER_ID_METADATA_KEY.to_string(),
        serde_json::Value::String(owner_id.to_string()),
    );
    mount.metadata = serde_json::Value::Object(metadata);
}

/// 从 mount metadata 提取 owner 坐标（owner_kind, owner_id, container_id）
pub fn parse_inline_mount_owner(mount: &Mount) -> Result<(InlineFileOwnerKind, Uuid, String), String> {
    let owner_kind_str = mount
        .metadata
        .get(CONTEXT_OWNER_KIND_METADATA_KEY)
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!(
                "mount {} 缺少 {}",
                mount.id, CONTEXT_OWNER_KIND_METADATA_KEY
            )
        })?;
    let owner_kind = InlineFileOwnerKind::from_str(owner_kind_str).ok_or_else(|| {
        format!(
            "mount {} 的 owner_kind 无效: {}",
            mount.id, owner_kind_str
        )
    })?;
    let owner_id_str = mount
        .metadata
        .get(CONTEXT_OWNER_ID_METADATA_KEY)
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!(
                "mount {} 缺少 {}",
                mount.id, CONTEXT_OWNER_ID_METADATA_KEY
            )
        })?;
    let owner_id = Uuid::parse_str(owner_id_str)
        .map_err(|e| format!("mount {} 的 owner_id 无效: {}", mount.id, e))?;
    let container_id = mount
        .metadata
        .get("container_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("mount {} 缺少 container_id", mount.id))?
        .to_string();
    Ok((owner_kind, owner_id, container_id))
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
    if mount.provider == PROVIDER_CANVAS_FS {
        return ResolvedMountOwnerKind::Canvas;
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
        Some(value) if value == InlineFileOwnerKind::ProjectAgentLink.as_str() => {
            ResolvedMountOwnerKind::ProjectAgentLink
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
        PROVIDER_CANVAS_FS => ResolvedMountPurpose::Canvas,
        PROVIDER_INLINE_FS => match mount_owner_kind(mount) {
            ResolvedMountOwnerKind::Project => ResolvedMountPurpose::ProjectContainer,
            ResolvedMountOwnerKind::Story => ResolvedMountPurpose::StoryContainer,
            ResolvedMountOwnerKind::ProjectAgentLink => ResolvedMountPurpose::AgentKnowledge,
            _ => ResolvedMountPurpose::ExternalService,
        },
        _ => ResolvedMountPurpose::ExternalService,
    }
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
        ContextContainerDefinition, ContextContainerExposure, ContextContainerFile,
        ContextContainerProvider,
    };

    fn inline_container(id: &str, mount_id: &str, path: &str) -> ContextContainerDefinition {
        ContextContainerDefinition {
            id: id.to_string(),
            mount_id: mount_id.to_string(),
            display_name: id.to_string(),
            provider: ContextContainerProvider::InlineFiles {
                files: vec![ContextContainerFile {
                    path: path.to_string(),
                    content: "content".to_string(),
                }],
            },
            capabilities: vec![],
            default_write: false,
            exposure: ContextContainerExposure::default(),
        }
    }

    #[test]
    fn story_override_container_is_marked_as_story_owned() {
        let mut project = Project::new("proj".to_string(), "desc".to_string());
        project.config.context_containers = vec![inline_container("brief", "brief", "project.md")];

        let mut story = Story::new(project.id, "story".to_string(), "desc".to_string());
        story.context.context_containers = vec![inline_container("brief", "brief", "story.md")];

        let vfs = build_derived_vfs(
            &project,
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
        // 验证新的 owner_kind / owner_id metadata
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
        // 验证 container_id metadata
        assert_eq!(
            mount.metadata.get("container_id").and_then(|v| v.as_str()),
            Some("brief")
        );
    }

    #[test]
    fn inherited_project_container_is_marked_as_project_owned() {
        let mut project = Project::new("proj".to_string(), "desc".to_string());
        project.config.context_containers = vec![inline_container("spec", "spec", "project.md")];

        let story = Story::new(project.id, "story".to_string(), "desc".to_string());

        let vfs = build_derived_vfs(
            &project,
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
        // 验证新的 owner_kind / owner_id metadata
        assert_eq!(
            mount.metadata.get(CONTEXT_OWNER_KIND_METADATA_KEY),
            Some(&serde_json::Value::String("project".to_string()))
        );
        assert_eq!(
            mount
                .metadata
                .get(CONTEXT_OWNER_ID_METADATA_KEY)
                .and_then(|v| v.as_str()),
            Some(project.id.to_string()).as_deref()
        );
    }
}

pub fn normalize_inline_files(
    files: &[agentdash_domain::context_container::ContextContainerFile],
) -> Result<BTreeMap<String, String>, String> {
    let mut normalized = BTreeMap::new();
    for file in files {
        let path = normalize_mount_relative_path(&file.path, false)?;
        normalized.insert(path, file.content.clone());
    }
    Ok(normalized)
}

pub fn build_lifecycle_mount(run_id: Uuid, lifecycle_key: &str) -> Mount {
    build_lifecycle_mount_with_ports(run_id, lifecycle_key, &[])
}

/// 构建带 output port 写入权限的 lifecycle mount。
/// `writable_port_keys` 非空时启用 Write capability，agent 可写入 `artifacts/{port_key}`。
pub fn build_lifecycle_mount_with_ports(
    run_id: Uuid,
    lifecycle_key: &str,
    writable_port_keys: &[String],
) -> Mount {
    let mut capabilities = vec![
        MountCapability::Read,
        MountCapability::List,
        MountCapability::Search,
    ];
    if !writable_port_keys.is_empty() {
        capabilities.push(MountCapability::Write);
    }

    Mount {
        id: "lifecycle".to_string(),
        provider: PROVIDER_LIFECYCLE_VFS.to_string(),
        backend_id: String::new(),
        root_ref: format!("lifecycle://run/{run_id}"),
        capabilities,
        default_write: false,
        display_name: "Lifecycle 执行记录".to_string(),
        metadata: serde_json::json!({
            "run_id": run_id.to_string(),
            "lifecycle_key": lifecycle_key,
            "writable_port_keys": writable_port_keys,
            "directory_hint": {
                "description": "Lifecycle 执行记录，包含当前 run 的步骤状态、port 产出和 session 记录",
                "index": [
                    { "path": "active", "description": "当前活跃 run 的概览（JSON）" },
                    { "path": "active/steps", "description": "各步骤执行状态，子路径为 step_key" },
                    { "path": "active/steps/{step_key}", "description": "单步骤详情（JSON）" },
                    { "path": "artifacts", "description": "Port output 产出，子路径为 port_key" },
                    { "path": "artifacts/{port_key}", "description": "指定 port 的产出内容（纯文本）" },
                    { "path": "nodes/{step_key}/state", "description": "Node 步骤状态（JSON）" },
                    { "path": "nodes/{step_key}/session/meta", "description": "Node 关联 session 元信息" },
                    { "path": "nodes/{step_key}/session/summary", "description": "Node session 摘要" },
                    { "path": "nodes/{step_key}/session/turns", "description": "Node session turn 列表" },
                    { "path": "nodes/{step_key}/session/turns/{turn_id}", "description": "单 turn 完整消息流" },
                    { "path": "active/log", "description": "执行日志（JSON 数组）" },
                    { "path": "runs", "description": "历史 run 列表" }
                ]
            }
        }),
    }
}

pub fn build_canvas_mount_id(canvas: &Canvas) -> String {
    format!("cvs-{}", canvas.mount_id)
}

pub fn build_canvas_mount(canvas: &Canvas) -> Mount {
    Mount {
        id: build_canvas_mount_id(canvas),
        provider: PROVIDER_CANVAS_FS.to_string(),
        backend_id: String::new(),
        root_ref: format!("canvas://{}", canvas.id),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: false,
        display_name: if canvas.title.trim().is_empty() {
            format!("Canvas {}", canvas.id)
        } else {
            canvas.title.clone()
        },
        metadata: serde_json::json!({
            "canvas_id": canvas.id.to_string(),
            "mount_id": canvas.mount_id,
            "project_id": canvas.project_id.to_string(),
            "entry_file": canvas.entry_file,
        }),
    }
}

pub fn append_canvas_mounts(vfs: &mut Vfs, canvases: &[Canvas]) {
    for canvas in canvases {
        let mount = build_canvas_mount(canvas);
        vfs
            .mounts
            .retain(|existing| existing.id != mount.id);
        vfs.mounts.push(mount);
    }
}

pub fn list_inline_entries(
    files: &BTreeMap<String, String>,
    base_path: &str,
    pattern: Option<&str>,
    recursive: bool,
) -> Vec<RuntimeFileEntry> {
    let normalized_base = base_path.trim_matches('/');
    let mut dirs = BTreeSet::new();
    let mut file_entries = BTreeMap::new();

    for (path, content) in files {
        let matches_base = if normalized_base.is_empty() {
            true
        } else {
            path == normalized_base
                || path
                    .strip_prefix(normalized_base)
                    .is_some_and(|rest| rest.starts_with('/'))
        };
        if !matches_base {
            continue;
        }

        let relative = if normalized_base.is_empty() {
            path.as_str()
        } else if path == normalized_base {
            ""
        } else {
            path.strip_prefix(normalized_base)
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or("")
        };

        if relative.is_empty() {
            file_entries.insert(path.clone(), content.len() as u64);
            continue;
        }

        let parts = relative.split('/').collect::<Vec<_>>();
        if recursive {
            let full_parts = path.split('/').collect::<Vec<_>>();
            for depth in 1..full_parts.len() {
                dirs.insert(full_parts[..depth].join("/"));
            }
            file_entries.insert(path.clone(), content.len() as u64);
        } else if parts.len() == 1 {
            file_entries.insert(path.clone(), content.len() as u64);
        } else {
            let dir_path = if normalized_base.is_empty() {
                parts[0].to_string()
            } else {
                format!("{}/{}", normalized_base, parts[0])
            };
            dirs.insert(dir_path);
        }
    }

    let normalized_pattern = pattern.map(str::trim).filter(|value| !value.is_empty());
    let mut entries = Vec::new();
    for dir in dirs {
        if path_matches_pattern(&dir, normalized_pattern) {
            entries.push(RuntimeFileEntry {
                path: dir,
                size: None,
                modified_at: None,
                is_dir: true,
                is_virtual: false,
                attributes: None,
            });
        }
    }
    for (path, size) in file_entries {
        if path_matches_pattern(&path, normalized_pattern) {
            entries.push(RuntimeFileEntry {
                path,
                size: Some(size),
                modified_at: None,
                is_dir: false,
                is_virtual: false,
                attributes: None,
            });
        }
    }
    entries
}

fn path_matches_pattern(path: &str, pattern: Option<&str>) -> bool {
    match pattern {
        None => true,
        Some(pat)
            if pat.contains('*') || pat.contains('?') || pat.contains('[') || pat.contains('{') =>
        {
            globset::Glob::new(pat)
                .ok()
                .map(|g| g.compile_matcher().is_match(path))
                .unwrap_or(false)
        }
        Some(pat) => path.contains(pat),
    }
}
