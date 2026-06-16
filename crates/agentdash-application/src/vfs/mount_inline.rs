use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::context_container::{ContextContainerDefinition, ContextContainerProvider};
use agentdash_domain::inline_file::InlineFileOwnerKind;
use uuid::Uuid;

use crate::runtime::{Mount, MountCapability, RuntimeFileEntry};

use super::mount::{
    CONTEXT_CONTAINER_ID_METADATA_KEY, CONTEXT_OWNER_ID_METADATA_KEY,
    CONTEXT_OWNER_KIND_METADATA_KEY, PROVIDER_INLINE_FS,
};
use super::path::normalize_mount_relative_path;

pub fn build_context_container_mount(
    container: &ContextContainerDefinition,
) -> Result<Mount, String> {
    let id = non_empty_trimmed(&container.mount_id, "mount_id")?.to_string();
    let display_name = if container.display_name.trim().is_empty() {
        id.clone()
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

    let container_id_trimmed = container.mount_id.trim();
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
pub fn parse_inline_mount_owner(
    mount: &Mount,
) -> Result<(InlineFileOwnerKind, Uuid, String), String> {
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
    let owner_kind = owner_kind_str
        .parse::<InlineFileOwnerKind>()
        .map_err(|_| format!("mount {} 的 owner_kind 无效: {}", mount.id, owner_kind_str))?;
    let owner_id_str = mount
        .metadata
        .get(CONTEXT_OWNER_ID_METADATA_KEY)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("mount {} 缺少 {}", mount.id, CONTEXT_OWNER_ID_METADATA_KEY))?;
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

fn non_empty_trimmed<'a>(value: &'a str, field_name: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field_name} 不能为空"))
    } else {
        Ok(trimmed)
    }
}
