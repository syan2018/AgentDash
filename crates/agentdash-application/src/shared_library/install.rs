use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::common::AgentPresetConfig;
use agentdash_domain::mcp_preset::{McpPreset, McpPresetSource};
use agentdash_domain::shared_library::{
    InstalledAssetSource, LibraryAsset, LibraryAssetPayload, ProjectExtensionInstallation,
    SharedLibrarySourceStatus,
};
use agentdash_domain::skill_asset::{SkillAsset, SkillAssetFile};
use agentdash_domain::workflow::WorkflowDefinitionSource;

use crate::repository_set::RepositorySet;
use crate::workflow::BuiltinWorkflowTemplateBundle;

#[derive(Debug, Clone)]
pub struct InstallLibraryAssetInput {
    pub project_id: Uuid,
    pub library_asset_id: Uuid,
    pub target_key: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone)]
pub enum InstallLibraryAssetOutput {
    ProjectAgent {
        project_agent_id: Uuid,
    },
    McpPreset {
        id: Uuid,
    },
    WorkflowTemplate {
        workflow_ids: Vec<Uuid>,
        lifecycle_id: Uuid,
    },
    SkillAsset {
        id: Uuid,
    },
    ExtensionInstallation {
        id: Uuid,
    },
}

#[derive(Debug, Clone)]
pub struct ProjectAssetSourceStatus {
    pub project_agents: Vec<ProjectAssetSourceStatusItem>,
    pub mcp_presets: Vec<ProjectAssetSourceStatusItem>,
    pub skill_assets: Vec<ProjectAssetSourceStatusItem>,
    pub workflow_definitions: Vec<ProjectAssetSourceStatusItem>,
    pub lifecycle_definitions: Vec<ProjectAssetSourceStatusItem>,
    pub extension_installations: Vec<ProjectAssetSourceStatusItem>,
}

#[derive(Debug, Clone)]
pub struct ProjectAssetSourceStatusItem {
    pub asset_kind: &'static str,
    pub project_asset_id: Uuid,
    pub project_asset_key: String,
    pub installed_source: InstalledAssetSource,
    pub source_status: SharedLibrarySourceStatus,
    pub current_source_version: Option<String>,
    pub current_source_digest: Option<String>,
}

pub async fn install_library_asset_to_project(
    repos: &RepositorySet,
    input: InstallLibraryAssetInput,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    let asset = repos
        .shared_library_repo
        .get(input.library_asset_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "library_asset",
            id: input.library_asset_id.to_string(),
        })?;
    if asset.deprecated {
        return Err(DomainError::InvalidConfig(
            "已废弃的 LibraryAsset 不能安装".to_string(),
        ));
    }

    match asset.typed_payload()? {
        LibraryAssetPayload::AgentTemplate(payload) => {
            install_agent_template(repos, input, asset, payload.config).await
        }
        LibraryAssetPayload::McpServerTemplate(payload) => {
            let key = target_key_or_asset_key(input.target_key.as_deref(), &asset.key);
            let installed_source = installed_source_from_asset(&asset);
            let mut preset = McpPreset::new_user(
                input.project_id,
                key,
                asset.display_name.clone(),
                asset.description.clone(),
                payload.transport,
                payload.route_policy.unwrap_or_default(),
            );
            preset.installed_source = Some(installed_source);
            upsert_mcp_preset(repos, preset, input.overwrite).await
        }
        LibraryAssetPayload::WorkflowTemplate(payload) => {
            install_workflow_template(repos, input, asset, payload.template).await
        }
        LibraryAssetPayload::SkillTemplate(payload) => {
            let key = target_key_or_asset_key(input.target_key.as_deref(), &asset.key);
            let mut skill = SkillAsset::new_user(
                input.project_id,
                key,
                asset.display_name.clone(),
                asset.description.clone().unwrap_or_default(),
                payload.disable_model_invocation,
            );
            skill.installed_source = Some(installed_source_from_asset(&asset));
            skill.files = payload
                .files
                .into_iter()
                .map(|file| SkillAssetFile::new(skill.id, file.path, file.content, file.kind))
                .collect();
            upsert_skill_asset(repos, skill, input.overwrite).await
        }
        LibraryAssetPayload::ExtensionTemplate(payload) => {
            let key = target_key_or_asset_key(input.target_key.as_deref(), &asset.key);
            let installed_source = installed_source_from_asset(&asset);
            let installation = ProjectExtensionInstallation::new(
                input.project_id,
                key,
                asset.display_name.clone(),
                payload,
                installed_source,
            )?;
            upsert_extension_installation(repos, installation, input.overwrite).await
        }
    }
}

pub async fn list_project_asset_source_status(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<ProjectAssetSourceStatus, DomainError> {
    let mut project_agents = Vec::new();
    for agent in repos.project_agent_repo.list_by_project(project_id).await? {
        if let Some(installed_source) = agent.installed_source {
            project_agents.push(
                source_status_item(
                    repos,
                    "project_agent",
                    agent.id,
                    agent.name,
                    installed_source,
                )
                .await?,
            );
        }
    }

    let mut mcp_presets = Vec::new();
    for preset in repos.mcp_preset_repo.list_by_project(project_id).await? {
        if let Some(installed_source) = preset.installed_source {
            mcp_presets.push(
                source_status_item(repos, "mcp_preset", preset.id, preset.key, installed_source)
                    .await?,
            );
        }
    }

    let mut skill_assets = Vec::new();
    for skill in repos.skill_asset_repo.list_by_project(project_id).await? {
        if let Some(installed_source) = skill.installed_source {
            skill_assets.push(
                source_status_item(repos, "skill_asset", skill.id, skill.key, installed_source)
                    .await?,
            );
        }
    }

    let mut workflow_definitions = Vec::new();
    for workflow in repos
        .workflow_definition_repo
        .list_by_project(project_id)
        .await?
    {
        if let Some(installed_source) = workflow.installed_source {
            workflow_definitions.push(
                source_status_item(
                    repos,
                    "workflow_definition",
                    workflow.id,
                    workflow.key,
                    installed_source,
                )
                .await?,
            );
        }
    }

    let mut lifecycle_definitions = Vec::new();
    for lifecycle in repos
        .lifecycle_definition_repo
        .list_by_project(project_id)
        .await?
    {
        if let Some(installed_source) = lifecycle.installed_source {
            lifecycle_definitions.push(
                source_status_item(
                    repos,
                    "lifecycle_definition",
                    lifecycle.id,
                    lifecycle.key,
                    installed_source,
                )
                .await?,
            );
        }
    }

    let mut extension_installations = Vec::new();
    for installation in repos
        .project_extension_installation_repo
        .list_by_project(project_id)
        .await?
    {
        extension_installations.push(
            source_status_item(
                repos,
                "extension_installation",
                installation.id,
                installation.extension_key,
                installation.installed_source,
            )
            .await?,
        );
    }

    Ok(ProjectAssetSourceStatus {
        project_agents,
        mcp_presets,
        skill_assets,
        workflow_definitions,
        lifecycle_definitions,
        extension_installations,
    })
}

async fn install_agent_template(
    repos: &RepositorySet,
    input: InstallLibraryAssetInput,
    asset: LibraryAsset,
    config: agentdash_domain::shared_library::AgentTemplateConfig,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    let key = target_key_or_asset_key(input.target_key.as_deref(), &asset.key);
    let installed_source = installed_source_from_asset(&asset);
    let mut agent = ProjectAgent::new(
        input.project_id,
        key,
        config
            .executor
            .clone()
            .unwrap_or_else(|| "PI_AGENT".to_string()),
    );
    let agent_config = AgentPresetConfig {
        executor: config.executor,
        provider_id: config.provider_id,
        model_id: config.model_id,
        agent_id: config.agent_id,
        thinking_level: config.thinking_level,
        permission_policy: config.permission_policy,
        system_prompt: config.system_prompt,
        system_prompt_mode: config.system_prompt_mode,
        display_name: Some(asset.display_name),
        description: asset.description,
        capability_directives: (!config.capability_directives.is_empty())
            .then_some(config.capability_directives),
        mcp_preset_keys: None,
        skill_asset_keys: None,
        allowed_companions: None,
    };
    agent.config = serde_json::to_value(agent_config).map_err(DomainError::Serialization)?;
    agent.installed_source = Some(installed_source);
    repos.project_agent_repo.create(&agent).await?;
    Ok(InstallLibraryAssetOutput::ProjectAgent {
        project_agent_id: agent.id,
    })
}

async fn upsert_mcp_preset(
    repos: &RepositorySet,
    preset: McpPreset,
    overwrite: bool,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    if let Some(existing) = repos
        .mcp_preset_repo
        .get_by_project_and_key(preset.project_id, &preset.key)
        .await?
    {
        if !overwrite {
            return Err(DomainError::InvalidConfig(format!(
                "Project MCP Preset key 已存在: {}",
                preset.key
            )));
        }
        let mut merged = preset;
        merged.id = existing.id;
        merged.created_at = existing.created_at;
        merged.updated_at = chrono::Utc::now();
        merged.source = McpPresetSource::User;
        repos.mcp_preset_repo.update(&merged).await?;
        return Ok(InstallLibraryAssetOutput::McpPreset { id: merged.id });
    }

    let id = preset.id;
    repos.mcp_preset_repo.create(&preset).await?;
    Ok(InstallLibraryAssetOutput::McpPreset { id })
}

async fn install_workflow_template(
    repos: &RepositorySet,
    input: InstallLibraryAssetInput,
    asset: LibraryAsset,
    template: serde_json::Value,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    let template: BuiltinWorkflowTemplateBundle =
        serde_json::from_value(template).map_err(DomainError::Serialization)?;
    let mut bundle = template
        .build_bundle(input.project_id)
        .map_err(DomainError::InvalidConfig)?;
    let installed_source = installed_source_from_asset(&asset);
    let mut workflow_ids = Vec::new();
    for mut workflow in bundle.workflows.drain(..) {
        workflow.source = WorkflowDefinitionSource::UserAuthored;
        workflow.installed_source = Some(installed_source.clone());
        if let Some(existing) = repos
            .workflow_definition_repo
            .get_by_project_and_key(input.project_id, &workflow.key)
            .await?
        {
            if !input.overwrite {
                return Err(DomainError::InvalidConfig(format!(
                    "Project Workflow key 已存在: {}",
                    workflow.key
                )));
            }
            workflow.id = existing.id;
            workflow.created_at = existing.created_at;
            repos.workflow_definition_repo.update(&workflow).await?;
        } else {
            repos.workflow_definition_repo.create(&workflow).await?;
        }
        workflow_ids.push(workflow.id);
    }

    let mut lifecycle = bundle.lifecycle;
    lifecycle.source = WorkflowDefinitionSource::UserAuthored;
    lifecycle.installed_source = Some(installed_source);
    if let Some(existing) = repos
        .lifecycle_definition_repo
        .get_by_project_and_key(input.project_id, &lifecycle.key)
        .await?
    {
        if !input.overwrite {
            return Err(DomainError::InvalidConfig(format!(
                "Project Lifecycle key 已存在: {}",
                lifecycle.key
            )));
        }
        lifecycle.id = existing.id;
        lifecycle.created_at = existing.created_at;
        repos.lifecycle_definition_repo.update(&lifecycle).await?;
    } else {
        repos.lifecycle_definition_repo.create(&lifecycle).await?;
    }

    Ok(InstallLibraryAssetOutput::WorkflowTemplate {
        workflow_ids,
        lifecycle_id: lifecycle.id,
    })
}

async fn upsert_skill_asset(
    repos: &RepositorySet,
    skill: SkillAsset,
    overwrite: bool,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    if let Some(existing) = repos
        .skill_asset_repo
        .get_by_project_and_key(skill.project_id, &skill.key)
        .await?
    {
        if !overwrite {
            return Err(DomainError::InvalidConfig(format!(
                "Project SkillAsset key 已存在: {}",
                skill.key
            )));
        }
        let mut merged = skill;
        merged.id = existing.id;
        merged.created_at = existing.created_at;
        for file in &mut merged.files {
            file.skill_asset_id = merged.id;
        }
        repos.skill_asset_repo.update(&merged).await?;
        return Ok(InstallLibraryAssetOutput::SkillAsset { id: merged.id });
    }

    let id = skill.id;
    repos.skill_asset_repo.create(&skill).await?;
    Ok(InstallLibraryAssetOutput::SkillAsset { id })
}

async fn upsert_extension_installation(
    repos: &RepositorySet,
    installation: ProjectExtensionInstallation,
    overwrite: bool,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    if let Some(existing) = repos
        .project_extension_installation_repo
        .get_by_project_and_key(installation.project_id, &installation.extension_key)
        .await?
    {
        if !overwrite {
            return Err(DomainError::InvalidConfig(format!(
                "Project Extension key 已存在: {}",
                installation.extension_key
            )));
        }
        let mut merged = installation;
        merged.id = existing.id;
        merged.created_at = existing.created_at;
        merged.updated_at = chrono::Utc::now();
        repos
            .project_extension_installation_repo
            .update(&merged)
            .await?;
        return Ok(InstallLibraryAssetOutput::ExtensionInstallation { id: merged.id });
    }

    let id = installation.id;
    repos
        .project_extension_installation_repo
        .create(&installation)
        .await?;
    Ok(InstallLibraryAssetOutput::ExtensionInstallation { id })
}

async fn source_status_item(
    repos: &RepositorySet,
    asset_kind: &'static str,
    project_asset_id: Uuid,
    project_asset_key: String,
    installed_source: InstalledAssetSource,
) -> Result<ProjectAssetSourceStatusItem, DomainError> {
    let source = repos
        .shared_library_repo
        .get(installed_source.library_asset_id)
        .await?;
    let status = SharedLibrarySourceStatus::from_installed_source(
        &installed_source,
        source.as_ref().map(|asset| asset.version.as_str()),
        source.as_ref().map(|asset| asset.payload_digest.as_str()),
        source.as_ref().is_none_or(|asset| asset.deprecated),
    );
    Ok(ProjectAssetSourceStatusItem {
        asset_kind,
        project_asset_id,
        project_asset_key,
        current_source_version: source.as_ref().map(|asset| asset.version.clone()),
        current_source_digest: source.as_ref().map(|asset| asset.payload_digest.clone()),
        installed_source,
        source_status: status,
    })
}

fn installed_source_from_asset(asset: &LibraryAsset) -> InstalledAssetSource {
    InstalledAssetSource::new(
        asset.id,
        asset
            .source_ref
            .clone()
            .unwrap_or_else(|| asset.key.clone()),
        asset.version.clone(),
        asset.payload_digest.clone(),
    )
}

fn target_key_or_asset_key(target_key: Option<&str>, asset_key: &str) -> String {
    target_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(asset_key)
        .to_string()
}
