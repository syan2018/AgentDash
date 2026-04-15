use async_trait::async_trait;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::decisions::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, BeforeProviderRequestInput,
    BeforeStopInput, BeforeToolCallInput, CompactionParams, CompactionResult,
    EvaluateCompactionInput, StopDecision, ToolCallDecision, TransformContextInput,
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
    async fn evaluate_compaction(
        &self,
        input: EvaluateCompactionInput,
        cancel: CancellationToken,
    ) -> Result<Option<CompactionParams>, AgentRuntimeError>;

    async fn after_compaction(
        &self,
        result: CompactionResult,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError>;

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

    /// LLM API 请求发出前的观测回调（仅通知，不改写 payload）。
    /// 默认空实现，hook 层可用于日志记录、token 统计等。
    async fn on_before_provider_request(
        &self,
        _input: BeforeProviderRequestInput,
        _cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        Ok(())
    }
}

pub type DynAgentRuntimeDelegate = std::sync::Arc<dyn AgentRuntimeDelegate>;
