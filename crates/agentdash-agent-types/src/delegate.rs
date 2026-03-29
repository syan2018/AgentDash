use async_trait::async_trait;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::decisions::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, BeforeStopInput,
    BeforeToolCallInput, StopDecision, ToolCallDecision, TransformContextInput,
    TransformContextOutput, TurnControlDecision,
};

// ─── AgentRuntimeDelegate ──────────────────────────────────

#[derive(Debug, Error)]
pub enum AgentRuntimeError {
    #[error("{0}")]
    Runtime(String),
}

/// Agent 运行时委托接口 — 由 Hook Runtime / Application 层实现。
///
/// Agent Loop 在关键生命周期节点调用此 trait，实现上下文变换、
/// 工具调用拦截、轮次控制和停止决策等编排能力。
#[async_trait]
pub trait AgentRuntimeDelegate: Send + Sync {
    async fn transform_context(
        &self,
        input: TransformContextInput,
        cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError>;

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError>;

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError>;

    async fn after_turn(
        &self,
        input: AfterTurnInput,
        cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError>;

    async fn before_stop(
        &self,
        input: BeforeStopInput,
        cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError>;
}

pub type DynAgentRuntimeDelegate = std::sync::Arc<dyn AgentRuntimeDelegate>;
