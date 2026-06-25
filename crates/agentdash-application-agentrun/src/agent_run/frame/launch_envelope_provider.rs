//! AgentRun frame launch construction 输入契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! frame construction 逻辑才能拿到 context / MCP server 绑定 / flow capabilities /
//! context bundle 等运行时字段，否则会出现"通道漂移"。

use crate::agent_run::runtime_session_boundary::LaunchCommand;
use crate::agent_run::runtime_session_boundary::RuntimeCommandRecord;
use crate::agent_run::runtime_session_boundary::RuntimeTraceLaunchState;
pub use agentdash_application_ports::frame_launch_envelope::{
    CompanionLaunchSource, CompanionLaunchWorkflowSource, RoutineLaunchSource,
};

#[derive(Clone)]
pub struct FrameLaunchEnvelopeConstructionInput {
    pub session_id: String,
    pub command: LaunchCommand,
    pub runtime_trace_state: RuntimeTraceLaunchState,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
    pub agent_needs_bootstrap: bool,
}
