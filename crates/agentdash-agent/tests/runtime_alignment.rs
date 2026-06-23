use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use agentdash_agent::agent_loop::AgentLoopConfig;
use agentdash_agent::types::TokenUsage;
use agentdash_agent::{
    Agent, AgentConfig, AgentContext, AgentError, AgentEvent, AgentMessage, AgentTool,
    AgentToolError, AgentToolResult, AssistantStreamEvent, BeforeStopInput, BridgeError,
    BridgeRequest, BridgeResponse, ContentPart, DynAgentTool, LlmBridge, ReadableIdRegistry,
    StopDecision, StopReason, ToolApprovalOutcome, ToolCallInfo, ToolDefinition,
    ToolResultCacheWrite, ToolResultRefContext, agent_loop::AgentEventSink,
};
use async_trait::async_trait;
use futures::Stream;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
enum ScriptStep {
    Chunk(Box<agentdash_agent::StreamChunk>),
    Signal(Arc<Notify>),
    Wait(Arc<Notify>),
}

impl ScriptStep {
    fn chunk(chunk: agentdash_agent::StreamChunk) -> Self {
        Self::Chunk(Box::new(chunk))
    }
}

#[derive(Clone)]
struct ScriptedBridge {
    scripts: Arc<Mutex<VecDeque<Vec<ScriptStep>>>>,
    tool_snapshots: Arc<Mutex<Vec<Vec<String>>>>,
    message_snapshots: Arc<Mutex<Vec<Vec<String>>>>,
}

impl ScriptedBridge {
    fn new(scripts: Vec<Vec<ScriptStep>>) -> Self {
        Self {
            scripts: Arc::new(Mutex::new(scripts.into())),
            tool_snapshots: Arc::new(Mutex::new(Vec::new())),
            message_snapshots: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn tool_snapshots(&self) -> Vec<Vec<String>> {
        self.tool_snapshots.lock().await.clone()
    }

    async fn message_snapshots(&self) -> Vec<Vec<String>> {
        self.message_snapshots.lock().await.clone()
    }
}

#[async_trait]
impl LlmBridge for ScriptedBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn Stream<Item = agentdash_agent::StreamChunk> + Send>> {
        self.message_snapshots.lock().await.push(
            request
                .messages
                .iter()
                .map(|message| message.first_text().unwrap_or_default().to_string())
                .collect(),
        );
        self.tool_snapshots
            .lock()
            .await
            .push(request.tools.iter().map(|tool| tool.name.clone()).collect());
        let script = self
            .scripts
            .lock()
            .await
            .pop_front()
            .expect("missing scripted bridge response");
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            for step in script {
                match step {
                    ScriptStep::Chunk(chunk) => {
                        if tx.send(*chunk).await.is_err() {
                            return;
                        }
                    }
                    ScriptStep::Signal(notify) => notify.notify_waiters(),
                    ScriptStep::Wait(notify) => notify.notified().await,
                }
            }
        });
        Box::pin(ReceiverStream::new(rx))
    }
}

#[derive(Clone)]
struct RecordingTool {
    executed: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct NamedTool {
    name: String,
    executed: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct LargeResultTool {
    name: String,
    final_text: String,
    update_text: Option<String>,
    executed: Arc<AtomicUsize>,
}

impl NamedTool {
    fn new(name: impl Into<String>, executed: Arc<AtomicUsize>) -> Self {
        Self {
            name: name.into(),
            executed,
        }
    }
}

impl LargeResultTool {
    fn new(name: impl Into<String>, final_text: String, update_text: Option<String>) -> Self {
        Self {
            name: name.into(),
            final_text,
            update_text,
            executed: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl AgentTool for NamedTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "named test tool"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            },
            "required": ["value"],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<agentdash_agent::ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        self.executed.fetch_add(1, Ordering::SeqCst);
        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!("{}:{args}", self.name))],
            is_error: false,
            details: None,
        })
    }
}

#[async_trait]
impl AgentTool for LargeResultTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "large result test tool"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        _args: serde_json::Value,
        _cancel: CancellationToken,
        on_update: Option<agentdash_agent::ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        self.executed.fetch_add(1, Ordering::SeqCst);
        if let (Some(on_update), Some(update_text)) = (on_update, self.update_text.as_ref()) {
            on_update(AgentToolResult {
                content: vec![ContentPart::text(update_text.clone())],
                is_error: false,
                details: Some(serde_json::json!({ "phase": "update" })),
            });
        }
        Ok(AgentToolResult {
            content: vec![ContentPart::text(self.final_text.clone())],
            is_error: false,
            details: Some(serde_json::json!({ "phase": "final" })),
        })
    }
}

#[async_trait]
impl AgentTool for RecordingTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "echo"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            },
            "required": ["value"],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<agentdash_agent::ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        self.executed.fetch_add(1, Ordering::SeqCst);
        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!("executed:{args}"))],
            is_error: false,
            details: None,
        })
    }
}

fn bridge_response(message: AgentMessage) -> BridgeResponse {
    BridgeResponse {
        message,
        raw_content: vec![],
        usage: TokenUsage::default(),
    }
}

fn assistant_text(text: &str) -> AgentMessage {
    AgentMessage::Assistant {
        content: vec![ContentPart::text(text)],
        tool_calls: vec![],
        stop_reason: Some(StopReason::Stop),
        error_message: None,
        usage: None,
        timestamp: Some(agentdash_agent::types::now_millis()),
    }
}

fn assistant_tool_call(id: &str, arguments: serde_json::Value) -> AgentMessage {
    assistant_tool_call_named(id, "echo", arguments)
}

fn assistant_tool_call_named(id: &str, name: &str, arguments: serde_json::Value) -> AgentMessage {
    AgentMessage::Assistant {
        content: vec![],
        tool_calls: vec![ToolCallInfo {
            id: id.to_string(),
            call_id: Some(id.to_string()),
            name: name.to_string(),
            arguments,
        }],
        stop_reason: Some(StopReason::ToolUse),
        error_message: None,
        usage: None,
        timestamp: Some(agentdash_agent::types::now_millis()),
    }
}

fn collecting_sink(events: Arc<Mutex<Vec<AgentEvent>>>) -> AgentEventSink {
    Arc::new(move |event| {
        let events = events.clone();
        Box::pin(async move {
            events.lock().await.push(event);
        })
    })
}

fn event_kind(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::AgentStart => "agent_start",
        AgentEvent::AgentEnd { .. } => "agent_end",
        AgentEvent::TurnStart => "turn_start",
        AgentEvent::TurnEnd { .. } => "turn_end",
        AgentEvent::MessageStart { .. } => "message_start",
        AgentEvent::MessageUpdate { .. } => "message_update",
        AgentEvent::MessageEnd { .. } => "message_end",
        AgentEvent::ContextCompactionStarted { .. } => "context_compaction_started",
        AgentEvent::ContextCompacted { .. } => "context_compacted",
        AgentEvent::ContextCompactionFailed { .. } => "context_compaction_failed",
        AgentEvent::ProviderAttemptStatus { .. } => "provider_attempt_status",
        AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
        AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
        AgentEvent::ToolExecutionPendingApproval { .. } => "tool_execution_pending_approval",
        AgentEvent::ToolExecutionApprovalResolved { .. } => "tool_execution_approval_resolved",
        AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
    }
}

fn provider_statuses(events: &[AgentEvent]) -> Vec<agentdash_agent::ProviderAttemptStatus> {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ProviderAttemptStatus { status } => Some(status.clone()),
            _ => None,
        })
        .collect()
}

const LARGE_RESULT_SENTINEL: &str = "AGENTDASH_RUNTIME_ALIGNMENT_LARGE_RESULT_SENTINEL";

fn large_result_text() -> String {
    format!(
        "{}{}{}",
        "h".repeat(70_000),
        LARGE_RESULT_SENTINEL,
        "t".repeat(70_000)
    )
}

fn assert_bounded_tool_result(result: &AgentToolResult, lifecycle_item_id: &str) {
    let expected_lifecycle_path = lifecycle_path_for_test_item(lifecycle_item_id);
    let text = result
        .content
        .first()
        .and_then(ContentPart::extract_text)
        .expect("bounded result should have text");
    assert!(text.contains("[tool result truncated]"));
    assert!(!text.contains(LARGE_RESULT_SENTINEL));
    assert!(text.contains(&expected_lifecycle_path));
    assert_eq!(
        result
            .details
            .as_ref()
            .and_then(|details| details.get("lifecycle_path"))
            .and_then(serde_json::Value::as_str),
        Some(expected_lifecycle_path.as_str())
    );
    assert_eq!(
        result
            .details
            .as_ref()
            .and_then(|details| details.get("truncation"))
            .and_then(|truncation| truncation.get("truncated"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

fn lifecycle_path_for_test_item(item_id: &str) -> String {
    item_id
        .split_once(':')
        .map(|(turn_alias, body_alias)| {
            format!("lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt")
        })
        .unwrap_or_else(|| format!("lifecycle://session/tool-results/{item_id}/result.txt"))
}

#[tokio::test]
async fn agent_loop_emits_prompt_before_assistant_and_returns_new_messages() {
    let bridge = ScriptedBridge::new(vec![vec![ScriptStep::chunk(
        agentdash_agent::StreamChunk::Done(bridge_response(assistant_text("hi"))),
    )]]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };

    let tool_instances: Vec<DynAgentTool> = vec![];
    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &tool_instances,
        &AgentLoopConfig::default(),
        &bridge,
        &sink,
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");

    let collected = events.lock().await.clone();
    let kinds = collected.iter().map(event_kind).collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            "agent_start",
            "turn_start",
            "message_start",
            "message_end",
            "provider_attempt_status",
            "provider_attempt_status",
            "provider_attempt_status",
            "message_start",
            "message_end",
            "turn_end",
            "agent_end",
        ]
    );
    assert_eq!(new_messages.len(), 2);
    assert_eq!(context.messages.len(), 2);
}

#[tokio::test]
async fn pre_delta_retry_does_not_pollute_context_and_retries_request() {
    let retryable_error = BridgeError::provider(
        "upstream 503",
        agentdash_agent::ProviderErrorClassification::retryable()
            .with_http_status(503)
            .with_retry_after_ms(0),
    );
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Error(
            retryable_error,
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("recovered")),
        ))],
    ]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &[],
        &AgentLoopConfig::default(),
        &bridge,
        &sink,
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should recover from pre-delta retryable provider error");

    let snapshots = bridge.message_snapshots().await;
    assert_eq!(snapshots.len(), 2);
    assert_eq!(new_messages.len(), 2);
    assert_eq!(context.messages.len(), 2);
    assert_eq!(context.messages[1].first_text(), Some("recovered"));
    assert!(
        !context
            .messages
            .iter()
            .any(|message| message.first_text() == Some("upstream 503"))
    );

    let provider_status_count = events
        .lock()
        .await
        .iter()
        .map(event_kind)
        .filter(|kind| *kind == "provider_attempt_status")
        .count();
    assert!(provider_status_count >= 5);
}

#[tokio::test]
async fn pre_delta_retryable_error_exhaustion_emits_single_final_failure_without_polluting_context()
{
    let retryable_error = BridgeError::provider(
        "upstream unavailable",
        agentdash_agent::ProviderErrorClassification::retryable()
            .with_http_status(503)
            .with_retry_after_ms(0),
    );
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Error(
            retryable_error.clone(),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Error(
            retryable_error.clone(),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Error(
            retryable_error,
        ))],
    ]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &[],
        &AgentLoopConfig::default(),
        &bridge,
        &sink,
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should resolve exhausted retry as final assistant failure");

    let snapshots = bridge.message_snapshots().await;
    assert_eq!(snapshots.len(), 3);
    assert!(
        snapshots
            .iter()
            .all(|snapshot| snapshot == &vec!["hello".to_string()])
    );
    assert_eq!(new_messages.len(), 2);
    assert_eq!(context.messages.len(), 2);
    assert!(matches!(
        context.messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Error),
            ..
        })
    ));
    assert_eq!(
        context
            .messages
            .iter()
            .filter(|message| message.first_text() == Some("upstream unavailable"))
            .count(),
        1
    );

    let collected = events.lock().await.clone();
    let statuses = provider_statuses(&collected);
    assert_eq!(
        statuses
            .iter()
            .filter(|status| status.phase == agentdash_agent::ProviderAttemptPhase::RetryScheduled)
            .count(),
        2
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| status.phase == agentdash_agent::ProviderAttemptPhase::Failed)
            .count(),
        1
    );
}

#[tokio::test]
async fn retryable_error_after_visible_delta_does_not_retry() {
    let retryable_error = BridgeError::provider(
        "upstream 503 after delta",
        agentdash_agent::ProviderErrorClassification::retryable()
            .with_http_status(503)
            .with_retry_after_ms(0),
    );
    let bridge = ScriptedBridge::new(vec![
        vec![
            ScriptStep::chunk(agentdash_agent::StreamChunk::TextDelta(
                "partial".to_string(),
            )),
            ScriptStep::chunk(agentdash_agent::StreamChunk::Error(retryable_error)),
        ],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("should not retry")),
        ))],
    ]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &[],
        &AgentLoopConfig::default(),
        &bridge,
        &sink,
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should surface post-delta provider error without retrying");

    assert_eq!(bridge.message_snapshots().await.len(), 1);
    assert!(matches!(
        new_messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Error),
            ..
        })
    ));
    assert_eq!(
        context.messages.last().and_then(AgentMessage::first_text),
        Some("upstream 503 after delta")
    );

    let collected = events.lock().await.clone();
    let statuses = provider_statuses(&collected);
    assert!(statuses.iter().any(|status| {
        status.phase == agentdash_agent::ProviderAttemptPhase::Streaming && status.attempt == 1
    }));
    assert!(
        !statuses
            .iter()
            .any(|status| status.phase == agentdash_agent::ProviderAttemptPhase::RetryScheduled)
    );
}

#[tokio::test]
async fn provider_abort_error_does_not_retry() {
    let aborted_error = BridgeError::provider(
        "request aborted",
        agentdash_agent::ProviderErrorClassification::aborted(),
    );
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Error(
            aborted_error,
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("should not retry")),
        ))],
    ]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &[],
        &AgentLoopConfig::default(),
        &bridge,
        &sink,
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should surface provider abort without retrying");

    assert_eq!(bridge.message_snapshots().await.len(), 1);
    assert!(matches!(
        new_messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Aborted),
            ..
        })
    ));

    let collected = events.lock().await.clone();
    assert!(
        !provider_statuses(&collected)
            .iter()
            .any(|status| status.phase == agentdash_agent::ProviderAttemptPhase::RetryScheduled)
    );
}

#[tokio::test]
async fn fatal_provider_error_does_not_retry() {
    let fatal_error = BridgeError::provider(
        "invalid request schema",
        agentdash_agent::ProviderErrorClassification::fatal().with_provider_code("invalid_request"),
    );
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Error(
            fatal_error,
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("should not retry")),
        ))],
    ]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &[],
        &AgentLoopConfig::default(),
        &bridge,
        &sink,
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should surface fatal provider error without retrying");

    assert_eq!(bridge.message_snapshots().await.len(), 1);
    assert!(matches!(
        new_messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Error),
            ..
        })
    ));
    assert_eq!(
        context.messages.last().and_then(AgentMessage::first_text),
        Some("invalid request schema")
    );

    let collected = events.lock().await.clone();
    assert!(
        !provider_statuses(&collected)
            .iter()
            .any(|status| status.phase == agentdash_agent::ProviderAttemptPhase::RetryScheduled)
    );
}

#[tokio::test]
async fn agent_updates_runtime_state_and_rejects_reentrancy() {
    let first_delta_sent = Arc::new(Notify::new());
    let release_stream = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![vec![
        ScriptStep::chunk(agentdash_agent::StreamChunk::TextDelta("hel".to_string())),
        ScriptStep::Signal(first_delta_sent.clone()),
        ScriptStep::Wait(release_stream.clone()),
        ScriptStep::chunk(agentdash_agent::StreamChunk::Done(bridge_response(
            assistant_text("hello"),
        ))),
    ]]);
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());

    let (_rx, handle) = agent
        .prompt(AgentMessage::user("hi"))
        .expect("prompt should start");
    first_delta_sent.notified().await;

    let state = agent.state().await;
    assert!(state.is_streaming);
    assert_eq!(state.messages.len(), 1);
    assert!(matches!(
        state.stream_message,
        Some(AgentMessage::Assistant { .. })
    ));
    assert_eq!(
        state
            .stream_message
            .as_ref()
            .and_then(AgentMessage::first_text),
        Some("hel")
    );

    assert!(matches!(
        agent.prompt(AgentMessage::user("second")),
        Err(AgentError::InvalidState(_))
    ));
    assert!(matches!(
        agent.continue_loop(),
        Err(AgentError::InvalidState(_))
    ));

    release_stream.notify_waiters();
    let new_messages = handle
        .await
        .expect("task should not panic")
        .expect("run should succeed");
    assert_eq!(new_messages.len(), 2);

    let final_state = agent.state().await;
    assert!(!final_state.is_streaming);
    assert!(final_state.stream_message.is_none());
    assert_eq!(final_state.messages.len(), 2);
    assert_eq!(
        final_state
            .messages
            .last()
            .and_then(AgentMessage::first_text),
        Some("hello")
    );
}

#[tokio::test]
async fn continue_from_assistant_tail_consumes_queued_messages_one_at_a_time() {
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("after steering 1")),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("after steering 2")),
        ))],
    ]);
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());
    agent
        .replace_messages(vec![AgentMessage::user("initial"), assistant_text("seed")])
        .await;
    agent.steer(AgentMessage::user("steering 1")).await;
    agent.steer(AgentMessage::user("steering 2")).await;

    let (_rx, handle) = agent.continue_loop().expect("continue should start");
    handle
        .await
        .expect("task should not panic")
        .expect("run should succeed");

    let state = agent.state().await;
    let texts = state
        .messages
        .iter()
        .map(|message| message.first_text().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        vec![
            "initial",
            "seed",
            "steering 1",
            "after steering 1",
            "steering 2",
            "after steering 2",
        ]
    );
}

#[tokio::test]
async fn continue_from_assistant_tail_consumes_follow_up_messages() {
    let bridge = ScriptedBridge::new(vec![vec![ScriptStep::chunk(
        agentdash_agent::StreamChunk::Done(bridge_response(assistant_text("after follow up"))),
    )]]);
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());
    agent
        .replace_messages(vec![AgentMessage::user("initial"), assistant_text("seed")])
        .await;
    agent.follow_up(AgentMessage::user("follow up")).await;

    let (_rx, handle) = agent.continue_loop().expect("continue should start");
    handle
        .await
        .expect("task should not panic")
        .expect("run should succeed");

    let state = agent.state().await;
    let texts = state
        .messages
        .iter()
        .map(|message| message.first_text().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        vec!["initial", "seed", "follow up", "after follow up"]
    );
}

#[tokio::test]
async fn running_agent_refreshes_tool_schema_before_next_llm_request() {
    let first_request_started = Arc::new(Notify::new());
    let release_first_response = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![
        vec![
            ScriptStep::Signal(first_request_started.clone()),
            ScriptStep::Wait(release_first_response.clone()),
            ScriptStep::chunk(agentdash_agent::StreamChunk::Done(bridge_response(
                assistant_text("first pass"),
            ))),
        ],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("second pass")),
        ))],
    ]);
    let old_tool: DynAgentTool =
        Arc::new(NamedTool::new("old_tool", Arc::new(AtomicUsize::new(0))));
    let new_tool: DynAgentTool =
        Arc::new(NamedTool::new("new_tool", Arc::new(AtomicUsize::new(0))));
    let mut agent = Agent::new(Arc::new(bridge.clone()), AgentConfig::default());
    agent.set_runtime_delegate(Some(Arc::new(EmptyContinueDelegate::default())));
    agent.set_tools(vec![old_tool]);

    let (_rx, handle) = agent
        .prompt(AgentMessage::user("start"))
        .expect("prompt should start");
    first_request_started.notified().await;
    agent.set_tools(vec![new_tool]);
    release_first_response.notify_waiters();

    handle
        .await
        .expect("task should not panic")
        .expect("agent loop should succeed");

    let snapshots = bridge.tool_snapshots().await;
    assert_eq!(
        snapshots,
        vec![vec!["old_tool".to_string()], vec!["new_tool".to_string()]]
    );
}

#[tokio::test]
async fn running_agent_uses_live_tool_instances_for_tool_lookup() {
    let first_request_started = Arc::new(Notify::new());
    let release_first_response = Arc::new(Notify::new());
    let new_tool_executed = Arc::new(AtomicUsize::new(0));
    let bridge = ScriptedBridge::new(vec![
        vec![
            ScriptStep::Signal(first_request_started.clone()),
            ScriptStep::Wait(release_first_response.clone()),
            ScriptStep::chunk(agentdash_agent::StreamChunk::Done(bridge_response(
                assistant_text("first pass"),
            ))),
        ],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call_named(
                "tool-new-1",
                "new_tool",
                serde_json::json!({ "value": "from live registry" }),
            )),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let old_tool: DynAgentTool =
        Arc::new(NamedTool::new("old_tool", Arc::new(AtomicUsize::new(0))));
    let new_tool: DynAgentTool = Arc::new(NamedTool::new("new_tool", new_tool_executed.clone()));
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());
    agent.set_runtime_delegate(Some(Arc::new(EmptyContinueDelegate::default())));
    agent.set_tools(vec![old_tool]);

    let (_rx, handle) = agent
        .prompt(AgentMessage::user("start"))
        .expect("prompt should start");
    first_request_started.notified().await;
    agent.set_tools(vec![new_tool]);
    release_first_response.notify_waiters();

    let new_messages = handle
        .await
        .expect("task should not panic")
        .expect("agent loop should succeed");

    assert_eq!(new_tool_executed.load(Ordering::SeqCst), 1);
    assert!(!new_messages.iter().any(|message| {
        matches!(
            message,
            AgentMessage::ToolResult { is_error: true, .. }
                if message
                    .first_text()
                    .is_some_and(|text| text.contains("Tool new_tool not found"))
        )
    }));
}

#[tokio::test]
async fn empty_continue_decision_keeps_loop_running_without_fake_messages() {
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("first pass")),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("second pass")),
        ))],
    ]);
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };
    let tool_instances: Vec<DynAgentTool> = vec![];
    let config = AgentLoopConfig {
        runtime_delegate: Some(Arc::new(EmptyContinueDelegate::default())),
        ..AgentLoopConfig::default()
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &tool_instances,
        &config,
        &bridge,
        &collecting_sink(Arc::new(Mutex::new(Vec::new()))),
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");

    let texts = new_messages
        .iter()
        .filter_map(|message| message.first_text().map(ToString::to_string))
        .collect::<Vec<_>>();
    assert_eq!(texts, vec!["hello", "first pass", "second pass"]);
}

#[tokio::test]
async fn repeated_empty_continue_decision_fails_instead_of_spinning() {
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("first pass")),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("second pass")),
        ))],
    ]);
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![],
    };
    let tool_instances: Vec<DynAgentTool> = vec![];
    let before_stop_calls = Arc::new(AtomicUsize::new(0));
    let config = AgentLoopConfig {
        runtime_delegate: Some(Arc::new(EmptyContinueDelegate {
            before_stop_calls: before_stop_calls.clone(),
            always_continue: true,
        })),
        ..AgentLoopConfig::default()
    };

    let error = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("hello")],
        &mut context,
        &tool_instances,
        &config,
        &bridge,
        &collecting_sink(Arc::new(Mutex::new(Vec::new()))),
        CancellationToken::new(),
    )
    .await
    .expect_err("连续空 continue 应被截断为错误");

    assert!(matches!(error, AgentError::ContinueError(_)));
    assert!(error.to_string().contains("空 continuation 连续触发"));
    assert_eq!(before_stop_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn tool_arguments_are_validated_before_before_tool_call_hook() {
    let before_calls = Arc::new(AtomicUsize::new(0));
    let executed = Arc::new(AtomicUsize::new(0));
    let tool: DynAgentTool = Arc::new(RecordingTool {
        executed: executed.clone(),
    });
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call(
                "tool-1",
                serde_json::json!({ "value": 1 }),
            )),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let tool_instances: Vec<DynAgentTool> = vec![tool.clone()];
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![ToolDefinition::from_tool(tool.as_ref())],
    };
    let before_calls_clone = before_calls.clone();
    let config = AgentLoopConfig {
        before_tool_call: Some(Arc::new(move |_ctx, _cancel| {
            let before_calls = before_calls_clone.clone();
            Box::pin(async move {
                before_calls.fetch_add(1, Ordering::SeqCst);
                None
            })
        })),
        ..AgentLoopConfig::default()
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("run tool")],
        &mut context,
        &tool_instances,
        &config,
        &bridge,
        &collecting_sink(Arc::new(Mutex::new(Vec::new()))),
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");

    assert_eq!(before_calls.load(Ordering::SeqCst), 0);
    assert_eq!(executed.load(Ordering::SeqCst), 0);
    let tool_result = new_messages
        .iter()
        .find(|message| matches!(message, AgentMessage::ToolResult { .. }))
        .expect("tool result should exist");
    assert!(matches!(
        tool_result,
        AgentMessage::ToolResult { is_error: true, .. }
    ));
    assert!(
        tool_result
            .first_text()
            .expect("tool result should have text")
            .contains("arguments are invalid")
    );
}

#[tokio::test]
async fn large_final_tool_result_is_bounded_before_events_and_next_request() {
    let tool_call_id = "tool-large-final-1";
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call_named(
                tool_call_id,
                "large_tool",
                serde_json::json!({}),
            )),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let tool: DynAgentTool = Arc::new(LargeResultTool::new(
        "large_tool",
        large_result_text(),
        None,
    ));
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![ToolDefinition::from_tool(tool.as_ref())],
    };
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache_writes = Arc::new(StdMutex::new(Vec::<ToolResultCacheWrite>::new()));
    let cache_writes_for_writer = cache_writes.clone();
    let readable_ids = ReadableIdRegistry::new();
    let stable_item_id = "turn_001:tool_001".to_string();
    let config = AgentLoopConfig {
        tool_result_ref_context: Some(ToolResultRefContext {
            session_id: "session-large".to_string(),
            raw_turn_id: "turn-large".to_string(),
            readable_ids,
            cache_writer: Some(Arc::new(move |write| {
                cache_writes_for_writer
                    .lock()
                    .expect("cache write lock poisoned")
                    .push(write);
            })),
        }),
        ..AgentLoopConfig::default()
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("run large tool")],
        &mut context,
        &[tool],
        &config,
        &bridge,
        &collecting_sink(events.clone()),
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");

    let tool_result_message = new_messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::ToolResult {
                content,
                details,
                is_error,
                ..
            } => Some(AgentToolResult {
                content: content.clone(),
                details: details.clone(),
                is_error: *is_error,
            }),
            _ => None,
        })
        .expect("tool result message should exist");
    assert!(!tool_result_message.is_error);
    assert_bounded_tool_result(&tool_result_message, &stable_item_id);

    let collected = events.lock().await.clone();
    let end_result = collected
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionEnd {
                tool_call_id: id,
                tool_name,
                result,
                is_error,
            } if id == tool_call_id => {
                assert_eq!(tool_name, "large_tool");
                assert!(!is_error);
                Some(
                    serde_json::from_value::<AgentToolResult>(result.clone())
                        .expect("tool end result should decode"),
                )
            }
            _ => None,
        })
        .expect("tool execution end should exist");
    assert_bounded_tool_result(&end_result, &stable_item_id);

    let message_end_result = collected
        .iter()
        .find_map(|event| match event {
            AgentEvent::MessageEnd {
                message:
                    AgentMessage::ToolResult {
                        content,
                        details,
                        is_error,
                        ..
                    },
            } => Some(AgentToolResult {
                content: content.clone(),
                details: details.clone(),
                is_error: *is_error,
            }),
            _ => None,
        })
        .expect("tool result message_end should exist");
    assert_bounded_tool_result(&message_end_result, &stable_item_id);
    {
        let writes = cache_writes.lock().expect("cache write lock poisoned");
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].session_id, "session-large");
        assert_eq!(writes[0].item_id, stable_item_id);
        assert_eq!(writes[0].turn_alias, "turn_001");
        assert_eq!(writes[0].body_alias, "tool_001");
        assert_eq!(writes[0].body_kind, "tool_result");
        assert_eq!(writes[0].raw_turn_id, "turn-large");
        assert_eq!(writes[0].raw_tool_call_id, tool_call_id);
        assert!(
            writes[0].text.contains(LARGE_RESULT_SENTINEL),
            "cache writer should receive the original body"
        );
        assert_eq!(
            writes[0].lifecycle_path,
            "lifecycle://session/tool-results/turn_001/tool_001/result.txt"
        );
    }

    let snapshots = bridge.message_snapshots().await;
    let second_request_messages = snapshots
        .get(1)
        .expect("second provider request should include tool result context");
    assert!(
        second_request_messages
            .iter()
            .any(|text| text.contains("[tool result truncated]"))
    );
    assert!(
        !second_request_messages
            .iter()
            .any(|text| text.contains(LARGE_RESULT_SENTINEL))
    );
}

#[tokio::test]
async fn large_tool_update_partial_result_is_bounded_before_serialization() {
    let tool_call_id = "tool-large-update-1";
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call_named(
                tool_call_id,
                "large_update_tool",
                serde_json::json!({}),
            )),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let tool: DynAgentTool = Arc::new(LargeResultTool::new(
        "large_update_tool",
        "small final".to_string(),
        Some(large_result_text()),
    ));
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![ToolDefinition::from_tool(tool.as_ref())],
    };
    let events = Arc::new(Mutex::new(Vec::new()));

    agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("run large update tool")],
        &mut context,
        &[tool],
        &AgentLoopConfig::default(),
        &bridge,
        &collecting_sink(events.clone()),
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");
    tokio::task::yield_now().await;

    let collected = events.lock().await.clone();
    let partial_result = collected
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionUpdate {
                tool_call_id: id,
                tool_name,
                args,
                partial_result,
            } if id == tool_call_id => {
                assert_eq!(tool_name, "large_update_tool");
                assert_eq!(args, &serde_json::json!({}));
                Some(
                    serde_json::from_value::<AgentToolResult>(partial_result.clone())
                        .expect("partial result should decode"),
                )
            }
            _ => None,
        })
        .expect("tool update should exist");
    assert_bounded_tool_result(&partial_result, "turn_001:tool_001");
}

#[tokio::test]
async fn large_immediate_tool_result_is_bounded() {
    let tool_call_id = "tool-large-immediate-1";
    let executed = Arc::new(AtomicUsize::new(0));
    let tool: DynAgentTool = Arc::new(RecordingTool {
        executed: executed.clone(),
    });
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call(
                tool_call_id,
                serde_json::json!({ "value": "x" }),
            )),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![ToolDefinition::from_tool(tool.as_ref())],
    };
    let events = Arc::new(Mutex::new(Vec::new()));
    let config = AgentLoopConfig {
        before_tool_call: Some(Arc::new(|_ctx, _cancel| {
            Box::pin(async move {
                Some(agentdash_agent::BeforeToolCallResult {
                    block: true,
                    reason: Some(large_result_text()),
                })
            })
        })),
        ..AgentLoopConfig::default()
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("blocked large tool")],
        &mut context,
        &[tool],
        &config,
        &bridge,
        &collecting_sink(events.clone()),
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");

    assert_eq!(executed.load(Ordering::SeqCst), 0);
    let tool_result_message = new_messages
        .iter()
        .find(|message| matches!(message, AgentMessage::ToolResult { .. }))
        .expect("immediate tool result should exist");
    assert!(
        !tool_result_message
            .first_text()
            .unwrap_or_default()
            .contains(LARGE_RESULT_SENTINEL)
    );
    assert!(
        tool_result_message
            .first_text()
            .unwrap_or_default()
            .contains("[tool result truncated]")
    );

    let collected = events.lock().await.clone();
    let end_result = collected
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionEnd {
                tool_call_id: id,
                result,
                is_error,
                ..
            } if id == tool_call_id => {
                assert!(*is_error);
                Some(
                    serde_json::from_value::<AgentToolResult>(result.clone())
                        .expect("tool end result should decode"),
                )
            }
            _ => None,
        })
        .expect("tool execution end should exist for immediate result");
    assert_bounded_tool_result(&end_result, "turn_001:tool_001");
}

#[tokio::test]
async fn large_approval_rejection_result_is_bounded_without_tool_execution_end() {
    let tool_call_id = "tool-large-reject-1";
    let executed = Arc::new(AtomicUsize::new(0));
    let tool: DynAgentTool = Arc::new(RecordingTool {
        executed: executed.clone(),
    });
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call(
                tool_call_id,
                serde_json::json!({ "value": "x" }),
            )),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![ToolDefinition::from_tool(tool.as_ref())],
    };
    let events = Arc::new(Mutex::new(Vec::new()));
    let config = AgentLoopConfig {
        runtime_delegate: Some(Arc::new(RejectingRuntimeDelegate)),
        await_tool_approval: Some(Arc::new(|_request, _cancel| {
            Box::pin(async move {
                ToolApprovalOutcome::Rejected {
                    reason: Some(large_result_text()),
                }
            })
        })),
        ..AgentLoopConfig::default()
    };

    let new_messages = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("reject large tool")],
        &mut context,
        &[tool],
        &config,
        &bridge,
        &collecting_sink(events.clone()),
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");

    assert_eq!(executed.load(Ordering::SeqCst), 0);
    let tool_result_message = new_messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::ToolResult {
                content,
                details,
                is_error,
                ..
            } => Some(AgentToolResult {
                content: content.clone(),
                details: details.clone(),
                is_error: *is_error,
            }),
            _ => None,
        })
        .expect("rejected tool result should exist");
    assert!(tool_result_message.is_error);
    assert_bounded_tool_result(&tool_result_message, "turn_001:tool_001");
    assert_eq!(
        tool_result_message
            .details
            .as_ref()
            .and_then(|details| details.get("approval_state"))
            .and_then(serde_json::Value::as_str),
        Some("rejected")
    );

    let collected = events.lock().await.clone();
    assert!(collected.iter().any(|event| matches!(
        event,
        AgentEvent::ToolExecutionApprovalResolved {
            tool_call_id: id,
            approved: false,
            ..
        } if id == tool_call_id
    )));
    assert!(!collected.iter().any(|event| matches!(
        event,
        AgentEvent::ToolExecutionEnd { tool_call_id: id, .. } if id == tool_call_id
    )));
    assert!(!collected.iter().any(|event| matches!(
        event,
        AgentEvent::MessageEnd {
            message:
                AgentMessage::ToolResult {
                    content,
                    ..
                },
        } if content
            .iter()
            .filter_map(ContentPart::extract_text)
            .any(|text| text.contains(LARGE_RESULT_SENTINEL))
    )));
}

#[tokio::test]
async fn responses_tool_name_delta_emits_start_before_arguments_finish() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let tool: DynAgentTool = Arc::new(RecordingTool {
        executed: Arc::new(AtomicUsize::new(0)),
    });
    let bridge = ScriptedBridge::new(vec![
        vec![
            ScriptStep::chunk(agentdash_agent::StreamChunk::ToolCallDelta {
                id: "tool-echo-1".to_string(),
                content: agentdash_agent::ToolCallDeltaContent::Name("echo".to_string()),
            }),
            ScriptStep::chunk(agentdash_agent::StreamChunk::ToolCallDelta {
                id: "tool-echo-1".to_string(),
                content: agentdash_agent::ToolCallDeltaContent::Arguments(
                    "{\"value\":\"hello".to_string(),
                ),
            }),
            ScriptStep::chunk(agentdash_agent::StreamChunk::ToolCall {
                info: ToolCallInfo {
                    id: "tool-echo-1".to_string(),
                    call_id: Some("tool-echo-1".to_string()),
                    name: "echo".to_string(),
                    arguments: serde_json::json!({
                        "value": "hello world"
                    }),
                },
            }),
            ScriptStep::chunk(agentdash_agent::StreamChunk::Done(bridge_response(
                AgentMessage::Assistant {
                    content: vec![],
                    tool_calls: vec![ToolCallInfo {
                        id: "tool-echo-1".to_string(),
                        call_id: Some("tool-echo-1".to_string()),
                        name: "echo".to_string(),
                        arguments: serde_json::json!({
                            "value": "hello world"
                        }),
                    }],
                    stop_reason: Some(StopReason::ToolUse),
                    error_message: None,
                    usage: None,
                    timestamp: Some(agentdash_agent::types::now_millis()),
                },
            ))),
        ],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![ToolDefinition::from_tool(tool.as_ref())],
    };
    let tool_instances = vec![tool];

    let result = agentdash_agent::agent_loop::agent_loop(
        vec![AgentMessage::user("write it")],
        &mut context,
        &tool_instances,
        &AgentLoopConfig::default(),
        &bridge,
        &collecting_sink(events.clone()),
        CancellationToken::new(),
    )
    .await
    .expect("agent loop should succeed");

    let collected = events.lock().await.clone();
    let start_index = collected
        .iter()
        .position(|event| {
            matches!(
                event,
                AgentEvent::MessageUpdate {
                    event: AssistantStreamEvent::ToolCallStart { tool_call_id, name, .. },
                    ..
                } if tool_call_id == "tool-echo-1" && name == "echo"
            )
        })
        .expect("should emit tool_call_start from name delta");
    let delta_index = collected
        .iter()
        .position(|event| {
            matches!(
                event,
                AgentEvent::MessageUpdate {
                    event: AssistantStreamEvent::ToolCallDelta { tool_call_id, draft, is_parseable, .. },
                    ..
                } if tool_call_id == "tool-echo-1"
                    && draft == "{\"value\":\"hello"
                    && !is_parseable
            )
        })
        .expect("should emit tool_call_delta for partial arguments");
    assert!(start_index < delta_index);
    assert!(result.iter().any(|message| {
        matches!(
            message,
            AgentMessage::Assistant { tool_calls, .. }
                if tool_calls.iter().any(|tool_call| tool_call.id == "tool-echo-1")
        )
    }));
}

#[tokio::test]
async fn stream_errors_become_error_assistant_messages() {
    let bridge = ScriptedBridge::new(vec![vec![ScriptStep::chunk(
        agentdash_agent::StreamChunk::Error(BridgeError::CompletionFailed("boom".to_string())),
    )]]);
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());

    let (_rx, handle) = agent
        .prompt(AgentMessage::user("hi"))
        .expect("prompt should start");
    let new_messages = handle
        .await
        .expect("task should not panic")
        .expect("run should succeed");

    assert_eq!(new_messages.len(), 2);
    assert!(matches!(
        new_messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Error),
            ..
        })
    ));

    let state = agent.state().await;
    assert!(matches!(
        state.messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Error),
            ..
        })
    ));
}

#[tokio::test]
async fn runtime_delegate_errors_after_assistant_do_not_become_assistant_messages() {
    let bridge = ScriptedBridge::new(vec![vec![ScriptStep::chunk(
        agentdash_agent::StreamChunk::Done(bridge_response(assistant_text("done"))),
    )]]);
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());
    agent.set_runtime_delegate(Some(Arc::new(FailingBeforeStopDelegate)));

    let (_rx, handle) = agent
        .prompt(AgentMessage::user("hi"))
        .expect("prompt should start");
    let error = handle
        .await
        .expect("task should not panic")
        .expect_err("runtime delegate failure should remain an internal run error");

    assert!(matches!(error, AgentError::RuntimeDelegate(_)));
    assert!(error.to_string().contains("运行时委托错误: 内部数据库错误"));

    let state = agent.state().await;
    assert_eq!(state.messages.len(), 2);
    assert!(matches!(
        state.messages.first(),
        Some(AgentMessage::User { .. })
    ));
    assert!(matches!(
        state.messages.last(),
        Some(AgentMessage::Assistant {
            error_message: None,
            ..
        })
    ));
    assert_eq!(
        state.error.as_deref(),
        Some("运行时委托错误: 内部数据库错误")
    );
}

#[tokio::test]
async fn abort_becomes_aborted_assistant_message() {
    let first_delta_sent = Arc::new(Notify::new());
    let release_stream = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![vec![
        ScriptStep::chunk(agentdash_agent::StreamChunk::TextDelta("hel".to_string())),
        ScriptStep::Signal(first_delta_sent.clone()),
        ScriptStep::Wait(release_stream.clone()),
        ScriptStep::chunk(agentdash_agent::StreamChunk::TextDelta(
            "ignored".to_string(),
        )),
    ]]);
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());

    let (_rx, handle) = agent
        .prompt(AgentMessage::user("hi"))
        .expect("prompt should start");
    first_delta_sent.notified().await;
    agent.abort();
    release_stream.notify_waiters();

    let new_messages = handle
        .await
        .expect("task should not panic")
        .expect("run should succeed");
    assert!(matches!(
        new_messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Aborted),
            ..
        })
    ));

    let state = agent.state().await;
    assert!(matches!(
        state.messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Aborted),
            ..
        })
    ));
}

#[tokio::test]
async fn abort_interrupts_pending_provider_stream_and_waits_for_idle() {
    let provider_stream_started = Arc::new(Notify::new());
    let release_provider_task = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![
        vec![
            ScriptStep::Signal(provider_stream_started.clone()),
            ScriptStep::Wait(release_provider_task.clone()),
            ScriptStep::chunk(agentdash_agent::StreamChunk::Done(bridge_response(
                assistant_text("ignored after cancel"),
            ))),
        ],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("second turn")),
        ))],
    ]);
    let mut agent = Agent::new(Arc::new(bridge), AgentConfig::default());

    let provider_started = provider_stream_started.notified();
    tokio::pin!(provider_started);
    let (_rx, first_handle) = agent
        .prompt(AgentMessage::user("first"))
        .expect("first prompt should start");
    provider_started.await;

    agent.abort();
    tokio::time::timeout(Duration::from_secs(1), agent.wait_for_idle())
        .await
        .expect("wait_for_idle should complete after aborting a pending provider stream");
    release_provider_task.notify_waiters();

    let first_messages = first_handle
        .await
        .expect("first task should not panic")
        .expect("first run should resolve as aborted assistant message");
    assert!(matches!(
        first_messages.last(),
        Some(AgentMessage::Assistant {
            stop_reason: Some(StopReason::Aborted),
            ..
        })
    ));

    let (_rx, second_handle) = agent
        .prompt(AgentMessage::user("second"))
        .expect("second prompt should not see stale is_streaming");
    let second_messages = second_handle
        .await
        .expect("second task should not panic")
        .expect("second run should succeed");
    assert_eq!(
        second_messages.last().and_then(AgentMessage::first_text),
        Some("second turn")
    );
}

#[test]
fn assistant_stream_event_type_is_tool_call_delta_complete() {
    let event = AssistantStreamEvent::ToolCallDelta {
        content_index: 0,
        tool_call_id: "tool-1".to_string(),
        name: "echo".to_string(),
        delta: "{\"value\":\"x\"}".to_string(),
        draft: "{\"value\":\"x\"}".to_string(),
        is_parseable: true,
    };
    assert!(matches!(event, AssistantStreamEvent::ToolCallDelta { .. }));
}

#[test]
fn provider_attempt_status_serializes_as_snake_case_contract() {
    let event = AgentEvent::ProviderAttemptStatus {
        status: agentdash_agent::ProviderAttemptStatus {
            phase: agentdash_agent::ProviderAttemptPhase::RetryScheduled,
            attempt: 2,
            max_attempts: 3,
            will_retry: true,
            delay_ms: Some(2_000),
            reason_code: Some("stream_disconnected".to_string()),
            message: Some("Reconnecting... 2/3".to_string()),
            provider: Some("openai".to_string()),
            model: Some("gpt-4.1".to_string()),
        },
    };

    let value = serde_json::to_value(event).expect("serialize provider status");
    assert_eq!(value["type"], "provider_attempt_status");
    assert_eq!(value["status"]["phase"], "retry_scheduled");
    assert_eq!(value["status"]["attempt"], 2);
    assert_eq!(value["status"]["max_attempts"], 3);
    assert_eq!(value["status"]["will_retry"], true);
    assert_eq!(value["status"]["delay_ms"], 2_000);
    assert_eq!(value["status"]["reason_code"], "stream_disconnected");
    assert_eq!(value["status"]["provider"], "openai");
    assert_eq!(value["status"]["model"], "gpt-4.1");
}

#[tokio::test]
async fn ask_decision_waits_for_approval_and_rejection_keeps_tool_unexecuted() {
    let executed = Arc::new(AtomicUsize::new(0));
    let tool: DynAgentTool = Arc::new(RecordingTool {
        executed: executed.clone(),
    });
    let approval_requested = Arc::new(Notify::new());
    let release_approval = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call(
                "tool-approval-1",
                serde_json::json!({ "value": "x" }),
            )),
        ))],
        vec![ScriptStep::chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("收到拒绝，改走别的方案")),
        ))],
    ]);
    let tool_instances: Vec<DynAgentTool> = vec![tool.clone()];
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        message_refs: vec![],
        tools: vec![ToolDefinition::from_tool(tool.as_ref())],
    };
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());

    let approval_requested_for_cfg = approval_requested.clone();
    let release_approval_for_cfg = release_approval.clone();
    let config = AgentLoopConfig {
        runtime_delegate: Some(Arc::new(RejectingRuntimeDelegate)),
        await_tool_approval: Some(Arc::new(move |_request, _cancel| {
            let approval_requested = approval_requested_for_cfg.clone();
            let release_approval = release_approval_for_cfg.clone();
            Box::pin(async move {
                approval_requested.notify_waiters();
                release_approval.notified().await;
                ToolApprovalOutcome::Rejected {
                    reason: Some("用户拒绝执行该工具".to_string()),
                }
            })
        })),
        ..AgentLoopConfig::default()
    };

    let run = tokio::spawn(async move {
        agentdash_agent::agent_loop::agent_loop(
            vec![AgentMessage::user("run tool")],
            &mut context,
            &tool_instances,
            &config,
            &bridge,
            &sink,
            CancellationToken::new(),
        )
        .await
    });

    approval_requested.notified().await;
    release_approval.notify_waiters();

    let new_messages = run
        .await
        .expect("task should not panic")
        .expect("agent loop should succeed");

    assert_eq!(executed.load(Ordering::SeqCst), 0);
    assert!(new_messages.iter().any(|message| {
        matches!(
            message,
            AgentMessage::ToolResult { is_error: true, details: Some(details), .. }
                if details.get("approval_state").and_then(serde_json::Value::as_str) == Some("rejected")
        )
    }));

    let kinds = events
        .lock()
        .await
        .iter()
        .map(event_kind)
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"tool_execution_pending_approval"));
    assert!(kinds.contains(&"tool_execution_approval_resolved"));
    assert!(!kinds.contains(&"tool_execution_end"));
}

#[derive(Clone)]
struct RejectingRuntimeDelegate;

#[derive(Clone)]
struct FailingBeforeStopDelegate;

#[derive(Clone, Default)]
struct EmptyContinueDelegate {
    before_stop_calls: Arc<AtomicUsize>,
    always_continue: bool,
}

#[async_trait]
impl agentdash_agent::AgentRuntimeDelegate for RejectingRuntimeDelegate {
    async fn evaluate_compaction(
        &self,
        _input: agentdash_agent::EvaluateCompactionInput,
        _cancel: CancellationToken,
    ) -> Result<Option<agentdash_agent::CompactionParams>, agentdash_agent::AgentRuntimeError> {
        Ok(None)
    }

    async fn after_compaction(
        &self,
        _result: agentdash_agent::CompactionResult,
        _cancel: CancellationToken,
    ) -> Result<(), agentdash_agent::AgentRuntimeError> {
        Ok(())
    }

    async fn transform_context(
        &self,
        input: agentdash_agent::TransformContextInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::TransformContextOutput, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::TransformContextOutput {
            steering_messages: input.context.messages,
            blocked: None,
        })
    }

    async fn before_tool_call(
        &self,
        _input: agentdash_agent::BeforeToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::ToolCallDecision, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::ToolCallDecision::Ask {
            reason: "需要用户审批".to_string(),
            args: None,
            details: Some(serde_json::json!({ "source": "unit_test" })),
        })
    }

    async fn after_tool_call(
        &self,
        _input: agentdash_agent::AfterToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::AfterToolCallEffects, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::AfterToolCallEffects::default())
    }

    async fn after_turn(
        &self,
        _input: agentdash_agent::AfterTurnInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::TurnControlDecision, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::TurnControlDecision::default())
    }

    async fn before_stop(
        &self,
        _input: agentdash_agent::BeforeStopInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::StopDecision, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::StopDecision::Stop)
    }
}

#[async_trait]
impl agentdash_agent::AgentRuntimeDelegate for FailingBeforeStopDelegate {
    async fn evaluate_compaction(
        &self,
        _input: agentdash_agent::EvaluateCompactionInput,
        _cancel: CancellationToken,
    ) -> Result<Option<agentdash_agent::CompactionParams>, agentdash_agent::AgentRuntimeError> {
        Ok(None)
    }

    async fn after_compaction(
        &self,
        _result: agentdash_agent::CompactionResult,
        _cancel: CancellationToken,
    ) -> Result<(), agentdash_agent::AgentRuntimeError> {
        Ok(())
    }

    async fn transform_context(
        &self,
        input: agentdash_agent::TransformContextInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::TransformContextOutput, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::TransformContextOutput {
            steering_messages: input.context.messages,
            blocked: None,
        })
    }

    async fn before_tool_call(
        &self,
        _input: agentdash_agent::BeforeToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::ToolCallDecision, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::ToolCallDecision::Allow)
    }

    async fn after_tool_call(
        &self,
        _input: agentdash_agent::AfterToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::AfterToolCallEffects, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::AfterToolCallEffects::default())
    }

    async fn after_turn(
        &self,
        _input: agentdash_agent::AfterTurnInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::TurnControlDecision, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::TurnControlDecision::default())
    }

    async fn before_stop(
        &self,
        _input: BeforeStopInput,
        _cancel: CancellationToken,
    ) -> Result<StopDecision, agentdash_agent::AgentRuntimeError> {
        Err(agentdash_agent::AgentRuntimeError::Runtime(
            "内部数据库错误".to_string(),
        ))
    }
}

#[async_trait]
impl agentdash_agent::AgentRuntimeDelegate for EmptyContinueDelegate {
    async fn evaluate_compaction(
        &self,
        _input: agentdash_agent::EvaluateCompactionInput,
        _cancel: CancellationToken,
    ) -> Result<Option<agentdash_agent::CompactionParams>, agentdash_agent::AgentRuntimeError> {
        Ok(None)
    }

    async fn after_compaction(
        &self,
        _result: agentdash_agent::CompactionResult,
        _cancel: CancellationToken,
    ) -> Result<(), agentdash_agent::AgentRuntimeError> {
        Ok(())
    }

    async fn transform_context(
        &self,
        input: agentdash_agent::TransformContextInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::TransformContextOutput, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::TransformContextOutput {
            steering_messages: input.context.messages,
            blocked: None,
        })
    }

    async fn before_tool_call(
        &self,
        _input: agentdash_agent::BeforeToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::ToolCallDecision, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::ToolCallDecision::Allow)
    }

    async fn after_tool_call(
        &self,
        _input: agentdash_agent::AfterToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::AfterToolCallEffects, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::AfterToolCallEffects::default())
    }

    async fn after_turn(
        &self,
        _input: agentdash_agent::AfterTurnInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::TurnControlDecision, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::TurnControlDecision::default())
    }

    async fn before_stop(
        &self,
        _input: BeforeStopInput,
        _cancel: CancellationToken,
    ) -> Result<StopDecision, agentdash_agent::AgentRuntimeError> {
        let attempt = self.before_stop_calls.fetch_add(1, Ordering::SeqCst);
        if self.always_continue || attempt == 0 {
            Ok(StopDecision::Continue {
                steering: vec![],
                follow_up: vec![],
                reason: Some("retry once".to_string()),
                allow_empty: true,
            })
        } else {
            Ok(StopDecision::Stop)
        }
    }
}
