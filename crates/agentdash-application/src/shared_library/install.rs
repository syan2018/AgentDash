use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::common::AgentPresetConfig;
use agentdash_domain::extension_package::ExtensionPackageArtifactOwner;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind};
use agentdash_domain::mcp_preset::{McpPreset, McpPresetSource};
use agentdash_domain::project_vfs_mount::{ProjectVfsMount, ProjectVfsMountContent};
use agentdash_domain::shared_library::{
    AgentMcpDependencyTemplate, InstalledAssetSource, LibraryAsset, LibraryAssetListFilter,
    LibraryAssetPayload, LibraryAssetType, McpServerTemplatePayload, ProjectExtensionInstallation,
    SharedLibrarySourceStatus, VfsMountTemplatePayload, normalize_workflow_template_value,
};

use crate::vfs::PROJECT_VFS_MOUNT_CONTAINER_ID;
use agentdash_domain::skill_asset::{SkillAsset, SkillAssetFile};
use agentdash_domain::workflow::{DefinitionSource, WorkflowTemplateInstallBundle};

use crate::repository_set::RepositorySet;
use crate::workflow::BuiltinWorkflowTemplateBundle;

#[derive(Debug, Clone)]
pub struct InstallLibraryAssetInput {
    pub project_id: Uuid,
    pub library_asset_id: Uuid,
    pub target_key: Option<String>,
    pub overwrite: bool,
    pub install_options: Option<InstallLibraryAssetOptions>,
}

#[derive(Debug, Clone)]
pub enum InstallLibraryAssetOptions {
    McpServerTemplate {
        parameters: Value,
    },
    AgentTemplate {
        dependency_mode: AgentTemplateDependencyMode,
        dependency_parameters: BTreeMap<String, Value>,
        overwrite_dependencies: bool,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentTemplateDependencyMode {
    #[default]
    Required,
    All,
    Skip,
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
    VfsMount {
        id: Uuid,
        mount_id: String,
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
    pub vfs_mounts: Vec<ProjectAssetSourceStatusItem>,
    pub agent_procedures: Vec<ProjectAssetSourceStatusItem>,
    pub workflow_graphs: Vec<ProjectAssetSourceStatusItem>,
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
            reject_install_options_for_non_agent(&input)?;
            install_agent_template(repos, input, asset, payload.config).await
        }
        LibraryAssetPayload::McpServerTemplate(payload) => {
            let preset = mcp_preset_from_template_install(
                input.project_id,
                input.target_key.as_deref(),
                &asset,
                payload,
                mcp_template_parameters(input.install_options.as_ref())?,
            )?;
            upsert_mcp_preset(repos, preset, input.overwrite).await
        }
        LibraryAssetPayload::WorkflowTemplate(payload) => {
            reject_install_options_for_non_mcp_or_agent(&input)?;
            install_workflow_template(repos, input, asset, payload.template).await
        }
        LibraryAssetPayload::SkillTemplate(payload) => {
            reject_install_options_for_non_mcp_or_agent(&input)?;
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
        LibraryAssetPayload::VfsMountTemplate(payload) => {
            reject_install_options_for_non_mcp_or_agent(&input)?;
            install_vfs_mount_template(repos, input, asset, payload).await
        }
        LibraryAssetPayload::ExtensionTemplate(payload) => {
            reject_install_options_for_non_mcp_or_agent(&input)?;
            let key = target_key_or_asset_key(input.target_key.as_deref(), &asset.key);
            let installed_source = installed_source_from_asset(&asset);
            let package_artifact =
                extension_template_package_artifact_for_install(repos, &asset, &payload).await?;
            let installation = if let Some(package_artifact) = package_artifact {
                ProjectExtensionInstallation::new_from_library_package(
                    input.project_id,
                    key,
                    asset.display_name.clone(),
                    payload,
                    installed_source,
                    package_artifact.package_ref(),
                )?
            } else {
                ProjectExtensionInstallation::new(
                    input.project_id,
                    key,
                    asset.display_name.clone(),
                    payload,
                    installed_source,
                )?
            };
            upsert_extension_installation(repos, installation, input.overwrite).await
        }
    }
}

fn mcp_preset_from_template_install(
    project_id: Uuid,
    target_key: Option<&str>,
    asset: &LibraryAsset,
    payload: McpServerTemplatePayload,
    parameters: Option<&Value>,
) -> Result<McpPreset, DomainError> {
    let key = target_key_or_asset_key(target_key, &asset.key);
    let transport = payload.resolve_transport(parameters)?;
    let mut preset = McpPreset::new_user(
        project_id,
        key,
        asset.display_name.clone(),
        asset.description.clone(),
        transport,
        payload.route_policy.unwrap_or_default(),
    );
    preset.installed_source = Some(installed_source_from_asset(asset));
    Ok(preset)
}

fn mcp_template_parameters(
    install_options: Option<&InstallLibraryAssetOptions>,
) -> Result<Option<&Value>, DomainError> {
    match install_options {
        None => Ok(None),
        Some(InstallLibraryAssetOptions::McpServerTemplate { parameters }) => Ok(Some(parameters)),
        Some(InstallLibraryAssetOptions::AgentTemplate { .. }) => Err(DomainError::InvalidConfig(
            "mcp_server_template 不支持 agent_template install_options".to_string(),
        )),
    }
}

fn reject_install_options_for_non_mcp_or_agent(
    input: &InstallLibraryAssetInput,
) -> Result<(), DomainError> {
    if input.install_options.is_none() {
        return Ok(());
    }
    Err(DomainError::InvalidConfig(
        "install_options 仅支持 mcp_server_template / agent_template".to_string(),
    ))
}

fn reject_install_options_for_non_agent(
    input: &InstallLibraryAssetInput,
) -> Result<(), DomainError> {
    match &input.install_options {
        None | Some(InstallLibraryAssetOptions::AgentTemplate { .. }) => Ok(()),
        Some(InstallLibraryAssetOptions::McpServerTemplate { .. }) => {
            Err(DomainError::InvalidConfig(
                "agent_template 不支持 mcp_server_template install_options".to_string(),
            ))
        }
    }
}

async fn extension_template_package_artifact_for_install(
    repos: &RepositorySet,
    asset: &LibraryAsset,
    payload: &agentdash_domain::shared_library::ExtensionTemplatePayload,
) -> Result<Option<agentdash_domain::extension_package::ExtensionPackageArtifact>, DomainError> {
    let owner = ExtensionPackageArtifactOwner::library_asset(asset.id);
    let artifacts = repos
        .extension_package_artifact_repo
        .list_by_owner(&owner)
        .await?;
    let artifact = artifacts
        .into_iter()
        .find(|artifact| artifact.matches_extension_template(payload));
    match (payload.requires_package_artifact(), artifact) {
        (_, Some(artifact)) => Ok(Some(artifact)),
        (false, None) => Ok(None),
        (true, None) => Err(DomainError::InvalidConfig(format!(
            "ExtensionTemplate `{}` 需要 package artifact，但 LibraryAsset 未关联可用包工件",
            asset.key
        ))),
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

    let mut vfs_mounts = Vec::new();
    for mount in repos
        .project_vfs_mount_repo
        .list_by_project(project_id)
        .await?
    {
        if let Some(installed_source) = mount.installed_source {
            vfs_mounts.push(
                source_status_item(
                    repos,
                    "project_vfs_mount",
                    mount.id,
                    mount.mount_id,
                    installed_source,
                )
                .await?,
            );
        }
    }

    let mut agent_procedures = Vec::new();
    for workflow in repos
        .agent_procedure_repo
        .list_by_project(project_id)
        .await?
    {
        if let Some(installed_source) = workflow.installed_source {
            agent_procedures.push(
                source_status_item(
                    repos,
                    "agent_procedure",
                    workflow.id,
                    workflow.key,
                    installed_source,
                )
                .await?,
            );
        }
    }

    let mut workflow_graphs = Vec::new();
    for lifecycle in repos
        .workflow_graph_repo
        .list_by_project(project_id)
        .await?
    {
        if let Some(installed_source) = lifecycle.installed_source {
            workflow_graphs.push(
                source_status_item(
                    repos,
                    "workflow_graph",
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
        if let Some(installed_source) = installation.installed_source {
            extension_installations.push(
                source_status_item(
                    repos,
                    "extension_installation",
                    installation.id,
                    installation.extension_key,
                    installed_source,
                )
                .await?,
            );
        }
    }

    Ok(ProjectAssetSourceStatus {
        project_agents,
        mcp_presets,
        skill_assets,
        vfs_mounts,
        agent_procedures,
        workflow_graphs,
        extension_installations,
    })
}

async fn install_agent_template(
    repos: &RepositorySet,
    input: InstallLibraryAssetInput,
    asset: LibraryAsset,
    config: agentdash_domain::shared_library::AgentTemplateConfig,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    let dependency_plans = resolve_agent_mcp_preset_install_plans(
        repos,
        &asset,
        &config.mcp_dependencies,
        input.install_options.as_ref(),
    )
    .await?;

    let mut mcp_preset_keys = Vec::with_capacity(dependency_plans.len());
    for plan in dependency_plans {
        let preset = mcp_preset_from_template_install(
            input.project_id,
            Some(&plan.target_key),
            &plan.asset,
            plan.payload,
            plan.parameters.as_ref(),
        )?;
        let installed = upsert_mcp_preset(repos, preset, plan.overwrite).await?;
        if let InstallLibraryAssetOutput::McpPreset { .. } = installed {
            mcp_preset_keys.push(plan.target_key);
        }
    }

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
        mcp_preset_keys: (!mcp_preset_keys.is_empty()).then_some(mcp_preset_keys),
        vfs_access_grants: None,
        skill_asset_keys: None,
        default_companion_enabled: None,
        extra_companions: None,
        // 模板安装不预设 module 可见性（module 为项目作用域），项目内按 agent 编辑。
        visible_workspace_module_refs: None,
    };
    agent.config = serde_json::to_value(agent_config).map_err(DomainError::Serialization)?;
    agent.installed_source = Some(installed_source);
    repos.project_agent_repo.create(&agent).await?;
    Ok(InstallLibraryAssetOutput::ProjectAgent {
        project_agent_id: agent.id,
    })
}

struct AgentMcpPresetInstallPlan {
    asset: LibraryAsset,
    payload: McpServerTemplatePayload,
    target_key: String,
    parameters: Option<Value>,
    overwrite: bool,
}

async fn resolve_agent_mcp_preset_install_plans(
    repos: &RepositorySet,
    agent_asset: &LibraryAsset,
    dependencies: &[AgentMcpDependencyTemplate],
    install_options: Option<&InstallLibraryAssetOptions>,
) -> Result<Vec<AgentMcpPresetInstallPlan>, DomainError> {
    let options = agent_template_install_options(install_options)?;
    if options.dependency_mode == AgentTemplateDependencyMode::Skip || dependencies.is_empty() {
        return Ok(vec![]);
    }

    let active_dependencies = dependencies
        .iter()
        .filter(|dependency| {
            dependency.required || options.dependency_mode == AgentTemplateDependencyMode::All
        })
        .collect::<Vec<_>>();
    if active_dependencies.is_empty() {
        return Ok(vec![]);
    }

    let candidate_assets = repos
        .shared_library_repo
        .list(LibraryAssetListFilter {
            asset_type: Some(LibraryAssetType::McpServerTemplate),
            include_deprecated: false,
            ..Default::default()
        })
        .await?;

    let mut seen_target_keys = BTreeSet::new();
    let mut plans = Vec::with_capacity(active_dependencies.len());
    for dependency in active_dependencies {
        let asset = resolve_agent_mcp_dependency_asset(agent_asset, dependency, &candidate_assets)?;
        let payload = match asset.typed_payload()? {
            LibraryAssetPayload::McpServerTemplate(payload) => payload,
            other => {
                return Err(DomainError::InvalidConfig(format!(
                    "Agent MCP 依赖 `{}` 指向非 MCP 模板 payload: {other:?}",
                    dependency.asset_key
                )));
            }
        };
        let target_key = dependency_target_key(dependency);
        if !seen_target_keys.insert(target_key.clone()) {
            return Err(DomainError::InvalidConfig(format!(
                "Agent MCP 依赖 target_key 重复: {target_key}"
            )));
        }
        let parameters = merged_dependency_parameters(dependency, options.dependency_parameters)?;
        // 预先解析一次，确保参数错误发生在任何写入之前。
        payload.resolve_transport(parameters.as_ref())?;
        plans.push(AgentMcpPresetInstallPlan {
            asset,
            payload,
            target_key,
            parameters,
            overwrite: true,
        });
    }
    Ok(plans)
}

struct AgentTemplateInstallOptions<'a> {
    dependency_mode: AgentTemplateDependencyMode,
    dependency_parameters: &'a BTreeMap<String, Value>,
}

fn agent_template_install_options(
    install_options: Option<&InstallLibraryAssetOptions>,
) -> Result<AgentTemplateInstallOptions<'_>, DomainError> {
    static EMPTY_PARAMETERS: std::sync::OnceLock<BTreeMap<String, Value>> =
        std::sync::OnceLock::new();
    match install_options {
        None => Ok(AgentTemplateInstallOptions {
            dependency_mode: AgentTemplateDependencyMode::Required,
            dependency_parameters: EMPTY_PARAMETERS.get_or_init(BTreeMap::new),
        }),
        Some(InstallLibraryAssetOptions::AgentTemplate {
            dependency_mode,
            dependency_parameters,
            overwrite_dependencies: _,
        }) => Ok(AgentTemplateInstallOptions {
            dependency_mode: *dependency_mode,
            dependency_parameters,
        }),
        Some(InstallLibraryAssetOptions::McpServerTemplate { .. }) => {
            Err(DomainError::InvalidConfig(
                "agent_template 不支持 mcp_server_template install_options".to_string(),
            ))
        }
    }
}

fn resolve_agent_mcp_dependency_asset(
    agent_asset: &LibraryAsset,
    dependency: &AgentMcpDependencyTemplate,
    candidate_assets: &[LibraryAsset],
) -> Result<LibraryAsset, DomainError> {
    let candidates = candidate_assets
        .iter()
        .filter(|asset| asset.key == dependency.asset_key)
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(DomainError::NotFound {
            entity: "mcp_server_template",
            id: dependency.asset_key.clone(),
        });
    }

    if let Some(agent_family) = source_family(agent_asset) {
        let same_family = candidates
            .iter()
            .filter(|asset| source_family(asset).as_deref() == Some(agent_family.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if same_family.len() == 1 {
            return Ok(same_family[0].clone());
        }
        if same_family.len() > 1 {
            return Err(ambiguous_dependency_error(dependency, &same_family));
        }
    }

    let same_scope = candidates
        .iter()
        .filter(|asset| {
            asset.scope == agent_asset.scope
                && asset.owner_id.as_deref() == agent_asset.owner_id.as_deref()
        })
        .cloned()
        .collect::<Vec<_>>();
    if same_scope.len() == 1 {
        return Ok(same_scope[0].clone());
    }
    if same_scope.len() > 1 {
        return Err(ambiguous_dependency_error(dependency, &same_scope));
    }

    if candidates.len() == 1 {
        return Ok(candidates[0].clone());
    }
    Err(ambiguous_dependency_error(dependency, &candidates))
}

fn ambiguous_dependency_error(
    dependency: &AgentMcpDependencyTemplate,
    candidates: &[LibraryAsset],
) -> DomainError {
    let identities = candidates
        .iter()
        .map(|asset| {
            format!(
                "{}:{:?}:{}",
                asset.key,
                asset.scope,
                asset.source_ref.as_deref().unwrap_or("<none>")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    DomainError::InvalidConfig(format!(
        "Agent MCP 依赖 `{}` 匹配到多个 MCP 模板: {identities}",
        dependency.asset_key
    ))
}

fn dependency_target_key(dependency: &AgentMcpDependencyTemplate) -> String {
    dependency
        .target_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&dependency.asset_key)
        .to_string()
}

fn merged_dependency_parameters(
    dependency: &AgentMcpDependencyTemplate,
    request_parameters: &BTreeMap<String, Value>,
) -> Result<Option<Value>, DomainError> {
    let mut merged = serde_json::Map::new();
    if let Some(defaults) = &dependency.parameters {
        let defaults = defaults.as_object().ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "Agent MCP 依赖 `{}` parameters 必须是对象",
                dependency.asset_key
            ))
        })?;
        for (key, value) in defaults {
            merged.insert(key.clone(), value.clone());
        }
    }
    let override_parameters = request_parameters
        .get(&dependency.slot_key)
        .or_else(|| request_parameters.get(&dependency.asset_key));
    if let Some(override_parameters) = override_parameters {
        let override_parameters = override_parameters.as_object().ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "install_options.agent_template.dependency_parameters.{} 必须是对象",
                dependency.slot_key
            ))
        })?;
        for (key, value) in override_parameters {
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok((!merged.is_empty()).then_some(Value::Object(merged)))
}

fn source_family(asset: &LibraryAsset) -> Option<String> {
    let source_ref = asset.source_ref.as_deref()?;
    let mut parts = source_ref.split(':');
    match parts.next()? {
        "integration" => parts.next().map(|name| format!("integration:{name}")),
        "market" => parts.next().map(|name| format!("market:{name}")),
        "builtin" => Some("builtin".to_string()),
        _ => None,
    }
}

async fn install_vfs_mount_template(
    repos: &RepositorySet,
    input: InstallLibraryAssetInput,
    asset: LibraryAsset,
    payload: VfsMountTemplatePayload,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    let target_mount_id = target_key_or_asset_key(
        input.target_key.as_deref(),
        if payload.mount_id().trim().is_empty() {
            asset.key.as_str()
        } else {
            payload.mount_id()
        },
    );
    let installed_source = installed_source_from_asset(&asset);
    let display_name = if payload.display_name().trim().is_empty() {
        asset.display_name.clone()
    } else {
        payload.display_name().to_string()
    };
    let description = payload
        .description()
        .map(str::to_string)
        .or_else(|| asset.description.clone());
    let capabilities = payload.capabilities().to_vec();

    let (mut mount, files) = match payload {
        VfsMountTemplatePayload::Inline { files, .. } => {
            let mount = ProjectVfsMount {
                id: Uuid::new_v4(),
                project_id: input.project_id,
                mount_id: target_mount_id.clone(),
                display_name: display_name.clone(),
                description: description.clone(),
                capabilities: capabilities.clone(),
                installed_source: Some(installed_source.clone()),
                content: ProjectVfsMountContent::Inline,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            (mount, Some(files))
        }
        VfsMountTemplatePayload::ExternalService {
            service_id,
            root_ref,
            ..
        } => {
            let mount = ProjectVfsMount {
                id: Uuid::new_v4(),
                project_id: input.project_id,
                mount_id: target_mount_id.clone(),
                display_name: display_name.clone(),
                description: description.clone(),
                capabilities: capabilities.clone(),
                installed_source: Some(installed_source.clone()),
                content: ProjectVfsMountContent::ExternalService {
                    service_id,
                    root_ref,
                },
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            (mount, None)
        }
    };

    if let Some(existing) = repos
        .project_vfs_mount_repo
        .get_by_project_and_mount_id(input.project_id, &target_mount_id)
        .await?
    {
        if !input.overwrite {
            return Err(DomainError::InvalidConfig(format!(
                "Project VFS Mount 已存在: {target_mount_id}"
            )));
        }
        mount.id = existing.id;
        mount.created_at = existing.created_at;
        mount.updated_at = chrono::Utc::now();
        repos.project_vfs_mount_repo.update(&mount).await?;
        repos
            .inline_file_repo
            .delete_by_owner(InlineFileOwnerKind::ProjectVfsMount, mount.id)
            .await?;
    } else {
        repos.project_vfs_mount_repo.create(&mount).await?;
    }

    if let Some(files) = files {
        let inline_files = files
            .into_iter()
            .map(|file| {
                let path = crate::vfs::normalize_mount_relative_path(&file.path, false)
                    .map_err(DomainError::InvalidConfig)?;
                match file.content_kind.as_str() {
                    "text" => Ok(InlineFile::new_text(
                        InlineFileOwnerKind::ProjectVfsMount,
                        mount.id,
                        PROJECT_VFS_MOUNT_CONTAINER_ID,
                        path,
                        file.content.unwrap_or_default(),
                    )),
                    "binary" => {
                        let encoded = file.data_base64.ok_or_else(|| {
                            DomainError::InvalidConfig(
                                "vfs_mount_template binary 文件缺少 data_base64".to_string(),
                            )
                        })?;
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(encoded)
                            .map_err(|error| {
                                DomainError::InvalidConfig(format!(
                                    "vfs_mount_template binary base64 非法: {error}"
                                ))
                            })?;
                        if bytes.len() as u64 != file.size_bytes {
                            return Err(DomainError::InvalidConfig(format!(
                                "vfs_mount_template 文件 `{}` 的 size_bytes 与 data_base64 不一致",
                                file.path
                            )));
                        }
                        Ok(InlineFile::new_binary(
                            InlineFileOwnerKind::ProjectVfsMount,
                            mount.id,
                            PROJECT_VFS_MOUNT_CONTAINER_ID,
                            path,
                            bytes,
                            file.mime_type
                                .unwrap_or_else(|| "application/octet-stream".to_string()),
                        ))
                    }
                    other => Err(DomainError::InvalidConfig(format!(
                        "vfs_mount_template content_kind 非法: {other}"
                    ))),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        repos.inline_file_repo.upsert_files(&inline_files).await?;
    }

    Ok(InstallLibraryAssetOutput::VfsMount {
        id: mount.id,
        mount_id: mount.mount_id,
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
    mut template: serde_json::Value,
) -> Result<InstallLibraryAssetOutput, DomainError> {
    normalize_workflow_template_value(&mut template)?;
    let template: BuiltinWorkflowTemplateBundle =
        serde_json::from_value(template).map_err(DomainError::Serialization)?;
    let mut bundle = template
        .build_bundle(input.project_id)
        .map_err(DomainError::InvalidConfig)?;
    let installed_source = installed_source_from_asset(&asset);
    for procedure in &mut bundle.procedures {
        procedure.source = DefinitionSource::UserAuthored;
        procedure.installed_source = Some(installed_source.clone());
    }

    let mut lifecycle = bundle.graph;
    lifecycle.source = DefinitionSource::UserAuthored;
    lifecycle.installed_source = Some(installed_source);
    let result = repos
        .workflow_template_install_repo
        .install_workflow_template_bundle(WorkflowTemplateInstallBundle {
            procedures: bundle.procedures,
            graph: lifecycle,
            overwrite: input.overwrite,
        })
        .await?;

    Ok(InstallLibraryAssetOutput::WorkflowTemplate {
        workflow_ids: result.procedures.iter().map(|p| p.id).collect(),
        lifecycle_id: result.graph.id,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use agentdash_domain::mcp_preset::{McpRoutePolicy, McpTransportConfig};
    use agentdash_domain::shared_library::{
        LibraryAssetScope, LibraryAssetSource, LibraryAssetType, seed_digest,
    };

    use super::*;

    #[test]
    fn mcp_template_install_options_create_project_preset_with_installed_source() {
        let asset = mcp_template_asset(json!({
            "transport_template": {
                "type": "http",
                "url_template": "https://mcp.example.com/${workspace}/mcp"
            },
            "route_policy": "direct",
            "parameter_schema": {
                "type": "object",
                "required": ["workspace"],
                "properties": {
                    "workspace": { "type": "string" }
                }
            },
            "capabilities": ["search"]
        }));
        let payload = match asset.typed_payload().expect("typed") {
            LibraryAssetPayload::McpServerTemplate(payload) => payload,
            other => panic!("unexpected payload: {other:?}"),
        };
        let parameters = json!({ "workspace": "acme" });

        let preset = mcp_preset_from_template_install(
            Uuid::new_v4(),
            Some("corp-search"),
            &asset,
            payload,
            Some(&parameters),
        )
        .expect("preset");

        assert_eq!(preset.key, "corp-search");
        assert_eq!(preset.route_policy, McpRoutePolicy::Direct);
        assert_eq!(
            preset.transport,
            McpTransportConfig::Http {
                url: "https://mcp.example.com/acme/mcp".to_string(),
                headers: vec![]
            }
        );
        let installed = preset.installed_source.expect("installed source");
        assert_eq!(installed.library_asset_id, asset.id);
        assert_eq!(
            installed.source_ref,
            "market:agentdash.dev.marketplace:mcp_server_template:workspace-http-mcp"
        );
        assert_eq!(installed.source_version, "0.1.0");
        assert_eq!(installed.source_digest, asset.payload_digest);
    }

    #[test]
    fn mcp_template_install_rejects_missing_required_parameter() {
        let asset = mcp_template_asset(json!({
            "transport_template": {
                "type": "sse",
                "url_template": "https://mcp.example.com/${workspace}/sse"
            },
            "parameter_schema": {
                "type": "object",
                "required": ["workspace"],
                "properties": {
                    "workspace": { "type": "string" }
                }
            }
        }));
        let payload = match asset.typed_payload().expect("typed") {
            LibraryAssetPayload::McpServerTemplate(payload) => payload,
            other => panic!("unexpected payload: {other:?}"),
        };
        let parameters = json!({});

        let err = mcp_preset_from_template_install(
            Uuid::new_v4(),
            None,
            &asset,
            payload,
            Some(&parameters),
        )
        .expect_err("missing parameter");

        assert!(err.to_string().contains("workspace"));
    }

    #[test]
    fn agent_dependency_parameters_merge_defaults_and_request_overrides() {
        let dependency = AgentMcpDependencyTemplate {
            slot_key: "abc-copilot-tool".to_string(),
            asset_key: "abc-copilot-tool".to_string(),
            target_key: None,
            display_name: None,
            required: true,
            parameters: Some(json!({
                "local_ip": "127.0.0.1",
                "port": 7321
            })),
        };
        let mut request_parameters = BTreeMap::new();
        request_parameters.insert(
            "abc-copilot-tool".to_string(),
            json!({
                "local_ip": "10.1.2.3"
            }),
        );

        let merged = merged_dependency_parameters(&dependency, &request_parameters)
            .expect("merged parameters")
            .expect("non-empty parameters");

        assert_eq!(
            merged,
            json!({
                "local_ip": "10.1.2.3",
                "port": 7321
            })
        );
    }

    #[test]
    fn agent_template_install_options_default_to_required_dependencies() {
        let options = agent_template_install_options(None).expect("default options");

        assert_eq!(
            options.dependency_mode,
            AgentTemplateDependencyMode::Required
        );
        assert!(options.dependency_parameters.is_empty());
    }

    fn mcp_template_asset(payload: serde_json::Value) -> LibraryAsset {
        LibraryAsset::new(
            LibraryAssetType::McpServerTemplate,
            LibraryAssetScope::User,
            Some("user-1".to_string()),
            "workspace-http-mcp".to_string(),
            "Workspace HTTP MCP".to_string(),
            Some("HTTP MCP template".to_string()),
            "0.1.0".to_string(),
            LibraryAssetSource::RemoteImported,
            Some(
                "market:agentdash.dev.marketplace:mcp_server_template:workspace-http-mcp"
                    .to_string(),
            ),
            seed_digest(&payload).expect("digest"),
            payload,
        )
        .expect("library asset")
    }
}
