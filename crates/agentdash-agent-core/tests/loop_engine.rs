use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use agentdash_agent_core::{
    CoreCallbacks, CoreContext, CoreError, CoreEvent, CoreInput, CoreMessage, CoreProvider,
    CoreTokenUsage, CoreTool, CoreToolCall, CoreToolCallbacks, CoreToolResult, FinishReason,
    ProviderEvent, ProviderEventStream, ProviderRequest, run_agent_loop,
};
use async_trait::async_trait;
use futures::stream;
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct ScriptedProvider {
    rounds: Mutex<VecDeque<Vec<ProviderEvent>>>,
    requests: Mutex<Vec<ProviderRequest>>,
}

#[async_trait]
impl CoreProvider for ScriptedProvider {
    async fn stream(&self, request: ProviderRequest) -> Result<ProviderEventStream, CoreError> {
        self.requests.lock().unwrap().push(request);
        let events = self.rounds.lock().unwrap().pop_front().unwrap();
        Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
    }
}

struct ToolRecorder {
    calls: Mutex<Vec<CoreToolCall>>,
}

#[async_trait]
impl CoreToolCallbacks for ToolRecorder {
    async fn invoke(&self, call: CoreToolCall) -> Result<CoreToolResult, CoreError> {
        self.calls.lock().unwrap().push(call.clone());
        Ok(CoreToolResult {
            call_id: call.call_id,
            content: vec![agentdash_agent_core::CoreToolContent::Text {
                text: "tool-result".into(),
            }],
            is_error: false,
            details: None,
        })
    }
}

#[derive(Default)]
struct EventRecorder {
    events: Mutex<Vec<CoreEvent>>,
}

#[async_trait]
impl CoreCallbacks for EventRecorder {
    async fn emit(&self, event: CoreEvent) -> Result<(), CoreError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

#[tokio::test]
async fn provider_tool_loop_has_only_explicit_state() {
    let provider = Arc::new(ScriptedProvider {
        rounds: Mutex::new(VecDeque::from([
            vec![
                ProviderEvent::ToolCall {
                    call: CoreToolCall {
                        call_id: "call-1".into(),
                        name: "read".into(),
                        arguments: json!({"path": "README.md"}),
                    },
                },
                ProviderEvent::Completed {
                    finish_reason: FinishReason::ToolCalls,
                    usage: CoreTokenUsage {
                        input_tokens: 10,
                        output_tokens: 2,
                    },
                },
            ],
            vec![
                ProviderEvent::TextDelta {
                    delta: "done".into(),
                },
                ProviderEvent::Completed {
                    finish_reason: FinishReason::Stop,
                    usage: CoreTokenUsage {
                        input_tokens: 12,
                        output_tokens: 3,
                    },
                },
            ],
        ])),
        requests: Mutex::new(Vec::new()),
    });
    let tools = ToolRecorder {
        calls: Mutex::new(Vec::new()),
    };
    let callbacks = EventRecorder::default();

    let output = run_agent_loop(
        CoreInput {
            message: CoreMessage::user("inspect"),
        },
        CoreContext {
            system_prompt: "be exact".into(),
            history: vec![CoreMessage::assistant("prior")],
            tools: vec![CoreTool {
                name: "read".into(),
                description: "read a file".into(),
                input_schema: json!({"type": "object"}),
            }],
        },
        provider.as_ref(),
        &tools,
        &callbacks,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(output.assistant_message.content, "done");
    assert_eq!(output.provider_rounds, 2);
    assert_eq!(output.usage.input_tokens, 22);
    assert_eq!(tools.calls.lock().unwrap().len(), 1);
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|message| message.tool_call_id.as_deref() == Some("call-1"))
    );
}

#[tokio::test]
async fn provider_tool_loop_is_not_terminated_by_an_internal_round_budget() {
    let mut rounds = VecDeque::new();
    for round in 1..=12 {
        rounds.push_back(vec![
            ProviderEvent::ToolCall {
                call: CoreToolCall {
                    call_id: format!("call-{round}"),
                    name: "inspect".into(),
                    arguments: json!({"round": round}),
                },
            },
            ProviderEvent::Completed {
                finish_reason: FinishReason::ToolCalls,
                usage: CoreTokenUsage::default(),
            },
        ]);
    }
    rounds.push_back(vec![
        ProviderEvent::TextDelta {
            delta: "done".into(),
        },
        ProviderEvent::Completed {
            finish_reason: FinishReason::Stop,
            usage: CoreTokenUsage::default(),
        },
    ]);
    let provider = ScriptedProvider {
        rounds: Mutex::new(rounds),
        requests: Mutex::new(Vec::new()),
    };
    let tools = ToolRecorder {
        calls: Mutex::new(Vec::new()),
    };

    let output = run_agent_loop(
        CoreInput {
            message: CoreMessage::user("complete the whole tool chain"),
        },
        CoreContext {
            system_prompt: String::new(),
            history: Vec::new(),
            tools: Vec::new(),
        },
        &provider,
        &tools,
        &EventRecorder::default(),
        CancellationToken::new(),
    )
    .await
    .expect("provider Stop、显式失败或取消之前不应由内部轮次计数终止");

    assert_eq!(output.provider_rounds, 13);
    assert_eq!(output.assistant_message.content, "done");
    assert_eq!(tools.calls.lock().unwrap().len(), 12);
}

#[tokio::test]
async fn cancellation_is_observed_before_provider_side_effect() {
    let provider = ScriptedProvider {
        rounds: Mutex::new(VecDeque::new()),
        requests: Mutex::new(Vec::new()),
    };
    let tools = ToolRecorder {
        calls: Mutex::new(Vec::new()),
    };
    let callbacks = EventRecorder::default();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = run_agent_loop(
        CoreInput {
            message: CoreMessage::user("never sent"),
        },
        CoreContext {
            system_prompt: String::new(),
            history: Vec::new(),
            tools: Vec::new(),
        },
        &provider,
        &tools,
        &callbacks,
        cancel,
    )
    .await
    .unwrap_err();

    assert_eq!(error, CoreError::Cancelled);
    assert!(provider.requests.lock().unwrap().is_empty());
}
