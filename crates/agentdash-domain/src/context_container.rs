use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::MountCapability;

fn bool_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ContextContainerFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextContainerProvider {
    InlineFiles {
        files: Vec<ContextContainerFile>,
    },
    ExternalService {
        service_id: String,
        root_ref: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ContextContainerExposure {
    #[serde(default = "bool_true")]
    pub include_in_project_sessions: bool,
    #[serde(default = "bool_true")]
    pub include_in_task_sessions: bool,
    #[serde(default = "bool_true")]
    pub include_in_story_sessions: bool,
    #[serde(default)]
    pub allowed_agent_types: Vec<String>,
}

impl Default for ContextContainerExposure {
    fn default() -> Self {
        Self {
            include_in_project_sessions: true,
            include_in_task_sessions: true,
            include_in_story_sessions: true,
            allowed_agent_types: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ContextContainerDefinition {
    pub id: String,
    pub mount_id: String,
    pub display_name: String,
    pub provider: ContextContainerProvider,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default)]
    pub default_write: bool,
    #[serde(default)]
    pub exposure: ContextContainerExposure,
}

pub fn validate_context_containers(
    containers: &[ContextContainerDefinition],
) -> Result<(), String> {
    let mut seen_ids = BTreeSet::new();
    let mut seen_mount_ids = BTreeSet::new();

    for (index, container) in containers.iter().enumerate() {
        validate_context_container(container)
            .map_err(|error| format!("context_containers[{index}]: {error}"))?;

        let id = container.id.trim();
        if !seen_ids.insert(id.to_string()) {
            return Err(format!(
                "context_containers[{index}]: container id 重复: {id}"
            ));
        }

        let mount_id = container.mount_id.trim();
        if !seen_mount_ids.insert(mount_id.to_string()) {
            return Err(format!(
                "context_containers[{index}]: mount_id 重复: {mount_id}"
            ));
        }
    }

    Ok(())
}

pub fn validate_context_container(container: &ContextContainerDefinition) -> Result<(), String> {
    let _ = validate_identifier(&container.id, "id", false)?;
    let mount_id = validate_identifier(&container.mount_id, "mount_id", true)?;

    if mount_id.eq_ignore_ascii_case("main") {
        return Err("mount_id `main` 为保留字，不能用于自定义上下文容器".to_string());
    }

    if container.default_write
        && !container
            .capabilities
            .iter()
            .any(|item| matches!(item, MountCapability::Write))
    {
        return Err("default_write=true 时必须显式声明 write capability".to_string());
    }

    match &container.provider {
        ContextContainerProvider::InlineFiles { files } => {
            if files.is_empty() {
                return Err("inline_files 至少需要提供一个文件".to_string());
            }
            if container
                .capabilities
                .iter()
                .any(|item| matches!(item, MountCapability::Exec))
            {
                return Err("inline_files 不支持 exec capability".to_string());
            }

            let mut seen_paths = BTreeSet::new();
            for (index, file) in files.iter().enumerate() {
                let normalized = validate_relative_file_path(&file.path)
                    .map_err(|error| format!("inline_files.files[{index}].path 非法: {error}"))?;
                if !seen_paths.insert(normalized.clone()) {
                    return Err(format!(
                        "inline_files.files[{index}].path 重复: {normalized}"
                    ));
                }
            }
        }
        ContextContainerProvider::ExternalService {
            service_id,
            root_ref,
        } => {
            if service_id.trim().is_empty() {
                return Err("external_service.service_id 不能为空".to_string());
            }
            if root_ref.trim().is_empty() {
                return Err("external_service.root_ref 不能为空".to_string());
            }
            if container
                .capabilities
                .iter()
                .any(|item| matches!(item, MountCapability::Exec))
            {
                return Err("external_service 不支持 exec capability".to_string());
            }
        }
    }

    for (index, agent_type) in container.exposure.allowed_agent_types.iter().enumerate() {
        if agent_type.trim().is_empty() {
            return Err(format!(
                "exposure.allowed_agent_types[{index}] 不能为空字符串"
            ));
        }
    }

    Ok(())
}

pub fn validate_disabled_container_ids(
    disabled_container_ids: &[String],
    inherited_containers: &[ContextContainerDefinition],
) -> Result<(), String> {
    let available = inherited_containers
        .iter()
        .map(|item| item.id.trim().to_string())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();

    for (index, raw) in disabled_container_ids.iter().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(format!("disabled_container_ids[{index}] 不能为空"));
        }
        if !seen.insert(trimmed.to_string()) {
            return Err(format!("disabled_container_ids[{index}] 重复: {trimmed}"));
        }
        if !available.contains(trimmed) {
            return Err(format!(
                "disabled_container_ids[{index}] 引用了不存在的项目级容器: {trimmed}"
            ));
        }
    }

    Ok(())
}

fn validate_identifier<'a>(
    raw: &'a str,
    field_name: &str,
    reject_reserved_mount_chars: bool,
) -> Result<&'a str, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{field_name} 不能为空"));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(format!("{field_name} 不能包含空白字符"));
    }
    if reject_reserved_mount_chars && trimmed.chars().any(|ch| matches!(ch, '/' | '\\' | ':')) {
        return Err(format!("{field_name} 不能包含 `/`、`\\` 或 `:`"));
    }
    Ok(trimmed)
}

fn validate_relative_file_path(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("路径不能为空".to_string());
    }

    let normalized = trimmed.replace('\\', "/");
    if normalized.starts_with('/') || normalized.starts_with("//") {
        return Err("不能使用绝对路径".to_string());
    }
    if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
        return Err("不能使用绝对路径".to_string());
    }

    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return Err("不能包含 `..`".to_string()),
            other => segments.push(other),
        }
    }

    if segments.is_empty() {
        return Err("路径不能为空".to_string());
    }

    Ok(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline_container(path: &str) -> ContextContainerDefinition {
        ContextContainerDefinition {
            id: "spec-docs".to_string(),
            mount_id: "spec".to_string(),
            display_name: "Spec".to_string(),
            provider: ContextContainerProvider::InlineFiles {
                files: vec![ContextContainerFile {
                    path: path.to_string(),
                    content: "# spec".to_string(),
                }],
            },
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: false,
            exposure: ContextContainerExposure::default(),
        }
    }

    #[test]
    fn validate_context_containers_rejects_reserved_main_mount() {
        let mut container = inline_container("docs/spec.md");
        container.mount_id = "main".to_string();

        let error = validate_context_containers(&[container]).expect_err("should fail");
        assert!(error.contains("保留字"));
    }

    #[test]
    fn validate_context_containers_rejects_escape_path() {
        let container = inline_container("../secret.md");

        let error = validate_context_containers(&[container]).expect_err("should fail");
        assert!(error.contains("不能包含 `..`"));
    }

    #[test]
    fn validate_context_containers_allows_write_on_external_service() {
        let container = ContextContainerDefinition {
            id: "km".to_string(),
            mount_id: "km".to_string(),
            display_name: "KM".to_string(),
            provider: ContextContainerProvider::ExternalService {
                service_id: "km_bridge".to_string(),
                root_ref: "18724/doc123".to_string(),
            },
            capabilities: vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
            ],
            default_write: false,
            exposure: ContextContainerExposure::default(),
        };

        validate_context_containers(&[container])
            .expect("write on external_service should be allowed");
    }

    #[test]
    fn validate_disabled_container_ids_requires_existing_project_container() {
        let available = vec![inline_container("docs/spec.md")];
        let error = validate_disabled_container_ids(&["missing".to_string()], &available)
            .expect_err("fail");
        assert!(error.contains("不存在的项目级容器"));
    }
}
