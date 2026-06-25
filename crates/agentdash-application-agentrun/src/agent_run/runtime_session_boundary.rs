use std::collections::HashMap;
use std::path::PathBuf;
use std::{io, sync::Arc};

use agentdash_agent_protocol::{
    BackboneEnvelope, UserInputBlock, UserInputSubmissionKind, text_user_input_blocks,
};
use agentdash_application_ports::frame_launch_envelope::{
    CompanionLaunchSource, FrameLaunchCommand, FrameLaunchLocalRelayPayload, FrameLaunchModifier,
    FrameLaunchSource, FrameLaunchUserInput, RoutineLaunchSource,
};
use agentdash_spi::context::capability::{
    SessionBaselineCapabilities, SkillEntry, SkillProviderCluster,
};
use agentdash_spi::{AgentConfig, AuthIdentity, ConnectorError, PromptPayload, RuntimeMcpServer};
use async_trait::async_trait;

use crate::error::WorkflowApplicationError;

pub use agentdash_spi::session_persistence::{
    ExecutionStatus, RuntimeCommandRecord, SessionEventPage, SessionMeta, SessionStoreError,
    TitleSource,
};

#[derive(Debug, Clone)]
pub struct UserPromptInput {
    pub input: Option<Vec<UserInputBlock>>,
    pub env: HashMap<String, String>,
    pub executor_config: Option<AgentConfig>,
    pub backend_selection: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPromptPayload {
    pub text_prompt: String,
    pub prompt_payload: PromptPayload,
    pub input: Vec<UserInputBlock>,
}

impl UserPromptInput {
    pub fn resolve_prompt_payload(&self) -> Result<ResolvedPromptPayload, String> {
        let input = self
            .input
            .as_ref()
            .ok_or_else(|| "必须提供 input".to_string())?;
        if input.is_empty() {
            return Err("input 不能为空数组".to_string());
        }
        let prompt_payload = PromptPayload::Input(input.clone());
        let text_prompt = prompt_payload.to_fallback_text();
        if text_prompt.trim().is_empty() {
            return Err("input 中没有有效内容".to_string());
        }
        Ok(ResolvedPromptPayload {
            text_prompt,
            prompt_payload,
            input: input.clone(),
        })
    }

    pub fn from_text(text: impl AsRef<str>) -> Self {
        Self {
            input: Some(text_user_input_blocks(text.as_ref().trim())),
            env: HashMap::new(),
            executor_config: None,
            backend_selection: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecutionState {
    Idle,
    Running {
        turn_id: Option<String>,
    },
    Cancelling {
        turn_id: Option<String>,
    },
    Completed {
        turn_id: String,
    },
    Failed {
        turn_id: String,
        message: Option<String>,
    },
    Interrupted {
        turn_id: Option<String>,
        message: Option<String>,
    },
    Lost {
        turn_id: Option<String>,
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRepositoryRehydrateMode {
    SystemContext,
    ExecutorState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptLaunchPath {
    Plain,
    OwnerBootstrap,
    RepositoryRehydrate(SessionRepositoryRehydrateMode),
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeTraceLaunchState {
    pub executor_session_id: Option<String>,
    pub last_event_seq: u64,
}

impl RuntimeTraceLaunchState {
    pub fn has_executor_follow_up(&self) -> bool {
        self.executor_session_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    }
}

impl From<&SessionMeta> for RuntimeTraceLaunchState {
    fn from(meta: &SessionMeta) -> Self {
        Self {
            executor_session_id: meta.executor_session_id.clone(),
            last_event_seq: meta.last_event_seq,
        }
    }
}

pub fn resolve_prompt_launch_path(
    runtime_trace_state: &RuntimeTraceLaunchState,
    has_live_executor_session: bool,
    supports_repository_restore: bool,
    agent_needs_bootstrap: bool,
) -> PromptLaunchPath {
    if agent_needs_bootstrap {
        return PromptLaunchPath::OwnerBootstrap;
    }

    if !has_live_executor_session
        && runtime_trace_state.last_event_seq > 0
        && !runtime_trace_state.has_executor_follow_up()
    {
        return PromptLaunchPath::RepositoryRehydrate(if supports_repository_restore {
            SessionRepositoryRehydrateMode::ExecutorState
        } else {
            SessionRepositoryRehydrateMode::SystemContext
        });
    }

    PromptLaunchPath::Plain
}

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
    identity: Option<AuthIdentity>,
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

    pub fn to_frame_launch_command(&self) -> FrameLaunchCommand {
        let modifiers = self
            .modifiers
            .iter()
            .map(|modifier| match modifier {
                LaunchModifier::Companion(companion) => {
                    FrameLaunchModifier::Companion(Box::new(companion.as_ref().clone()))
                }
                LaunchModifier::Routine(routine) => FrameLaunchModifier::Routine(routine.clone()),
                LaunchModifier::LocalRelay(payload) => {
                    FrameLaunchModifier::LocalRelay(FrameLaunchLocalRelayPayload {
                        mcp_servers: payload.mcp_servers.clone(),
                        workspace_root: payload.workspace_root.clone(),
                    })
                }
                LaunchModifier::HookAutoResume => FrameLaunchModifier::HookAutoResume,
            })
            .collect();
        FrameLaunchCommand {
            user_input: FrameLaunchUserInput {
                input: self.user_input.input.clone(),
                environment_variables: self.user_input.env.clone(),
                executor_config: self.user_input.executor_config.clone(),
            },
            source: match self.source {
                LaunchSource::HttpPrompt => FrameLaunchSource::HttpPrompt,
                LaunchSource::LifecycleAgentUserMessage => {
                    FrameLaunchSource::LifecycleAgentUserMessage
                }
                LaunchSource::HookAutoResume => FrameLaunchSource::HookAutoResume,
                LaunchSource::CompanionDispatch => FrameLaunchSource::CompanionDispatch,
                LaunchSource::CompanionParentResume => FrameLaunchSource::CompanionParentResume,
                LaunchSource::WorkflowOrchestrator => FrameLaunchSource::WorkflowOrchestrator,
                LaunchSource::RoutineExecutor => FrameLaunchSource::RoutineExecutor,
                LaunchSource::LocalRelayPrompt => FrameLaunchSource::LocalRelayPrompt,
            },
            follow_up_session_id: self.follow_up_session_id.clone(),
            identity: self.identity.clone(),
            modifiers,
        }
    }

    fn command_with(
        input: UserPromptInput,
        identity: Option<AuthIdentity>,
        modifiers: Vec<LaunchModifier>,
        source: LaunchSource,
    ) -> Self {
        let mut command = Self::new(input, source);
        command.identity = identity;
        command.modifiers = modifiers;
        command
    }

    pub fn http_prompt_input(input: UserPromptInput, identity: Option<AuthIdentity>) -> Self {
        Self::command_with(input, identity, Vec::new(), LaunchSource::HttpPrompt)
    }

    pub fn lifecycle_agent_user_message_input(
        input: UserPromptInput,
        identity: Option<AuthIdentity>,
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
        Self::new(input, LaunchSource::CompanionParentResume)
    }

    pub fn companion_dispatch_input(
        input: UserPromptInput,
        identity: Option<AuthIdentity>,
        companion: CompanionLaunchSource,
    ) -> Self {
        Self::command_with(
            input,
            identity,
            vec![LaunchModifier::Companion(Box::new(companion))],
            LaunchSource::CompanionDispatch,
        )
    }

    pub fn workflow_orchestrator_input(input: UserPromptInput) -> Self {
        Self::new(input, LaunchSource::WorkflowOrchestrator)
    }

    pub fn routine_executor_input(
        input: UserPromptInput,
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

    pub fn local_relay_prompt_input(
        input: UserPromptInput,
        mcp_servers: Vec<RuntimeMcpServer>,
        workspace_root: PathBuf,
    ) -> Self {
        Self::command_with(
            input,
            None,
            vec![LaunchModifier::LocalRelay(LocalRelayLaunchPayload {
                mcp_servers,
                workspace_root,
            })],
            LaunchSource::LocalRelayPrompt,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionTurnSteerCommand {
    pub session_id: String,
    pub expected_turn_id: String,
    pub input: Vec<UserInputBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalHookEffectBinding {
    pub handler: serde_json::Value,
    pub supported_effect_kinds: Vec<String>,
}

#[async_trait]
pub trait RuntimeSessionCorePort: Send + Sync {
    async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> Result<SessionExecutionState, WorkflowApplicationError>;

    async fn get_session_meta(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionMeta>, WorkflowApplicationError>;

    async fn delete_session(&self, session_id: &str) -> Result<(), WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct SessionCoreService {
    port: Arc<dyn RuntimeSessionCorePort>,
}

impl SessionCoreService {
    pub fn new(port: Arc<dyn RuntimeSessionCorePort>) -> Self {
        Self { port }
    }

    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> Result<SessionExecutionState, WorkflowApplicationError> {
        self.port.inspect_session_execution_state(session_id).await
    }

    pub async fn get_session_meta(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionMeta>, WorkflowApplicationError> {
        self.port.get_session_meta(session_id).await
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), WorkflowApplicationError> {
        self.port.delete_session(session_id).await
    }
}

#[async_trait]
pub trait RuntimeSessionControlPort: Send + Sync {
    async fn supports_session_steering(&self, session_id: &str) -> bool;

    async fn steer_session(&self, command: SessionTurnSteerCommand) -> Result<(), ConnectorError>;
}

#[derive(Clone)]
pub struct SessionControlService {
    port: Arc<dyn RuntimeSessionControlPort>,
}

impl SessionControlService {
    pub fn new(port: Arc<dyn RuntimeSessionControlPort>) -> Self {
        Self { port }
    }

    pub async fn supports_session_steering(&self, session_id: &str) -> bool {
        self.port.supports_session_steering(session_id).await
    }

    pub async fn steer_session(
        &self,
        command: SessionTurnSteerCommand,
    ) -> Result<(), ConnectorError> {
        self.port.steer_session(command).await
    }
}

#[async_trait]
pub trait RuntimeSessionEventingPort: Send + Sync {
    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage>;

    async fn persist_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> Result<(), WorkflowApplicationError>;

    async fn emit_user_input_submitted(
        &self,
        session_id: &str,
        turn_id: &str,
        event_id: &str,
        kind: UserInputSubmissionKind,
        input: Vec<UserInputBlock>,
    ) -> Result<(), WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct SessionEventingService {
    port: Arc<dyn RuntimeSessionEventingPort>,
}

impl SessionEventingService {
    pub fn new(port: Arc<dyn RuntimeSessionEventingPort>) -> Self {
        Self { port }
    }

    pub async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage> {
        self.port
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    pub async fn persist_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> Result<(), WorkflowApplicationError> {
        self.port.persist_notification(session_id, envelope).await
    }

    pub async fn emit_user_input_submitted(
        &self,
        session_id: &str,
        turn_id: &str,
        event_id: &str,
        kind: UserInputSubmissionKind,
        input: Vec<UserInputBlock>,
    ) -> Result<(), WorkflowApplicationError> {
        self.port
            .emit_user_input_submitted(session_id, turn_id, event_id, kind, input)
            .await
    }
}

#[async_trait]
pub trait RuntimeSessionLaunchPort: Send + Sync {
    async fn launch_command_in_task(
        &self,
        session_id: String,
        command: LaunchCommand,
    ) -> Result<String, WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct SessionLaunchService {
    port: Arc<dyn RuntimeSessionLaunchPort>,
}

impl SessionLaunchService {
    pub fn new(port: Arc<dyn RuntimeSessionLaunchPort>) -> Self {
        Self { port }
    }

    pub async fn launch_command_in_task(
        &self,
        session_id: String,
        command: LaunchCommand,
    ) -> Result<String, WorkflowApplicationError> {
        self.port.launch_command_in_task(session_id, command).await
    }
}

pub fn execution_state_from_meta(meta: Option<&SessionMeta>) -> SessionExecutionState {
    let Some(meta) = meta else {
        return SessionExecutionState::Idle;
    };
    match meta.last_delivery_status {
        ExecutionStatus::Idle => SessionExecutionState::Idle,
        ExecutionStatus::Running => SessionExecutionState::Running {
            turn_id: meta.last_turn_id.clone(),
        },
        ExecutionStatus::Completed => SessionExecutionState::Completed {
            turn_id: meta.last_turn_id.clone().unwrap_or_default(),
        },
        ExecutionStatus::Failed => SessionExecutionState::Failed {
            turn_id: meta.last_turn_id.clone().unwrap_or_default(),
            message: meta.last_terminal_message.clone(),
        },
        ExecutionStatus::Interrupted => SessionExecutionState::Interrupted {
            turn_id: meta.last_turn_id.clone(),
            message: meta.last_terminal_message.clone(),
        },
        ExecutionStatus::Lost => SessionExecutionState::Lost {
            turn_id: meta.last_turn_id.clone(),
            message: meta.last_terminal_message.clone(),
        },
    }
}

pub fn title_source_is_user(source: TitleSource) -> bool {
    matches!(source, TitleSource::User)
}

pub const WORKSPACE_SKILL_PROVIDER_KEY: &str = "workspace";
pub const INTEGRATION_STATIC_SKILL_PROVIDER_KEY: &str = "integration-static";

pub fn build_session_baseline_capabilities_from_clusters(
    skill_clusters: Vec<SkillProviderCluster>,
    skill_diagnostics: Vec<agentdash_spi::SkillDiscoveryDiagnostic>,
) -> SessionBaselineCapabilities {
    let skills = skill_clusters
        .iter()
        .flat_map(|cluster| cluster.default_exposed_skills.iter())
        .filter(|skill| skill.exposure.is_default_exposed())
        .map(SkillEntry::from_capability_entry)
        .collect();

    SessionBaselineCapabilities {
        skills,
        skill_clusters,
        skill_diagnostics,
    }
}
