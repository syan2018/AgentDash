use std::{io, sync::Arc};

use agentdash_agent_protocol::{
    BackboneEnvelope, UserInputBlock, UserInputSource, UserInputSubmissionKind,
};
use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput};
use agentdash_spi::ConnectorError;
use agentdash_spi::context::capability::{
    SessionBaselineCapabilities, SkillEntry, SkillProviderCluster,
};
use async_trait::async_trait;

use crate::error::WorkflowApplicationError;

pub use agentdash_spi::session_persistence::{
    PersistedSessionEvent, RuntimeCommandRecord, SessionEventPage, SessionMeta, SessionStoreError,
};

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

impl SessionExecutionState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. }
                | Self::Failed { .. }
                | Self::Interrupted { .. }
                | Self::Lost { .. }
        )
    }
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

#[derive(Debug, Clone, PartialEq)]
pub struct SessionTurnSteerCommand {
    pub session_id: String,
    pub expected_turn_id: String,
    pub input: Vec<UserInputBlock>,
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
        source: UserInputSource,
        input: Vec<UserInputBlock>,
    ) -> Result<(), WorkflowApplicationError>;

    async fn subscribe_after(
        &self,
        _session_id: &str,
        _after_seq: u64,
    ) -> io::Result<RuntimeSessionEventSubscription> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "runtime session event subscription is not available",
        ))
    }

    fn ephemeral_epoch(&self) -> u64 {
        0
    }
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
        source: UserInputSource,
        input: Vec<UserInputBlock>,
    ) -> Result<(), WorkflowApplicationError> {
        self.port
            .emit_user_input_submitted(session_id, turn_id, event_id, kind, source, input)
            .await
    }

    pub async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<RuntimeSessionEventSubscription> {
        self.port.subscribe_after(session_id, after_seq).await
    }

    pub fn ephemeral_epoch(&self) -> u64 {
        self.port.ephemeral_epoch()
    }
}

pub struct RuntimeSessionEventSubscription {
    pub snapshot_seq: u64,
    pub backlog: Vec<PersistedSessionEvent>,
    pub ephemeral_backlog: Vec<PersistedSessionEvent>,
    pub rx: tokio::sync::broadcast::Receiver<PersistedSessionEvent>,
}

#[async_trait]
pub trait RuntimeSessionLaunchPort: Send + Sync {
    async fn launch_command_in_task(
        &self,
        session_id: String,
        command: LaunchCommand,
        planning_input: LaunchPlanningInput,
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
        planning_input: LaunchPlanningInput,
    ) -> Result<String, WorkflowApplicationError> {
        self.port
            .launch_command_in_task(session_id, command, planning_input)
            .await
    }
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
