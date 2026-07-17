use std::collections::HashMap;

use agentdash_agent_protocol::{UserInputBlock, text_user_input_blocks};
use agentdash_domain::common::AgentBackendRequirement;
use agentdash_spi::{AgentConfig, AuthIdentity};
use serde::{Deserialize, Serialize};

use super::modifier::{CompanionLaunchSource, LaunchModifier, RoutineLaunchSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchSource {
    HttpPrompt,
    LifecycleAgentUserMessage,
    HookAutoResume,
    CompanionDispatch,
    CompanionParentResume,
    SystemDelivery,
    WorkflowOrchestrator,
    RoutineExecutor,
    ContextCompaction,
}

#[derive(Debug, Clone, Default)]
pub struct LaunchPromptInput {
    pub input: Option<Vec<UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: Option<AgentConfig>,
}

impl LaunchPromptInput {
    pub fn from_text(text: impl AsRef<str>) -> Self {
        Self {
            input: Some(text_user_input_blocks(text.as_ref().trim())),
            environment_variables: HashMap::new(),
            executor_config: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct LaunchInputSource {
    pub namespace: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub display_label_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl LaunchInputSource {
    pub fn new(
        namespace: impl Into<String>,
        kind: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let namespace = namespace.into();
        let kind = kind.into();
        Self {
            display_label_key: format!("mailbox.source.{namespace}.{kind}"),
            namespace,
            kind,
            source_ref: None,
            correlation_ref: None,
            actor: actor.into(),
            route: None,
            metadata: None,
        }
    }

    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub fn core_composer() -> Self {
        Self::new("core", "composer", "user")
    }

    pub fn companion_parent_resume() -> Self {
        Self::new("companion", "parent_resume", "agent").with_route("parent")
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct LaunchPlanningInput {
    pub backend_selection: Option<BackendSelectionInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_requirement: Option<AgentBackendRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authorized_backend_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BackendSelectionInput {
    pub mode: BackendSelectionInputMode,
    #[serde(default)]
    pub backend_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendSelectionInputMode {
    Explicit,
    AutoIdle,
    WorkspaceBinding,
}

#[derive(Clone)]
pub struct LaunchCommand {
    prompt: LaunchPromptInput,
    source: LaunchSource,
    input_source: Option<LaunchInputSource>,
    follow_up_session_id: Option<String>,
    identity: Option<AuthIdentity>,
    modifiers: Vec<LaunchModifier>,
}

impl LaunchCommand {
    fn new(prompt: LaunchPromptInput, source: LaunchSource) -> Self {
        Self {
            prompt,
            source,
            input_source: None,
            follow_up_session_id: None,
            identity: None,
            modifiers: Vec::new(),
        }
    }

    pub fn with_follow_up(mut self, session_id: Option<impl Into<String>>) -> Self {
        self.follow_up_session_id = session_id.map(Into::into);
        self
    }

    pub fn with_input_source(mut self, input_source: LaunchInputSource) -> Self {
        self.input_source = Some(input_source);
        self
    }

    pub fn prompt(&self) -> &LaunchPromptInput {
        &self.prompt
    }

    pub fn input_source(&self) -> Option<&LaunchInputSource> {
        self.input_source.as_ref()
    }

    pub fn identity(&self) -> Option<AuthIdentity> {
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
            LaunchSource::SystemDelivery => "system_delivery",
            LaunchSource::WorkflowOrchestrator => "workflow_orchestrator",
            LaunchSource::RoutineExecutor => "routine_executor",
            LaunchSource::ContextCompaction => "context_compaction",
        }
    }

    fn source_input(input: LaunchPromptInput, source: LaunchSource) -> Self {
        Self::new(input, source)
    }

    fn command_with(
        input: LaunchPromptInput,
        identity: Option<AuthIdentity>,
        modifiers: Vec<LaunchModifier>,
        source: LaunchSource,
    ) -> Self {
        let mut command = Self::new(input, source);
        command.identity = identity;
        command.modifiers = modifiers;
        command
    }

    pub fn http_prompt_input(input: LaunchPromptInput, identity: Option<AuthIdentity>) -> Self {
        Self::command_with(input, identity, Vec::new(), LaunchSource::HttpPrompt)
            .with_input_source(LaunchInputSource::core_composer())
    }

    pub fn lifecycle_agent_user_message_input(
        input: LaunchPromptInput,
        identity: Option<AuthIdentity>,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            Vec::new(),
            LaunchSource::LifecycleAgentUserMessage,
        )
        .with_input_source(LaunchInputSource::core_composer())
    }

    pub fn hook_auto_resume_input(input: LaunchPromptInput) -> Self {
        Self::command_with(
            input,
            None,
            vec![LaunchModifier::HookAutoResume],
            LaunchSource::HookAutoResume,
        )
    }

    pub fn companion_parent_resume_input(input: LaunchPromptInput) -> Self {
        Self::source_input(input, LaunchSource::CompanionParentResume)
            .with_input_source(LaunchInputSource::companion_parent_resume())
    }

    pub fn system_delivery_input(input: LaunchPromptInput) -> Self {
        Self::source_input(input, LaunchSource::SystemDelivery)
    }

    pub fn companion_dispatch_input(
        input: LaunchPromptInput,
        identity: Option<AuthIdentity>,
        companion: CompanionLaunchSource,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            vec![LaunchModifier::Companion(Box::new(companion))],
            LaunchSource::CompanionDispatch,
        )
        .with_input_source(
            LaunchInputSource::new("companion", "dispatch", "agent").with_route("sub"),
        )
    }

    pub fn workflow_orchestrator_input(input: LaunchPromptInput) -> Self {
        Self::source_input(input, LaunchSource::WorkflowOrchestrator)
    }

    pub fn routine_executor_input(
        input: LaunchPromptInput,
        identity: Option<AuthIdentity>,
        routine: RoutineLaunchSource,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            vec![LaunchModifier::Routine(routine)],
            LaunchSource::RoutineExecutor,
        )
    }

    pub fn context_compaction_input(input: LaunchPromptInput) -> Self {
        Self::source_input(input, LaunchSource::ContextCompaction)
    }
}

#[cfg(test)]
mod tests {
    use agentdash_spi::{AgentConfig, CompanionSliceMode};
    use uuid::Uuid;

    use super::{LaunchCommand, LaunchPromptInput, LaunchSource};
    use crate::launch::{CompanionLaunchSource, LaunchModifier, RoutineLaunchSource};

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
            LaunchPromptInput::from_text("review this"),
            None,
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
            LaunchPromptInput::from_text("run"),
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
        let command = LaunchCommand::hook_auto_resume_input(LaunchPromptInput::from_text("resume"));

        assert_eq!(command.source(), LaunchSource::HookAutoResume);
        assert!(matches!(
            command.modifiers().first(),
            Some(LaunchModifier::HookAutoResume)
        ));
    }
}
