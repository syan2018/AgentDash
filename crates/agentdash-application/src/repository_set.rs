use std::sync::Arc;

use agentdash_application_shared_library::SharedLibraryRepositorySet;
use agentdash_application_workflow::WorkflowRepositorySet;
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::auth_session::AuthSessionRepository;
use agentdash_domain::backend::{
    BackendExecutionLeaseRepository, BackendRepository, BackendWorkspaceInventoryRepository,
    ProjectBackendAccessRepository, RunnerRegistrationTokenRepository, RuntimeHealthRepository,
};
use agentdash_domain::canvas::{CanvasRepository, CanvasRuntimeStateRepository};
use agentdash_domain::extension_package::ExtensionPackageArtifactRepository;
use agentdash_domain::identity::UserDirectoryRepository;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::llm_provider::{LlmProviderCredentialRepository, LlmProviderRepository};
use agentdash_domain::mcp_preset::McpPresetRepository;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::project_vfs_mount::ProjectVfsMountRepository;
use agentdash_domain::routine::{RoutineExecutionRepository, RoutineRepository};
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::shared_library::{
    LibraryAssetRepository, ProjectExtensionInstallationRepository,
};
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, AgentProcedureRepository,
    AgentRunCommandReceiptRepository, AgentRunLineageRepository,
    GateResultDeliveryMarkerRepository, LifecycleAgentRepository, LifecycleGateRepository,
    LifecycleRunRepository, LifecycleSubjectAssociationRepository, WorkflowGraphRepository,
    WorkflowTemplateInstallRepository,
};
use agentdash_domain::workspace::WorkspaceRepository;

/// Product and platform repositories only.
///
/// Managed Runtime, Complete Agent Host, Product projection sagas and their effects are composed
/// through their owning concrete stores instead of being flattened into this business repository
/// bag.
#[derive(Clone)]
pub struct RepositorySet {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub canvas_repo: Arc<dyn CanvasRepository>,
    pub canvas_runtime_state_repo: Arc<dyn CanvasRuntimeStateRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    pub runtime_health_repo: Arc<dyn RuntimeHealthRepository>,
    pub backend_execution_lease_repo: Arc<dyn BackendExecutionLeaseRepository>,
    pub project_backend_access_repo: Arc<dyn ProjectBackendAccessRepository>,
    pub backend_workspace_inventory_repo: Arc<dyn BackendWorkspaceInventoryRepository>,
    pub runner_registration_token_repo: Arc<dyn RunnerRegistrationTokenRepository>,
    pub auth_session_repo: Arc<dyn AuthSessionRepository>,
    pub user_directory_repo: Arc<dyn UserDirectoryRepository>,
    pub settings_repo: Arc<dyn SettingsRepository>,
    pub shared_library_repo: Arc<dyn LibraryAssetRepository>,
    pub extension_package_artifact_repo: Arc<dyn ExtensionPackageArtifactRepository>,
    pub project_extension_installation_repo: Arc<dyn ProjectExtensionInstallationRepository>,
    pub llm_provider_repo: Arc<dyn LlmProviderRepository>,
    pub llm_provider_credential_repo: Arc<dyn LlmProviderCredentialRepository>,
    pub mcp_preset_repo: Arc<dyn McpPresetRepository>,
    pub skill_asset_repo: Arc<dyn SkillAssetRepository>,
    pub project_agent_repo: Arc<dyn ProjectAgentRepository>,
    pub project_vfs_mount_repo: Arc<dyn ProjectVfsMountRepository>,
    pub agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    pub workflow_template_install_repo: Arc<dyn WorkflowTemplateInstallRepository>,
    pub workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    pub gate_result_delivery_marker_repo: Arc<dyn GateResultDeliveryMarkerRepository>,
    pub agent_lineage_repo: Arc<dyn AgentLineageRepository>,
    pub agent_run_lineage_repo: Arc<dyn AgentRunLineageRepository>,
    pub agent_run_command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
    pub routine_repo: Arc<dyn RoutineRepository>,
    pub routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
    pub inline_file_repo: Arc<dyn InlineFileRepository>,
}

impl RepositorySet {
    pub fn to_workflow_repository_set(&self) -> WorkflowRepositorySet {
        WorkflowRepositorySet {
            lifecycle_run_repo: self.lifecycle_run_repo.clone(),
            lifecycle_gate_repo: self.lifecycle_gate_repo.clone(),
        }
    }

    pub fn to_shared_library_repository_set(&self) -> SharedLibraryRepositorySet {
        SharedLibraryRepositorySet {
            shared_library_repo: self.shared_library_repo.clone(),
            extension_package_artifact_repo: self.extension_package_artifact_repo.clone(),
            project_extension_installation_repo: self.project_extension_installation_repo.clone(),
            mcp_preset_repo: self.mcp_preset_repo.clone(),
            skill_asset_repo: self.skill_asset_repo.clone(),
            project_agent_repo: self.project_agent_repo.clone(),
            project_vfs_mount_repo: self.project_vfs_mount_repo.clone(),
            agent_procedure_repo: self.agent_procedure_repo.clone(),
            workflow_template_install_repo: self.workflow_template_install_repo.clone(),
            workflow_graph_repo: self.workflow_graph_repo.clone(),
            inline_file_repo: self.inline_file_repo.clone(),
        }
    }
}

impl agentdash_workspace_module::canvas::CanvasRepositorySet for RepositorySet {
    fn project_repo(&self) -> &dyn ProjectRepository {
        self.project_repo.as_ref()
    }

    fn canvas_repo(&self) -> &dyn CanvasRepository {
        self.canvas_repo.as_ref()
    }
}
