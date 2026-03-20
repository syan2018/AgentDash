use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::context_container::{
    ContextContainerCapability, ContextContainerDefinition, ContextContainerProvider,
    MountDerivationPolicy,
};
use agentdash_domain::{project::Project, story::Story, workspace::Workspace};
use agentdash_executor::{ExecutionAddressSpace, ExecutionMount, ExecutionMountCapability};
use agentdash_relay::FileEntryRelay;

use super::path::normalize_mount_relative_path;

pub const PROVIDER_RELAY_FS: &str = "relay_fs";
pub const PROVIDER_INLINE_FS: &str = "inline_fs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMountTarget {
    Project,
    Story,
    Task,
}

/// 从 Project / Story / Workspace 策略构建最终 Address Space
pub fn build_derived_address_space(
    project: &Project,
    story: Option<&Story>,
    workspace: Option<&Workspace>,
    agent_type: Option<&str>,
    target: SessionMountTarget,
) -> Result<ExecutionAddressSpace, String> {
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

    Ok(ExecutionAddressSpace {
        mounts,
        default_mount_id,
    })
}

/// 为 Workspace 创建简易单 mount Address Space
pub fn build_workspace_address_space(
    workspace: &Workspace,
) -> Result<ExecutionAddressSpace, String> {
    Ok(ExecutionAddressSpace {
        mounts: vec![workspace_mount_from_policy(
            workspace,
            &MountDerivationPolicy::default(),
        )?],
        default_mount_id: Some("main".to_string()),
    })
}

pub fn workspace_mount_from_policy(
    workspace: &Workspace,
    policy: &MountDerivationPolicy,
) -> Result<ExecutionMount, String> {
    let backend_id = workspace.backend_id.trim();
    if backend_id.is_empty() {
        return Err("Workspace.backend_id 不能为空".to_string());
    }
    if workspace.container_ref.trim().is_empty() {
        return Err("Workspace.container_ref 不能为空".to_string());
    }

    let capabilities = if policy.local_workspace_capabilities.is_empty() {
        vec![
            ExecutionMountCapability::Read,
            ExecutionMountCapability::Write,
            ExecutionMountCapability::List,
            ExecutionMountCapability::Search,
            ExecutionMountCapability::Exec,
        ]
    } else {
        map_container_capabilities(&policy.local_workspace_capabilities)
    };

    Ok(ExecutionMount {
        id: "main".to_string(),
        provider: PROVIDER_RELAY_FS.to_string(),
        backend_id: backend_id.to_string(),
        root_ref: workspace.container_ref.clone(),
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
) -> Result<ExecutionMount, String> {
    let id = non_empty_trimmed(&container.mount_id, "mount_id")?.to_string();
    let display_name = if container.display_name.trim().is_empty() {
        container.id.trim().to_string()
    } else {
        container.display_name.trim().to_string()
    };
    let capabilities = if container.capabilities.is_empty() {
        vec![
            ExecutionMountCapability::Read,
            ExecutionMountCapability::List,
            ExecutionMountCapability::Search,
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
            "external_service".to_string(),
            root_ref.trim().to_string(),
            serde_json::json!({
                "service_id": service_id.trim(),
                "root_ref": root_ref.trim(),
            }),
        ),
    };

    Ok(ExecutionMount {
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
) -> Vec<ExecutionMountCapability> {
    let mut mapped = Vec::new();
    for capability in capabilities {
        let next = match capability {
            ContextContainerCapability::Read => ExecutionMountCapability::Read,
            ContextContainerCapability::Write => ExecutionMountCapability::Write,
            ContextContainerCapability::List => ExecutionMountCapability::List,
            ContextContainerCapability::Search => ExecutionMountCapability::Search,
            ContextContainerCapability::Exec => ExecutionMountCapability::Exec,
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

pub fn inline_files_from_mount(mount: &ExecutionMount) -> Result<BTreeMap<String, String>, String> {
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
) -> Vec<FileEntryRelay> {
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
            entries.push(FileEntryRelay {
                path: dir,
                size: None,
                modified_at: None,
                is_dir: true,
            });
        }
    }
    for (path, size) in file_entries {
        if path_matches_pattern(&path, normalized_pattern) {
            entries.push(FileEntryRelay {
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
