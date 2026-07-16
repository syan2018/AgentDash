use std::sync::Arc;

use agentdash_application_agentrun::agent_run::ProjectAgentLifecycleLaunchPort;
use agentdash_application_lifecycle::run_view_builder::LifecycleReadModelRepos;
use agentdash_application_lifecycle::{
    LifecycleDispatchFacade, LifecycleOrchestratorDeps, LifecycleRunCommandDeps,
    LifecycleWorkflowAgentNodeMaterializationAdapter,
    LifecycleWorkflowAgentNodeMaterializationDeps,
};
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_application_ports::agent_run_delete::AgentRunDeleteStore;
use agentdash_application_ports::agent_run_fork::AgentRunForkGraphStore;
use agentdash_application_ports::agent_run_message_submission::AgentRunMessageSubmissionStore;
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBindingRepository, AgentRunRuntimeProvisioner,
};
use agentdash_application_ports::hook_workflow_projection::{
    HookActiveWorkflowFacts, HookExecutionLogAppendCommand, HookWorkflowProjection,
    HookWorkflowProjectionError, HookWorkflowProjectionPort, HookWorkflowProjectionQuery,
};
use agentdash_application_ports::lifecycle_surface_projection as ports_lifecycle_surface;
use agentdash_application_ports::project_projection_notification::ProjectProjectionNotificationPort;
use agentdash_application_ports::workflow_agent_frame_materialization::WorkflowAgentNodeFrameMaterializationPort;
use agentdash_application_shared_library::SharedLibraryRepositorySet;
use agentdash_application_workflow::{OrchestrationExecutorLauncher, WorkflowRepositorySet};
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository;
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
use async_trait::async_trait;

use crate::wait_activity::WaitActivityRepositories;

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
    pub agent_run_fork_graph_store: Arc<dyn AgentRunForkGraphStore>,
    pub agent_run_delete_store: Arc<dyn AgentRunDeleteStore>,
    pub agent_run_command_receipt_repo: Arc<dyn AgentRunCommandReceiptRepository>,
    pub agent_run_message_submission_store: Arc<dyn AgentRunMessageSubmissionStore>,
    pub agent_run_runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub agent_run_runtime_provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
    pub workflow_agent_run_delivery:
        agentdash_application_ports::workflow_agent_run_delivery::SharedWorkflowAgentRunDeliveryHandle,
    pub agent_run_mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
    pub agent_frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    pub workflow_agent_frame_materialization: Arc<dyn WorkflowAgentNodeFrameMaterializationPort>,
    pub project_agent_lifecycle_launch: Arc<dyn ProjectAgentLifecycleLaunchPort>,
    pub routine_repo: Arc<dyn RoutineRepository>,
    pub routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
    pub inline_file_repo: Arc<dyn InlineFileRepository>,
    pub project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
}

impl RepositorySet {
    pub fn wait_activity_repositories(&self) -> WaitActivityRepositories {
        WaitActivityRepositories {
            lifecycle_agent_repo: self.lifecycle_agent_repo.clone(),
            agent_frame_repo: self.agent_frame_repo.clone(),
            agent_run_runtime_binding_repo: self.agent_run_runtime_binding_repo.clone(),
            lifecycle_gate_repo: self.lifecycle_gate_repo.clone(),
            mailbox_repo: self.agent_run_mailbox_repo.clone(),
        }
    }

    pub fn lifecycle_read_model_repos(&self) -> LifecycleReadModelRepos {
        LifecycleReadModelRepos {
            lifecycle_run_repo: self.lifecycle_run_repo.clone(),
            lifecycle_agent_repo: self.lifecycle_agent_repo.clone(),
            agent_frame_repo: self.agent_frame_repo.clone(),
            lifecycle_subject_association_repo: self.lifecycle_subject_association_repo.clone(),
            agent_lineage_repo: self.agent_lineage_repo.clone(),
            agent_run_runtime_binding_repo: self.agent_run_runtime_binding_repo.clone(),
        }
    }

    pub fn lifecycle_workflow_agent_node_materialization_deps(
        &self,
    ) -> LifecycleWorkflowAgentNodeMaterializationDeps {
        LifecycleWorkflowAgentNodeMaterializationDeps {
            run_repo: self.lifecycle_run_repo.clone(),
            workflow_graph_repo: self.workflow_graph_repo.clone(),
            agent_repo: self.lifecycle_agent_repo.clone(),
            frame_repo: self.agent_frame_repo.clone(),
            association_repo: self.lifecycle_subject_association_repo.clone(),
            gate_repo: self.lifecycle_gate_repo.clone(),
            lineage_repo: self.agent_lineage_repo.clone(),
            frame_construction: self.agent_frame_construction.clone(),
            workflow_agent_frame_materialization: self.workflow_agent_frame_materialization.clone(),
        }
    }

    pub fn lifecycle_orchestrator_deps(&self) -> LifecycleOrchestratorDeps {
        LifecycleOrchestratorDeps {
            run_repo: self.lifecycle_run_repo.clone(),
            agent_repo: self.lifecycle_agent_repo.clone(),
            frame_repo: self.agent_frame_repo.clone(),
            binding_repo: self.agent_run_runtime_binding_repo.clone(),
            inline_file_repo: self.inline_file_repo.clone(),
            orchestration_launcher: OrchestrationExecutorLauncher::new(
                self.to_workflow_repository_set(),
            ),
        }
    }

    pub fn lifecycle_run_command_deps(&self) -> LifecycleRunCommandDeps {
        LifecycleRunCommandDeps {
            run_repo: self.lifecycle_run_repo.clone(),
            workflow_graph_repo: self.workflow_graph_repo.clone(),
            agent_repo: self.lifecycle_agent_repo.clone(),
            frame_repo: self.agent_frame_repo.clone(),
            association_repo: self.lifecycle_subject_association_repo.clone(),
            gate_repo: self.lifecycle_gate_repo.clone(),
            lineage_repo: self.agent_lineage_repo.clone(),
            frame_construction: self.agent_frame_construction.clone(),
            orchestration_launcher: OrchestrationExecutorLauncher::new(
                self.to_workflow_repository_set(),
            ),
        }
    }

    pub fn to_workflow_repository_set(&self) -> WorkflowRepositorySet {
        WorkflowRepositorySet {
            lifecycle_run_repo: self.lifecycle_run_repo.clone(),
            agent_procedure_repo: self.agent_procedure_repo.clone(),
            lifecycle_gate_repo: self.lifecycle_gate_repo.clone(),
            workflow_agent_node_materialization: Arc::new(
                LifecycleWorkflowAgentNodeMaterializationAdapter::new(
                    self.lifecycle_workflow_agent_node_materialization_deps(),
                ),
            ),
            agent_run_runtime_provisioner: self.agent_run_runtime_provisioner.clone(),
            workflow_agent_run_delivery: Arc::new(self.workflow_agent_run_delivery.clone()),
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

    pub fn hook_workflow_projection_port(&self) -> Arc<dyn HookWorkflowProjectionPort> {
        Arc::new(ApplicationHookWorkflowProjectionAdapter {
            agent_procedure_repo: self.agent_procedure_repo.clone(),
            agent_frame_repo: self.agent_frame_repo.clone(),
            lifecycle_run_repo: self.lifecycle_run_repo.clone(),
            lifecycle_subject_association_repo: self.lifecycle_subject_association_repo.clone(),
            agent_run_runtime_binding_repo: self.agent_run_runtime_binding_repo.clone(),
            lifecycle_agent_repo: self.lifecycle_agent_repo.clone(),
            story_repo: self.story_repo.clone(),
            inline_file_repo: self.inline_file_repo.clone(),
        })
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
    frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
}

pub struct LifecycleProjectAgentLaunchDeps {
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub gate_repo: Arc<dyn LifecycleGateRepository>,
    pub lineage_repo: Arc<dyn AgentLineageRepository>,
    pub frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
}

impl LifecycleProjectAgentLaunchAdapter {
    pub fn new(deps: LifecycleProjectAgentLaunchDeps) -> Self {
        Self {
            run_repo: deps.run_repo,
            workflow_graph_repo: deps.workflow_graph_repo,
            agent_repo: deps.agent_repo,
            frame_repo: deps.frame_repo,
            association_repo: deps.association_repo,
            gate_repo: deps.gate_repo,
            lineage_repo: deps.lineage_repo,
            frame_construction: deps.frame_construction,
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

#[derive(Clone)]
struct ApplicationHookWorkflowProjectionAdapter {
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    agent_run_runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    story_repo: Arc<dyn StoryRepository>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
}

#[async_trait]
impl HookWorkflowProjectionPort for ApplicationHookWorkflowProjectionAdapter {
    async fn load_hook_workflow_projection(
        &self,
        query: HookWorkflowProjectionQuery,
    ) -> Result<HookWorkflowProjection, HookWorkflowProjectionError> {
        let workflow =
            agentdash_application_lifecycle::resolve_active_workflow_projection_for_target(
                &query.target,
                self.agent_procedure_repo.as_ref(),
                self.agent_frame_repo.as_ref(),
                self.lifecycle_run_repo.as_ref(),
            )
            .await
            .map_err(|message| HookWorkflowProjectionError::Projection { message })?;

        let Some(workflow) = workflow else {
            return Ok(HookWorkflowProjection {
                run_context: None,
                active_workflow: None,
            });
        };

        let run_context = agentdash_application_lifecycle::SubjectRunContextResolver::new(
            self.lifecycle_run_repo.as_ref(),
            self.lifecycle_subject_association_repo.as_ref(),
            self.agent_run_runtime_binding_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.story_repo.as_ref(),
        )
        .resolve_for_run(&workflow.run)
        .await
        .map_err(|error| HookWorkflowProjectionError::Projection {
            message: error.to_string(),
        })?;
        let artifact_scope = agentdash_application_lifecycle::RuntimeNodeArtifactScope {
            run_id: workflow.run.id,
            orchestration_id: workflow.orchestration_id,
            node_path: workflow.node_path.clone(),
            attempt: workflow.active_attempt.attempt,
        };
        let fulfilled_output_ports = agentdash_application_lifecycle::load_scoped_port_output_map(
            self.inline_file_repo.as_ref(),
            &artifact_scope,
        )
        .await;

        Ok(HookWorkflowProjection {
            run_context: Some(run_context),
            active_workflow: Some(HookActiveWorkflowFacts {
                projection: convert_lifecycle_active_workflow_projection(workflow),
                fulfilled_output_ports,
            }),
        })
    }

    async fn append_execution_log(
        &self,
        command: HookExecutionLogAppendCommand,
    ) -> Result<(), HookWorkflowProjectionError> {
        agentdash_application_lifecycle::lifecycle::execution_log::flush_execution_log_entries(
            self.lifecycle_run_repo.as_ref(),
            command.entries,
        )
        .await
        .map_err(|error| HookWorkflowProjectionError::Effect {
            message: error.to_string(),
        })
    }
}

fn convert_lifecycle_active_workflow_projection(
    workflow: agentdash_application_lifecycle::ActiveWorkflowProjection,
) -> ports_lifecycle_surface::ActiveWorkflowProjection {
    ports_lifecycle_surface::ActiveWorkflowProjection {
        run: workflow.run,
        orchestration_id: workflow.orchestration_id,
        node_path: workflow.node_path,
        lifecycle_graph_id: workflow.lifecycle_graph_id,
        lifecycle_key: workflow.lifecycle_key,
        lifecycle_name: workflow.lifecycle_name,
        active_activity: workflow.active_activity,
        active_attempt: workflow.active_attempt,
        active_node_type: workflow.active_node_type,
        active_procedure_key: workflow.active_procedure_key,
        snapshot_contract: workflow.snapshot_contract,
        primary_workflow: workflow.primary_workflow,
    }
}
