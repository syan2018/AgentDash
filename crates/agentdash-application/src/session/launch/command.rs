use std::path::PathBuf;

use agentdash_spi::RuntimeMcpServer;

use crate::agent_run::frame::launch_envelope_provider::{
    CompanionLaunchSource, RoutineLaunchSource,
};
use crate::session::types::UserPromptInput;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchSource {
    HttpPrompt,
    LifecycleAgentUserMessage,
    HookAutoResume,
    CompanionDispatch,
    CompanionParentResume,
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
    modifiers: Vec<LaunchModifier>,
}

#[derive(Clone)]
pub enum LaunchModifier {
    Companion(Box<CompanionLaunchSource>),
    Routine(RoutineLaunchSource),
    LocalRelay(LocalRelayLaunchPayload),
    HookAutoResume,
}

#[derive(Clone)]
pub struct LocalRelayLaunchPayload {
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub workspace_root: PathBuf,
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
            modifiers: Vec::new(),
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

    pub fn companion_modifier(&self) -> Option<CompanionLaunchSource> {
        self.modifiers.iter().find_map(|modifier| match modifier {
            LaunchModifier::Companion(companion) => Some(companion.as_ref().clone()),
            _ => None,
        })
    }

    pub fn routine_modifier(&self) -> Option<RoutineLaunchSource> {
        self.modifiers.iter().find_map(|modifier| match modifier {
            LaunchModifier::Routine(routine) => Some(routine.clone()),
            _ => None,
        })
    }

    pub fn local_relay_modifier(&self) -> Option<&LocalRelayLaunchPayload> {
        self.modifiers.iter().find_map(|modifier| match modifier {
            LaunchModifier::LocalRelay(payload) => Some(payload),
            _ => None,
        })
    }

    pub fn modifiers(&self) -> &[LaunchModifier] {
        &self.modifiers
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
        modifiers: Vec<LaunchModifier>,
        source: LaunchSource,
    ) -> Self {
        let mut command = Self::new(input, source);
        command.identity = identity;
        command.modifiers = modifiers;
        command
    }

    pub fn http_prompt_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(input, identity, Vec::new(), LaunchSource::HttpPrompt)
    }

    pub fn lifecycle_agent_user_message_input(
        input: UserPromptInput,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            Vec::new(),
            LaunchSource::LifecycleAgentUserMessage,
        )
    }

    pub fn hook_auto_resume_input(input: UserPromptInput) -> Self {
        Self::command_with(
            input,
            None,
            vec![LaunchModifier::HookAutoResume],
            LaunchSource::HookAutoResume,
        )
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
            vec![LaunchModifier::Companion(Box::new(companion))],
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
            vec![LaunchModifier::Routine(routine)],
            LaunchSource::RoutineExecutor,
        )
    }

    pub fn local_relay_prompt_input(
        input: UserPromptInput,
        mcp_servers: Vec<RuntimeMcpServer>,
        workspace_root: PathBuf,
    ) -> Self {
        let mut command = Self::new(input, LaunchSource::LocalRelayPrompt);
        command
            .modifiers
            .push(LaunchModifier::LocalRelay(LocalRelayLaunchPayload {
                mcp_servers,
                workspace_root,
            }));
        command
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agentdash_spi::{AgentConfig, CompanionSliceMode, McpTransportConfig};
    use uuid::Uuid;

    use super::{LaunchCommand, LaunchModifier, LaunchSource};
    use crate::agent_run::frame::launch_envelope_provider::{
        CompanionLaunchSource, RoutineLaunchSource,
    };
    use crate::session::types::UserPromptInput;

    #[test]
    fn source_specific_facts_are_stored_as_modifiers() {
        let mcp_server = agentdash_spi::RuntimeMcpServer {
            name: "local-tools".to_string(),
            transport: McpTransportConfig::Stdio {
                command: "tool".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
            },
            uses_relay: false,
        };
        let command = LaunchCommand::local_relay_prompt_input(
            UserPromptInput::from_text("ping"),
            vec![mcp_server.clone()],
            PathBuf::from("/workspace"),
        );

        assert_eq!(command.source(), LaunchSource::LocalRelayPrompt);
        let local_relay = command
            .local_relay_modifier()
            .expect("local relay modifier");
        assert_eq!(local_relay.mcp_servers, vec![mcp_server]);
        assert_eq!(
            local_relay.workspace_root.as_path(),
            std::path::Path::new("/workspace")
        );
        assert!(matches!(
            command.modifiers().first(),
            Some(LaunchModifier::LocalRelay(_))
        ));
    }

    #[test]
    fn companion_and_routine_constructors_push_typed_modifiers() {
        let companion = CompanionLaunchSource {
            parent_session_id: "parent-session".to_string(),
            selected_project_agent_id: Some(Uuid::new_v4()),
            selected_agent_key: Some("reviewer".to_string()),
            slice_mode: CompanionSliceMode::Compact,
            companion_executor_config: AgentConfig::new("codex".to_string()),
            dispatch_prompt: "review this".to_string(),
            workflow: None,
        };
        let companion_command = LaunchCommand::companion_dispatch_input(
            UserPromptInput::from_text("review this"),
            companion.clone(),
        );

        assert_eq!(companion_command.source(), LaunchSource::CompanionDispatch);
        assert_eq!(
            companion_command
                .companion_modifier()
                .expect("companion modifier")
                .parent_session_id,
            companion.parent_session_id
        );
        assert!(matches!(
            companion_command.modifiers().first(),
            Some(LaunchModifier::Companion(_))
        ));

        let routine = RoutineLaunchSource {
            routine_id: Uuid::new_v4(),
            execution_id: Uuid::new_v4(),
            trigger_source: "manual".to_string(),
            entity_key: Some("entity-1".to_string()),
        };
        let routine_command = LaunchCommand::routine_executor_input(
            UserPromptInput::from_text("run"),
            None,
            routine.clone(),
        );

        assert_eq!(routine_command.source(), LaunchSource::RoutineExecutor);
        assert_eq!(routine_command.routine_modifier(), Some(routine));
        assert!(matches!(
            routine_command.modifiers().first(),
            Some(LaunchModifier::Routine(_))
        ));
    }

    #[test]
    fn hook_auto_resume_keeps_source_tag_as_modifier() {
        let command = LaunchCommand::hook_auto_resume_input(UserPromptInput::from_text("resume"));

        assert_eq!(command.source(), LaunchSource::HookAutoResume);
        assert!(matches!(
            command.modifiers().first(),
            Some(LaunchModifier::HookAutoResume)
        ));
    }
}
