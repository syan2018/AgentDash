use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_agent_types::DynAgentRuntimeDelegate;
use agentdash_domain::common::AgentConfig;
use agentdash_spi::hooks::SharedHookSessionRuntime;
use agentdash_spi::{
    CapabilityState, ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame,
    RestoredSessionState, SessionMcpServer, Vfs,
};

use super::augmenter::{
    PromptAugmentCompanionInput, PromptAugmentInput, PromptAugmentTaskInput, PromptAugmentTaskPhase,
};
use super::construction::SessionConstructionPlan;
use super::types::{HookSnapshotReloadTrigger, SessionPromptLifecycle, UserPromptInput};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStrictness {
    Strict,
    Relaxed,
}

pub struct LaunchCommand {
    user_input: UserPromptInput,
    source: LaunchSource,
    strictness: LaunchStrictness,
    follow_up_session_id: Option<String>,
    identity: Option<agentdash_spi::AuthIdentity>,
    task: Option<PromptAugmentTaskInput>,
    companion: Option<PromptAugmentCompanionInput>,
    local_relay_mcp_declarations: Vec<SessionMcpServer>,
    local_relay_workspace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchCommandOutcome {
    pub turn_id: String,
    pub context_sources: Vec<String>,
}

impl LaunchCommand {
    fn new(
        user_input: UserPromptInput,
        source: LaunchSource,
        strictness: LaunchStrictness,
    ) -> Self {
        Self {
            user_input,
            source,
            strictness,
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

    pub fn to_augment_input(&self) -> PromptAugmentInput {
        let mut input = PromptAugmentInput::from_user_input(self.user_input.clone());
        input.mcp_servers = self.local_relay_mcp_declarations.clone();
        input.vfs = self
            .local_relay_workspace_root
            .as_ref()
            .map(|root| super::local_workspace_vfs(root));
        input.identity = self.identity.clone();
        input.task = self.task.clone();
        input.companion = self.companion.clone();
        input
    }

    pub fn source(&self) -> LaunchSource {
        self.source
    }

    pub fn strictness(&self) -> LaunchStrictness {
        self.strictness
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

    fn requires_augment_input(input: UserPromptInput, source: LaunchSource) -> Self {
        Self::new(input, source, LaunchStrictness::Strict)
    }

    fn command_with(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
        task: Option<PromptAugmentTaskInput>,
        companion: Option<PromptAugmentCompanionInput>,
        source: LaunchSource,
        strictness: LaunchStrictness,
    ) -> Self {
        let mut command = Self::new(input, source, strictness);
        command.identity = identity;
        command.task = task;
        command.companion = companion;
        command
    }

    pub fn http_prompt_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            None,
            None,
            LaunchSource::HttpPrompt,
            LaunchStrictness::Strict,
        )
    }

    pub fn hook_auto_resume_input(input: UserPromptInput) -> Self {
        Self::requires_augment_input(input, LaunchSource::HookAutoResume)
    }

    pub fn companion_parent_resume_input(input: UserPromptInput) -> Self {
        Self::requires_augment_input(input, LaunchSource::CompanionParentResume)
    }

    pub fn companion_dispatch_input(
        input: UserPromptInput,
        companion: PromptAugmentCompanionInput,
    ) -> Self {
        Self::command_with(
            input,
            None,
            None,
            Some(companion),
            LaunchSource::CompanionDispatch,
            LaunchStrictness::Strict,
        )
    }

    pub fn workflow_orchestrator_input(input: UserPromptInput) -> Self {
        Self::requires_augment_input(input, LaunchSource::WorkflowOrchestrator)
    }

    pub fn routine_executor_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            None,
            None,
            LaunchSource::RoutineExecutor,
            LaunchStrictness::Strict,
        )
    }

    pub fn task_service_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
        phase: PromptAugmentTaskPhase,
        override_prompt: Option<String>,
        additional_prompt: Option<String>,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            Some(PromptAugmentTaskInput {
                phase: Some(phase),
                override_prompt,
                additional_prompt,
            }),
            None,
            LaunchSource::TaskService,
            LaunchStrictness::Strict,
        )
    }

    pub fn local_relay_prompt_input(
        input: UserPromptInput,
        mcp_declarations: Vec<SessionMcpServer>,
        workspace_root: PathBuf,
    ) -> Self {
        let mut command = Self::new(
            input,
            LaunchSource::LocalRelayPrompt,
            LaunchStrictness::Relaxed,
        );
        command.local_relay_mcp_declarations = mcp_declarations;
        command.local_relay_workspace_root = Some(workspace_root);
        command
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchVfsSource {
    Request,
    CachedSessionProfile,
    HubDefault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchMcpSource {
    Request,
    CachedSessionProfile,
    Empty,
    PendingCapabilityTransition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchCapabilitySource {
    Request,
    CachedSessionProfile,
    Default,
    PendingCapabilityTransition,
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
    pub vfs_source: LaunchVfsSource,
    pub pending_vfs_overlay_applied: bool,
    pub mcp_source: LaunchMcpSource,
    pub capability_source: LaunchCapabilitySource,
    pub working_dir_input: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookLaunchPlan {
    pub snapshot_reload: HookSnapshotReloadTrigger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCommandLaunchPlan {
    pub pending_transition_count: usize,
    pub apply_after_connector_accept: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEffectPlan {
    pub terminal_event_first: bool,
    pub durable_outbox_required: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectorInputPlan {
    pub working_directory: PathBuf,
    pub executor_config: AgentConfig,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub has_vfs: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchExecutionTrace {
    pub entries: Vec<LaunchExecutionTraceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchExecutionTraceEntry {
    pub stage: &'static str,
    pub source: String,
}

pub struct LaunchExecution {
    pub construction: Option<SessionConstructionPlan>,
    pub lifecycle: LifecycleLaunchPlan,
    pub restore: RestoreLaunchPlan,
    pub hooks: HookLaunchPlan,
    pub runtime_commands: RuntimeCommandLaunchPlan,
    pub terminal_effects: TerminalEffectPlan,
    pub connector_input: ConnectorInputPlan,
    pub trace: LaunchExecutionTrace,
    pub context: ExecutionContext,
    pub summary: LaunchSummary,
}

pub struct LaunchExecutionInput {
    pub construction: Option<SessionConstructionPlan>,
    pub session_id: String,
    pub turn_id: String,
    pub lifecycle: SessionPromptLifecycle,
    pub restore_mode: LaunchRestoreMode,
    pub hook_snapshot_reload: HookSnapshotReloadTrigger,
    pub follow_up_session_id: Option<String>,
    pub follow_up_source: LaunchFollowUpSource,
    pub pending_transition_count: usize,
    pub vfs_source: LaunchVfsSource,
    pub pending_vfs_overlay_applied: bool,
    pub mcp_source: LaunchMcpSource,
    pub capability_source: LaunchCapabilitySource,
    pub working_dir_input: Option<String>,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub vfs: Option<Vfs>,
    pub identity: Option<agentdash_spi::AuthIdentity>,
    pub hook_session: Option<SharedHookSessionRuntime>,
    pub capability_state: CapabilityState,
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
    pub restored_session_state: Option<RestoredSessionState>,
}

impl LaunchExecution {
    pub fn build(input: LaunchExecutionInput) -> Self {
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
            pending_transition_count: input.pending_transition_count,
            vfs_source: input.vfs_source,
            pending_vfs_overlay_applied: input.pending_vfs_overlay_applied,
            mcp_source: input.mcp_source,
            capability_source: input.capability_source,
            working_dir_input: input.working_dir_input,
            working_directory: input.working_directory.clone(),
            has_vfs: input.vfs.is_some(),
            mcp_server_count: input.mcp_servers.len(),
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
        };
        let runtime_commands = RuntimeCommandLaunchPlan {
            pending_transition_count: input.pending_transition_count,
            apply_after_connector_accept: true,
        };
        let terminal_effects = TerminalEffectPlan {
            terminal_event_first: true,
            durable_outbox_required: true,
        };
        let connector_input = ConnectorInputPlan {
            working_directory: input.working_directory.clone(),
            executor_config: input.executor_config.clone(),
            mcp_servers: input.mcp_servers.clone(),
            has_vfs: input.vfs.is_some(),
        };
        let trace = LaunchExecutionTrace {
            entries: vec![
                LaunchExecutionTraceEntry {
                    stage: "construction",
                    source: "SessionConstructionPlan".to_string(),
                },
                LaunchExecutionTraceEntry {
                    stage: "runtime_command",
                    source: if input.pending_transition_count > 0 {
                        "pending_projection"
                    } else {
                        "none"
                    }
                    .to_string(),
                },
                LaunchExecutionTraceEntry {
                    stage: "terminal_effect",
                    source: "durable_outbox".to_string(),
                },
            ],
        };
        let session = ExecutionSessionFrame {
            turn_id: input.turn_id,
            working_directory: input.working_directory,
            environment_variables: input.environment_variables,
            executor_config: input.executor_config,
            mcp_servers: input.mcp_servers,
            vfs: input.vfs,
            identity: input.identity,
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
    use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};

    use super::super::construction::{
        SessionConstructionContextProjection, SessionConstructionPlan,
    };
    use super::super::ownership::SessionOwnerResolver;
    use super::super::types::UserPromptInput;
    use super::*;

    fn input_for(lifecycle: SessionPromptLifecycle) -> LaunchExecutionInput {
        let binding = SessionBinding::new(
            uuid::Uuid::new_v4(),
            "sess-launch".to_string(),
            SessionOwnerType::Project,
            uuid::Uuid::new_v4(),
            "execution",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
        let construction = SessionConstructionPlan::new(
            "sess-launch",
            owner,
            SessionConstructionContextProjection::default(),
        );
        LaunchExecutionInput {
            construction: Some(construction),
            session_id: "sess-launch".to_string(),
            turn_id: "t1".to_string(),
            lifecycle,
            restore_mode: LaunchRestoreMode::None,
            hook_snapshot_reload: HookSnapshotReloadTrigger::Reload,
            follow_up_session_id: None,
            follow_up_source: LaunchFollowUpSource::None,
            pending_transition_count: 2,
            vfs_source: LaunchVfsSource::Request,
            pending_vfs_overlay_applied: false,
            mcp_source: LaunchMcpSource::Request,
            capability_source: LaunchCapabilitySource::Request,
            working_dir_input: Some("project".to_string()),
            working_directory: PathBuf::from("/workspace/project"),
            environment_variables: HashMap::from([("A".to_string(), "B".to_string())]),
            executor_config: AgentConfig::new("PI_AGENT"),
            mcp_servers: Vec::new(),
            vfs: None,
            identity: None,
            hook_session: None,
            capability_state: CapabilityState::default(),
            runtime_delegate: None,
            restored_session_state: None,
        }
    }

    #[test]
    fn launch_execution_projects_connector_context_and_summary() {
        let lifecycle = SessionPromptLifecycle::OwnerBootstrap;
        let input = input_for(lifecycle);

        let execution = LaunchExecution::build(input);

        assert_eq!(execution.context.session.turn_id, "t1");
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
        assert_eq!(execution.summary.vfs_source, LaunchVfsSource::Request);
        assert_eq!(execution.summary.mcp_source, LaunchMcpSource::Request);
        assert_eq!(
            execution.summary.capability_source,
            LaunchCapabilitySource::Request
        );
        assert_eq!(
            execution.summary.working_dir_input,
            Some("project".to_string())
        );
        assert!(!execution.summary.has_vfs);
        assert!(!execution.summary.restored_executor_state);
        assert_eq!(
            execution
                .construction
                .as_ref()
                .map(|plan| plan.session_id.as_str()),
            Some("sess-launch")
        );
        assert!(execution.runtime_commands.apply_after_connector_accept);
        assert!(execution.terminal_effects.durable_outbox_required);
    }

    #[test]
    fn launch_command_carries_source_policy_and_follow_up() {
        let command = LaunchCommand::local_relay_prompt_input(
            UserPromptInput::from_text("ping"),
            Vec::new(),
            PathBuf::from("/workspace"),
        )
        .with_follow_up(Some("follow-up-1"));

        assert_eq!(command.source(), LaunchSource::LocalRelayPrompt);
        assert_eq!(command.strictness(), LaunchStrictness::Relaxed);
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

        let execution = LaunchExecution::build(input);

        assert!(execution.summary.restored_executor_state);
        assert_eq!(
            execution.summary.lifecycle,
            SessionPromptLifecycle::RepositoryRehydrate(
                super::super::types::SessionRepositoryRehydrateMode::ExecutorState
            )
        );
    }

    #[test]
    fn launch_summary_records_fallback_sources() {
        let mut input = input_for(SessionPromptLifecycle::Plain);
        input.follow_up_session_id = Some("executor-session-1".to_string());
        input.follow_up_source = LaunchFollowUpSource::SessionMeta;
        input.pending_transition_count = 1;
        input.vfs_source = LaunchVfsSource::CachedSessionProfile;
        input.pending_vfs_overlay_applied = true;
        input.mcp_source = LaunchMcpSource::PendingCapabilityTransition;
        input.capability_source = LaunchCapabilitySource::PendingCapabilityTransition;

        let execution = LaunchExecution::build(input);

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
            execution.summary.vfs_source,
            LaunchVfsSource::CachedSessionProfile
        );
        assert!(execution.summary.pending_vfs_overlay_applied);
        assert_eq!(
            execution.summary.mcp_source,
            LaunchMcpSource::PendingCapabilityTransition
        );
        assert_eq!(
            execution.summary.capability_source,
            LaunchCapabilitySource::PendingCapabilityTransition
        );
    }
}
