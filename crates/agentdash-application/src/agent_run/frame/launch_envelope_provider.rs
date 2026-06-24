//! AgentRun frame launch envelope provider 契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! frame construction 逻辑才能拿到 context / MCP server 绑定 / flow capabilities /
//! context bundle 等运行时字段，否则会出现"通道漂移"。
//!
//! API 层实现此 trait，在 AppState 初始化时通过 `SessionRuntimeInner::set_frame_launch_envelope_provider`
//! 注入。SessionRuntimeInner 内部 follow-up 一律先经过 provider，与 HTTP 主通道对齐。

use crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope;
use crate::session::launch::LaunchCommand;
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::types::RuntimeTraceLaunchState;
pub use agentdash_application_ports::frame_launch_envelope::{
    CompanionLaunchSource, CompanionLaunchWorkflowSource, RoutineLaunchSource,
};
use agentdash_spi::ConnectorError;
use async_trait::async_trait;

#[derive(Clone)]
pub struct FrameLaunchEnvelopeProviderInput {
    pub session_id: String,
    pub command: LaunchCommand,
    pub runtime_trace_state: RuntimeTraceLaunchState,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
    pub agent_needs_bootstrap: bool,
}

/// Frame launch envelope provider 契约。
///
/// 实现方负责从 runtime trace、AgentFrame、workspace、capability 与 context facts
/// 产出 `FrameLaunchEnvelope` 驱动 connector 启动。
#[async_trait]
pub trait FrameLaunchEnvelopeProvider: Send + Sync {
    /// 产出 FrameLaunchEnvelope —— session launch 的唯一输入。
    async fn build_frame_launch_envelope(
        &self,
        input: FrameLaunchEnvelopeProviderInput,
    ) -> Result<FrameLaunchEnvelope, ConnectorError>;
}

/// 动态类型别名，便于在 launch runtime 内存储。
pub type SharedFrameLaunchEnvelopeProvider = std::sync::Arc<dyn FrameLaunchEnvelopeProvider>;
