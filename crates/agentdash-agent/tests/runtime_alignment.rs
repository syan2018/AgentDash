use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use agentdash_agent::agent_loop::AgentLoopConfig;
use agentdash_agent::types::TokenUsage;
use agentdash_agent::{
    Agent, AgentConfig, AgentContext, AgentError, AgentEvent, AgentMessage, AgentTool,
    AgentToolError, AgentToolResult, AssistantStreamEvent, BridgeError, BridgeRequest,
    BridgeResponse, ContentPart, DynAgentTool, LlmBridge, StopReason, ToolApprovalOutcome,
    ToolCallInfo, ToolDefinition, agent_loop::AgentEventSink,
};
use async_trait::async_trait;
use futures::Stream;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
enum ScriptStep {
    Chunk(agentdash_agent::StreamChunk),
    Signal(Arc<Notify>),
    Wait(Arc<Notify>),
}

#[derive(Clone)]
struct ScriptedBridge {
    scripts: Arc<Mutex<VecDeque<Vec<ScriptStep>>>>,
}

impl ScriptedBridge {
    fn new(scripts: Vec<Vec<ScriptStep>>) -> Self {
        Self {
            scripts: Arc::new(Mutex::new(scripts.into())),
        }
    }
}

#[async_trait]
impl LlmBridge for ScriptedBridge {
    async fn stream_complete(
        &self,
        _request: BridgeRequest,
    ) -> Pin<Box<dyn Stream<Item = agentdash_agent::StreamChunk> + Send>> {
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
                        if tx.send(chunk).await.is_err() {
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
    AgentMessage::Assistant {
        content: vec![],
        tool_calls: vec![ToolCallInfo {
            id: id.to_string(),
            call_id: Some(id.to_string()),
            name: "echo".to_string(),
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
        AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
        AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
        AgentEvent::ToolExecutionPendingApproval { .. } => "tool_execution_pending_approval",
        AgentEvent::ToolExecutionApprovalResolved { .. } => "tool_execution_approval_resolved",
        AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
    }
}

#[tokio::test]
async fn agent_loop_emits_prompt_before_assistant_and_returns_new_messages() {
    let bridge = ScriptedBridge::new(vec![vec![ScriptStep::Chunk(
        agentdash_agent::StreamChunk::Done(bridge_response(assistant_text("hi"))),
    )]]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = collecting_sink(events.clone());
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
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
async fn agent_updates_runtime_state_and_rejects_reentrancy() {
    let first_delta_sent = Arc::new(Notify::new());
    let release_stream = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![vec![
        ScriptStep::Chunk(agentdash_agent::StreamChunk::TextDelta("hel".to_string())),
        ScriptStep::Signal(first_delta_sent.clone()),
        ScriptStep::Wait(release_stream.clone()),
        ScriptStep::Chunk(agentdash_agent::StreamChunk::Done(bridge_response(
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
        vec![ScriptStep::Chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("after steering 1")),
        ))],
        vec![ScriptStep::Chunk(agentdash_agent::StreamChunk::Done(
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
    let bridge = ScriptedBridge::new(vec![vec![ScriptStep::Chunk(
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
async fn tool_arguments_are_validated_before_before_tool_call_hook() {
    let before_calls = Arc::new(AtomicUsize::new(0));
    let executed = Arc::new(AtomicUsize::new(0));
    let tool: DynAgentTool = Arc::new(RecordingTool {
        executed: executed.clone(),
    });
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::Chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call(
                "tool-1",
                serde_json::json!({ "value": 1 }),
            )),
        ))],
        vec![ScriptStep::Chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("done")),
        ))],
    ]);
    let tool_instances: Vec<DynAgentTool> = vec![tool.clone()];
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
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
async fn stream_errors_become_error_assistant_messages() {
    let bridge = ScriptedBridge::new(vec![vec![ScriptStep::Chunk(
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
async fn abort_becomes_aborted_assistant_message() {
    let first_delta_sent = Arc::new(Notify::new());
    let release_stream = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![vec![
        ScriptStep::Chunk(agentdash_agent::StreamChunk::TextDelta("hel".to_string())),
        ScriptStep::Signal(first_delta_sent.clone()),
        ScriptStep::Wait(release_stream.clone()),
        ScriptStep::Chunk(agentdash_agent::StreamChunk::TextDelta(
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

#[tokio::test]
async fn ask_decision_waits_for_approval_and_rejection_keeps_tool_unexecuted() {
    let executed = Arc::new(AtomicUsize::new(0));
    let tool: DynAgentTool = Arc::new(RecordingTool {
        executed: executed.clone(),
    });
    let approval_requested = Arc::new(Notify::new());
    let release_approval = Arc::new(Notify::new());
    let bridge = ScriptedBridge::new(vec![
        vec![ScriptStep::Chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_tool_call(
                "tool-approval-1",
                serde_json::json!({ "value": "x" }),
            )),
        ))],
        vec![ScriptStep::Chunk(agentdash_agent::StreamChunk::Done(
            bridge_response(assistant_text("收到拒绝，改走别的方案")),
        ))],
    ]);
    let tool_instances: Vec<DynAgentTool> = vec![tool.clone()];
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
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

#[async_trait]
impl agentdash_agent::AgentRuntimeDelegate for RejectingRuntimeDelegate {
    async fn transform_context(
        &self,
        input: agentdash_agent::TransformContextInput,
        _cancel: CancellationToken,
    ) -> Result<agentdash_agent::TransformContextOutput, agentdash_agent::AgentRuntimeError> {
        Ok(agentdash_agent::TransformContextOutput {
            messages: input.context.messages,
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
