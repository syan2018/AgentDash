use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod commit;
mod connector_start;
mod deps;
mod ingestion;
mod orchestrator;
mod planner;
mod preparation;
mod service;

pub(in crate::session) use commit::TurnCommitter;
pub(in crate::session) use connector_start::ConnectorStarter;
pub(in crate::session) use deps::SessionLaunchDeps;
pub(in crate::session) use ingestion::StreamIngestionAttacher;
pub(in crate::session) use orchestrator::SessionLaunchOrchestrator;
pub(in crate::session) use planner::{LaunchPlanner, LaunchPlannerInput};
pub(in crate::session) use preparation::{TurnPreparationInput, TurnPreparer};
pub use service::SessionLaunchService;

use agentdash_agent_types::DynAgentRuntimeDelegate;
use agentdash_domain::common::AgentConfig;
use agentdash_spi::hooks::SharedHookSessionRuntime;
use agentdash_spi::{
    CapabilityState, ContextFragment, DiscoveredGuideline, ExecutionContext, ExecutionSessionFrame,
    ExecutionTurnFrame, RestoredSessionState, SessionMcpServer,
};

use crate::session::construction::SessionConstructionPlan;
use crate::session::construction_provider::{
    CompanionLaunchSource, TaskLaunchPhase, TaskLaunchSource,
};
use crate::session::post_turn_handler::DynPostTurnHandler;
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::types::{
    HookSnapshotReloadTrigger, PendingCapabilityStateTransition, ResolvedPromptPayload,
    SessionPromptLifecycle, UserPromptInput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchSource {
    HttpPrompt,
    HookAutoResume,
    CompanionDispatch,
    CompanionParentResume,
    TaskService,
    WorkflowOrchestrator,
    RoutineExecutor,
    LocalRelayPrompt,
}

#[derive(Clone)]
pub struct LaunchCommand {
    user_input: UserPromptInput,
    source: LaunchSource,
    follow_up_session_id: Option<String>,
    identity: Option<agentdash_spi::AuthIdentity>,
    task: Option<TaskLaunchSource>,
    companion: Option<CompanionLaunchSource>,
    local_relay_mcp_declarations: Vec<SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchCommandOutcome {
    pub turn_id: String,
    pub context_sources: Vec<String>,
}

impl LaunchCommand {
    fn new(user_input: UserPromptInput, source: LaunchSource) -> Self {
        Self {
            user_input,
            source,
            follow_up_session_id: None,
            identity: None,
            task: None,
            companion: None,
            local_relay_mcp_declarations: Vec::new(),
            local_relay_workspace_root: None,
        }
    }

    pub fn with_follow_up(mut self, session_id: Option<impl Into<String>>) -> Self {
        self.follow_up_session_id = session_id.map(Into::into);
        self
    }

    pub fn user_input(&self) -> &UserPromptInput {
        &self.user_input
    }

    pub fn identity(&self) -> Option<agentdash_spi::AuthIdentity> {
        self.identity.clone()
    }

    pub fn task_hint(&self) -> Option<TaskLaunchSource> {
        self.task.clone()
    }

    pub fn companion_hint(&self) -> Option<CompanionLaunchSource> {
        self.companion.clone()
    }

    pub fn local_relay_mcp_declarations(&self) -> &[SessionMcpServer] {
        &self.local_relay_mcp_declarations
    }

    pub fn local_relay_workspace_root(&self) -> Option<&Path> {
        self.local_relay_workspace_root.as_deref()
    }

    pub fn source(&self) -> LaunchSource {
        self.source
    }

    pub fn follow_up_session_id(&self) -> Option<&str> {
        self.follow_up_session_id.as_deref()
    }

    pub fn reason_tag(&self) -> &'static str {
        match self.source {
            LaunchSource::HttpPrompt => "http_prompt",
            LaunchSource::HookAutoResume => "hook_auto_resume",
            LaunchSource::CompanionDispatch => "companion_dispatch",
            LaunchSource::CompanionParentResume => "companion_parent_resume",
            LaunchSource::TaskService => "task_service",
            LaunchSource::WorkflowOrchestrator => "workflow_orchestrator",
            LaunchSource::RoutineExecutor => "routine_executor",
            LaunchSource::LocalRelayPrompt => "local_relay_prompt",
        }
    }

    fn source_input(input: UserPromptInput, source: LaunchSource) -> Self {
        Self::new(input, source)
    }

    fn command_with(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
        task: Option<TaskLaunchSource>,
        companion: Option<CompanionLaunchSource>,
        source: LaunchSource,
    ) -> Self {
        let mut command = Self::new(input, source);
        command.identity = identity;
        command.task = task;
        command.companion = companion;
        command
    }

    pub fn http_prompt_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(input, identity, None, None, LaunchSource::HttpPrompt)
    }

    pub fn hook_auto_resume_input(input: UserPromptInput) -> Self {
        Self::source_input(input, LaunchSource::HookAutoResume)
    }

    pub fn companion_parent_resume_input(input: UserPromptInput) -> Self {
        Self::source_input(input, LaunchSource::CompanionParentResume)
    }

    pub fn companion_dispatch_input(
        input: UserPromptInput,
        companion: CompanionLaunchSource,
    ) -> Self {
        Self::command_with(
            input,
            None,
            None,
            Some(companion),
            LaunchSource::CompanionDispatch,
        )
    }

    pub fn workflow_orchestrator_input(input: UserPromptInput) -> Self {
        Self::source_input(input, LaunchSource::WorkflowOrchestrator)
    }

    pub fn routine_executor_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(input, identity, None, None, LaunchSource::RoutineExecutor)
    }

    pub fn task_service_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
        phase: TaskLaunchPhase,
        override_prompt: Option<String>,
        additional_prompt: Option<String>,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            Some(TaskLaunchSource {
                phase: Some(phase),
                override_prompt,
                additional_prompt,
            }),
            None,
            LaunchSource::TaskService,
        )
    }

    pub fn local_relay_prompt_input(
        input: UserPromptInput,
        mcp_declarations: Vec<SessionMcpServer>,
        workspace_root: PathBuf,
    ) -> Self {
        let mut command = Self::new(input, LaunchSource::LocalRelayPrompt);
        command.local_relay_mcp_declarations = mcp_declarations;
        command.local_relay_workspace_root = Some(workspace_root);
        command
    }
}

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
    pub mcp_servers: Vec<SessionMcpServer>,
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
    pub resolved_payload: ResolvedPromptPayload,
    pub title_hint: String,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub construction: SessionConstructionPlan,
    pub lifecycle: LifecycleLaunchPlan,
    pub restore: RestoreLaunchPlan,
    pub hooks: HookLaunchPlan,
    pub runtime_commands: RuntimeCommandLaunchPlan,
    pub terminal_effects: TerminalEffectPlan,
    pub connector_input: ConnectorInputPlan,
    pub trace: LaunchPlanTrace,
    pub context: ExecutionContext,
    pub summary: LaunchSummary,
}

pub struct LaunchPlanInput {
    pub resolved_payload: ResolvedPromptPayload,
    pub construction: SessionConstructionPlan,
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
    pub hook_session: Option<SharedHookSessionRuntime>,
    pub capability_state: CapabilityState,
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
    pub restored_session_state: Option<RestoredSessionState>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

impl LaunchPlan {
    pub fn build(input: LaunchPlanInput) -> Self {
        let working_directory = input
            .construction
            .workspace
            .working_directory
            .clone()
            .expect("SessionConstructionPlan.workspace.working_directory 必须在 launch 前解析");
        let executor_config = input
            .construction
            .execution_profile
            .executor_config
            .clone()
            .expect(
                "SessionConstructionPlan.execution_profile.executor_config 必须在 launch 前解析",
            );
        let mcp_servers = input.construction.projections.mcp_servers.clone();
        let vfs = input.construction.surface.vfs.clone();
        let identity = input.construction.identity.identity.clone();
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
            vfs_source: input.construction.resolution.vfs_source.clone(),
            pending_vfs_overlay_applied: input.construction.resolution.pending_overlay_applied,
            mcp_source: input.construction.resolution.mcp_source.clone(),
            capability_source: input.construction.resolution.capability_source.clone(),
            working_directory: working_directory.clone(),
            has_vfs: vfs.is_some(),
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
            has_vfs: vfs.is_some(),
        };
        let trace = LaunchPlanTrace {
            entries: vec![
                LaunchPlanTraceEntry {
                    stage: "construction",
                    source: "SessionConstructionPlan".to_string(),
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
            vfs,
            identity,
        };
        let turn = ExecutionTurnFrame {
            hook_session: input.hook_session,
            capability_state: input.capability_state,
            runtime_delegate: input.runtime_delegate,
            restored_session_state: input.restored_session_state,
            context_frames: Vec::new(),
            assembled_tools: Vec::new(),
        };
        Self {
            resolved_payload: input.resolved_payload,
            title_hint,
            discovered_guidelines: input.construction.projections.discovered_guidelines.clone(),
            construction: input.construction,
            lifecycle,
            restore,
            hooks,
            runtime_commands,
            terminal_effects,
            connector_input,
            trace,
            context: ExecutionContext { session, turn },
            summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
    use agentdash_spi::Vfs;

    use super::super::construction::{
        ConstructionResolutionPlan, SessionConstructionContextProjection, SessionConstructionPlan,
    };
    use super::super::ownership::SessionOwnerResolver;
    use super::super::types::{RuntimeCapabilityTransition, UserPromptInput};
    use super::*;

    fn input_for(lifecycle: SessionPromptLifecycle) -> LaunchPlanInput {
        let binding = SessionBinding::new(
            uuid::Uuid::new_v4(),
            "sess-launch".to_string(),
            SessionOwnerType::Project,
            uuid::Uuid::new_v4(),
            "execution",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
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
        let mut construction = SessionConstructionPlan::new(
            "sess-launch",
            owner,
            SessionConstructionContextProjection::default(),
        );
        construction.workspace.working_directory = Some(PathBuf::from("/workspace/project"));
        construction.execution_profile.executor_config = Some(AgentConfig::new("PI_AGENT"));
        construction.surface.vfs = Some(vfs);
        construction.projections.capability_state = Some(capability_state);
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
        LaunchPlanInput {
            resolved_payload,
            construction,
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
            hook_session: None,
            capability_state: CapabilityState::default(),
            runtime_delegate: None,
            restored_session_state: None,
            post_turn_handler: None,
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
        assert_eq!(execution.construction.session_id.as_str(), "sess-launch");
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
            super::super::types::SessionRepositoryRehydrateMode::ExecutorState,
        ));
        input.restore_mode = LaunchRestoreMode::ExecutorState;
        input.restored_session_state = Some(RestoredSessionState::default());

        let execution = LaunchPlan::build(input);

        assert!(execution.summary.restored_executor_state);
        assert_eq!(
            execution.summary.lifecycle,
            SessionPromptLifecycle::RepositoryRehydrate(
                super::super::types::SessionRepositoryRehydrateMode::ExecutorState
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
        input.construction.resolution.vfs_source =
            Some("runtime_command.pending_vfs_overlay".to_string());
        input.construction.resolution.pending_overlay_applied = true;
        input.construction.resolution.mcp_source =
            Some("runtime_command.pending_transition".to_string());
        input.construction.resolution.capability_source =
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
