use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_agent_types::DynAgentRuntimeDelegate;
use agentdash_domain::common::AgentConfig;
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::hooks::ContextFrame;
use agentdash_spi::hooks::SharedHookRuntime;
use agentdash_spi::{
    CapabilityState, ContextFragment, DiscoveredGuideline, ExecutionBackendPlacement,
    ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame, RestoredSessionState,
    RuntimeMcpServer, SessionContextBundle,
};

use crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope;
use crate::backend_execution_placement::ExecutionPlacementPlan;
use crate::session::post_turn_handler::DynPostTurnHandler;
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::types::{
    HookSnapshotReloadTrigger, PendingCapabilityStateTransition, ResolvedPromptPayload,
    SessionPromptLifecycle,
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchFollowUpSource {
    Explicit,
    SessionMeta,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchRestoreMode {
    None,
    SystemContext,
    ExecutorState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSummary {
    pub session_id: String,
    pub turn_id: String,
    pub lifecycle: SessionPromptLifecycle,
    pub restore_mode: LaunchRestoreMode,
    pub hook_snapshot_reload: HookSnapshotReloadTrigger,
    pub follow_up_session_id: Option<String>,
    pub follow_up_source: LaunchFollowUpSource,
    pub pending_transition_count: usize,
    pub vfs_source: Option<String>,
    pub pending_vfs_overlay_applied: bool,
    pub mcp_source: Option<String>,
    pub capability_source: Option<String>,
    pub working_directory: PathBuf,
    pub has_vfs: bool,
    pub backend_execution_backend_id: Option<String>,
    pub backend_execution_lease_id: Option<uuid::Uuid>,
    pub mcp_server_count: usize,
    pub restored_executor_state: bool,
    pub capability_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LifecycleLaunchPlan {
    pub lifecycle: SessionPromptLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreLaunchPlan {
    pub mode: LaunchRestoreMode,
    pub restored_executor_state: bool,
}

pub struct HookLaunchPlan {
    pub snapshot_reload: HookSnapshotReloadTrigger,
    pub snapshot_contribution: Option<Vec<ContextFragment>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCommandLaunchPlan {
    pub requested_commands: Vec<RuntimeCommandRecord>,
    pub pending_capability_transitions: Vec<PendingCapabilityStateTransition>,
    pub base_capability_state: CapabilityState,
    pub apply_after_connector_accept: bool,
}

pub struct TerminalEffectPlan {
    pub terminal_event_first: bool,
    pub durable_outbox_required: bool,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

#[derive(Debug, Clone)]
pub struct ConnectorInputPlan {
    pub working_directory: PathBuf,
    pub executor_config: AgentConfig,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub has_vfs: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchPlanTrace {
    pub entries: Vec<LaunchPlanTraceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlanTraceEntry {
    pub stage: &'static str,
    pub source: String,
}

pub struct LaunchPlan {
    pub pending_frame: Option<AgentFrame>,
    pub resolved_payload: ResolvedPromptPayload,
    pub title_hint: String,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub context_bundle: Option<SessionContextBundle>,
    pub continuation_context_frame: Option<ContextFrame>,
    pub lifecycle: LifecycleLaunchPlan,
    pub restore: RestoreLaunchPlan,
    pub hooks: HookLaunchPlan,
    pub runtime_commands: RuntimeCommandLaunchPlan,
    pub terminal_effects: TerminalEffectPlan,
    pub connector_input: ConnectorInputPlan,
    pub backend_execution: Option<ExecutionPlacementPlan>,
    pub trace: LaunchPlanTrace,
    pub context: ExecutionContext,
    pub summary: LaunchSummary,
}

pub struct LaunchPlanInput {
    pub resolved_payload: ResolvedPromptPayload,
    pub launch_envelope: FrameLaunchEnvelope,
    pub session_id: String,
    pub turn_id: String,
    pub lifecycle: SessionPromptLifecycle,
    pub restore_mode: LaunchRestoreMode,
    pub hook_snapshot_reload: HookSnapshotReloadTrigger,
    pub hook_snapshot_contribution: Option<Vec<ContextFragment>>,
    pub follow_up_session_id: Option<String>,
    pub follow_up_source: LaunchFollowUpSource,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
    pub pending_capability_transitions: Vec<PendingCapabilityStateTransition>,
    pub base_capability_state: CapabilityState,
    pub environment_variables: HashMap<String, String>,
    pub hook_runtime: Option<SharedHookRuntime>,
    pub capability_state: CapabilityState,
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
    pub restored_session_state: Option<RestoredSessionState>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
    pub backend_execution: Option<ExecutionPlacementPlan>,
}

impl LaunchPlan {
    pub fn build(input: LaunchPlanInput) -> Self {
        let pending_frame = input.launch_envelope.pending_frame.clone();
        let working_directory = input.launch_envelope.working_directory.clone();
        let executor_config = input.launch_envelope.launch_executor_config().clone();
        let mcp_servers = input.launch_envelope.launch_mcp_servers().to_vec();
        let vfs = input.launch_envelope.launch_vfs().clone();
        let has_vfs = !vfs.mounts.is_empty();
        let identity = input.launch_envelope.intent.identity.clone();
        let title_hint = input
            .resolved_payload
            .text_prompt
            .chars()
            .take(30)
            .collect::<String>();
        let pending_transition_count = input.pending_capability_transitions.len();
        let restored_executor_state = input.restored_session_state.is_some();
        let capability_keys = input
            .capability_state
            .capability_keys()
            .into_iter()
            .collect::<Vec<_>>();
        let summary = LaunchSummary {
            session_id: input.session_id,
            turn_id: input.turn_id.clone(),
            lifecycle: input.lifecycle,
            restore_mode: input.restore_mode.clone(),
            hook_snapshot_reload: input.hook_snapshot_reload,
            follow_up_session_id: input.follow_up_session_id,
            follow_up_source: input.follow_up_source,
            pending_transition_count,
            vfs_source: input.launch_envelope.resolution_trace.vfs_source.clone(),
            pending_vfs_overlay_applied: input
                .launch_envelope
                .resolution_trace
                .pending_overlay_applied,
            mcp_source: input.launch_envelope.resolution_trace.mcp_source.clone(),
            capability_source: input
                .launch_envelope
                .resolution_trace
                .capability_source
                .clone(),
            working_directory: working_directory.clone(),
            has_vfs,
            backend_execution_backend_id: input
                .backend_execution
                .as_ref()
                .map(|placement| placement.backend_id.clone()),
            backend_execution_lease_id: input
                .backend_execution
                .as_ref()
                .and_then(|placement| placement.lease_id),
            mcp_server_count: mcp_servers.len(),
            restored_executor_state,
            capability_keys,
        };
        let lifecycle = LifecycleLaunchPlan {
            lifecycle: input.lifecycle,
        };
        let restore = RestoreLaunchPlan {
            mode: input.restore_mode,
            restored_executor_state,
        };
        let hooks = HookLaunchPlan {
            snapshot_reload: input.hook_snapshot_reload,
            snapshot_contribution: input.hook_snapshot_contribution,
        };
        let runtime_commands = RuntimeCommandLaunchPlan {
            requested_commands: input.requested_runtime_commands,
            pending_capability_transitions: input.pending_capability_transitions,
            base_capability_state: input.base_capability_state,
            apply_after_connector_accept: true,
        };
        let terminal_effects = TerminalEffectPlan {
            terminal_event_first: true,
            durable_outbox_required: true,
            post_turn_handler: input.post_turn_handler,
        };
        let connector_input = ConnectorInputPlan {
            working_directory: working_directory.clone(),
            executor_config: executor_config.clone(),
            mcp_servers: mcp_servers.clone(),
            has_vfs,
        };
        let trace = LaunchPlanTrace {
            entries: vec![
                LaunchPlanTraceEntry {
                    stage: "frame_launch_envelope",
                    source: "FrameLaunchEnvelope".to_string(),
                },
                LaunchPlanTraceEntry {
                    stage: "runtime_command",
                    source: if pending_transition_count > 0 {
                        "pending_projection"
                    } else {
                        "none"
                    }
                    .to_string(),
                },
                LaunchPlanTraceEntry {
                    stage: "terminal_effect",
                    source: "durable_outbox".to_string(),
                },
            ],
        };
        let session = ExecutionSessionFrame {
            turn_id: input.turn_id,
            working_directory,
            environment_variables: input.environment_variables,
            executor_config,
            mcp_servers,
            vfs: if has_vfs { Some(vfs) } else { None },
            backend_execution: input
                .backend_execution
                .as_ref()
                .map(execution_backend_placement_from_plan)
                .transpose()
                .expect("backend_execution placement 必须已 claim lease"),
            identity,
        };
        let turn = ExecutionTurnFrame {
            hook_runtime: input.hook_runtime,
            capability_state: input.capability_state,
            runtime_delegate: input.runtime_delegate,
            restored_session_state: input.restored_session_state,
            context_frames: Vec::new(),
            assembled_tools: Vec::new(),
        };
        let context_bundle = input.launch_envelope.context_bundle.clone();
        let continuation_context_frame = input.launch_envelope.continuation_context_frame.clone();
        Self {
            pending_frame,
            resolved_payload: input.resolved_payload,
            title_hint,
            discovered_guidelines: input.launch_envelope.intent.discovered_guidelines.clone(),
            context_bundle,
            continuation_context_frame,
            lifecycle,
            restore,
            hooks,
            runtime_commands,
            terminal_effects,
            connector_input,
            backend_execution: input.backend_execution,
            trace,
            context: ExecutionContext { session, turn },
            summary,
        }
    }
}

fn execution_backend_placement_from_plan(
    plan: &ExecutionPlacementPlan,
) -> Result<ExecutionBackendPlacement, String> {
    Ok(ExecutionBackendPlacement {
        backend_id: plan.backend_id.clone(),
        lease_id: plan
            .lease_id
            .ok_or_else(|| "ExecutionPlacementPlan 缺少已 claim 的 lease_id".to_string())?,
        selection_mode: plan.selection_mode,
    })
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_spi::Vfs;

    use super::*;
    use crate::agent_run::FrameSurfaceDraft;
    use crate::agent_run::frame::runtime_launch::{
        FrameLaunchEnvelope, FrameLaunchIntent, FrameLaunchSurface, FrameRuntimeSurface,
        LaunchResolutionTrace,
    };
    use crate::session::construction::{
        ConstructionResolutionPlan, OwnerResolutionTrace, ResolvedSessionOwner,
        RuntimeContextInspectionPlan, SessionConstructionContextProjection,
    };
    use crate::session::launch::{LaunchCommand, LaunchSource};
    use crate::session::types::{
        RuntimeCapabilityTransition, SessionRepositoryRehydrateMode, UserPromptInput,
    };
    use std::path::{Path, PathBuf};

    fn input_for(lifecycle: SessionPromptLifecycle) -> LaunchPlanInput {
        let owner = ResolvedSessionOwner {
            owner_type: agentdash_spi::CapabilityScope::Project,
            project_id: Some(uuid::Uuid::new_v4()),
            trace: OwnerResolutionTrace {
                selected_reason: "test".to_string(),
            },
        };
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mut capability_state = CapabilityState::default();
        capability_state.vfs.active = Some(vfs.clone());
        let mut construction = RuntimeContextInspectionPlan::new(
            "sess-launch",
            owner,
            SessionConstructionContextProjection::default(),
        );
        construction.workspace.working_directory = Some(PathBuf::from("/workspace/project"));
        let executor_config = AgentConfig::new("PI_AGENT");
        construction.surface.vfs = Some(vfs.clone());
        construction.projections.frame_surface_draft = Some(FrameSurfaceDraft {
            capability_state: Some(capability_state.clone()),
            vfs: Some(vfs),
            mcp_servers: Vec::new(),
            context_bundle_summary: None,
            execution_profile: Some(executor_config.clone()),
        });
        construction.execution_profile.executor_config = Some(executor_config);
        construction.resolution = ConstructionResolutionPlan {
            vfs_source: Some("construction.test".to_string()),
            mcp_source: Some("construction.test".to_string()),
            capability_source: Some("construction.test".to_string()),
            executor_source: Some("construction.test".to_string()),
            working_directory_source: Some("construction.test".to_string()),
            pending_overlay_applied: false,
            runtime_base_capability_state: None,
        };
        let resolved_payload = UserPromptInput::from_text("hello")
            .resolve_prompt_payload()
            .expect("resolved payload");
        let launch_envelope = envelope_from_construction(construction);
        LaunchPlanInput {
            resolved_payload,
            launch_envelope,
            session_id: "sess-launch".to_string(),
            turn_id: "t1".to_string(),
            lifecycle,
            restore_mode: LaunchRestoreMode::None,
            hook_snapshot_reload: HookSnapshotReloadTrigger::Reload,
            hook_snapshot_contribution: None,
            follow_up_session_id: None,
            follow_up_source: LaunchFollowUpSource::None,
            requested_runtime_commands: Vec::new(),
            pending_capability_transitions: vec![
                PendingCapabilityStateTransition {
                    id: "pending-1".to_string(),
                    run_id: uuid::Uuid::new_v4(),
                    lifecycle_key: "dev".to_string(),
                    phase_node: "phase-a".to_string(),
                    capability_keys: Default::default(),
                    transition: RuntimeCapabilityTransition::default(),
                    created_at: 1,
                    source_turn_id: None,
                },
                PendingCapabilityStateTransition {
                    id: "pending-2".to_string(),
                    run_id: uuid::Uuid::new_v4(),
                    lifecycle_key: "dev".to_string(),
                    phase_node: "phase-b".to_string(),
                    capability_keys: Default::default(),
                    transition: RuntimeCapabilityTransition::default(),
                    created_at: 2,
                    source_turn_id: None,
                },
            ],
            base_capability_state: CapabilityState::default(),
            environment_variables: HashMap::from([("A".to_string(), "B".to_string())]),
            hook_runtime: None,
            capability_state: CapabilityState::default(),
            runtime_delegate: None,
            restored_session_state: None,
            post_turn_handler: None,
            backend_execution: None,
        }
    }

    fn envelope_from_construction(
        construction: RuntimeContextInspectionPlan,
    ) -> FrameLaunchEnvelope {
        let working_directory = construction
            .workspace
            .working_directory
            .clone()
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let surface_draft = construction
            .projections
            .frame_surface_draft
            .clone()
            .expect("launch plan tests must provide complete FrameSurfaceDraft");
        let launch_surface = FrameLaunchSurface::from_surface_draft(&surface_draft)
            .expect("launch plan tests must provide launch-ready typed surface");
        FrameLaunchEnvelope {
            surface: FrameRuntimeSurface {
                agent_id: uuid::Uuid::new_v4(),
                frame_id: uuid::Uuid::new_v4(),
                frame_revision: 1,
                capability_surface: serde_json::Value::Null,
                context_slice: serde_json::Value::Null,
                vfs_surface: serde_json::Value::Null,
                mcp_surface: serde_json::Value::Null,
                runtime_session_id: Some("sess-launch".to_string()),
            },
            surface_draft,
            launch_surface,
            pending_frame: None,
            intent: FrameLaunchIntent {
                input: None,
                environment_variables: HashMap::new(),
                identity: None,
                terminal_hook_effect_binding: None,
                discovered_guidelines: construction.projections.discovered_guidelines,
            },
            working_directory,
            context_bundle: construction.context.bundle,
            continuation_context_frame: None,
            base_capability_state: construction.resolution.runtime_base_capability_state,
            resolution_trace: LaunchResolutionTrace {
                vfs_source: construction.resolution.vfs_source,
                mcp_source: construction.resolution.mcp_source,
                capability_source: construction.resolution.capability_source,
                pending_overlay_applied: construction.resolution.pending_overlay_applied,
            },
        }
    }

    #[test]
    fn launch_plan_projects_connector_context_and_summary() {
        let lifecycle = SessionPromptLifecycle::OwnerBootstrap;
        let input = input_for(lifecycle);

        let execution = LaunchPlan::build(input);

        assert_eq!(execution.context.session.turn_id, "t1");
        assert_eq!(execution.resolved_payload.text_prompt, "hello");
        assert_eq!(execution.title_hint, "hello");
        assert_eq!(
            execution.context.session.executor_config.executor,
            "PI_AGENT"
        );
        assert_eq!(execution.context.session.environment_variables["A"], "B");
        assert_eq!(execution.summary.session_id, "sess-launch");
        assert_eq!(execution.summary.lifecycle, lifecycle);
        assert_eq!(execution.summary.restore_mode, LaunchRestoreMode::None);
        assert_eq!(
            execution.summary.follow_up_source,
            LaunchFollowUpSource::None
        );
        assert_eq!(execution.summary.pending_transition_count, 2);
        assert_eq!(
            execution.summary.vfs_source.as_deref(),
            Some("construction.test")
        );
        assert_eq!(
            execution.summary.mcp_source.as_deref(),
            Some("construction.test")
        );
        assert_eq!(
            execution.summary.capability_source.as_deref(),
            Some("construction.test")
        );
        assert!(execution.summary.has_vfs);
        assert!(!execution.summary.restored_executor_state);
        assert_eq!(execution.summary.session_id.as_str(), "sess-launch");
        assert_eq!(
            execution
                .runtime_commands
                .pending_capability_transitions
                .len(),
            2
        );
        assert!(execution.runtime_commands.apply_after_connector_accept);
        assert!(execution.terminal_effects.durable_outbox_required);
    }

    #[test]
    fn launch_command_carries_source_intent_and_follow_up() {
        let command = LaunchCommand::local_relay_prompt_input(
            UserPromptInput::from_text("ping"),
            Vec::new(),
            PathBuf::from("/workspace"),
        )
        .with_follow_up(Some("follow-up-1"));

        assert_eq!(command.source(), LaunchSource::LocalRelayPrompt);
        assert_eq!(
            command.local_relay_workspace_root(),
            Some(Path::new("/workspace"))
        );
        assert_eq!(command.follow_up_session_id(), Some("follow-up-1"));
        assert_eq!(command.reason_tag(), "local_relay_prompt");
    }

    #[test]
    fn launch_summary_marks_repository_restore_state() {
        let mut input = input_for(SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ));
        input.restore_mode = LaunchRestoreMode::ExecutorState;
        input.restored_session_state = Some(RestoredSessionState::default());

        let execution = LaunchPlan::build(input);

        assert!(execution.summary.restored_executor_state);
        assert_eq!(
            execution.summary.lifecycle,
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState
            )
        );
    }

    #[test]
    fn launch_summary_records_construction_sources() {
        let mut input = input_for(SessionPromptLifecycle::Plain);
        input.follow_up_session_id = Some("executor-session-1".to_string());
        input.follow_up_source = LaunchFollowUpSource::SessionMeta;
        input.pending_capability_transitions = vec![PendingCapabilityStateTransition {
            id: "pending-3".to_string(),
            run_id: uuid::Uuid::new_v4(),
            lifecycle_key: "dev".to_string(),
            phase_node: "phase-c".to_string(),
            capability_keys: Default::default(),
            transition: RuntimeCapabilityTransition::default(),
            created_at: 3,
            source_turn_id: None,
        }];
        input.launch_envelope.resolution_trace.vfs_source =
            Some("runtime_command.pending_vfs_overlay".to_string());
        input
            .launch_envelope
            .resolution_trace
            .pending_overlay_applied = true;
        input.launch_envelope.resolution_trace.mcp_source =
            Some("runtime_command.pending_transition".to_string());
        input.launch_envelope.resolution_trace.capability_source =
            Some("runtime_command.pending_transition".to_string());

        let execution = LaunchPlan::build(input);

        assert_eq!(
            execution.summary.follow_up_session_id.as_deref(),
            Some("executor-session-1")
        );
        assert_eq!(
            execution.summary.follow_up_source,
            LaunchFollowUpSource::SessionMeta
        );
        assert_eq!(execution.summary.pending_transition_count, 1);
        assert_eq!(
            execution.summary.vfs_source.as_deref(),
            Some("runtime_command.pending_vfs_overlay")
        );
        assert!(execution.summary.pending_vfs_overlay_applied);
        assert_eq!(
            execution.summary.mcp_source.as_deref(),
            Some("runtime_command.pending_transition")
        );
        assert_eq!(
            execution.summary.capability_source.as_deref(),
            Some("runtime_command.pending_transition")
        );
    }
}
