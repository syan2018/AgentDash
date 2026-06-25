use std::sync::Arc;

use agentdash_application_agentrun::agent_run::ProjectAgentLifecycleLaunchPort;
use agentdash_application_lifecycle::LifecycleDispatchFacade;
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_application_ports::runtime_session_delivery::RuntimeSessionCreationPort;
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository;
use agentdash_domain::auth_session::AuthSessionRepository;
use agentdash_domain::backend::{
    BackendExecutionLeaseRepository, BackendRepository, BackendWorkspaceInventoryRepository,
    ProjectBackendAccessRepository, RuntimeHealthRepository,
};
use agentdash_domain::canvas::{CanvasRepository, CanvasRuntimeStateRepository};
use agentdash_domain::extension_package::ExtensionPackageArtifactRepository;
use agentdash_domain::identity::UserDirectoryRepository;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::llm_provider::{LlmProviderCredentialRepository, LlmProviderRepository};
use agentdash_domain::mcp_preset::McpPresetRepository;
use agentdash_domain::permission::PermissionGrantRepository;
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
    AgentRunCommandReceiptRepository, LifecycleAgentRepository, LifecycleGateRepository,
    LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    RuntimeSessionExecutionAnchorRepository, WorkflowGraphRepository,
    WorkflowTemplateInstallRepository,
};
use agentdash_domain::workspace::WorkspaceRepository;
use async_trait::async_trait;

/// 持久化层端口 — 所有 Repository trait 对象的集合
///
/// 在 application 层定义，使 gateway / service 可直接持有仓储引用，
/// 无需依赖 api 层的 `AppState`。
///
/// Task plan facts live in `LifecycleRun.tasks`; Story repository only owns Story topic facts.
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
    pub agent_lineage_repo: Arc<dyn AgentLineageRepository>,
    pub execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub agent_run_command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
    pub agent_run_mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
    pub runtime_session_creator: Arc<dyn RuntimeSessionCreationPort>,
    pub agent_frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    pub project_agent_lifecycle_launch: Arc<dyn ProjectAgentLifecycleLaunchPort>,
    pub routine_repo: Arc<dyn RoutineRepository>,
    pub routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
    pub inline_file_repo: Arc<dyn InlineFileRepository>,
    pub permission_grant_repo: Arc<dyn PermissionGrantRepository>,
}

impl RepositorySet {
    pub fn to_agent_run_repository_set(
        &self,
    ) -> agentdash_application_agentrun::AgentRunRepositorySet {
        agentdash_application_agentrun::AgentRunRepositorySet {
            project_repo: self.project_repo.clone(),
            canvas_repo: self.canvas_repo.clone(),
            canvas_runtime_state_repo: self.canvas_runtime_state_repo.clone(),
            workspace_repo: self.workspace_repo.clone(),
            story_repo: self.story_repo.clone(),
            state_change_repo: self.state_change_repo.clone(),
            backend_repo: self.backend_repo.clone(),
            runtime_health_repo: self.runtime_health_repo.clone(),
            backend_execution_lease_repo: self.backend_execution_lease_repo.clone(),
            project_backend_access_repo: self.project_backend_access_repo.clone(),
            backend_workspace_inventory_repo: self.backend_workspace_inventory_repo.clone(),
            auth_session_repo: self.auth_session_repo.clone(),
            user_directory_repo: self.user_directory_repo.clone(),
            settings_repo: self.settings_repo.clone(),
            shared_library_repo: self.shared_library_repo.clone(),
            extension_package_artifact_repo: self.extension_package_artifact_repo.clone(),
            project_extension_installation_repo: self.project_extension_installation_repo.clone(),
            llm_provider_repo: self.llm_provider_repo.clone(),
            llm_provider_credential_repo: self.llm_provider_credential_repo.clone(),
            mcp_preset_repo: self.mcp_preset_repo.clone(),
            skill_asset_repo: self.skill_asset_repo.clone(),
            project_agent_repo: self.project_agent_repo.clone(),
            project_vfs_mount_repo: self.project_vfs_mount_repo.clone(),
            agent_procedure_repo: self.agent_procedure_repo.clone(),
            workflow_template_install_repo: self.workflow_template_install_repo.clone(),
            workflow_graph_repo: self.workflow_graph_repo.clone(),
            lifecycle_run_repo: self.lifecycle_run_repo.clone(),
            lifecycle_agent_repo: self.lifecycle_agent_repo.clone(),
            agent_frame_repo: self.agent_frame_repo.clone(),
            lifecycle_subject_association_repo: self.lifecycle_subject_association_repo.clone(),
            lifecycle_gate_repo: self.lifecycle_gate_repo.clone(),
            agent_lineage_repo: self.agent_lineage_repo.clone(),
            execution_anchor_repo: self.execution_anchor_repo.clone(),
            agent_run_command_receipt_repo: self.agent_run_command_receipt_repo.clone(),
            agent_run_mailbox_repo: self.agent_run_mailbox_repo.clone(),
            runtime_session_creator: self.runtime_session_creator.clone(),
            agent_frame_construction: self.agent_frame_construction.clone(),
            project_agent_lifecycle_launch: self.project_agent_lifecycle_launch.clone(),
            routine_repo: self.routine_repo.clone(),
            routine_execution_repo: self.routine_execution_repo.clone(),
            inline_file_repo: self.inline_file_repo.clone(),
            permission_grant_repo: self.permission_grant_repo.clone(),
        }
    }

    pub fn to_lifecycle_repository_set(&self) -> agentdash_application_lifecycle::RepositorySet {
        agentdash_application_lifecycle::RepositorySet {
            project_repo: self.project_repo.clone(),
            canvas_repo: self.canvas_repo.clone(),
            workspace_repo: self.workspace_repo.clone(),
            story_repo: self.story_repo.clone(),
            state_change_repo: self.state_change_repo.clone(),
            backend_repo: self.backend_repo.clone(),
            runtime_health_repo: self.runtime_health_repo.clone(),
            backend_execution_lease_repo: self.backend_execution_lease_repo.clone(),
            project_backend_access_repo: self.project_backend_access_repo.clone(),
            backend_workspace_inventory_repo: self.backend_workspace_inventory_repo.clone(),
            auth_session_repo: self.auth_session_repo.clone(),
            user_directory_repo: self.user_directory_repo.clone(),
            settings_repo: self.settings_repo.clone(),
            shared_library_repo: self.shared_library_repo.clone(),
            extension_package_artifact_repo: self.extension_package_artifact_repo.clone(),
            project_extension_installation_repo: self.project_extension_installation_repo.clone(),
            llm_provider_repo: self.llm_provider_repo.clone(),
            llm_provider_credential_repo: self.llm_provider_credential_repo.clone(),
            mcp_preset_repo: self.mcp_preset_repo.clone(),
            skill_asset_repo: self.skill_asset_repo.clone(),
            project_agent_repo: self.project_agent_repo.clone(),
            project_vfs_mount_repo: self.project_vfs_mount_repo.clone(),
            agent_procedure_repo: self.agent_procedure_repo.clone(),
            workflow_template_install_repo: self.workflow_template_install_repo.clone(),
            workflow_graph_repo: self.workflow_graph_repo.clone(),
            lifecycle_run_repo: self.lifecycle_run_repo.clone(),
            lifecycle_agent_repo: self.lifecycle_agent_repo.clone(),
            agent_frame_repo: self.agent_frame_repo.clone(),
            lifecycle_subject_association_repo: self.lifecycle_subject_association_repo.clone(),
            lifecycle_gate_repo: self.lifecycle_gate_repo.clone(),
            agent_lineage_repo: self.agent_lineage_repo.clone(),
            execution_anchor_repo: self.execution_anchor_repo.clone(),
            agent_run_command_receipt_repo: self.agent_run_command_receipt_repo.clone(),
            agent_run_mailbox_repo: self.agent_run_mailbox_repo.clone(),
            runtime_session_creator: self.runtime_session_creator.clone(),
            agent_frame_construction: self.agent_frame_construction.clone(),
            routine_repo: self.routine_repo.clone(),
            routine_execution_repo: self.routine_execution_repo.clone(),
            inline_file_repo: self.inline_file_repo.clone(),
            permission_grant_repo: self.permission_grant_repo.clone(),
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

pub struct LifecycleProjectAgentLaunchAdapter {
    run_repo: Arc<dyn LifecycleRunRepository>,
    workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    gate_repo: Arc<dyn LifecycleGateRepository>,
    lineage_repo: Arc<dyn AgentLineageRepository>,
    anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    runtime_session_creator: Arc<dyn RuntimeSessionCreationPort>,
    frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
}

impl LifecycleProjectAgentLaunchAdapter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_repo: Arc<dyn LifecycleRunRepository>,
        workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
        association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
        gate_repo: Arc<dyn LifecycleGateRepository>,
        lineage_repo: Arc<dyn AgentLineageRepository>,
        anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        runtime_session_creator: Arc<dyn RuntimeSessionCreationPort>,
        frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    ) -> Self {
        Self {
            run_repo,
            workflow_graph_repo,
            agent_repo,
            frame_repo,
            association_repo,
            gate_repo,
            lineage_repo,
            anchor_repo,
            runtime_session_creator,
            frame_construction,
        }
    }
}

#[async_trait]
impl ProjectAgentLifecycleLaunchPort for LifecycleProjectAgentLaunchAdapter {
    async fn launch_project_agent(
        &self,
        intent: &agentdash_domain::workflow::AgentLaunchIntent,
    ) -> Result<
        agentdash_domain::workflow::AgentLaunchDispatchResult,
        agentdash_application_agentrun::WorkflowApplicationError,
    > {
        let facade = LifecycleDispatchFacade::new(
            self.run_repo.as_ref(),
            self.workflow_graph_repo.as_ref(),
            self.agent_repo.as_ref(),
            self.frame_repo.as_ref(),
            self.association_repo.as_ref(),
            self.gate_repo.as_ref(),
            self.lineage_repo.as_ref(),
            self.anchor_repo.as_ref(),
            self.runtime_session_creator.as_ref(),
            self.frame_construction.as_ref(),
        );
        facade
            .launch_agent(intent)
            .await
            .map_err(workflow_error_from_lifecycle)
    }
}

fn workflow_error_from_lifecycle(
    error: agentdash_application_lifecycle::WorkflowApplicationError,
) -> agentdash_application_agentrun::WorkflowApplicationError {
    match error {
        agentdash_application_lifecycle::WorkflowApplicationError::BadRequest(message) => {
            agentdash_application_agentrun::WorkflowApplicationError::BadRequest(message)
        }
        agentdash_application_lifecycle::WorkflowApplicationError::ModelRequired(message) => {
            agentdash_application_agentrun::WorkflowApplicationError::ModelRequired(message)
        }
        agentdash_application_lifecycle::WorkflowApplicationError::NotFound(message) => {
            agentdash_application_agentrun::WorkflowApplicationError::NotFound(message)
        }
        agentdash_application_lifecycle::WorkflowApplicationError::Conflict(message) => {
            agentdash_application_agentrun::WorkflowApplicationError::Conflict(message)
        }
        agentdash_application_lifecycle::WorkflowApplicationError::Internal(message) => {
            agentdash_application_agentrun::WorkflowApplicationError::Internal(message)
        }
    }
}
