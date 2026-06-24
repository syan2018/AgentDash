use agentdash_application_ports::lifecycle_surface_projection as ports_lifecycle_surface;
use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxState;
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::agent_run::{
    AgentConversationSnapshotInput, AgentConversationSnapshotResolver, AgentFrameSurfaceExt,
    ConversationModelConfigInput, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, DeliveryRuntimeSelection, DeliveryRuntimeSelectionError,
    DeliveryRuntimeSelectionService, ValidationSeverityModel,
};
use crate::lifecycle::WorkflowApplicationError;
use crate::lifecycle::run_view_builder::{
    LifecycleSubjectAssociationView, RuntimeSessionRefView, build_lifecycle_run_view,
};
use crate::repository_set::RepositorySet;
use crate::session::{SessionCoreService, SessionExecutionState};
use crate::vfs::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource, VfsSurfaceRuntimeProjection,
    build_surface_summary,
};

use super::projection::{AgentRunWorkspaceProjection, is_terminal_agent_status};
use super::types::{
    AgentRunResourceSurfaceCoordinateModel, AgentRunResourceSurfaceSourceAnchorModel,
    AgentRunWorkspaceFrameRefModel, AgentRunWorkspaceFrameRuntimeModel,
    AgentRunWorkspaceMailboxStateModel, AgentRunWorkspaceProjectionInput,
    AgentRunWorkspaceQueryInput, AgentRunWorkspaceShellModel, AgentRunWorkspaceSnapshot,
    AgentRunWorkspaceTraceMetaModel, SubjectRefModel,
};

pub struct AgentRunWorkspaceQueryService<'a> {
    repos: &'a RepositorySet,
    session_core: SessionCoreService,
    session_control: crate::session::SessionControlService,
    vfs_runtime: &'a dyn VfsSurfaceRuntimeProjection,
    lifecycle_surface_projection: &'a dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort,
}

impl<'a> AgentRunWorkspaceQueryService<'a> {
    pub fn new(
        repos: &'a RepositorySet,
        session_core: SessionCoreService,
        session_control: crate::session::SessionControlService,
        vfs_runtime: &'a dyn VfsSurfaceRuntimeProjection,
        lifecycle_surface_projection: &'a dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort,
    ) -> Self {
        Self {
            repos,
            session_core,
            session_control,
            vfs_runtime,
            lifecycle_surface_projection,
        }
    }

    pub async fn resolve(
        &self,
        input: AgentRunWorkspaceQueryInput,
    ) -> Result<AgentRunWorkspaceSnapshot, WorkflowApplicationError> {
        let run = input.run;
        let agent = input.agent;
        let current_delivery = self.current_delivery_selection(&run, &agent).await?;
        let delivery_runtime_session_id = current_delivery
            .as_ref()
            .map(|selection| selection.runtime_session_id.clone());
        let meta = match delivery_runtime_session_id.as_deref() {
            Some(session_id) => self.session_core.get_session_meta(session_id).await?,
            None => None,
        };
        let frame_resolution = self
            .resolve_agent_run_frame_vfs(&run, &agent, current_delivery.as_ref())
            .await?;
        let frame = frame_resolution
            .as_ref()
            .map(|resolution| resolution.frame.clone());
        let frame_ref = frame.as_ref().map(|frame| (frame.id, frame.revision));
        let frame_execution_profile = frame
            .as_ref()
            .and_then(|frame| frame.typed_execution_profile());
        let resource_surface = match frame_resolution.as_ref() {
            Some(resolution) => {
                let source = ResolvedVfsSurfaceSource::AgentRun {
                    run_id: run.id,
                    agent_id: agent.id,
                };
                Some(
                    build_surface_summary(
                        self.repos.inline_file_repo.as_ref(),
                        self.vfs_runtime,
                        &source,
                        &resolution.vfs,
                    )
                    .await,
                )
            }
            None => None,
        };
        let resource_surface_coordinate = match (resource_surface.as_ref(), frame.as_ref()) {
            (Some(_), Some(frame)) => Some(resource_surface_coordinate_model(
                frame,
                current_delivery.as_ref(),
            )),
            _ => None,
        };
        let frame_runtime = match frame.as_ref() {
            Some(frame) => {
                let runtime_refs = self.runtime_refs_for_agent(agent.id).await?;
                Some(frame_runtime_model(frame, runtime_refs))
            }
            None => None,
        };
        let run_view = build_lifecycle_run_view(self.repos, &run)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let mut agent_view = run_view
            .agents
            .iter()
            .find(|view| view.agent_ref.agent_id == agent.id.to_string())
            .cloned();
        if let Some(agent_view) = agent_view.as_mut() {
            agent_view.delivery_runtime_ref = delivery_runtime_session_id
                .clone()
                .map(|runtime_session_id| RuntimeSessionRefView { runtime_session_id });
        }
        let subject_associations =
            filter_agent_subject_associations(run_view.subject_associations, agent.id);
        let execution_state = match delivery_runtime_session_id.as_deref() {
            Some(session_id) => {
                self.session_core
                    .inspect_session_execution_state(session_id)
                    .await?
            }
            None => SessionExecutionState::Idle,
        };
        let terminal_agent = is_terminal_agent_status(&agent.status);
        let supports_steering = match delivery_runtime_session_id.as_deref() {
            Some(session_id)
                if matches!(
                    execution_state,
                    SessionExecutionState::Running { turn_id: Some(_) }
                ) =>
            {
                self.session_control
                    .supports_session_steering(session_id)
                    .await
            }
            _ => false,
        };
        let projection = AgentRunWorkspaceProjection::derive(
            AgentRunWorkspaceProjectionInput::new(&execution_state, &agent.status),
        );

        let mailbox_messages = self
            .repos
            .agent_run_mailbox_repo
            .list_messages(run.id, agent.id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let mailbox_visible_message_count = mailbox_messages
            .iter()
            .filter(|message| mailbox_message_visible(message))
            .count();
        let mailbox_state = self
            .repos
            .agent_run_mailbox_repo
            .get_state(run.id, agent.id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let user_prefs = self
            .repos
            .backend_repo
            .get_preferences()
            .await
            .unwrap_or_default();
        let mailbox = mailbox_state_model(
            mailbox_state.as_ref(),
            delivery_runtime_session_id.is_some() && !terminal_agent,
            mailbox_visible_message_count,
            user_prefs.hide_system_steer_messages,
        );
        let visible_mailbox_messages = mailbox_messages
            .into_iter()
            .filter(mailbox_message_visible)
            .collect::<Vec<_>>();

        let project_agent = self.load_project_agent(&run, &agent).await?;
        let project_agent_preset_config = project_agent
            .as_ref()
            .map(|project_agent| {
                project_agent
                    .preset_config()
                    .map(|preset| preset.to_agent_config(&project_agent.agent_type))
            })
            .transpose()
            .map_err(WorkflowApplicationError::from)?;
        let model_config = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: project_agent_preset_config.as_ref(),
            frame_execution_profile: frame_execution_profile.as_ref(),
            ..Default::default()
        })
        .view;
        let resource_diagnostics =
            workspace_resource_diagnostics(run.id, resource_surface.as_ref());
        let conversation =
            AgentConversationSnapshotResolver::resolve(AgentConversationSnapshotInput {
                project_id: run.project_id,
                run_id: run.id,
                agent_id: agent.id,
                frame_ref,
                delivery_runtime_session_id: delivery_runtime_session_id.clone(),
                subject_associations: subject_associations.clone(),
                execution_state: execution_state.clone(),
                terminal_agent,
                supports_steering,
                mailbox_paused: mailbox.paused,
                mailbox_visible_message_count,
                resource_surface: resource_surface.clone(),
                resource_surface_coordinate: resource_surface_coordinate.clone(),
                resource_diagnostics,
                model_config,
            });
        let shell = shell_model(
            meta.as_ref(),
            project_agent.as_ref(),
            &agent,
            &projection.delivery_status,
            projection.last_turn_id.clone(),
        );
        let delivery_trace_meta = meta
            .as_ref()
            .map(AgentRunWorkspaceTraceMetaModel::from_session_meta);

        Ok(AgentRunWorkspaceSnapshot {
            run,
            agent,
            shell,
            delivery_runtime_session_id,
            delivery_trace_meta,
            projection,
            agent_view,
            frame_runtime,
            subject_associations,
            mailbox,
            mailbox_messages: visible_mailbox_messages,
            resource_surface,
            resource_surface_coordinate,
            conversation,
        })
    }

    /// 列表视图的轻量解析：只取标题 / 投递状态 / subject 归属，
    /// 跳过 vfs surface、run view、mailbox、conversation 等重量级解析。
    pub async fn resolve_list_projection(
        &self,
        input: AgentRunWorkspaceQueryInput,
    ) -> Result<super::types::AgentRunListProjection, WorkflowApplicationError> {
        let run = input.run;
        let agent = input.agent;
        let current_delivery = self.current_delivery_selection(&run, &agent).await?;
        let delivery_runtime_session_id = current_delivery
            .as_ref()
            .map(|selection| selection.runtime_session_id.clone());
        let meta = match delivery_runtime_session_id.as_deref() {
            Some(session_id) => self.session_core.get_session_meta(session_id).await?,
            None => None,
        };
        let execution_state = match delivery_runtime_session_id.as_deref() {
            Some(session_id) => {
                self.session_core
                    .inspect_session_execution_state(session_id)
                    .await?
            }
            None => SessionExecutionState::Idle,
        };
        let projection = AgentRunWorkspaceProjection::derive(
            AgentRunWorkspaceProjectionInput::new(&execution_state, &agent.status),
        );
        let project_agent = self.load_project_agent(&run, &agent).await?;
        let shell = shell_model(
            meta.as_ref(),
            project_agent.as_ref(),
            &agent,
            &projection.delivery_status,
            projection.last_turn_id.clone(),
        );
        let delivery_trace_meta = meta
            .as_ref()
            .map(AgentRunWorkspaceTraceMetaModel::from_session_meta);

        let association = self
            .repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, Some(agent.id))
            .await
            .map_err(WorkflowApplicationError::from)?
            .into_iter()
            .next();
        let subject_ref = association.as_ref().map(|assoc| SubjectRefModel {
            kind: assoc.subject_kind.clone(),
            id: assoc.subject_id.to_string(),
        });
        let subject_label = association
            .as_ref()
            .and_then(|assoc| subject_label_from_metadata(assoc.metadata_json.as_ref()));

        Ok(super::types::AgentRunListProjection {
            run,
            agent: agent.clone(),
            shell,
            project_agent_label: project_agent.as_ref().map(project_agent_display_label),
            delivery_runtime_session_id,
            delivery_trace_meta,
            subject_ref,
            subject_label,
        })
    }

    async fn current_delivery_selection(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
    ) -> Result<Option<DeliveryRuntimeSelection>, WorkflowApplicationError> {
        if agent.current_delivery.is_none() {
            return Ok(None);
        }
        DeliveryRuntimeSelectionService::from_repository_set(self.repos)
            .select_current_delivery(run.id, agent.id)
            .await
            .map(Some)
            .map_err(workflow_error_from_selection_error)
    }

    async fn resolve_agent_run_frame_vfs(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        current_delivery: Option<&DeliveryRuntimeSelection>,
    ) -> Result<Option<AgentRunFrameVfsResolution>, WorkflowApplicationError> {
        let frame = self
            .resolve_workspace_current_frame(agent, current_delivery)
            .await?;
        let Some(frame) = frame else {
            return Ok(None);
        };
        let vfs = match current_delivery {
            Some(selection) => self
                .lifecycle_surface_projection
                .project_lifecycle_surface(ports_lifecycle_surface::AgentRunLifecycleSurfaceInput {
                    base_vfs: frame.typed_vfs(),
                    address: selection.address.clone(),
                    message_stream: Some(selection.message_stream.clone()),
                    project_id: run.project_id,
                    mode:
                        ports_lifecycle_surface::AgentRunLifecycleSurfaceMode::WorkspaceReadSurface,
                    explicit_skill_asset_keys: Vec::new(),
                    builtin_skills:
                        ports_lifecycle_surface::BuiltinLifecycleSkillPolicy::PreserveProjected,
                    node_evidence: orchestration_node_evidence_from_anchor(&selection.anchor),
                    node_projection: None,
                })
                .await
                .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?
                .vfs,
            None => frame.typed_vfs().unwrap_or_else(empty_vfs),
        };

        Ok(Some(AgentRunFrameVfsResolution { frame, vfs }))
    }

    async fn resolve_workspace_current_frame(
        &self,
        agent: &LifecycleAgent,
        current_delivery: Option<&DeliveryRuntimeSelection>,
    ) -> Result<Option<AgentFrame>, WorkflowApplicationError> {
        if let Some(selection) = current_delivery {
            return self
                .repos
                .agent_frame_repo
                .get(selection.current_frame_id)
                .await
                .map_err(WorkflowApplicationError::from);
        }
        self.repos
            .agent_frame_repo
            .get_current(agent.id)
            .await
            .map_err(WorkflowApplicationError::from)
    }

    async fn runtime_refs_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<RuntimeSessionRefView>, WorkflowApplicationError> {
        Ok(self
            .repos
            .execution_anchor_repo
            .list_by_agent(agent_id)
            .await?
            .into_iter()
            .map(|anchor| RuntimeSessionRefView {
                runtime_session_id: anchor.runtime_session_id,
            })
            .collect())
    }

    async fn load_project_agent(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
    ) -> Result<Option<ProjectAgent>, WorkflowApplicationError> {
        let Some(project_agent_id) = agent.project_agent_id else {
            return Ok(None);
        };
        self.repos
            .project_agent_repo
            .get_by_project_and_id(run.project_id, project_agent_id)
            .await
            .map_err(WorkflowApplicationError::from)
    }
}

fn orchestration_node_evidence_from_anchor(
    anchor: &agentdash_domain::workflow::RuntimeSessionExecutionAnchor,
) -> Option<ports_lifecycle_surface::OrchestrationNodeEvidenceRef> {
    match (
        anchor.orchestration_id,
        anchor.node_path.as_ref(),
        anchor.node_attempt,
    ) {
        (Some(orchestration_id), Some(node_path), Some(attempt)) => {
            Some(ports_lifecycle_surface::OrchestrationNodeEvidenceRef {
                run_id: anchor.run_id,
                orchestration_id,
                node_path: node_path.clone(),
                attempt,
            })
        }
        _ => None,
    }
}

fn workflow_error_from_selection_error(
    error: DeliveryRuntimeSelectionError,
) -> WorkflowApplicationError {
    match error {
        DeliveryRuntimeSelectionError::RunNotFound { .. }
        | DeliveryRuntimeSelectionError::AgentNotFound { .. }
        | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::SubjectNotFound { .. } => {
            WorkflowApplicationError::NotFound(error.to_string())
        }
        DeliveryRuntimeSelectionError::Repository(source) => WorkflowApplicationError::from(source),
        other => WorkflowApplicationError::Conflict(other.to_string()),
    }
}

#[derive(Debug, Clone)]
struct AgentRunFrameVfsResolution {
    frame: AgentFrame,
    vfs: Vfs,
}

/// Project Agent 面向用户的显示名：优先 preset.display_name，回退 ProjectAgent.name。
/// 与 project_agent_context 的 display_name 解析同语义，仅依赖实体本地 config，无额外查询。
fn project_agent_display_label(project_agent: &ProjectAgent) -> String {
    project_agent
        .preset_config()
        .ok()
        .and_then(|preset| preset.display_name)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| project_agent.name.clone())
}

fn shell_model(
    meta: Option<&crate::session::SessionMeta>,
    project_agent: Option<&ProjectAgent>,
    agent: &LifecycleAgent,
    delivery_status: &str,
    last_turn_id: Option<String>,
) -> AgentRunWorkspaceShellModel {
    let (display_title, title_source) = match meta {
        Some(meta) => (meta.title.clone(), serialized_string(&meta.title_source)),
        None => (
            project_agent
                .map(|project_agent| project_agent.name.clone())
                .unwrap_or_else(|| format!("AgentRun {}", agent.id)),
            "agentrun_workspace".to_string(),
        ),
    };

    AgentRunWorkspaceShellModel {
        display_title,
        title_source,
        workspace_status: agent.status.clone(),
        delivery_status: delivery_status.to_string(),
        last_turn_id,
        last_activity_at: agent.updated_at.to_rfc3339(),
    }
}

/// 从 subject association 的 metadata 取可读 label（label/title/name 任一字符串）。
fn subject_label_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    let metadata = metadata?;
    ["label", "title", "name"]
        .iter()
        .find_map(|key| metadata.get(key).and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn frame_runtime_model(
    frame: &AgentFrame,
    runtime_session_refs: Vec<RuntimeSessionRefView>,
) -> AgentRunWorkspaceFrameRuntimeModel {
    AgentRunWorkspaceFrameRuntimeModel {
        frame_ref: AgentRunWorkspaceFrameRefModel {
            agent_id: frame.agent_id.to_string(),
            frame_id: frame.id.to_string(),
            revision: Some(frame.revision),
        },
        capability_surface: frame
            .effective_capability_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        context_slice: frame
            .context_slice_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        vfs_surface: frame
            .vfs_surface_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        mcp_surface: frame
            .mcp_surface_json
            .clone()
            .unwrap_or(serde_json::Value::Null),
        runtime_session_refs,
        execution_profile: frame.execution_profile_json.clone(),
        effective_executor_config: frame.typed_execution_profile().map(|config| {
            ConversationModelConfigResolver::view_for_config(
                &config,
                ConversationModelConfigSourceModel::FrameExecutionProfile,
            )
        }),
    }
}

fn resource_surface_coordinate_model(
    frame: &AgentFrame,
    current_delivery: Option<&DeliveryRuntimeSelection>,
) -> AgentRunResourceSurfaceCoordinateModel {
    AgentRunResourceSurfaceCoordinateModel {
        surface_frame_ref: AgentRunWorkspaceFrameRefModel {
            agent_id: frame.agent_id.to_string(),
            frame_id: frame.id.to_string(),
            revision: Some(frame.revision),
        },
        source_anchor: current_delivery.map(|selection| AgentRunResourceSurfaceSourceAnchorModel {
            runtime_session_id: selection.runtime_session_id.clone(),
            launch_frame_id: selection.anchor.launch_frame_id.to_string(),
            orchestration_id: selection.orchestration_id.map(|id| id.to_string()),
            node_path: selection.node_path.clone(),
            node_attempt: selection.node_attempt,
            delivery_status: selection.status.as_str().to_string(),
            observed_at: selection.observed_at.to_rfc3339(),
        }),
    }
}

fn filter_agent_subject_associations(
    associations: Vec<LifecycleSubjectAssociationView>,
    agent_id: Uuid,
) -> Vec<LifecycleSubjectAssociationView> {
    let agent_id = agent_id.to_string();
    associations
        .into_iter()
        .filter(|association| {
            association.anchor_agent_id.as_deref() == Some(agent_id.as_str())
                || association.anchor_agent_id.is_none()
        })
        .collect()
}

fn mailbox_state_model(
    state: Option<&AgentRunMailboxState>,
    can_resume: bool,
    visible_message_count: usize,
    hide_system_steer_messages: bool,
) -> AgentRunWorkspaceMailboxStateModel {
    let paused = state.is_some_and(|state| state.paused) && visible_message_count > 0;
    AgentRunWorkspaceMailboxStateModel {
        paused,
        pause_reason: state.and_then(|state| state.pause_reason.clone()),
        message: state.and_then(|state| state.pause_message.clone()),
        can_resume: can_resume && paused,
        hide_system_steer_messages,
    }
}

pub fn mailbox_message_visible(
    message: &agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
) -> bool {
    !matches!(
        message.status,
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Dispatched
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Steered
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Deleted
    )
}

fn workspace_resource_diagnostics(
    run_id: Uuid,
    resource_surface: Option<&ResolvedVfsSurface>,
) -> Vec<crate::agent_run::ConversationDiagnosticModel> {
    let has_lifecycle_mount = resource_surface
        .map(|surface| {
            surface
                .mounts
                .iter()
                .any(|mount| mount.id == "lifecycle" && mount.provider == "lifecycle_vfs")
        })
        .unwrap_or(false);
    if has_lifecycle_mount {
        return Vec::new();
    }

    vec![crate::agent_run::ConversationDiagnosticModel {
        code: "resource_surface_lifecycle_mount_missing".to_string(),
        severity: ValidationSeverityModel::Error,
        message: "当前 AgentRun workspace resource_surface 缺少 lifecycle_vfs mount。".to_string(),
        detail: Some(serde_json::json!({
            "run_id": run_id,
        })),
    }]
}

fn empty_vfs() -> Vfs {
    Vfs {
        mounts: Vec::new(),
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    }
}

fn serialized_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
