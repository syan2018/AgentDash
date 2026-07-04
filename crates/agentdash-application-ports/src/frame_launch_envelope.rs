use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::launch::LaunchCommand;
use agentdash_agent_protocol::UserInputBlock;
use agentdash_domain::backend::RuntimeBackendAnchor;
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::session_persistence::RuntimeCommandRecord;
use agentdash_spi::{
    AgentConfig, AuthIdentity, CapabilityState, DiscoveredGuideline, MemoryDiscoveryOutput,
    RuntimeMcpServer, SessionContextBundle, Vfs,
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

impl FrameRuntimeSurface {
    /// 从 `AgentFrame` 投影纯 surface 数据。
    pub fn from_frame(frame: &AgentFrame, runtime_session_id: Option<String>) -> Self {
        Self {
            agent_id: frame.agent_id,
            frame_id: frame.id,
            frame_revision: frame.revision,
            capability_surface: frame
                .effective_capability_json
                .clone()
                .unwrap_or(Value::Null),
            context_slice: frame.context_slice_json.clone().unwrap_or(Value::Null),
            vfs_surface: frame.vfs_surface_json.clone().unwrap_or(Value::Null),
            mcp_surface: frame.mcp_surface_json.clone().unwrap_or(Value::Null),
            runtime_session_id,
        }
    }
}

/// Command intent — 只表达用户请求事实（input/env/identity/terminal hook binding）。
#[derive(Debug, Clone, Default)]
pub struct FrameLaunchIntent {
    pub input: Option<Vec<UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub identity: Option<AuthIdentity>,
    pub terminal_hook_effect_binding: Option<TerminalHookEffectBinding>,
}

/// Context projection — launch-time runtime context discovery 派生物。
#[derive(Debug, Clone, Default)]
pub struct FrameLaunchContextProjection {
    pub context_bundle: Option<SessionContextBundle>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalHookEffectBinding {
    pub handler: Value,
    pub supported_effect_kinds: Vec<String>,
}

/// Frame refs — 持久化 frame surface 与 pending frame revision。
#[derive(Debug, Clone)]
pub struct FrameLaunchFrameRef {
    pub surface: FrameRuntimeSurface,
    pub pending_frame: Option<AgentFrame>,
}

/// Runtime surface — 闭包后的 launch execution surface。
#[derive(Debug, Clone)]
pub struct FrameLaunchRuntimeSurface {
    pub launch_surface: FrameLaunchSurface,
    pub working_directory: PathBuf,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    pub base_capability_state: Option<CapabilityState>,
}

/// Diagnostics — resolution trace 等可观测性投影。
#[derive(Debug, Clone, Default)]
pub struct FrameLaunchDiagnostics {
    pub resolution_trace: LaunchResolutionTrace,
}

#[derive(Debug, Clone)]
pub struct FrameLaunchEnvelope {
    pub frame: FrameLaunchFrameRef,
    pub command: FrameLaunchIntent,
    pub runtime: FrameLaunchRuntimeSurface,
    pub context: FrameLaunchContextProjection,
    pub diagnostics: FrameLaunchDiagnostics,
}

impl FrameLaunchEnvelope {
    pub fn launch_capability_state(&self) -> &CapabilityState {
        &self.runtime.launch_surface.capability_state
    }

    pub fn launch_vfs(&self) -> &Vfs {
        &self.runtime.launch_surface.vfs
    }

    pub fn launch_mcp_servers(&self) -> &[RuntimeMcpServer] {
        &self.runtime.launch_surface.mcp_servers
    }

    pub fn launch_executor_config(&self) -> &AgentConfig {
        &self.runtime.launch_surface.execution_profile
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeTraceLaunchStateRef {
    pub executor_session_id: Option<String>,
    pub last_event_seq: u64,
}

#[derive(Clone)]
pub struct FrameLaunchEnvelopeRequest {
    pub runtime_session_id: String,
    pub command: LaunchCommand,
    pub runtime_trace_state: RuntimeTraceLaunchStateRef,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
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
pub trait FrameLaunchEnvelopePort: Send + Sync {
    async fn build_launch_envelope(
        &self,
        input: FrameLaunchEnvelopeRequest,
    ) -> Result<FrameLaunchEnvelope, agentdash_spi::ConnectorError>;
}

pub type SharedFrameLaunchEnvelopePort = Arc<dyn FrameLaunchEnvelopePort>;

#[derive(Debug, Clone)]
pub struct AcceptedLaunchCommitInput {
    pub runtime_session_id: String,
    pub turn_id: String,
    pub pending_frame: Option<AgentFrame>,
    pub accepted_capability_state: CapabilityState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedLaunchCommitOutcome {
    pub frame_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub wrote_frame_revision: bool,
    pub bound_current_delivery: bool,
    pub synced_hook_runtime_target: bool,
    pub diagnostics: Vec<String>,
}

impl AcceptedLaunchCommitOutcome {
    pub fn empty() -> Self {
        Self {
            frame_id: None,
            agent_id: None,
            wrote_frame_revision: false,
            bound_current_delivery: false,
            synced_hook_runtime_target: false,
            diagnostics: Vec::new(),
        }
    }

    pub fn with_diagnostic(message: impl Into<String>) -> Self {
        let mut outcome = Self::empty();
        outcome.diagnostics.push(message.into());
        outcome
    }
}

#[async_trait]
pub trait AcceptedLaunchCommitPort: Send + Sync {
    async fn agent_needs_bootstrap(&self, runtime_session_id: &str) -> bool;

    async fn mark_agent_bootstrapped(&self, runtime_session_id: &str);

    async fn commit_accepted_launch(
        &self,
        input: AcceptedLaunchCommitInput,
    ) -> Result<AcceptedLaunchCommitOutcome, agentdash_spi::ConnectorError>;
}

#[async_trait]
pub trait AcceptedLaunchHookRuntimeSync: Send + Sync {
    async fn sync_accepted_launch_hook_runtime(
        &self,
        target: crate::runtime_surface_adoption::AgentFrameRuntimeTarget,
        turn_id: &str,
    ) -> Result<(), agentdash_spi::ConnectorError>;
}
