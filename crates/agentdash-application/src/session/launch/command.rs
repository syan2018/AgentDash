use std::path::{Path, PathBuf};

use agentdash_spi::SessionMcpServer;

use crate::session::construction_provider::{
    CompanionLaunchSource, RoutineLaunchSource, TaskLaunchPhase, TaskLaunchSource,
};
use crate::session::types::UserPromptInput;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchSource {
    HttpPrompt,
    LifecycleAgentUserMessage,
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
    routine: Option<RoutineLaunchSource>,
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
            routine: None,
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

    pub fn routine_hint(&self) -> Option<RoutineLaunchSource> {
        self.routine.clone()
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
            LaunchSource::LifecycleAgentUserMessage => "lifecycle_agent_user_message",
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
        routine: Option<RoutineLaunchSource>,
        companion: Option<CompanionLaunchSource>,
        source: LaunchSource,
    ) -> Self {
        let mut command = Self::new(input, source);
        command.identity = identity;
        command.task = task;
        command.routine = routine;
        command.companion = companion;
        command
    }

    pub fn http_prompt_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(input, identity, None, None, None, LaunchSource::HttpPrompt)
    }

    pub fn lifecycle_agent_user_message_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            None,
            None,
            None,
            LaunchSource::LifecycleAgentUserMessage,
        )
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
        routine: RoutineLaunchSource,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            None,
            Some(routine),
            None,
            LaunchSource::RoutineExecutor,
        )
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
