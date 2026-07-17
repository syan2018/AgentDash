use agentdash_application_agentrun::agent_run::{
    self as app_agent_run, project_capability_state_from_frame, workspace as app_workspace,
};
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_contracts::agent_run_mailbox::{MailboxMessageView, MailboxStateView};
use agentdash_contracts::workflow::{
    AgentConversationIdentity, AgentConversationLifecycleContext, AgentConversationSnapshot,
    AgentFrameRefDto, AgentFrameRuntimeView, AgentRunLineageRef, AgentRunOwnershipView,
    AgentRunRefDto, AgentRunResourceSurfaceCoordinateView, AgentRunResourceSurfaceSourceAnchorView,
    AgentRunView, AgentRunWorkspaceControlPlaneStatus, AgentRunWorkspaceControlPlaneView,
    AgentRunWorkspaceShell, AgentRunWorkspaceView, ConversationCommandKind,
    ConversationCommandPlacement, ConversationCommandSetView, ConversationCommandStaleGuardView,
    ConversationCommandView, ConversationDiagnosticView, ConversationEffectiveExecutorConfigView,
    ConversationExecutionStatus, ConversationExecutionView, ConversationKeyboardMapView,
    ConversationMailboxSnapshotView, ConversationModelConfigSource, ConversationModelConfigStatus,
    ConversationModelConfigView, ConversationWaitingItemView, LifecycleRunRefDto,
    LifecycleSubjectAssociationDto, RuntimeSessionRefDto, SubjectRefDto, ValidationSeverity,
};
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
use agentdash_workspace_module::workspace_module::{
    WorkspaceModuleVisibilityInput, project_agent_run_workspace_module_visibility,
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::project_authorization_context,
    routes::{
        lifecycle_agents::mailbox_message_contract, vfs_surfaces::dto as vfs_surface_dto,
        workspace_module::load_project_workspace_modules,
    },
    rpc::ApiError,
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

pub(crate) async fn load(
    state: &AppState,
    run: LifecycleRun,
    agent: LifecycleAgent,
    current_user: &agentdash_spi::AuthIdentity,
) -> Result<AgentRunWorkspaceView, ApiError> {
    let runtime_projection = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    let lifecycle_surface_projection = AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(
        state.repos.skill_asset_repo.clone(),
    );
    let service = app_workspace::AgentRunWorkspaceQueryService::new(
        app_workspace::AgentRunWorkspaceQueryDeps {
            delivery_selection_repos: app_agent_run::DeliveryRuntimeSelectionRepositories {
                lifecycle_runs: state.repos.lifecycle_run_repo.as_ref(),
                lifecycle_agents: state.repos.lifecycle_agent_repo.as_ref(),
                agent_frames: state.repos.agent_frame_repo.as_ref(),
                runtime_bindings: state.repos.agent_run_runtime_binding_repo.as_ref(),
            },
            agent_frame_repo: state.repos.agent_frame_repo.as_ref(),
            runtime_binding_repo: state.repos.agent_run_runtime_binding_repo.as_ref(),
            project_agent_repo: state.repos.project_agent_repo.as_ref(),
            agent_run_mailbox_repo: state.repos.agent_run_mailbox_repo.as_ref(),
            lifecycle_subject_association_repo: state
                .repos
                .lifecycle_subject_association_repo
                .as_ref(),
            lifecycle_gate_repo: state.repos.lifecycle_gate_repo.as_ref(),
            settings_repo: state.repos.settings_repo.as_ref(),
            inline_file_repo: state.repos.inline_file_repo.as_ref(),
        },
        state.services.agent_run_runtime.as_ref(),
        &runtime_projection,
        &lifecycle_surface_projection,
        state.services.lifecycle_read_model_query.as_ref(),
    );
    let snapshot = service
        .resolve(app_workspace::AgentRunWorkspaceQueryInput {
            run,
            agent,
            viewer_user_id: Some(current_user.user_id.clone()),
        })
        .await
        .map_err(ApiError::from)?;
    let workspace_modules = match snapshot.frame_runtime.as_ref() {
        Some(frame_runtime) => {
            let frame_id = Uuid::parse_str(&frame_runtime.frame_ref.frame_id).map_err(|_| {
                ApiError::Internal(format!(
                    "AgentRun workspace frame id is invalid: {}",
                    frame_runtime.frame_ref.frame_id
                ))
            })?;
            let frame = state
                .repos
                .agent_frame_repo
                .get(frame_id)
                .await?
                .ok_or_else(|| {
                    ApiError::NotFound(format!("AgentFrame `{frame_id}` does not exist"))
                })?;
            let capability_state = project_capability_state_from_frame(&frame);
            let runtime_vfs = capability_state.vfs.active.clone().unwrap_or_default();
            let project_modules = load_project_workspace_modules(
                state,
                &project_authorization_context(current_user),
                snapshot.run.project_id,
            )
            .await?;
            project_agent_run_workspace_module_visibility(
                project_modules,
                WorkspaceModuleVisibilityInput {
                    base_visibility: &capability_state.workspace_module,
                    runtime_vfs: &runtime_vfs,
                },
            )
            .modules
        }
        None => Vec::new(),
    };
    Ok(workspace_to_contract(snapshot, workspace_modules))
}

pub(crate) async fn resolve_lineage(
    state: &AppState,
    run: &LifecycleRun,
    agent: &LifecycleAgent,
) -> Result<(Option<AgentRunLineageRef>, Vec<AgentRunLineageRef>), ApiError> {
    let edges = state.repos.agent_lineage_repo.list_by_run(run.id).await?;
    let parent = match edges
        .iter()
        .find(|edge| edge.child_agent_id == agent.id)
        .and_then(|edge| {
            edge.parent_agent_id
                .map(|id| (id, edge.relation_kind.clone()))
        }) {
        Some((parent_id, relation_kind)) => {
            match state.repos.lifecycle_agent_repo.get(parent_id).await? {
                Some(parent) => Some(lineage_ref(state, &edges, parent, relation_kind).await?),
                None => None,
            }
        }
        None => None,
    };
    let mut children = Vec::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.parent_agent_id == Some(agent.id))
    {
        if let Some(child) = state
            .repos
            .lifecycle_agent_repo
            .get(edge.child_agent_id)
            .await?
        {
            children.push(lineage_ref(state, &edges, child, edge.relation_kind.clone()).await?);
        }
    }
    children.sort_by(|left, right| right.display_title.cmp(&left.display_title));
    Ok((parent, children))
}

async fn lineage_ref(
    state: &AppState,
    edges: &[agentdash_domain::workflow::AgentLineage],
    agent: LifecycleAgent,
    relation_kind: String,
) -> Result<AgentRunLineageRef, ApiError> {
    fn descendants(agent_id: Uuid, edges: &[agentdash_domain::workflow::AgentLineage]) -> u32 {
        edges
            .iter()
            .filter(|edge| edge.parent_agent_id == Some(agent_id))
            .map(|edge| 1 + descendants(edge.child_agent_id, edges))
            .sum()
    }
    let runtime = state
        .services
        .agent_run_runtime
        .inspect(AgentRunRuntimeTarget {
            run_id: agent.run_id,
            agent_id: agent.id,
        })
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let display_title = app_agent_run::resolve_agent_run_display_title(
        agent.workspace_title.as_deref(),
        agent.workspace_title_source.as_deref(),
        runtime
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.thread_name.as_deref()),
    )
    .value;
    Ok(AgentRunLineageRef {
        run_id: agent.run_id.to_string(),
        agent_id: agent.id.to_string(),
        source: agent.source.as_str().to_string(),
        relation_kind,
        display_title,
        subagent_count: descendants(agent.id, edges),
    })
}

fn workspace_to_contract(
    snapshot: app_workspace::AgentRunWorkspaceSnapshot,
    workspace_modules: Vec<agentdash_contracts::workspace_module::WorkspaceModuleDescriptor>,
) -> AgentRunWorkspaceView {
    let mailbox = mailbox_state_to_contract(snapshot.mailbox);
    let mailbox_messages = snapshot
        .mailbox_messages
        .into_iter()
        .map(mailbox_message_contract)
        .collect();
    let conversation =
        conversation_to_contract(snapshot.conversation, Some(mailbox), mailbox_messages);
    AgentRunWorkspaceView {
        run_ref: LifecycleRunRefDto {
            run_id: snapshot.run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: snapshot.run.id.to_string(),
            agent_id: snapshot.agent.id.to_string(),
        },
        project_id: snapshot.run.project_id.to_string(),
        shell: AgentRunWorkspaceShell {
            display_title: snapshot.shell.display_title,
            title_source: snapshot.shell.title_source,
            delivery_status: snapshot.shell.delivery_status,
            last_turn_id: snapshot.shell.last_turn_id,
            last_activity_at: snapshot.shell.last_activity_at,
        },
        control_plane: workspace_control_plane(&conversation),
        workspace_modules,
        agent: snapshot.agent_view.map(|agent| AgentRunView {
            agent_ref: AgentRunRefDto {
                run_id: agent.agent_ref.run_id,
                agent_id: agent.agent_ref.agent_id,
            },
            project_id: agent.project_id,
            source: agent.source,
            project_agent_id: agent.project_agent_id,
            status: agent.status,
            last_delivery_status: agent.last_delivery_status,
            created_at: agent.created_at,
            updated_at: agent.updated_at,
        }),
        frame_runtime: snapshot.frame_runtime.map(frame_runtime_to_contract),
        subject_associations: snapshot
            .subject_associations
            .into_iter()
            .map(subject_association_to_contract)
            .collect(),
        resource_surface: snapshot
            .resource_surface
            .map(vfs_surface_dto::surface_from_application),
        resource_surface_coordinate: snapshot
            .resource_surface_coordinate
            .map(resource_surface_coordinate_to_contract),
        conversation: Some(conversation),
        parent: None,
        children: Vec::new(),
    }
}

fn conversation_to_contract(
    conversation: app_agent_run::AgentConversationSnapshotModel,
    mailbox_state: Option<MailboxStateView>,
    mailbox_messages: Vec<MailboxMessageView>,
) -> AgentConversationSnapshot {
    AgentConversationSnapshot {
        snapshot_id: conversation.snapshot_id,
        identity: AgentConversationIdentity {
            run_ref: LifecycleRunRefDto {
                run_id: conversation.identity.run_id.clone(),
            },
            agent_ref: AgentRunRefDto {
                run_id: conversation.identity.run_id,
                agent_id: conversation.identity.agent_id,
            },
            project_id: conversation.identity.project_id,
        },
        lifecycle_context: AgentConversationLifecycleContext {
            frame_ref: conversation
                .lifecycle_context
                .frame_ref
                .map(|frame| AgentFrameRefDto {
                    agent_id: frame.agent_id,
                    frame_id: frame.frame_id,
                    revision: frame.revision,
                }),
            subject_associations: conversation
                .lifecycle_context
                .subject_associations
                .into_iter()
                .map(subject_association_to_contract)
                .collect(),
        },
        execution: execution_to_contract(conversation.execution),
        model_config: model_config_to_contract(conversation.model_config),
        commands: command_set_to_contract(conversation.commands),
        mailbox: ConversationMailboxSnapshotView {
            visible_message_count: conversation.mailbox.visible_message_count,
            paused: conversation.mailbox.paused,
            user_attention: conversation.mailbox.user_attention,
            resume_command: conversation.mailbox.resume_command.map(command_to_contract),
            state: mailbox_state,
            messages: mailbox_messages,
            waiting_items: conversation
                .mailbox
                .waiting_items
                .into_iter()
                .map(waiting_item_to_contract)
                .collect(),
        },
        resource_surface: conversation
            .resource_surface
            .map(vfs_surface_dto::surface_from_application),
        resource_surface_coordinate: conversation
            .resource_surface_coordinate
            .map(resource_surface_coordinate_to_contract),
        diagnostics: conversation
            .diagnostics
            .into_iter()
            .map(diagnostic_to_contract)
            .collect(),
    }
}

fn execution_to_contract(
    execution: app_agent_run::ConversationExecutionModel,
) -> ConversationExecutionView {
    ConversationExecutionView {
        status: match execution.status {
            app_agent_run::ConversationExecutionStatusModel::Draft => {
                ConversationExecutionStatus::Draft
            }
            app_agent_run::ConversationExecutionStatusModel::ModelRequired => {
                ConversationExecutionStatus::ModelRequired
            }
            app_agent_run::ConversationExecutionStatusModel::Ready => {
                ConversationExecutionStatus::Ready
            }
            app_agent_run::ConversationExecutionStatusModel::StartingClaimed => {
                ConversationExecutionStatus::StartingClaimed
            }
            app_agent_run::ConversationExecutionStatusModel::RunningActive => {
                ConversationExecutionStatus::RunningActive
            }
            app_agent_run::ConversationExecutionStatusModel::Cancelling => {
                ConversationExecutionStatus::Cancelling
            }
            app_agent_run::ConversationExecutionStatusModel::Terminal => {
                ConversationExecutionStatus::Terminal
            }
            app_agent_run::ConversationExecutionStatusModel::FrameMissing => {
                ConversationExecutionStatus::FrameMissing
            }
        },
        runtime_session_ref: execution
            .runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        active_turn_id: execution.active_turn_id,
        reason: execution.reason,
    }
}

fn model_config_to_contract(
    config: app_agent_run::ConversationModelConfigModel,
) -> ConversationModelConfigView {
    ConversationModelConfigView {
        status: match config.status {
            app_agent_run::ConversationModelConfigStatusModel::Resolved => {
                ConversationModelConfigStatus::Resolved
            }
            app_agent_run::ConversationModelConfigStatusModel::ModelRequired => {
                ConversationModelConfigStatus::ModelRequired
            }
        },
        effective_executor_config: config
            .effective_executor_config
            .map(effective_executor_config_to_contract),
        missing_fields: config.missing_fields,
        message: config.message,
    }
}

fn effective_executor_config_to_contract(
    config: app_agent_run::ConversationEffectiveExecutorConfigModel,
) -> ConversationEffectiveExecutorConfigView {
    ConversationEffectiveExecutorConfigView {
        executor: config.executor,
        provider_id: config.provider_id,
        model_id: config.model_id,
        agent_id: config.agent_id,
        thinking_level: config.thinking_level,
        source: match config.source {
            app_agent_run::ConversationModelConfigSourceModel::ProjectAgentPreset => {
                ConversationModelConfigSource::ProjectAgentPreset
            }
            app_agent_run::ConversationModelConfigSourceModel::FrameExecutionProfile => {
                ConversationModelConfigSource::FrameExecutionProfile
            }
            app_agent_run::ConversationModelConfigSourceModel::UserOverride => {
                ConversationModelConfigSource::UserOverride
            }
            app_agent_run::ConversationModelConfigSourceModel::ExecutorDiscoveryDefault => {
                ConversationModelConfigSource::ExecutorDiscoveryDefault
            }
            app_agent_run::ConversationModelConfigSourceModel::Unspecified => {
                ConversationModelConfigSource::Unspecified
            }
        },
    }
}

fn command_set_to_contract(
    commands: app_agent_run::ConversationCommandSetModel,
) -> ConversationCommandSetView {
    ConversationCommandSetView {
        ownership: ownership_to_contract(commands.ownership),
        commands: commands
            .commands
            .into_iter()
            .map(command_to_contract)
            .collect(),
        keyboard: ConversationKeyboardMapView {
            enter: commands.keyboard.enter,
            ctrl_enter: commands.keyboard.ctrl_enter,
        },
    }
}

fn ownership_to_contract(
    ownership: app_agent_run::AgentRunOwnershipModel,
) -> AgentRunOwnershipView {
    AgentRunOwnershipView {
        run_created_by_user_id: ownership.run_created_by_user_id,
        agent_created_by_user_id: ownership.agent_created_by_user_id,
        current_user_controls_run: ownership.current_user_controls_run,
    }
}

fn command_to_contract(
    command: app_agent_run::ConversationCommandModel,
) -> ConversationCommandView {
    ConversationCommandView {
        kind: match command.kind {
            app_agent_run::ConversationCommandKindModel::SubmitMessage => {
                ConversationCommandKind::SubmitMessage
            }
            app_agent_run::ConversationCommandKindModel::PromoteMailboxMessage => {
                ConversationCommandKind::PromoteMailboxMessage
            }
            app_agent_run::ConversationCommandKindModel::DeleteMailboxMessage => {
                ConversationCommandKind::DeleteMailboxMessage
            }
            app_agent_run::ConversationCommandKindModel::MoveMailboxMessage => {
                ConversationCommandKind::MoveMailboxMessage
            }
            app_agent_run::ConversationCommandKindModel::ResumeMailbox => {
                ConversationCommandKind::ResumeMailbox
            }
            app_agent_run::ConversationCommandKindModel::Cancel => ConversationCommandKind::Cancel,
            app_agent_run::ConversationCommandKindModel::CompactContext => {
                ConversationCommandKind::CompactContext
            }
        },
        command_id: command.command_id,
        enabled: command.enabled,
        unavailable_reason: command.unavailable_reason,
        disabled_code: command.disabled_code,
        shortcut: command.shortcut,
        requires_input: command.requires_input,
        executor_config_policy: command.executor_config_policy,
        placement: command
            .placement
            .into_iter()
            .map(|placement| match placement {
                app_agent_run::ConversationCommandPlacementModel::ComposerPrimary => {
                    ConversationCommandPlacement::ComposerPrimary
                }
                app_agent_run::ConversationCommandPlacementModel::ComposerSecondary => {
                    ConversationCommandPlacement::ComposerSecondary
                }
                app_agent_run::ConversationCommandPlacementModel::MailboxRow => {
                    ConversationCommandPlacement::MailboxRow
                }
                app_agent_run::ConversationCommandPlacementModel::MailboxBanner => {
                    ConversationCommandPlacement::MailboxBanner
                }
                app_agent_run::ConversationCommandPlacementModel::Header => {
                    ConversationCommandPlacement::Header
                }
            })
            .collect(),
        stale_guard: ConversationCommandStaleGuardView {
            snapshot_id: command.stale_guard.snapshot_id,
            run_id: command.stale_guard.run_id,
            agent_id: command.stale_guard.agent_id,
            frame_id: command.stale_guard.frame_id,
            active_turn_id: command.stale_guard.active_turn_id,
        },
    }
}

fn waiting_item_to_contract(
    item: app_agent_run::ConversationWaitingItemModel,
) -> ConversationWaitingItemView {
    ConversationWaitingItemView {
        wait_id: item.wait_id,
        gate_id: item.gate_id,
        kind: item.kind,
        source_ref: item.source_ref,
        correlation_ref: item.correlation_ref,
        status: item.status,
        source_label: item.source_label,
        preview: item.preview,
        created_at: item.created_at,
        resolved_at: item.resolved_at,
    }
}

fn diagnostic_to_contract(
    diagnostic: app_agent_run::ConversationDiagnosticModel,
) -> ConversationDiagnosticView {
    ConversationDiagnosticView {
        code: diagnostic.code,
        severity: match diagnostic.severity {
            app_agent_run::ValidationSeverityModel::Warning => ValidationSeverity::Warning,
            app_agent_run::ValidationSeverityModel::Error => ValidationSeverity::Error,
        },
        message: diagnostic.message,
        detail: diagnostic.detail,
    }
}

fn subject_association_to_contract(
    association: app_agent_run::PresentationLifecycleSubjectAssociationView,
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

fn resource_surface_coordinate_to_contract(
    coordinate: app_workspace::AgentRunResourceSurfaceCoordinateModel,
) -> AgentRunResourceSurfaceCoordinateView {
    AgentRunResourceSurfaceCoordinateView {
        surface_frame_ref: AgentFrameRefDto {
            agent_id: coordinate.surface_frame_ref.agent_id,
            frame_id: coordinate.surface_frame_ref.frame_id,
            revision: coordinate.surface_frame_ref.revision,
        },
        source_anchor: coordinate.source_anchor.map(|anchor| {
            AgentRunResourceSurfaceSourceAnchorView {
                runtime_session_ref: RuntimeSessionRefDto {
                    runtime_session_id: anchor.runtime_session_id,
                },
                launch_frame_id: anchor.launch_frame_id,
                orchestration_id: anchor.orchestration_id,
                node_path: anchor.node_path,
                node_attempt: anchor.node_attempt,
                delivery_status: anchor.delivery_status,
                observed_at: anchor.observed_at,
            }
        }),
    }
}

fn frame_runtime_to_contract(
    frame: app_workspace::AgentRunWorkspaceFrameRuntimeModel,
) -> AgentFrameRuntimeView {
    AgentFrameRuntimeView {
        frame_ref: AgentFrameRefDto {
            agent_id: frame.frame_ref.agent_id,
            frame_id: frame.frame_ref.frame_id,
            revision: frame.frame_ref.revision,
        },
        capability_surface: frame.capability_surface,
        context_slice: frame.context_slice,
        vfs_surface: frame.vfs_surface,
        mcp_surface: frame.mcp_surface,
        runtime_session_refs: frame
            .runtime_session_refs
            .into_iter()
            .map(|runtime_ref| RuntimeSessionRefDto {
                runtime_session_id: runtime_ref.runtime_session_id,
            })
            .collect(),
        execution_profile: frame.execution_profile,
        effective_executor_config: frame
            .effective_executor_config
            .map(effective_executor_config_to_contract),
    }
}

fn mailbox_state_to_contract(
    mailbox: app_workspace::AgentRunWorkspaceMailboxStateModel,
) -> MailboxStateView {
    MailboxStateView {
        paused: mailbox.paused,
        pause_reason: mailbox.pause_reason,
        message: mailbox.message,
        can_resume: mailbox.can_resume,
        hide_system_steer_messages: mailbox.hide_system_steer_messages,
    }
}

fn workspace_control_plane(
    conversation: &AgentConversationSnapshot,
) -> AgentRunWorkspaceControlPlaneView {
    let status = match conversation.execution.status {
        ConversationExecutionStatus::Draft
        | ConversationExecutionStatus::ModelRequired
        | ConversationExecutionStatus::Ready => AgentRunWorkspaceControlPlaneStatus::Ready,
        ConversationExecutionStatus::StartingClaimed
        | ConversationExecutionStatus::RunningActive => {
            AgentRunWorkspaceControlPlaneStatus::Running
        }
        ConversationExecutionStatus::Cancelling => AgentRunWorkspaceControlPlaneStatus::Cancelling,
        ConversationExecutionStatus::Terminal => AgentRunWorkspaceControlPlaneStatus::Terminal,
        ConversationExecutionStatus::FrameMissing => {
            AgentRunWorkspaceControlPlaneStatus::FrameMissing
        }
    };
    AgentRunWorkspaceControlPlaneView {
        status,
        reason: conversation.execution.reason.clone(),
        ownership: conversation.commands.ownership.clone(),
    }
}
