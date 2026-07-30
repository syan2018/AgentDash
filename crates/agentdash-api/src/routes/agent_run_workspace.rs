use agentdash_application::execution_authority::{
    ExecutionAuthorityRequest, ExecutionAuthorityResolveError,
};
use agentdash_application_agentrun::agent_run::{
    self as app_agent_run, workspace as app_workspace,
};
use agentdash_contracts::workflow::{
    AgentConversationIdentity, AgentConversationLifecycleContext, AgentConversationSnapshot,
    AgentFrameRefDto, AgentFrameRuntimeView, AgentRunLineageRef, AgentRunOwnershipView,
    AgentRunRefDto, AgentRunResourceSurfaceCoordinateView, AgentRunResourceSurfaceSourceAnchorView,
    AgentRunView, AgentRunWorkspaceShell, AgentRunWorkspaceView, ConversationCommandKind,
    ConversationCommandPlacement, ConversationCommandSetView, ConversationCommandView,
    ConversationDiagnosticView, ConversationEffectiveExecutorConfigView,
    ConversationKeyboardMapView, ConversationModelConfigSource, ConversationModelConfigStatus,
    ConversationModelConfigView, ConversationWaitingItemView, LifecycleRunRefDto,
    LifecycleSubjectAssociationDto, RuntimeThreadRefDto, SubjectRefDto, ValidationSeverity,
};
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    routes::{
        vfs_surfaces::dto as vfs_surface_dto,
        workspace_module::load_agent_run_workspace_module_surface,
    },
    rpc::ApiError,
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

pub(crate) async fn load(
    state: &AppState,
    run: LifecycleRun,
    agent: LifecycleAgent,
    current_user: &agentdash_platform_spi::AuthIdentity,
) -> Result<AgentRunWorkspaceView, ApiError> {
    let target = agentdash_domain::agent_run_target::AgentRunTarget {
        run_id: run.id,
        agent_id: agent.id,
    };
    let agent_owner_user_id = agent.created_by_user_id.clone();
    let execution_authority = match state
        .services
        .execution_authorities
        .resolve(ExecutionAuthorityRequest::for_target(target.clone()))
        .await
    {
        Ok(authority) => Some(authority),
        Err(ExecutionAuthorityResolveError::BindingMissing) => None,
        Err(error) => {
            return Err(ApiError::Conflict(format!("{}: {error}", error.code())));
        }
    };
    let runtime_projection = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    let service = app_workspace::AgentRunWorkspaceQueryService::new(
        app_workspace::AgentRunWorkspaceQueryDeps {
            product_projection: state.services.agent_run_product_projection.as_ref(),
            agent_frame_repo: state.repos.agent_frame_repo.as_ref(),
            project_agent_repo: state.repos.project_agent_repo.as_ref(),
            lifecycle_subject_association_repo: state
                .repos
                .lifecycle_subject_association_repo
                .as_ref(),
            lifecycle_gate_repo: state.repos.lifecycle_gate_repo.as_ref(),
            inline_file_repo: state.repos.inline_file_repo.as_ref(),
        },
        &runtime_projection,
    );
    let snapshot = service
        .resolve(app_workspace::AgentRunWorkspaceQueryInput {
            run,
            agent,
            viewer_user_id: Some(current_user.user_id.clone()),
            execution_authority_vfs: execution_authority
                .as_ref()
                .map(|authority| authority.resources().vfs(authority.project_id())),
        })
        .await
        .map_err(ApiError::from)?;
    let workspace_modules = match execution_authority.as_ref() {
        Some(authority) => {
            load_agent_run_workspace_module_surface(state, authority, target, agent_owner_user_id)
                .await?
                .0
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
                Some(parent) => Some(lineage_ref(&edges, parent, relation_kind)),
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
            children.push(lineage_ref(&edges, child, edge.relation_kind.clone()));
        }
    }
    children.sort_by(|left, right| right.display_title.cmp(&left.display_title));
    Ok((parent, children))
}

fn lineage_ref(
    edges: &[agentdash_domain::workflow::AgentLineage],
    agent: LifecycleAgent,
    relation_kind: String,
) -> AgentRunLineageRef {
    fn descendants(agent_id: Uuid, edges: &[agentdash_domain::workflow::AgentLineage]) -> u32 {
        edges
            .iter()
            .filter(|edge| edge.parent_agent_id == Some(agent_id))
            .map(|edge| 1 + descendants(edge.child_agent_id, edges))
            .sum()
    }
    let display_title = app_agent_run::resolve_agent_run_display_title(
        agent.workspace_title.as_deref(),
        agent.workspace_title_source.as_deref(),
    )
    .value;
    AgentRunLineageRef {
        run_id: agent.run_id.to_string(),
        agent_id: agent.id.to_string(),
        source: agent.source.as_str().to_string(),
        relation_kind,
        display_title,
        subagent_count: descendants(agent.id, edges),
    }
}

fn workspace_to_contract(
    snapshot: app_workspace::AgentRunWorkspaceSnapshot,
    workspace_modules: Vec<agentdash_contracts::workspace_module::WorkspaceModuleDescriptor>,
) -> AgentRunWorkspaceView {
    let conversation = conversation_to_contract(snapshot.conversation);
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
            last_activity_at: snapshot.shell.last_activity_at,
        },
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
) -> AgentConversationSnapshot {
    AgentConversationSnapshot {
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
        model_config: model_config_to_contract(conversation.model_config),
        commands: command_set_to_contract(conversation.commands),
        waiting_items: conversation
            .waiting_items
            .into_iter()
            .map(waiting_item_to_contract)
            .collect(),
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
            app_agent_run::ConversationCommandKindModel::Cancel => ConversationCommandKind::Cancel,
            app_agent_run::ConversationCommandKindModel::CompactContext => {
                ConversationCommandKind::CompactContext
            }
        },
        command_id: command.command_id,
        runtime_command: match command.kind {
            app_agent_run::ConversationCommandKindModel::SubmitMessage => None,
            app_agent_run::ConversationCommandKindModel::Cancel => {
                Some(agentdash_agent_runtime_contract::AgentRuntimeCommandKind::Interrupt)
            }
            app_agent_run::ConversationCommandKindModel::CompactContext => {
                Some(agentdash_agent_runtime_contract::AgentRuntimeCommandKind::RequestCompaction)
            }
        },
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
                app_agent_run::ConversationCommandPlacementModel::Header => {
                    ConversationCommandPlacement::Header
                }
            })
            .collect(),
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
    association: app_agent_run::LifecycleSubjectAssociationView,
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
                runtime_thread_ref: RuntimeThreadRefDto {
                    runtime_thread_id: anchor.runtime_thread_id,
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
        runtime_thread_refs: frame
            .runtime_thread_refs
            .into_iter()
            .map(|runtime_ref| RuntimeThreadRefDto {
                runtime_thread_id: runtime_ref.runtime_thread_id,
            })
            .collect(),
        execution_profile: frame.execution_profile,
        effective_executor_config: frame
            .effective_executor_config
            .map(effective_executor_config_to_contract),
    }
}
