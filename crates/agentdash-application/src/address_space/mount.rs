use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::context_container::{
    ContextContainerCapability, ContextContainerDefinition, ContextContainerProvider,
    MountDerivationPolicy,
};
use agentdash_domain::{
    canvas::Canvas,
    project::Project,
    story::Story,
    workspace::{Workspace, WorkspaceBinding},
};
use uuid::Uuid;
use super::path::normalize_mount_relative_path;
use crate::runtime::{AddressSpace, Mount, MountCapability, RuntimeFileEntry};

pub const PROVIDER_RELAY_FS: &str = "relay_fs";
pub const PROVIDER_INLINE_FS: &str = "inline_fs";
pub const PROVIDER_LIFECYCLE_VFS: &str = "lifecycle_vfs";
pub const PROVIDER_CANVAS_FS: &str = "canvas_fs";

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

/// 从 Project / Story / Workspace 策略构建最终 Address Space
pub fn build_derived_address_space(
    project: &Project,
    story: Option<&Story>,
    workspace: Option<&Workspace>,
    agent_type: Option<&str>,
    target: SessionMountTarget,
) -> Result<AddressSpace, String> {
    let mut mounts = Vec::new();
    let mount_policy = story
        .and_then(|item| item.context.mount_policy_override.clone())
        .unwrap_or_else(|| project.config.mount_policy.clone());

    if mount_policy.include_local_workspace
        && let Some(workspace) = workspace
    {
        mounts.push(workspace_mount_from_policy(workspace, &mount_policy)?);
    }

    for container in effective_context_containers(project, story) {
        if !container_visible_for_target(&container, target, agent_type) {
            continue;
        }
        mounts.push(build_context_container_mount(&container)?);
    }

    let default_mount_id = if mounts.iter().any(|mount| mount.id == "main") {
        Some("main".to_string())
    } else {
        mounts.first().map(|mount| mount.id.clone())
    };

    Ok(AddressSpace {
        mounts,
        default_mount_id,
        source_project_id: Some(project.id.to_string()),
        source_story_id: story.map(|s| s.id.to_string()),
    })
}

/// 为 Workspace 创建简易单 mount Address Space
pub fn build_workspace_address_space(workspace: &Workspace) -> Result<AddressSpace, String> {
    Ok(AddressSpace {
        mounts: vec![workspace_mount_from_policy(
            workspace,
            &MountDerivationPolicy::default(),
        )?],
        default_mount_id: Some("main".to_string()),
        source_project_id: None,
        source_story_id: None,
    })
}

pub fn workspace_mount_from_policy(
    workspace: &Workspace,
    policy: &MountDerivationPolicy,
) -> Result<Mount, String> {
    let binding = selected_workspace_binding(workspace)
        .ok_or_else(|| "Workspace 当前没有可用 binding".to_string())?;
    let backend_id = binding.backend_id.trim();
    if backend_id.is_empty() {
        return Err("Workspace binding.backend_id 不能为空".to_string());
    }
    if binding.root_ref.trim().is_empty() {
        return Err("Workspace binding.root_ref 不能为空".to_string());
    }

    let capabilities = if policy.local_workspace_capabilities.is_empty() {
        vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
            MountCapability::Exec,
        ]
    } else {
        map_container_capabilities(&policy.local_workspace_capabilities)
    };

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
    if let Some(default_binding_id) = workspace.default_binding_id
        && let Some(binding) = workspace
            .bindings
            .iter()
            .find(|binding| binding.id == default_binding_id)
    {
        return Some(binding);
    }
    workspace.bindings.first()
}

pub fn effective_context_containers(
    project: &Project,
    story: Option<&Story>,
) -> Vec<ContextContainerDefinition> {
    let mut containers = project.config.context_containers.clone();
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
            containers.retain(|container| !disabled.contains(container.id.trim()));
        }

        for container in &story.context.context_containers {
            containers.retain(|item| {
                item.id.trim() != container.id.trim()
                    && item.mount_id.trim() != container.mount_id.trim()
            });
            containers.push(container.clone());
        }
    }
    containers
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
        map_container_capabilities(&container.capabilities)
    };

    let (provider, root_ref, metadata) = match &container.provider {
        ContextContainerProvider::InlineFiles { files } => (
            PROVIDER_INLINE_FS.to_string(),
            format!("context://inline/{}", container.id.trim()),
            serde_json::json!({ "files": normalize_inline_files(files)? }),
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

pub fn map_container_capabilities(
    capabilities: &[ContextContainerCapability],
) -> Vec<MountCapability> {
    let mut mapped = Vec::new();
    for capability in capabilities {
        let next = match capability {
            ContextContainerCapability::Read => MountCapability::Read,
            ContextContainerCapability::Write => MountCapability::Write,
            ContextContainerCapability::List => MountCapability::List,
            ContextContainerCapability::Search => MountCapability::Search,
            ContextContainerCapability::Exec => MountCapability::Exec,
        };
        if !mapped.contains(&next) {
            mapped.push(next);
        }
    }
    mapped
}

fn non_empty_trimmed<'a>(value: &'a str, field_name: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field_name} 不能为空"))
    } else {
        Ok(trimmed)
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
    Mount {
        id: "lifecycle".to_string(),
        provider: PROVIDER_LIFECYCLE_VFS.to_string(),
        backend_id: String::new(),
        root_ref: format!("lifecycle://run/{run_id}"),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: false,
        display_name: "Lifecycle 执行记录".to_string(),
        metadata: serde_json::json!({
            "run_id": run_id.to_string(),
            "lifecycle_key": lifecycle_key,
            "directory_hint": {
                "description": "Lifecycle 执行记录，包含当前 run 的步骤状态和产物",
                "index": [
                    { "path": "active", "description": "当前活跃 run 的概览（JSON）" },
                    { "path": "active/steps", "description": "各步骤执行状态，子路径为 step_key" },
                    { "path": "active/steps/{step_key}", "description": "单步骤详情（JSON）" },
                    { "path": "active/artifacts", "description": "产物列表，子路径为 artifact UUID" },
                    { "path": "active/artifacts/{id}", "description": "产物内容（纯文本）" },
                    { "path": "active/log", "description": "执行日志（JSON 数组）" },
                    { "path": "runs", "description": "历史 run 列表" }
                ]
            }
        }),
    }
}

pub fn build_canvas_mount_id(canvas: &Canvas) -> String {
    canvas.mount_id.clone()
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

pub fn append_canvas_mounts(address_space: &mut AddressSpace, canvases: &[Canvas]) {
    for canvas in canvases {
        let mount = build_canvas_mount(canvas);
        address_space.mounts.retain(|existing| existing.id != mount.id);
        address_space.mounts.push(mount);
    }
}

pub fn inline_files_from_mount(mount: &Mount) -> Result<BTreeMap<String, String>, String> {
    let raw_files = mount
        .metadata
        .get("files")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::from_value::<BTreeMap<String, String>>(raw_files)
        .map_err(|error| format!("mount `{}` 的 inline metadata 无效: {error}", mount.id))
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
            });
        }
    }
    entries
}

fn path_matches_pattern(path: &str, pattern: Option<&str>) -> bool {
    pattern.is_none_or(|needle| path.contains(needle))
}
