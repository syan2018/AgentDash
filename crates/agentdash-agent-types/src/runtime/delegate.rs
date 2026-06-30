use async_trait::async_trait;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::runtime::decisions::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, BeforeProviderRequestInput,
    BeforeStopInput, BeforeToolCallInput, CompactionFailureInput, CompactionParams,
    CompactionResult, EvaluateCompactionInput, StopDecision, ToolCallDecision,
    TransformContextInput, TransformContextOutput, TurnControlDecision,
};

// ─── Runtime delegate facets ───────────────────────────────

#[derive(Debug, Error)]
pub enum AgentRuntimeError {
    #[error("{0}")]
    Runtime(String),
}

/// 上下文压缩策略与结果通知。
#[async_trait]
pub trait RuntimeCompactionDelegate: Send + Sync {
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

    async fn after_compaction_failed(
        &self,
        _input: CompactionFailureInput,
        _cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        Ok(())
    }
}

/// Provider 可见上下文变换。
#[async_trait]
pub trait RuntimeContextTransformDelegate: Send + Sync {
    async fn transform_context(
        &self,
        input: TransformContextInput,
        cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError>;
}

/// 工具调用准入、审批与结果修订策略。
#[async_trait]
pub trait RuntimeToolPolicyDelegate: Send + Sync {
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
}

/// Agent loop turn / stop 边界控制。
#[async_trait]
pub trait RuntimeTurnBoundaryDelegate: Send + Sync {
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

/// LLM API 请求发出前的观测回调（仅通知，不改写 payload）。
#[async_trait]
pub trait RuntimeProviderObserverDelegate: Send + Sync {
    async fn on_before_provider_request(
        &self,
        input: BeforeProviderRequestInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError>;
}

pub type DynRuntimeCompactionDelegate = std::sync::Arc<dyn RuntimeCompactionDelegate>;
pub type DynRuntimeContextTransformDelegate = std::sync::Arc<dyn RuntimeContextTransformDelegate>;
pub type DynRuntimeToolPolicyDelegate = std::sync::Arc<dyn RuntimeToolPolicyDelegate>;
pub type DynRuntimeTurnBoundaryDelegate = std::sync::Arc<dyn RuntimeTurnBoundaryDelegate>;
pub type DynRuntimeProviderObserverDelegate = std::sync::Arc<dyn RuntimeProviderObserverDelegate>;

/// Agent loop 消费的显式 runtime delegate facet 集合。
///
/// 每个字段只代表一个生命周期 concern。缺失 facet 时，方法 helper 会使用
/// agent loop 的默认行为：不压缩、保持上下文、允许工具、自然停止、provider
/// observer no-op。
#[derive(Clone, Default)]
pub struct AgentRuntimeDelegateSet {
    pub compaction: Option<DynRuntimeCompactionDelegate>,
    pub context_transform: Option<DynRuntimeContextTransformDelegate>,
    pub tool_policy: Option<DynRuntimeToolPolicyDelegate>,
    pub turn_boundary: Option<DynRuntimeTurnBoundaryDelegate>,
    pub provider_observer: Option<DynRuntimeProviderObserverDelegate>,
}

impl AgentRuntimeDelegateSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_all_facets<T>(delegate: std::sync::Arc<T>) -> Self
    where
        T: RuntimeCompactionDelegate
            + RuntimeContextTransformDelegate
            + RuntimeToolPolicyDelegate
            + RuntimeTurnBoundaryDelegate
            + RuntimeProviderObserverDelegate
            + 'static,
    {
        Self {
            compaction: Some(delegate.clone()),
            context_transform: Some(delegate.clone()),
            tool_policy: Some(delegate.clone()),
            turn_boundary: Some(delegate.clone()),
            provider_observer: Some(delegate),
        }
    }

    pub fn with_compaction(mut self, delegate: Option<DynRuntimeCompactionDelegate>) -> Self {
        self.compaction = delegate;
        self
    }

    pub fn with_context_transform(
        mut self,
        delegate: Option<DynRuntimeContextTransformDelegate>,
    ) -> Self {
        self.context_transform = delegate;
        self
    }

    pub fn with_tool_policy(mut self, delegate: Option<DynRuntimeToolPolicyDelegate>) -> Self {
        self.tool_policy = delegate;
        self
    }

    pub fn with_turn_boundary(mut self, delegate: Option<DynRuntimeTurnBoundaryDelegate>) -> Self {
        self.turn_boundary = delegate;
        self
    }

    pub fn with_provider_observer(
        mut self,
        delegate: Option<DynRuntimeProviderObserverDelegate>,
    ) -> Self {
        self.provider_observer = delegate;
        self
    }

    pub async fn evaluate_compaction(
        &self,
        input: EvaluateCompactionInput,
        cancel: CancellationToken,
    ) -> Result<Option<CompactionParams>, AgentRuntimeError> {
        let Some(delegate) = self.compaction.as_ref() else {
            return Ok(None);
        };
        delegate.evaluate_compaction(input, cancel).await
    }

    pub async fn after_compaction(
        &self,
        result: CompactionResult,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        let Some(delegate) = self.compaction.as_ref() else {
            return Ok(());
        };
        delegate.after_compaction(result, cancel).await
    }

    pub async fn after_compaction_failed(
        &self,
        input: CompactionFailureInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        let Some(delegate) = self.compaction.as_ref() else {
            return Ok(());
        };
        delegate.after_compaction_failed(input, cancel).await
    }

    pub async fn transform_context(
        &self,
        input: TransformContextInput,
        cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError> {
        let Some(delegate) = self.context_transform.as_ref() else {
            return Ok(TransformContextOutput {
                steering_messages: input.context.messages,
                blocked: None,
            });
        };
        delegate.transform_context(input, cancel).await
    }

    pub async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        let Some(delegate) = self.tool_policy.as_ref() else {
            return Ok(ToolCallDecision::Allow);
        };
        delegate.before_tool_call(input, cancel).await
    }

    pub async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
        let Some(delegate) = self.tool_policy.as_ref() else {
            return Ok(AfterToolCallEffects::default());
        };
        delegate.after_tool_call(input, cancel).await
    }

    pub async fn after_turn(
        &self,
        input: AfterTurnInput,
        cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError> {
        let Some(delegate) = self.turn_boundary.as_ref() else {
            return Ok(TurnControlDecision::default());
        };
        delegate.after_turn(input, cancel).await
    }

    pub async fn before_stop(
        &self,
        input: BeforeStopInput,
        cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError> {
        let Some(delegate) = self.turn_boundary.as_ref() else {
            return Ok(StopDecision::Stop);
        };
        delegate.before_stop(input, cancel).await
    }

    pub async fn on_before_provider_request(
        &self,
        input: BeforeProviderRequestInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        let Some(delegate) = self.provider_observer.as_ref() else {
            return Ok(());
        };
        delegate.on_before_provider_request(input, cancel).await
    }
}
