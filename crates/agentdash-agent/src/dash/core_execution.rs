use std::pin::Pin;

use agentdash_agent_core::{
    CoreCallbacks, CoreContext, CoreError, CoreEvent, CoreInput, CoreMessage, CoreOutput,
    CoreProvider, CoreRole, CoreTokenUsage, CoreTool, CoreToolCall, CoreToolCallbacks,
    CoreToolResult, FinishReason, ProviderEvent, ProviderEventStream, ProviderRequest,
    run_agent_loop,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::{AgentItemId, AgentTurnId, HistoryContribution, HistoryEntryId, HistoryPayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DashMessageRole {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashMessage {
    pub role: DashMessageRole,
    pub content: String,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashCoreContext {
    pub system_prompt: String,
    pub history: Vec<DashMessage>,
    pub tools: Vec<DashToolDefinition>,
    pub max_provider_rounds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashProviderRequest {
    pub system_prompt: String,
    pub messages: Vec<DashMessage>,
    pub tools: Vec<DashToolDefinition>,
    pub round: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashToolResult {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DashFinishReason {
    Stop,
    ToolCalls,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DashProviderEvent {
    TextDelta {
        delta: String,
    },
    ReasoningDelta {
        delta: String,
    },
    ToolCall {
        call: DashToolCall,
    },
    Completed {
        finish_reason: DashFinishReason,
        input_tokens: u64,
        output_tokens: u64,
    },
}

pub type DashProviderEventStream =
    Pin<Box<dyn Stream<Item = Result<DashProviderEvent, DashCoreError>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DashCoreEvent {
    ProviderRoundStarted {
        round: u32,
    },
    TextDelta {
        round: u32,
        delta: String,
    },
    ReasoningDelta {
        round: u32,
        delta: String,
    },
    ToolCallRequested {
        round: u32,
        call: DashToolCall,
    },
    ToolCallCompleted {
        round: u32,
        result: DashToolResult,
    },
    ProviderRoundCompleted {
        round: u32,
        finish_reason: DashFinishReason,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashCoreOutput {
    pub assistant_message: DashMessage,
    pub transcript_delta: Vec<DashMessage>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub provider_rounds: u32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DashCoreError {
    #[error("Dash Agent Core execution was cancelled")]
    Cancelled,
    #[error("Dash Agent provider stream disconnected before a terminal")]
    ProviderStreamDisconnected,
    #[error("Dash Agent provider returned an invalid terminal")]
    InvalidProviderTerminal,
    #[error("Dash Agent provider round limit reached: {max_rounds}")]
    ProviderRoundLimit { max_rounds: u32 },
    #[error("Dash Agent provider failed: {message}")]
    Provider { message: String, retryable: bool },
    #[error("Dash Agent tool callback failed: {message}")]
    Tool { message: String, retryable: bool },
    #[error("Dash Agent execution callback failed: {message}")]
    Callback { message: String },
    #[error("Dash Agent provider requested interaction {interaction_id}: {prompt}")]
    InteractionRequired {
        interaction_id: String,
        prompt: String,
    },
    #[error("Dash Agent provider context overflow requires compaction")]
    ContextOverflow,
}

impl DashCoreError {
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Provider {
                retryable: true,
                ..
            } | Self::Tool {
                retryable: true,
                ..
            }
        )
    }
}

#[async_trait]
pub trait DashProvider: Send + Sync {
    async fn stream(
        &self,
        request: DashProviderRequest,
    ) -> Result<DashProviderEventStream, DashCoreError>;

    async fn steer(&self, _turn_id: &AgentTurnId, _input: &str) -> Result<(), DashCoreError> {
        Err(DashCoreError::Provider {
            message: "provider does not accept in-flight steering".into(),
            retryable: false,
        })
    }
}

#[async_trait]
pub trait DashToolCallbacks: Send + Sync {
    async fn invoke(
        &self,
        turn_id: &AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError>;
}

#[async_trait]
pub trait DashExecutionCallbacks: Send + Sync {
    async fn emit(&self, event: DashCoreEvent) -> Result<(), DashCoreError>;
}

#[derive(Debug, Clone, Default)]
pub struct DashCancellation {
    token: CancellationToken,
}

impl DashCancellation {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

pub struct DashCoreTurn {
    pub turn_id: AgentTurnId,
    pub input: String,
    pub context: DashCoreContext,
    pub output_item_id: AgentItemId,
    pub output_started_entry_id: HistoryEntryId,
    pub output_entry_id: HistoryEntryId,
    pub output_completed_entry_id: HistoryEntryId,
    pub terminal_entry_id: HistoryEntryId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashCoreTurnResult {
    pub core_output: DashCoreOutput,
    pub history: Vec<HistoryContribution>,
}

impl DashCoreTurn {
    pub async fn run(
        self,
        provider: &dyn DashProvider,
        tools: &dyn DashToolCallbacks,
        callbacks: &dyn DashExecutionCallbacks,
        cancel: DashCancellation,
    ) -> Result<DashCoreTurnResult, DashCoreError> {
        let provider = ProviderAdapter(provider);
        let tools = ToolAdapter {
            inner: tools,
            turn_id: self.turn_id.clone(),
        };
        let callbacks = CallbackAdapter(callbacks);
        let core_output = run_agent_loop(
            CoreInput {
                message: CoreMessage::user(self.input),
            },
            core_context(self.context),
            &provider,
            &tools,
            &callbacks,
            cancel.token,
        )
        .await
        .map_err(dash_error)?;
        let output = dash_output(core_output);
        let history = vec![
            HistoryContribution {
                entry_id: self.output_started_entry_id,
                payload: HistoryPayload::ItemStarted {
                    turn_id: self.turn_id.clone(),
                    item_id: self.output_item_id.clone(),
                    kind: super::ItemKind::AssistantMessage,
                },
            },
            HistoryContribution {
                entry_id: self.output_entry_id,
                payload: HistoryPayload::AgentOutput {
                    turn_id: self.turn_id.clone(),
                    item_id: Some(self.output_item_id.clone()),
                    content: output.assistant_message.content.clone(),
                },
            },
            HistoryContribution {
                entry_id: self.output_completed_entry_id,
                payload: HistoryPayload::ItemCompleted {
                    turn_id: self.turn_id.clone(),
                    item_id: self.output_item_id,
                },
            },
            HistoryContribution {
                entry_id: self.terminal_entry_id,
                payload: HistoryPayload::TurnCompleted {
                    turn_id: self.turn_id,
                },
            },
        ];
        Ok(DashCoreTurnResult {
            core_output: output,
            history,
        })
    }
}

struct ProviderAdapter<'a>(&'a dyn DashProvider);

#[async_trait]
impl CoreProvider for ProviderAdapter<'_> {
    async fn stream(&self, request: ProviderRequest) -> Result<ProviderEventStream, CoreError> {
        let stream = self
            .0
            .stream(DashProviderRequest {
                system_prompt: request.system_prompt,
                messages: request.messages.into_iter().map(dash_message).collect(),
                tools: request
                    .tools
                    .into_iter()
                    .map(|tool| DashToolDefinition {
                        name: tool.name,
                        description: tool.description,
                        input_schema: tool.input_schema,
                    })
                    .collect(),
                round: request.round,
            })
            .await
            .map_err(core_error)?;
        Ok(Box::pin(stream.map(|event| {
            event.map(provider_event).map_err(core_error)
        })))
    }
}

struct ToolAdapter<'a> {
    inner: &'a dyn DashToolCallbacks,
    turn_id: AgentTurnId,
}

#[async_trait]
impl CoreToolCallbacks for ToolAdapter<'_> {
    async fn invoke(&self, call: CoreToolCall) -> Result<CoreToolResult, CoreError> {
        self.inner
            .invoke(
                &self.turn_id,
                DashToolCall {
                    call_id: call.call_id,
                    name: call.name,
                    arguments: call.arguments,
                },
            )
            .await
            .map(|result| CoreToolResult {
                call_id: result.call_id,
                content: result.content,
                is_error: result.is_error,
            })
            .map_err(core_error)
    }
}

struct CallbackAdapter<'a>(&'a dyn DashExecutionCallbacks);

#[async_trait]
impl CoreCallbacks for CallbackAdapter<'_> {
    async fn emit(&self, event: CoreEvent) -> Result<(), CoreError> {
        self.0.emit(dash_event(event)).await.map_err(core_error)
    }
}

fn core_context(context: DashCoreContext) -> CoreContext {
    CoreContext {
        system_prompt: context.system_prompt,
        history: context.history.into_iter().map(core_message).collect(),
        tools: context
            .tools
            .into_iter()
            .map(|tool| CoreTool {
                name: tool.name,
                description: tool.description,
                input_schema: tool.input_schema,
            })
            .collect(),
        max_provider_rounds: context.max_provider_rounds,
    }
}

fn core_message(message: DashMessage) -> CoreMessage {
    CoreMessage {
        role: match message.role {
            DashMessageRole::User => CoreRole::User,
            DashMessageRole::Assistant => CoreRole::Assistant,
            DashMessageRole::Tool => CoreRole::Tool,
        },
        content: message.content,
        tool_call_id: message.tool_call_id,
    }
}

fn dash_message(message: CoreMessage) -> DashMessage {
    DashMessage {
        role: match message.role {
            CoreRole::User => DashMessageRole::User,
            CoreRole::Assistant => DashMessageRole::Assistant,
            CoreRole::Tool => DashMessageRole::Tool,
        },
        content: message.content,
        tool_call_id: message.tool_call_id,
    }
}

fn provider_event(event: DashProviderEvent) -> ProviderEvent {
    match event {
        DashProviderEvent::TextDelta { delta } => ProviderEvent::TextDelta { delta },
        DashProviderEvent::ReasoningDelta { delta } => ProviderEvent::ReasoningDelta { delta },
        DashProviderEvent::ToolCall { call } => ProviderEvent::ToolCall {
            call: CoreToolCall {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            },
        },
        DashProviderEvent::Completed {
            finish_reason,
            input_tokens,
            output_tokens,
        } => ProviderEvent::Completed {
            finish_reason: match finish_reason {
                DashFinishReason::Stop => FinishReason::Stop,
                DashFinishReason::ToolCalls => FinishReason::ToolCalls,
            },
            usage: CoreTokenUsage {
                input_tokens,
                output_tokens,
            },
        },
    }
}

fn dash_event(event: CoreEvent) -> DashCoreEvent {
    match event {
        CoreEvent::ProviderRoundStarted { round } => DashCoreEvent::ProviderRoundStarted { round },
        CoreEvent::TextDelta { round, delta } => DashCoreEvent::TextDelta { round, delta },
        CoreEvent::ReasoningDelta { round, delta } => {
            DashCoreEvent::ReasoningDelta { round, delta }
        }
        CoreEvent::ToolCallRequested { round, call } => DashCoreEvent::ToolCallRequested {
            round,
            call: DashToolCall {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            },
        },
        CoreEvent::ToolCallCompleted { round, result } => DashCoreEvent::ToolCallCompleted {
            round,
            result: DashToolResult {
                call_id: result.call_id,
                content: result.content,
                is_error: result.is_error,
            },
        },
        CoreEvent::ProviderRoundCompleted {
            round,
            finish_reason,
        } => DashCoreEvent::ProviderRoundCompleted {
            round,
            finish_reason: match finish_reason {
                FinishReason::Stop => DashFinishReason::Stop,
                FinishReason::ToolCalls => DashFinishReason::ToolCalls,
            },
        },
    }
}

fn dash_output(output: CoreOutput) -> DashCoreOutput {
    DashCoreOutput {
        assistant_message: dash_message(output.assistant_message),
        transcript_delta: output
            .transcript_delta
            .into_iter()
            .map(dash_message)
            .collect(),
        input_tokens: output.usage.input_tokens,
        output_tokens: output.usage.output_tokens,
        provider_rounds: output.provider_rounds,
    }
}

fn core_error(error: DashCoreError) -> CoreError {
    match error {
        DashCoreError::Cancelled => CoreError::Cancelled,
        DashCoreError::ProviderStreamDisconnected => CoreError::ProviderStreamDisconnected,
        DashCoreError::InvalidProviderTerminal => CoreError::InvalidProviderTerminal,
        DashCoreError::ProviderRoundLimit { max_rounds } => {
            CoreError::ProviderRoundLimit { max_rounds }
        }
        DashCoreError::Provider { message, retryable } => {
            CoreError::Provider { message, retryable }
        }
        DashCoreError::Tool { message, retryable } => CoreError::Tool { message, retryable },
        DashCoreError::Callback { message } => CoreError::Callback { message },
        DashCoreError::InteractionRequired {
            interaction_id,
            prompt,
        } => CoreError::InteractionRequired {
            interaction_id,
            prompt,
        },
        DashCoreError::ContextOverflow => CoreError::ContextOverflow,
    }
}

fn dash_error(error: CoreError) -> DashCoreError {
    match error {
        CoreError::Cancelled => DashCoreError::Cancelled,
        CoreError::ProviderStreamDisconnected => DashCoreError::ProviderStreamDisconnected,
        CoreError::InvalidProviderTerminal => DashCoreError::InvalidProviderTerminal,
        CoreError::ProviderRoundLimit { max_rounds } => {
            DashCoreError::ProviderRoundLimit { max_rounds }
        }
        CoreError::Provider { message, retryable } => {
            DashCoreError::Provider { message, retryable }
        }
        CoreError::Tool { message, retryable } => DashCoreError::Tool { message, retryable },
        CoreError::Callback { message } => DashCoreError::Callback { message },
        CoreError::InteractionRequired {
            interaction_id,
            prompt,
        } => DashCoreError::InteractionRequired {
            interaction_id,
            prompt,
        },
        CoreError::ContextOverflow => DashCoreError::ContextOverflow,
    }
}
