use std::collections::BTreeSet;

use base64::Engine;
use serde_json::json;
use thiserror::Error;
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::inline_file::InlineFileOwnerKind;
use agentdash_domain::mcp_preset::{McpEnvVar, McpHttpHeader, McpTransportConfig};
use agentdash_domain::shared_library::{
    AgentTemplateConfig, InlineMountFilePayload, LibraryAsset, LibraryAssetScope,
    LibraryAssetSource, LibraryAssetType, McpServerTemplatePayload, SkillTemplateFilePayload,
    SkillTemplatePayload, VfsMountTemplatePayload,
};
use agentdash_domain::workflow::{
    ActivityExecutorSpec, ActivityLifecycleDefinition, WorkflowDefinition,
};

use crate::repository_set::RepositorySet;
use crate::shared_library::seed_digest;
use crate::workflow::{
    BuiltinLifecycleTemplate, BuiltinWorkflowTemplate, BuiltinWorkflowTemplateBundle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectAssetPublishKind {
    ProjectAgent,
    McpPreset,
    WorkflowBundle,
    SkillAsset,
    VfsMount,
}

impl ProjectAssetPublishKind {
    pub fn asset_type(self) -> LibraryAssetType {
        match self {
            Self::ProjectAgent => LibraryAssetType::AgentTemplate,
            Self::McpPreset => LibraryAssetType::McpServerTemplate,
            Self::WorkflowBundle => LibraryAssetType::WorkflowTemplate,
            Self::SkillAsset => LibraryAssetType::SkillTemplate,
            Self::VfsMount => LibraryAssetType::VfsMountTemplate,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PublishLibraryAssetInput {
    pub project_id: Uuid,
    pub project_asset_id: Uuid,
    pub asset_kind: ProjectAssetPublishKind,
    pub owner_id: String,
    pub scope: LibraryAssetScope,
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub version: String,
    pub overwrite: bool,
}

#[derive(Debug, Error)]
pub enum PublishLibraryAssetError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Domain(#[from] DomainError),
}

pub async fn publish_project_asset_to_library(
    repos: &RepositorySet,
    input: PublishLibraryAssetInput,
) -> Result<LibraryAsset, PublishLibraryAssetError> {
    validate_publish_input(&input)?;
    let asset_type = input.asset_kind.asset_type();
    let payload = match input.asset_kind {
        ProjectAssetPublishKind::ProjectAgent => publish_agent_payload(repos, &input).await?,
        ProjectAssetPublishKind::McpPreset => publish_mcp_payload(repos, &input).await?,
        ProjectAssetPublishKind::WorkflowBundle => publish_workflow_payload(repos, &input).await?,
        ProjectAssetPublishKind::SkillAsset => publish_skill_payload(repos, &input).await?,
        ProjectAssetPublishKind::VfsMount => publish_vfs_mount_payload(repos, &input).await?,
    };
    let payload_digest = seed_digest(&payload)?;
    let source_ref = Some(format!(
        "user:{}:{}:{}",
        input.owner_id,
        asset_type.as_str(),
        input.key
    ));
    let owner_id = Some(input.owner_id.clone());
    let asset = LibraryAsset::new(
        asset_type,
        input.scope,
        owner_id.clone(),
        input.key.clone(),
        input.display_name.clone(),
        input.description.clone(),
        input.version.clone(),
        LibraryAssetSource::UserAuthored,
        source_ref,
        payload_digest,
        payload,
    )?;

    let existing = repos
        .shared_library_repo
        .find_by_identity(
            asset.asset_type,
            asset.scope,
            owner_id.as_deref(),
            &asset.key,
        )
        .await?;

    match existing {
        Some(_) if !input.overwrite => Err(PublishLibraryAssetError::Conflict(format!(
            "LibraryAsset 已存在: {}",
            input.key
        ))),
        Some(existing) => {
            let mut updated = asset;
            updated.id = existing.id;
            updated.created_at = existing.created_at;
            updated.updated_at = chrono::Utc::now();
            repos.shared_library_repo.update(&updated).await?;
            Ok(updated)
        }
        None => {
            repos.shared_library_repo.create(&asset).await?;
            Ok(asset)
        }
    }
}

async fn publish_vfs_mount_payload(
    repos: &RepositorySet,
    input: &PublishLibraryAssetInput,
) -> Result<serde_json::Value, PublishLibraryAssetError> {
    let mount = repos
        .project_vfs_mount_repo
        .get_by_id(input.project_asset_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "project_vfs_mount",
            id: input.project_asset_id.to_string(),
        })?;
    if mount.project_id != input.project_id {
        return Err(PublishLibraryAssetError::BadRequest(
            "Project VFS Mount 不属于当前 Project".to_string(),
        ));
    }
    let payload = match &mount.content {
        agentdash_domain::project_vfs_mount::ProjectVfsMountContent::Inline => {
            let files = repos
                .inline_file_repo
                .list_files_by_owner(InlineFileOwnerKind::ProjectVfsMount, mount.id)
                .await?
                .into_iter()
                .map(|file| {
                    let (content_kind, content, mime_type, data_base64) = match file.content {
                        agentdash_domain::common::StoredFileContent::Text { content } => {
                            ("text".to_string(), Some(content), None, None)
                        }
                        agentdash_domain::common::StoredFileContent::Binary {
                            bytes,
                            mime_type,
                        } => (
                            "binary".to_string(),
                            None,
                            Some(mime_type),
                            Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                        ),
                    };
                    InlineMountFilePayload {
                        path: file.path,
                        content_kind,
                        content,
                        mime_type,
                        size_bytes: file.size_bytes,
                        data_base64,
                    }
                })
                .collect::<Vec<_>>();
            VfsMountTemplatePayload::Inline {
                mount_id: mount.mount_id.clone(),
                display_name: mount.display_name.clone(),
                description: mount.description.clone(),
                capabilities: mount.capabilities.clone(),
                files,
            }
        }
        agentdash_domain::project_vfs_mount::ProjectVfsMountContent::ExternalService {
            service_id,
            root_ref,
        } => VfsMountTemplatePayload::ExternalService {
            mount_id: mount.mount_id.clone(),
            display_name: mount.display_name.clone(),
            description: mount.description.clone(),
            capabilities: mount.capabilities.clone(),
            service_id: service_id.clone(),
            root_ref: root_ref.clone(),
        },
    };
    serde_json::to_value(payload)
        .map_err(|error| PublishLibraryAssetError::Domain(DomainError::Serialization(error)))
}

fn validate_publish_input(
    input: &PublishLibraryAssetInput,
) -> Result<(), PublishLibraryAssetError> {
    if input.scope != LibraryAssetScope::User {
        return Err(PublishLibraryAssetError::BadRequest(
            "当前发布入口仅支持 user scope".to_string(),
        ));
    }
    for (field, value) in [
        ("key", input.key.as_str()),
        ("display_name", input.display_name.as_str()),
        ("version", input.version.as_str()),
        ("owner_id", input.owner_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(PublishLibraryAssetError::BadRequest(format!(
                "{field} 不能为空"
            )));
        }
    }
    Ok(())
}

async fn publish_agent_payload(
    repos: &RepositorySet,
    input: &PublishLibraryAssetInput,
) -> Result<serde_json::Value, PublishLibraryAssetError> {
    let agent = repos
        .project_agent_repo
        .get_by_id(input.project_asset_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "project_agent",
            id: input.project_asset_id.to_string(),
        })?;
    if agent.project_id != input.project_id {
        return Err(PublishLibraryAssetError::BadRequest(
            "Project Agent 不属于当前 Project".to_string(),
        ));
    }
    let merged = agent.preset_config()?;
    let config = AgentTemplateConfig {
        executor: Some(
            merged
                .executor
                .clone()
                .unwrap_or_else(|| agent.agent_type.clone()),
        ),
        provider_id: merged.provider_id,
        model_id: merged.model_id,
        agent_id: merged.agent_id,
        thinking_level: merged.thinking_level,
        permission_policy: merged.permission_policy,
        system_prompt: merged.system_prompt,
        system_prompt_mode: merged.system_prompt_mode,
        capability_directives: merged.capability_directives.unwrap_or_default(),
        mcp_slots: vec![],
    };
    serde_json::to_value(agentdash_domain::shared_library::AgentTemplatePayload { config })
        .map_err(|error| PublishLibraryAssetError::Domain(DomainError::Serialization(error)))
}

async fn publish_mcp_payload(
    repos: &RepositorySet,
    input: &PublishLibraryAssetInput,
) -> Result<serde_json::Value, PublishLibraryAssetError> {
    let preset = repos
        .mcp_preset_repo
        .get(input.project_asset_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "mcp_preset",
            id: input.project_asset_id.to_string(),
        })?;
    if preset.project_id != input.project_id {
        return Err(PublishLibraryAssetError::BadRequest(
            "MCP Preset 不属于当前 Project".to_string(),
        ));
    }
    let transport = sanitize_mcp_transport(&preset.transport)?;
    let payload = McpServerTemplatePayload {
        transport,
        route_policy: Some(preset.route_policy),
        parameter_schema: None,
        capabilities: vec![],
    };
    serde_json::to_value(payload)
        .map_err(|error| PublishLibraryAssetError::Domain(DomainError::Serialization(error)))
}

async fn publish_skill_payload(
    repos: &RepositorySet,
    input: &PublishLibraryAssetInput,
) -> Result<serde_json::Value, PublishLibraryAssetError> {
    let skill = repos
        .skill_asset_repo
        .get(input.project_asset_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "skill_asset",
            id: input.project_asset_id.to_string(),
        })?;
    if skill.project_id != input.project_id {
        return Err(PublishLibraryAssetError::BadRequest(
            "SkillAsset 不属于当前 Project".to_string(),
        ));
    }
    let mut files = Vec::with_capacity(skill.files.len());
    for file in skill.files {
        let Some(content) = file.text_content() else {
            return Err(PublishLibraryAssetError::BadRequest(format!(
                "暂不支持发布包含二进制文件的 SkillAsset: {}",
                file.path
            )));
        };
        let content = content.to_string();
        files.push(SkillTemplateFilePayload {
            path: file.path,
            content,
            kind: file.kind,
        });
    }
    let payload = SkillTemplatePayload {
        files,
        disable_model_invocation: skill.disable_model_invocation,
    };
    serde_json::to_value(payload)
        .map_err(|error| PublishLibraryAssetError::Domain(DomainError::Serialization(error)))
}

async fn publish_workflow_payload(
    repos: &RepositorySet,
    input: &PublishLibraryAssetInput,
) -> Result<serde_json::Value, PublishLibraryAssetError> {
    let lifecycle = repos
        .activity_lifecycle_definition_repo
        .get_by_id(input.project_asset_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "activity_lifecycle_definition",
            id: input.project_asset_id.to_string(),
        })?;
    if lifecycle.project_id != input.project_id {
        return Err(PublishLibraryAssetError::BadRequest(
            "ActivityLifecycleDefinition 不属于当前 Project".to_string(),
        ));
    }
    let workflows = collect_lifecycle_workflows(repos, &lifecycle).await?;
    let template = workflow_template_bundle(&lifecycle, workflows);
    Ok(json!({
        "template": template,
        "schema_version": input.version,
    }))
}

async fn collect_lifecycle_workflows(
    repos: &RepositorySet,
    lifecycle: &ActivityLifecycleDefinition,
) -> Result<Vec<WorkflowDefinition>, PublishLibraryAssetError> {
    let workflow_keys = lifecycle
        .activities
        .iter()
        .filter_map(|activity| match &activity.executor {
            ActivityExecutorSpec::Agent(agent) => Some(agent.workflow_key.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let mut workflows = Vec::new();
    for key in workflow_keys {
        let workflow = repos
            .workflow_definition_repo
            .get_by_project_and_key(lifecycle.project_id, &key)
            .await?
            .ok_or_else(|| {
                PublishLibraryAssetError::BadRequest(format!(
                    "Activity Lifecycle 引用的 workflow `{key}` 不存在"
                ))
            })?;
        workflows.push(workflow);
    }
    Ok(workflows)
}

fn workflow_template_bundle(
    lifecycle: &ActivityLifecycleDefinition,
    workflows: Vec<WorkflowDefinition>,
) -> BuiltinWorkflowTemplateBundle {
    BuiltinWorkflowTemplateBundle {
        key: lifecycle.key.clone(),
        name: lifecycle.name.clone(),
        description: lifecycle.description.clone(),
        binding_kinds: lifecycle.binding_kinds.clone(),
        workflows: workflows
            .into_iter()
            .map(|workflow| BuiltinWorkflowTemplate {
                key: workflow.key,
                name: workflow.name,
                description: workflow.description,
                contract: workflow.contract,
            })
            .collect(),
        lifecycle: BuiltinLifecycleTemplate {
            key: lifecycle.key.clone(),
            name: lifecycle.name.clone(),
            description: lifecycle.description.clone(),
            entry_activity_key: lifecycle.entry_activity_key.clone(),
            activities: lifecycle.activities.clone(),
            transitions: lifecycle.transitions.clone(),
        },
    }
}

fn sanitize_mcp_transport(
    transport: &McpTransportConfig,
) -> Result<McpTransportConfig, PublishLibraryAssetError> {
    match transport {
        McpTransportConfig::Http { url, headers } => {
            reject_secret_like_value("transport.url", url)?;
            reject_local_url("transport.url", url)?;
            reject_headers(headers)?;
            Ok(McpTransportConfig::Http {
                url: url.clone(),
                headers: vec![],
            })
        }
        McpTransportConfig::Sse { url, headers } => {
            reject_secret_like_value("transport.url", url)?;
            reject_local_url("transport.url", url)?;
            reject_headers(headers)?;
            Ok(McpTransportConfig::Sse {
                url: url.clone(),
                headers: vec![],
            })
        }
        McpTransportConfig::Stdio { command, args, env } => {
            reject_secret_like_value("transport.command", command)?;
            reject_local_path_like_value("transport.command", command)?;
            for (index, arg) in args.iter().enumerate() {
                reject_secret_like_value(&format!("transport.args[{index}]"), arg)?;
                reject_local_path_like_value(&format!("transport.args[{index}]"), arg)?;
            }
            reject_env(env)?;
            Ok(McpTransportConfig::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: vec![],
            })
        }
    }
}

fn reject_headers(headers: &[McpHttpHeader]) -> Result<(), PublishLibraryAssetError> {
    if headers.is_empty() {
        return Ok(());
    }
    let names = headers
        .iter()
        .map(|header| header.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(PublishLibraryAssetError::BadRequest(format!(
        "MCP header 属于连接材料，不能发布到公共模板: {names}"
    )))
}

fn reject_env(env: &[McpEnvVar]) -> Result<(), PublishLibraryAssetError> {
    if env.is_empty() {
        return Ok(());
    }
    let names = env
        .iter()
        .map(|item| item.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(PublishLibraryAssetError::BadRequest(format!(
        "MCP env 属于连接材料，不能发布到公共模板: {names}"
    )))
}

fn reject_secret_like_value(field: &str, value: &str) -> Result<(), PublishLibraryAssetError> {
    let lower = value.to_ascii_lowercase();
    let secret_markers = [
        "token",
        "secret",
        "password",
        "passwd",
        "apikey",
        "api_key",
        "authorization",
        "bearer ",
    ];
    if secret_markers.iter().any(|marker| lower.contains(marker)) {
        return Err(PublishLibraryAssetError::BadRequest(format!(
            "{field} 看起来包含 credential/secret，不能发布到公共模板"
        )));
    }
    Ok(())
}

fn reject_local_url(field: &str, value: &str) -> Result<(), PublishLibraryAssetError> {
    let lower = value.to_ascii_lowercase();
    let without_scheme = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
        .unwrap_or(lower.as_str());
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host = authority
        .strip_prefix('[')
        .and_then(|rest| rest.split(']').next())
        .unwrap_or_else(|| authority.split(':').next().unwrap_or_default());
    let is_private_172 = host
        .strip_prefix("172.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|octet| octet.parse::<u8>().ok())
        .is_some_and(|octet| (16..=31).contains(&octet));
    let is_local = matches!(host, "localhost" | "::1" | "0.0.0.0")
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.ends_with(".local")
        || is_private_172;
    if is_local {
        return Err(PublishLibraryAssetError::BadRequest(format!(
            "{field} 指向本机或私有网络，不能发布到公共模板"
        )));
    }
    Ok(())
}

fn reject_local_path_like_value(field: &str, value: &str) -> Result<(), PublishLibraryAssetError> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let looks_like_windows_drive = trimmed.len() >= 3
        && trimmed.as_bytes()[1] == b':'
        && trimmed.as_bytes()[0].is_ascii_alphabetic()
        && matches!(trimmed.as_bytes()[2], b'\\' | b'/');
    let looks_like_path = looks_like_windows_drive
        || trimmed.starts_with("~/")
        || trimmed.starts_with("~\\")
        || trimmed.starts_with("./")
        || trimmed.starts_with(".\\")
        || trimmed.starts_with("../")
        || trimmed.starts_with("..\\")
        || trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || lower.starts_with("file://");
    if looks_like_path {
        return Err(PublishLibraryAssetError::BadRequest(format!(
            "{field} 看起来是本机路径，不能发布到公共模板"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agentdash_domain::mcp_preset::McpHttpHeader;

    use super::*;

    #[test]
    fn mcp_publish_rejects_headers() {
        let transport = McpTransportConfig::Http {
            url: "https://example.com/mcp".to_string(),
            headers: vec![McpHttpHeader {
                name: "Authorization".to_string(),
                value: "Bearer abc".to_string(),
            }],
        };

        let error = sanitize_mcp_transport(&transport).expect_err("headers rejected");

        assert!(matches!(error, PublishLibraryAssetError::BadRequest(_)));
        assert!(error.to_string().contains("header"));
    }

    #[test]
    fn mcp_publish_rejects_secret_like_args() {
        let transport = McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec!["--token=abc".to_string()],
            env: vec![],
        };

        let error = sanitize_mcp_transport(&transport).expect_err("secret rejected");

        assert!(error.to_string().contains("credential"));
    }

    #[test]
    fn mcp_publish_rejects_local_network_urls() {
        let transport = McpTransportConfig::Sse {
            url: "http://localhost:8765/sse".to_string(),
            headers: vec![],
        };

        let error = sanitize_mcp_transport(&transport).expect_err("local url rejected");

        assert!(error.to_string().contains("私有网络"));
    }

    #[test]
    fn mcp_publish_rejects_local_paths() {
        let transport = McpTransportConfig::Stdio {
            command: "node".to_string(),
            args: vec!["C:\\Users\\me\\server.js".to_string()],
            env: vec![],
        };

        let error = sanitize_mcp_transport(&transport).expect_err("local path rejected");

        assert!(error.to_string().contains("本机路径"));
    }
}
