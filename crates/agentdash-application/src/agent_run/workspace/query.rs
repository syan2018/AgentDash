use agentdash_contracts::vfs as contract_vfs;
use agentdash_contracts::workflow::{
    ConversationDiagnosticView, ConversationModelConfigSource, LifecycleSubjectAssociationDto,
    SubjectRefDto, ValidationSeverity,
};
use agentdash_domain::agent::ProjectAgent;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxState;
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::session::{SessionCoreService, SessionExecutionState};
use crate::vfs::{
    ResolvedMountEditCapabilities, ResolvedMountPurpose, ResolvedMountSummary, ResolvedVfsSurface,
    ResolvedVfsSurfaceSource, VfsSurfaceRuntimeProjection, build_surface_summary,
};
use crate::lifecycle::run_view_builder::{
    LifecycleSubjectAssociationView, RuntimeSessionRefView, build_lifecycle_run_view,
};
use crate::agent_run::{
    AgentConversationSnapshotInput, AgentConversationSnapshotResolver, AgentFrameSurfaceExt,
    ConversationModelConfigInput, ConversationModelConfigResolver,
};
use crate::lifecycle::{
    AgentRunLifecycleSurfaceInput, AgentRunLifecycleSurfaceMode, AgentRunLifecycleSurfaceProjector,
    AgentRunRuntimeAddress, BuiltinLifecycleSkillPolicy, MessageStreamProjectionRef,
    MessageStreamTraceKind,
    WorkflowApplicationError,
};

use super::projection::{AgentRunWorkspaceProjection, is_terminal_agent_status};
use super::types::{
    AgentRunWorkspaceFrameRefModel, AgentRunWorkspaceFrameRuntimeModel,
    AgentRunWorkspaceMailboxStateModel, AgentRunWorkspaceProjectionInput,
    AgentRunWorkspaceQueryInput, AgentRunWorkspaceShellModel, AgentRunWorkspaceSnapshot,
    AgentRunWorkspaceTraceMetaModel,
};

pub struct AgentRunWorkspaceQueryService<'a> {
    repos: &'a RepositorySet,
    session_core: SessionCoreService,
    session_control: crate::session::SessionControlService,
    vfs_runtime: &'a dyn VfsSurfaceRuntimeProjection,
}

impl<'a> AgentRunWorkspaceQueryService<'a> {
    pub fn new(
        repos: &'a RepositorySet,
        session_core: SessionCoreService,
        session_control: crate::session::SessionControlService,
        vfs_runtime: &'a dyn VfsSurfaceRuntimeProjection,
    ) -> Self {
        Self {
            repos,
            session_core,
            session_control,
            vfs_runtime,
        }
    }

    pub async fn resolve(
        &self,
        input: AgentRunWorkspaceQueryInput,
    ) -> Result<AgentRunWorkspaceSnapshot, WorkflowApplicationError> {
        let run = input.run;
        let agent = input.agent;
        let delivery_runtime_session_id = self
            .delivery_runtime_session_for_agent_run(run.id, agent.id)
            .await?;
        let meta = match delivery_runtime_session_id.as_deref() {
            Some(session_id) => self.session_core.get_session_meta(session_id).await?,
            None => None,
        };
        let frame_resolution = self.resolve_agent_run_frame_vfs(&run, &agent).await?;
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
        let agent_view = run_view
            .agents
            .iter()
            .find(|view| view.agent_ref.agent_id == agent.id.to_string())
            .cloned();
        let subject_associations =
            filter_agent_subject_associations(run_view.subject_associations, agent.id);
        let subject_association_contracts = subject_associations
            .iter()
            .cloned()
            .map(subject_association_to_contract)
            .collect::<Vec<_>>();
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
        let resource_surface_contract = resource_surface.clone().map(resolved_surface_to_contract);
        let resource_diagnostics =
            workspace_resource_diagnostics(run.id, resource_surface.as_ref());
        let conversation =
            AgentConversationSnapshotResolver::resolve(AgentConversationSnapshotInput {
                project_id: run.project_id,
                run_id: run.id,
                agent_id: agent.id,
                frame_ref,
                delivery_runtime_session_id: delivery_runtime_session_id.clone(),
                subject_associations: subject_association_contracts,
                execution_state: execution_state.clone(),
                terminal_agent,
                supports_steering,
                mailbox_paused: mailbox.paused,
                mailbox_visible_message_count,
                resource_surface: resource_surface_contract,
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
        let delivery_runtime_session_id = self
            .delivery_runtime_session_for_agent_run(run.id, agent.id)
            .await?;
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
        let subject_ref = association.as_ref().map(|assoc| SubjectRefDto {
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

    async fn delivery_runtime_session_for_agent_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<String>, WorkflowApplicationError> {
        let anchors = self.repos.execution_anchor_repo.list_by_run(run_id).await?;
        Ok(anchors
            .into_iter()
            .filter(|anchor| anchor.agent_id == agent_id)
            .max_by_key(|anchor| anchor.updated_at)
            .map(|anchor| anchor.runtime_session_id))
    }

    async fn resolve_agent_run_frame_vfs(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
    ) -> Result<Option<AgentRunFrameVfsResolution>, WorkflowApplicationError> {
        let anchor = self
            .repos
            .execution_anchor_repo
            .list_by_run(run.id)
            .await?
            .into_iter()
            .filter(|anchor| anchor.agent_id == agent.id)
            .max_by_key(|anchor| anchor.updated_at);
        let anchor_frame_id = anchor.as_ref().map(|anchor| anchor.launch_frame_id);
        let current_frame = self.repos.agent_frame_repo.get_current(agent.id).await?;
        let frame = match (current_frame, anchor_frame_id) {
            (Some(frame), _) => Some(frame),
            (None, Some(frame_id)) => self.repos.agent_frame_repo.get(frame_id).await?,
            (None, None) => None,
        };
        let Some(frame) = frame else {
            return Ok(None);
        };
        let vfs = match anchor.as_ref() {
            Some(anchor) => {
                AgentRunLifecycleSurfaceProjector::new(self.repos)
                    .project(AgentRunLifecycleSurfaceInput {
                        base_vfs: frame.typed_vfs(),
                        address: AgentRunRuntimeAddress {
                            run_id: anchor.run_id,
                            agent_id: anchor.agent_id,
                            frame_id: anchor.launch_frame_id,
                        },
                        message_stream: Some(MessageStreamProjectionRef {
                            runtime_session_id: anchor.runtime_session_id.clone(),
                            trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
                        }),
                        project_id: run.project_id,
                        mode: AgentRunLifecycleSurfaceMode::WorkspaceReadSurface,
                        explicit_skill_asset_keys: Vec::new(),
                        builtin_skills: BuiltinLifecycleSkillPolicy::PreserveProjected,
                        node_projection: None,
                    })
                    .await
                    .map_err(WorkflowApplicationError::Internal)?
                    .vfs
            }
            None => frame.typed_vfs().unwrap_or_else(empty_vfs),
        };

        Ok(Some(AgentRunFrameVfsResolution { frame, vfs }))
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

#[derive(Debug, Clone)]
struct AgentRunFrameVfsResolution {
    frame: AgentFrame,
    vfs: Vfs,
}

/// Project Agent 面向用户的显示名：优先 preset.display_name，回退 ProjectAgent.name。
/// 与 construction_planner 的 display_name 解析同语义，仅依赖实体本地 config，无额外查询。
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
                ConversationModelConfigSource::FrameExecutionProfile,
            )
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
) -> Vec<ConversationDiagnosticView> {
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

    vec![ConversationDiagnosticView {
        code: "resource_surface_lifecycle_mount_missing".to_string(),
        severity: ValidationSeverity::Error,
        message: "当前 AgentRun workspace resource_surface 缺少 lifecycle_vfs mount。".to_string(),
        detail: Some(serde_json::json!({
            "run_id": run_id,
        })),
    }]
}

fn subject_association_to_contract(
    association: LifecycleSubjectAssociationView,
) -> LifecycleSubjectAssociationDto {
    LifecycleSubjectAssociationDto {
        id: association.id,
        anchor_run_id: association.anchor_run_id,
        anchor_agent_id: association.anchor_agent_id,
        subject_ref: SubjectRefDto {
            kind: association.subject_ref.kind,
            id: association.subject_ref.id,
        },
        role: association.role,
        metadata: association.metadata,
        created_at: association.created_at,
    }
}

fn resolved_surface_to_contract(surface: ResolvedVfsSurface) -> contract_vfs::ResolvedVfsSurface {
    contract_vfs::ResolvedVfsSurface {
        surface_ref: surface.surface_ref,
        source: surface_source_to_contract(surface.source),
        mounts: surface
            .mounts
            .into_iter()
            .map(mount_summary_to_contract)
            .collect(),
        default_mount_id: surface.default_mount_id,
    }
}

fn surface_source_to_contract(
    source: ResolvedVfsSurfaceSource,
) -> contract_vfs::ResolvedVfsSurfaceSource {
    match source {
        ResolvedVfsSurfaceSource::ProjectPreview { project_id } => {
            contract_vfs::ResolvedVfsSurfaceSource::ProjectPreview {
                project_id: project_id.to_string(),
            }
        }
        ResolvedVfsSurfaceSource::StoryPreview {
            project_id,
            story_id,
        } => contract_vfs::ResolvedVfsSurfaceSource::StoryPreview {
            project_id: project_id.to_string(),
            story_id: story_id.to_string(),
        },
        ResolvedVfsSurfaceSource::TaskPreview {
            project_id,
            task_id,
        } => contract_vfs::ResolvedVfsSurfaceSource::TaskPreview {
            project_id: project_id.to_string(),
            task_id: task_id.to_string(),
        },
        ResolvedVfsSurfaceSource::SessionRuntime { session_id } => {
            contract_vfs::ResolvedVfsSurfaceSource::SessionRuntime { session_id }
        }
        ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id } => {
            contract_vfs::ResolvedVfsSurfaceSource::AgentRun {
                run_id: run_id.to_string(),
                agent_id: agent_id.to_string(),
            }
        }
        ResolvedVfsSurfaceSource::ProjectSkillAssets { project_id } => {
            contract_vfs::ResolvedVfsSurfaceSource::ProjectSkillAssets {
                project_id: project_id.to_string(),
            }
        }
        ResolvedVfsSurfaceSource::ProjectVfsMount {
            project_id,
            mount_id,
        } => contract_vfs::ResolvedVfsSurfaceSource::ProjectVfsMount {
            project_id: project_id.to_string(),
            mount_id,
        },
        ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
            project_id,
            project_agent_id,
        } => contract_vfs::ResolvedVfsSurfaceSource::ProjectAgentKnowledge {
            project_id: project_id.to_string(),
            project_agent_id: project_agent_id.to_string(),
        },
    }
}

fn mount_summary_to_contract(mount: ResolvedMountSummary) -> contract_vfs::ResolvedMountSummary {
    contract_vfs::ResolvedMountSummary {
        id: mount.id,
        display_name: mount.display_name,
        provider: mount.provider,
        backend_id: mount.backend_id,
        capabilities: mount.capabilities,
        default_write: mount.default_write,
        purpose: mount_purpose_to_contract(mount.purpose),
        backend_online: mount.backend_online,
        file_count: mount.file_count,
        edit_capabilities: mount_edit_capabilities_to_contract(mount.edit_capabilities),
    }
}

fn mount_purpose_to_contract(purpose: ResolvedMountPurpose) -> contract_vfs::ResolvedMountPurpose {
    match purpose {
        ResolvedMountPurpose::Workspace => contract_vfs::ResolvedMountPurpose::Workspace,
        ResolvedMountPurpose::ProjectContainer => {
            contract_vfs::ResolvedMountPurpose::ProjectContainer
        }
        ResolvedMountPurpose::VfsMount => contract_vfs::ResolvedMountPurpose::VfsMount,
        ResolvedMountPurpose::StoryContainer => contract_vfs::ResolvedMountPurpose::StoryContainer,
        ResolvedMountPurpose::AgentKnowledge => contract_vfs::ResolvedMountPurpose::AgentKnowledge,
        ResolvedMountPurpose::Lifecycle => contract_vfs::ResolvedMountPurpose::Lifecycle,
        ResolvedMountPurpose::Canvas => contract_vfs::ResolvedMountPurpose::Canvas,
        ResolvedMountPurpose::ExternalService => {
            contract_vfs::ResolvedMountPurpose::ExternalService
        }
    }
}

fn mount_edit_capabilities_to_contract(
    capabilities: ResolvedMountEditCapabilities,
) -> contract_vfs::ResolvedMountEditCapabilities {
    contract_vfs::ResolvedMountEditCapabilities {
        create: capabilities.create,
        delete: capabilities.delete,
        rename: capabilities.rename,
    }
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
