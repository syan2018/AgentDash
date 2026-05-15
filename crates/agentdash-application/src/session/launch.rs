use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_agent_types::DynAgentRuntimeDelegate;
use agentdash_domain::common::AgentConfig;
use agentdash_spi::hooks::{ContextFrame, SharedHookSessionRuntime};
use agentdash_spi::{
    CapabilityState, ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame,
    RestoredSessionState, SessionMcpServer, Vfs,
};

use super::assembler::{PreparedSessionInputs, finalize_request};
use super::augmenter::PromptAugmentInput;
use super::types::{
    HookSnapshotReloadTrigger, PreparedLaunchPrompt, SessionPromptLifecycle, UserPromptInput,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStrictness {
    Strict,
    Relaxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchPreparation {
    RequiresAugment,
    PreAssembled,
}

pub struct LaunchCommand {
    prepared_prompt: PreparedLaunchPrompt,
    source: LaunchSource,
    strictness: LaunchStrictness,
    preparation: LaunchPreparation,
    follow_up_session_id: Option<String>,
}

impl LaunchCommand {
    fn new(
        prepared_prompt: PreparedLaunchPrompt,
        source: LaunchSource,
        strictness: LaunchStrictness,
        preparation: LaunchPreparation,
    ) -> Self {
        Self {
            prepared_prompt,
            source,
            strictness,
            preparation,
            follow_up_session_id: None,
        }
    }

    pub fn with_follow_up(mut self, session_id: Option<impl Into<String>>) -> Self {
        self.follow_up_session_id = session_id.map(Into::into);
        self
    }

    pub fn with_continuation_context_frame(mut self, frame: Option<ContextFrame>) -> Self {
        self.prepared_prompt.continuation_context_frame = frame;
        self
    }

    pub fn into_prepared_prompt(self) -> PreparedLaunchPrompt {
        self.prepared_prompt
    }

    pub fn into_augment_input(self) -> PromptAugmentInput {
        PromptAugmentInput::from_prepared_prompt(self.prepared_prompt)
    }

    pub fn source(&self) -> LaunchSource {
        self.source
    }

    pub fn strictness(&self) -> LaunchStrictness {
        self.strictness
    }

    pub fn preparation(&self) -> LaunchPreparation {
        self.preparation
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
        Self::new(
            PreparedLaunchPrompt::from_user_input(input),
            source,
            LaunchStrictness::Strict,
            LaunchPreparation::RequiresAugment,
        )
    }

    pub fn http_prompt_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        let mut request = PreparedLaunchPrompt::from_user_input(input);
        request.identity = identity;
        Self::new(
            request,
            LaunchSource::HttpPrompt,
            LaunchStrictness::Strict,
            LaunchPreparation::RequiresAugment,
        )
    }

    pub fn hook_auto_resume_input(input: UserPromptInput) -> Self {
        Self::requires_augment_input(input, LaunchSource::HookAutoResume)
    }

    pub fn companion_parent_resume_input(input: UserPromptInput) -> Self {
        Self::requires_augment_input(input, LaunchSource::CompanionParentResume)
    }

    pub fn companion_dispatch_prepared(
        input: UserPromptInput,
        prepared: PreparedSessionInputs,
    ) -> Self {
        Self::preassembled_input(input, prepared, LaunchSource::CompanionDispatch)
    }

    fn preassembled_input(
        input: UserPromptInput,
        prepared: PreparedSessionInputs,
        source: LaunchSource,
    ) -> Self {
        Self::new(
            finalize_request(PreparedLaunchPrompt::from_user_input(input), prepared),
            source,
            LaunchStrictness::Strict,
            LaunchPreparation::PreAssembled,
        )
    }

    pub fn task_service_prepared(input: UserPromptInput, prepared: PreparedSessionInputs) -> Self {
        Self::preassembled_input(input, prepared, LaunchSource::TaskService)
    }

    pub fn workflow_orchestrator_prepared(
        input: UserPromptInput,
        prepared: PreparedSessionInputs,
    ) -> Self {
        Self::preassembled_input(input, prepared, LaunchSource::WorkflowOrchestrator)
    }

    pub fn routine_executor_prepared(
        input: UserPromptInput,
        prepared: PreparedSessionInputs,
    ) -> Self {
        Self::preassembled_input(input, prepared, LaunchSource::RoutineExecutor)
    }

    pub fn local_relay_prompt_input(
        input: UserPromptInput,
        mcp_servers: Vec<SessionMcpServer>,
        vfs: Vfs,
    ) -> Self {
        let mut request = PreparedLaunchPrompt::from_user_input(input);
        request.mcp_servers = mcp_servers;
        request.vfs = Some(vfs);
        Self::new(
            request,
            LaunchSource::LocalRelayPrompt,
            LaunchStrictness::Strict,
            LaunchPreparation::PreAssembled,
        )
    }

    #[cfg(test)]
    fn local_relay_prompt(prompt: PreparedLaunchPrompt) -> Self {
        Self::new(
            prompt,
            LaunchSource::LocalRelayPrompt,
            LaunchStrictness::Strict,
            LaunchPreparation::PreAssembled,
        )
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

pub struct LaunchExecution {
    pub context: ExecutionContext,
    pub summary: LaunchSummary,
}

pub struct LaunchExecutionInput {
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
        let capability_keys = input
            .capability_state
            .capability_keys()
            .into_iter()
            .collect::<Vec<_>>();
        let summary = LaunchSummary {
            session_id: input.session_id,
            turn_id: input.turn_id.clone(),
            lifecycle: input.lifecycle,
            restore_mode: input.restore_mode,
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
            restored_executor_state: input.restored_session_state.is_some(),
            capability_keys,
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
            context: ExecutionContext { session, turn },
            summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::UserPromptInput;
    use super::*;

    fn request() -> PreparedLaunchPrompt {
        PreparedLaunchPrompt::from_user_input(UserPromptInput::from_text("ping"))
    }

    fn input_for(lifecycle: SessionPromptLifecycle) -> LaunchExecutionInput {
        LaunchExecutionInput {
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
    }

    #[test]
    fn launch_command_carries_source_policy_and_follow_up() {
        let command =
            LaunchCommand::local_relay_prompt(request()).with_follow_up(Some("follow-up-1"));

        assert_eq!(command.source(), LaunchSource::LocalRelayPrompt);
        assert_eq!(command.strictness(), LaunchStrictness::Strict);
        assert_eq!(command.preparation(), LaunchPreparation::PreAssembled);
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
