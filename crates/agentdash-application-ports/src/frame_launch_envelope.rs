use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_domain::backend::RuntimeBackendAnchor;
use agentdash_spi::{
    AgentConfig, AuthIdentity, CapabilityState, DiscoveredGuideline, MemoryDiscoveryOutput,
    RuntimeMcpServer, Vfs,
};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FrameRuntimeSurface {
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub capability_surface: Value,
    pub context_slice: Value,
    pub vfs_surface: Value,
    pub mcp_surface: Value,
    pub runtime_session_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FrameLaunchIntent {
    pub input: Option<Vec<UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub identity: Option<AuthIdentity>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub discovered_memory: MemoryDiscoveryOutput,
}

#[derive(Debug, Clone)]
pub struct FrameLaunchSurface {
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub execution_profile: AgentConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchResolutionTrace {
    pub vfs_source: Option<String>,
    pub mcp_source: Option<String>,
    pub capability_source: Option<String>,
    pub pending_overlay_applied: bool,
}

#[derive(Debug, Clone)]
pub struct FrameLaunchEnvelope {
    pub surface: FrameRuntimeSurface,
    pub launch_surface: FrameLaunchSurface,
    pub intent: FrameLaunchIntent,
    pub working_directory: PathBuf,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    pub resolution_trace: LaunchResolutionTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameLaunchCommandSource {
    UserPrompt,
    FollowUp,
    AutoResume,
    RuntimeCommand,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDeliveryCommandRef {
    pub command_id: String,
    pub command_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTraceLaunchStateRef {
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct FrameLaunchEnvelopeProviderInput {
    pub runtime_session_id: String,
    pub command_source: FrameLaunchCommandSource,
    pub runtime_trace_state: RuntimeTraceLaunchStateRef,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeDeliveryCommandRef>,
    pub agent_needs_bootstrap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameLaunchEnvelopeError {
    #[error("frame launch envelope source was not found: {message}")]
    MissingSource { message: String },
    #[error("frame launch envelope is incomplete: field={field}")]
    MissingField { field: &'static str },
    #[error("frame launch envelope projection failed: {message}")]
    Projection { message: String },
}

#[async_trait]
pub trait FrameLaunchEnvelopeProvider: Send + Sync {
    async fn build_frame_launch_envelope(
        &self,
        input: FrameLaunchEnvelopeProviderInput,
    ) -> Result<FrameLaunchEnvelope, FrameLaunchEnvelopeError>;
}
