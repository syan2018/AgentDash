//! Session launch envelope provider 契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! frame construction 逻辑才能拿到 context / MCP server 绑定 / flow capabilities /
//! context bundle 等运行时字段，否则会出现"通道漂移"。
//!
//! API 层实现此 trait，在 AppState 初始化时通过 `SessionRuntimeInner::set_session_construction_provider`
//! 注入。SessionRuntimeInner 内部 follow-up 一律先经过 provider，与 HTTP 主通道对齐。

use std::sync::Arc;

use super::launch::LaunchCommand;
use super::runtime_commands::RuntimeCommandRecord;
use super::types::RuntimeTraceLaunchState;
use crate::workflow::runtime_launch::FrameLaunchEnvelope;
use agentdash_domain::workflow::{ActivityDefinition, AgentProcedure, LifecycleRun, WorkflowGraph};
use agentdash_spi::ConnectorError;
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutineLaunchSource {
    pub routine_id: Uuid,
    pub execution_id: Uuid,
    pub trigger_source: String,
    pub entity_key: Option<String>,
}

#[derive(Clone)]
pub struct CompanionLaunchWorkflowSource {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub lifecycle: WorkflowGraph,
    pub activity: ActivityDefinition,
    pub workflow: Option<AgentProcedure>,
}

#[derive(Clone)]
pub struct CompanionLaunchSource {
    pub parent_session_id: String,
    pub slice_mode: agentdash_spi::CompanionSliceMode,
    pub companion_executor_config: agentdash_spi::AgentConfig,
    pub dispatch_prompt: String,
    pub workflow: Option<CompanionLaunchWorkflowSource>,
}

#[derive(Clone)]
pub struct SessionConstructionProviderInput {
    pub session_id: String,
    pub command: LaunchCommand,
    pub runtime_trace_state: RuntimeTraceLaunchState,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
    pub agent_needs_bootstrap: bool,
}

/// Session launch 的 envelope provider 契约。
///
/// 实现方负责从 runtime trace、AgentFrame、workspace、capability 与 context facts
/// 产出 `FrameLaunchEnvelope` 驱动 connector 启动。
#[async_trait]
pub trait SessionConstructionProvider: Send + Sync {
    /// 产出 FrameLaunchEnvelope —— session launch 的唯一输入。
    async fn build_frame_construction(
        &self,
        input: SessionConstructionProviderInput,
    ) -> Result<FrameLaunchEnvelope, ConnectorError>;
}

/// 动态类型别名，便于在 hub 内存储。
pub type SharedSessionConstructionProvider = Arc<dyn SessionConstructionProvider>;
