use std::sync::Arc;

use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::extension_package::ExtensionPackageArtifactRepository;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::mcp_preset::McpPresetRepository;
use agentdash_domain::project_vfs_mount::ProjectVfsMountRepository;
use agentdash_domain::shared_library::{
    LibraryAssetRepository, ProjectExtensionInstallationRepository,
};
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::{
    AgentProcedureRepository, WorkflowGraphRepository, WorkflowTemplateInstallRepository,
};

#[derive(Clone)]
pub struct SharedLibraryRepositorySet {
    pub shared_library_repo: Arc<dyn LibraryAssetRepository>,
    pub extension_package_artifact_repo: Arc<dyn ExtensionPackageArtifactRepository>,
    pub project_extension_installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    pub mcp_preset_repo: Arc<dyn McpPresetRepository>,
    pub skill_asset_repo: Arc<dyn SkillAssetRepository>,
    pub project_agent_repo: Arc<dyn ProjectAgentRepository>,
    pub project_vfs_mount_repo: Arc<dyn ProjectVfsMountRepository>,
    pub agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    pub workflow_template_install_repo: Arc<dyn WorkflowTemplateInstallRepository>,
    pub workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    pub inline_file_repo: Arc<dyn InlineFileRepository>,
}
