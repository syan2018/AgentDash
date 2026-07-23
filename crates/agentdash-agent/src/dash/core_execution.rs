use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{Arc, Mutex},
};

use agentdash_agent_core::{
    CoreBeforeToolDecision, CoreCallbacks, CoreContext, CoreError, CoreEvent, CoreInput,
    CoreMessage, CoreOutput, CoreProvider, CoreRole, CoreTokenUsage, CoreTool, CoreToolCall,
    CoreToolCallbacks, CoreToolContent, CoreToolResult, FinishReason, ProviderEvent,
    ProviderEventStream, ProviderRequest, run_agent_loop,
};
use agentdash_agent_protocol::ToolProtocolProjector;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::{AgentItemId, AgentTurnId, HistoryContribution, HistoryEntryId, HistoryPayload};
use crate::ContentPart;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashExecutionFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

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
    pub tool_calls: Vec<DashToolCall>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub capability_key: String,
    pub source: String,
    pub tool_path: String,
    pub context_usage_kind: String,
    pub protocol_projector: ToolProtocolProjector,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashCoreContext {
    pub system_prompt: String,
    pub history: Vec<DashMessage>,
    pub tools: Vec<DashToolDefinition>,
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
    pub content: Vec<ContentPart>,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl DashToolResult {
    #[must_use]
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(ContentPart::extract_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum DashBeforeToolDecision {
    Invoke { call: DashToolCall },
    Deny { result: DashToolResult },
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
        call: DashToolCall,
        result: DashToolResult,
    },
    ProviderRoundCompleted {
        round: u32,
        finish_reason: DashFinishReason,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashExecutionEvent {
    pub turn_id: AgentTurnId,
    pub event: DashCoreEvent,
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
    #[error("Dash Agent provider failed ({code}): {message}")]
    Provider {
        code: String,
        message: String,
        retryable: bool,
    },
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
    pub fn code(&self) -> &str {
        match self {
            Self::Cancelled => "cancelled",
            Self::ProviderStreamDisconnected => "provider_stream_disconnected",
            Self::InvalidProviderTerminal => "invalid_provider_terminal",
            Self::Provider { code, .. } => code,
            Self::Tool { .. } => "tool_error",
            Self::Callback { .. } => "execution_callback_error",
            Self::InteractionRequired { .. } => "interaction_required",
            Self::ContextOverflow => "context_overflow",
        }
    }

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

    pub fn failure(&self) -> DashExecutionFailure {
        DashExecutionFailure {
            code: self.code().to_owned(),
            message: self.to_string(),
            retryable: self.retryable(),
        }
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
            code: "steering_unsupported".to_owned(),
            message: "provider does not accept in-flight steering".into(),
            retryable: false,
        })
    }
}

#[async_trait]
pub trait DashProviderRoundMaterializer: Send + Sync {
    async fn materialize_provider_round(
        &self,
        turn_id: &AgentTurnId,
        draft: DashProviderRequest,
    ) -> Result<DashProviderRequest, DashCoreError>;
}

#[derive(Clone, Default)]
pub struct DashProviderRoundSnapshots {
    requests: Arc<Mutex<BTreeMap<u32, DashProviderRequest>>>,
}

impl DashProviderRoundSnapshots {
    #[must_use]
    pub fn snapshot(&self, round: u32) -> Option<DashProviderRequest> {
        self.requests
            .lock()
            .expect("Dash provider round snapshot lock poisoned")
            .get(&round)
            .cloned()
    }

    fn pin(&self, request: DashProviderRequest) -> DashProviderRequest {
        let mut requests = self
            .requests
            .lock()
            .expect("Dash provider round snapshot lock poisoned");
        requests.entry(request.round).or_insert(request).clone()
    }

    #[must_use]
    pub fn tool_projector(&self, round: u32, tool_name: &str) -> Option<ToolProtocolProjector> {
        self.snapshot(round).and_then(|request| {
            request
                .tools
                .into_iter()
                .find(|tool| tool.name == tool_name)
                .map(|tool| tool.protocol_projector)
        })
    }
}

struct StaticDashProviderRoundMaterializer;

#[async_trait]
impl DashProviderRoundMaterializer for StaticDashProviderRoundMaterializer {
    async fn materialize_provider_round(
        &self,
        _turn_id: &AgentTurnId,
        draft: DashProviderRequest,
    ) -> Result<DashProviderRequest, DashCoreError> {
        Ok(draft)
    }
}

#[async_trait]
pub trait DashToolCallbacks: Send + Sync {
    async fn before_tool(
        &self,
        _turn_id: &AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashBeforeToolDecision, DashCoreError> {
        Ok(DashBeforeToolDecision::Invoke { call })
    }

    async fn invoke(
        &self,
        turn_id: &AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError>;

    async fn after_tool(
        &self,
        _turn_id: &AgentTurnId,
        _call: &DashToolCall,
        result: DashToolResult,
    ) -> Result<DashToolResult, DashCoreError> {
        Ok(result)
    }
}

#[async_trait]
pub trait DashExecutionCallbacks: Send + Sync {
    async fn emit(&self, event: DashExecutionEvent) -> Result<(), DashCoreError>;
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
        self.run_with_materializer(
            provider,
            tools,
            callbacks,
            &StaticDashProviderRoundMaterializer,
            DashProviderRoundSnapshots::default(),
            cancel,
        )
        .await
    }

    pub async fn run_with_materializer(
        self,
        provider: &dyn DashProvider,
        tools: &dyn DashToolCallbacks,
        callbacks: &dyn DashExecutionCallbacks,
        materializer: &dyn DashProviderRoundMaterializer,
        round_snapshots: DashProviderRoundSnapshots,
        cancel: DashCancellation,
    ) -> Result<DashCoreTurnResult, DashCoreError> {
        let seed_tools = self
            .context
            .tools
            .iter()
            .map(|tool| (tool.name.clone(), tool.clone()))
            .collect::<BTreeMap<_, _>>();
        let provider = ProviderAdapter {
            inner: provider,
            turn_id: self.turn_id.clone(),
            seed_tools,
            materializer,
            round_snapshots,
        };
        let tools = ToolAdapter {
            inner: tools,
            turn_id: self.turn_id.clone(),
        };
        let callbacks = CallbackAdapter {
            inner: callbacks,
            turn_id: self.turn_id.clone(),
        };
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
        let output_item_id = execution_assistant_item_id(&self.turn_id, output.provider_rounds);
        let history = vec![
            HistoryContribution {
                entry_id: self.output_started_entry_id,
                payload: HistoryPayload::ItemStarted {
                    turn_id: self.turn_id.clone(),
                    item_id: output_item_id.clone(),
                    kind: super::ItemKind::AssistantMessage,
                },
            },
            HistoryContribution {
                entry_id: self.output_entry_id,
                payload: HistoryPayload::AgentOutput {
                    turn_id: self.turn_id.clone(),
                    item_id: Some(output_item_id.clone()),
                    content: output.assistant_message.content.clone(),
                },
            },
            HistoryContribution {
                entry_id: self.output_completed_entry_id,
                payload: HistoryPayload::ItemCompleted {
                    turn_id: self.turn_id.clone(),
                    item_id: output_item_id,
                },
            },
            HistoryContribution {
                entry_id: self.terminal_entry_id,
                payload: HistoryPayload::TurnCompleted {
                    turn_id: self.turn_id,
                    completed_at_ms: crate::model::message::now_millis(),
                },
            },
        ];
        Ok(DashCoreTurnResult {
            core_output: output,
            history,
        })
    }
}

pub fn execution_assistant_item_id(turn_id: &AgentTurnId, round: u32) -> AgentItemId {
    AgentItemId::new(format!("{}:provider-round:{round}:assistant", turn_id.0))
}

pub fn execution_tool_item_id(turn_id: &AgentTurnId, call_id: &str) -> AgentItemId {
    AgentItemId::new(format!("{}:tool:{call_id}", turn_id.0))
}

struct ProviderAdapter<'a> {
    inner: &'a dyn DashProvider,
    turn_id: AgentTurnId,
    seed_tools: BTreeMap<String, DashToolDefinition>,
    materializer: &'a dyn DashProviderRoundMaterializer,
    round_snapshots: DashProviderRoundSnapshots,
}

#[async_trait]
impl CoreProvider for ProviderAdapter<'_> {
    async fn stream(&self, request: ProviderRequest) -> Result<ProviderEventStream, CoreError> {
        let draft = DashProviderRequest {
            system_prompt: request.system_prompt,
            messages: request.messages.into_iter().map(dash_message).collect(),
            tools: request
                .tools
                .into_iter()
                .map(|tool| {
                    let accepted = self
                        .seed_tools
                        .get(&tool.name)
                        .expect("Dash Core tool must retain its accepted definition");
                    DashToolDefinition {
                        name: tool.name,
                        description: tool.description,
                        input_schema: tool.input_schema,
                        capability_key: accepted.capability_key.clone(),
                        source: accepted.source.clone(),
                        tool_path: accepted.tool_path.clone(),
                        context_usage_kind: accepted.context_usage_kind.clone(),
                        protocol_projector: accepted.protocol_projector.clone(),
                    }
                })
                .collect(),
            round: request.round,
        };
        let request = if let Some(snapshot) = self.round_snapshots.snapshot(draft.round) {
            snapshot
        } else {
            let materialized = self
                .materializer
                .materialize_provider_round(&self.turn_id, draft)
                .await
                .map_err(core_error)?;
            self.round_snapshots.pin(materialized)
        };
        let stream = self.inner.stream(request).await.map_err(core_error)?;
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
    async fn before_tool(&self, call: CoreToolCall) -> Result<CoreBeforeToolDecision, CoreError> {
        self.inner
            .before_tool(
                &self.turn_id,
                DashToolCall {
                    call_id: call.call_id,
                    name: call.name,
                    arguments: call.arguments,
                },
            )
            .await
            .map(|decision| match decision {
                DashBeforeToolDecision::Invoke { call } => CoreBeforeToolDecision::Invoke {
                    call: CoreToolCall {
                        call_id: call.call_id,
                        name: call.name,
                        arguments: call.arguments,
                    },
                },
                DashBeforeToolDecision::Deny { result } => CoreBeforeToolDecision::Deny {
                    result: CoreToolResult {
                        call_id: result.call_id,
                        content: result.content.into_iter().map(core_tool_content).collect(),
                        is_error: result.is_error,
                        details: result.details,
                    },
                },
            })
            .map_err(core_error)
    }

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
                content: result.content.into_iter().map(core_tool_content).collect(),
                is_error: result.is_error,
                details: result.details,
            })
            .map_err(core_error)
    }

    async fn after_tool(
        &self,
        call: &CoreToolCall,
        result: CoreToolResult,
    ) -> Result<CoreToolResult, CoreError> {
        self.inner
            .after_tool(
                &self.turn_id,
                &DashToolCall {
                    call_id: call.call_id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                },
                DashToolResult {
                    call_id: result.call_id,
                    content: result.content.into_iter().map(dash_tool_content).collect(),
                    is_error: result.is_error,
                    details: result.details,
                },
            )
            .await
            .map(|result| CoreToolResult {
                call_id: result.call_id,
                content: result.content.into_iter().map(core_tool_content).collect(),
                is_error: result.is_error,
                details: result.details,
            })
            .map_err(core_error)
    }
}

struct CallbackAdapter<'a> {
    inner: &'a dyn DashExecutionCallbacks,
    turn_id: AgentTurnId,
}

#[async_trait]
impl CoreCallbacks for CallbackAdapter<'_> {
    async fn emit(&self, event: CoreEvent) -> Result<(), CoreError> {
        self.inner
            .emit(DashExecutionEvent {
                turn_id: self.turn_id.clone(),
                event: dash_event(event),
            })
            .await
            .map_err(core_error)
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
        tool_calls: message
            .tool_calls
            .into_iter()
            .map(|call| CoreToolCall {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            })
            .collect(),
        is_error: message.is_error,
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
        tool_calls: message
            .tool_calls
            .into_iter()
            .map(|call| DashToolCall {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            })
            .collect(),
        is_error: message.is_error,
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
        CoreEvent::ToolCallCompleted {
            round,
            call,
            result,
        } => DashCoreEvent::ToolCallCompleted {
            round,
            call: DashToolCall {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            },
            result: DashToolResult {
                call_id: result.call_id,
                content: result.content.into_iter().map(dash_tool_content).collect(),
                is_error: result.is_error,
                details: result.details,
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

fn core_tool_content(content: ContentPart) -> CoreToolContent {
    match content {
        ContentPart::Text { text } => CoreToolContent::Text { text },
        ContentPart::Image { mime_type, data } => CoreToolContent::Image { mime_type, data },
        ContentPart::Reasoning {
            text,
            id,
            signature,
        } => CoreToolContent::Reasoning {
            text,
            id,
            signature,
        },
    }
}

fn dash_tool_content(content: CoreToolContent) -> ContentPart {
    match content {
        CoreToolContent::Text { text } => ContentPart::Text { text },
        CoreToolContent::Image { mime_type, data } => ContentPart::Image { mime_type, data },
        CoreToolContent::Reasoning {
            text,
            id,
            signature,
        } => ContentPart::Reasoning {
            text,
            id,
            signature,
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
        DashCoreError::Provider {
            code,
            message,
            retryable,
        } => CoreError::Provider {
            code,
            message,
            retryable,
        },
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
        CoreError::Provider {
            code,
            message,
            retryable,
        } => DashCoreError::Provider {
            code,
            message,
            retryable,
        },
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
