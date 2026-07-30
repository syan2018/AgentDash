use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreBeforeToolDecision, CoreCallbacks, CoreContext, CoreError, CoreEvent, CoreInput,
    CoreMessage, CoreOutput, CoreProvider, CoreTokenUsage, CoreToolCall, CoreToolCallbacks,
    CoreToolExecutionEvent, CoreToolResult, FinishReason, ProviderEvent, ProviderRequest,
};

pub async fn run_agent_loop(
    input: CoreInput,
    context: CoreContext,
    provider: &dyn CoreProvider,
    tools: &dyn CoreToolCallbacks,
    callbacks: &dyn CoreCallbacks,
    cancel: CancellationToken,
) -> Result<CoreOutput, CoreError> {
    let mut messages = context.history;
    messages.push(input.message);
    let initial_len = messages.len();
    let mut total_usage = CoreTokenUsage::default();
    let mut round = 1_u32;

    loop {
        ensure_not_cancelled(&cancel)?;
        callbacks
            .emit(CoreEvent::ProviderRoundStarted { round })
            .await?;

        let request = ProviderRequest {
            system_prompt: context.system_prompt.clone(),
            messages: messages.clone(),
            tools: context.tools.clone(),
            round,
        };
        let mut stream = provider.stream(request).await?;
        let mut assistant_text = String::new();
        let mut tool_calls = Vec::<CoreToolCall>::new();
        let mut terminal = None;

        loop {
            let event = tokio::select! {
                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                event = stream.next() => event,
            };
            let Some(event) = event else {
                break;
            };

            match event? {
                ProviderEvent::TextDelta { delta } => {
                    assistant_text.push_str(&delta);
                    callbacks
                        .emit(CoreEvent::TextDelta { round, delta })
                        .await?;
                }
                ProviderEvent::ReasoningDelta { delta } => {
                    callbacks
                        .emit(CoreEvent::ReasoningDelta { round, delta })
                        .await?;
                }
                ProviderEvent::ToolCall { call } => {
                    tool_calls.push(call);
                }
                ProviderEvent::Completed {
                    finish_reason,
                    usage,
                    context_window,
                } => {
                    terminal = Some((finish_reason, usage, context_window));
                    break;
                }
            }
        }

        let Some((finish_reason, usage, context_window)) = terminal else {
            return Err(CoreError::ProviderStreamDisconnected);
        };
        total_usage.accumulate(usage);
        callbacks
            .emit(CoreEvent::ProviderRoundCompleted {
                round,
                finish_reason,
                usage,
                context_window,
            })
            .await?;

        match finish_reason {
            FinishReason::Stop if tool_calls.is_empty() => {
                let assistant_message = CoreMessage::assistant(assistant_text);
                messages.push(assistant_message.clone());
                let steering = callbacks.drain_steering(round, true).await?;
                if steering.is_empty() {
                    return Ok(CoreOutput {
                        assistant_message,
                        transcript_delta: messages.split_off(initial_len),
                        usage: total_usage,
                        provider_rounds: round,
                    });
                }
                messages.extend(steering);
            }
            FinishReason::ToolCalls if !tool_calls.is_empty() => {
                messages.push(CoreMessage::assistant_with_tool_calls(
                    assistant_text,
                    tool_calls.clone(),
                ));
                for call in tool_calls {
                    let decision = tokio::select! {
                        _ = cancel.cancelled() => {
                            close_cancelled_tool(round, &call, callbacks).await?;
                            return Err(CoreError::Cancelled);
                        },
                        decision = tools.before_tool(call.clone()) => match decision {
                            Ok(decision) => decision,
                            Err(error) => CoreBeforeToolDecision::Deny {
                                result: failed_tool_result(
                                    &call,
                                    "before_tool_failed",
                                    &error.to_string(),
                                ),
                            },
                        },
                    };
                    let (effective_call, result) = match decision {
                        CoreBeforeToolDecision::Invoke { call } => {
                            let result =
                                consume_tool_execution(round, &call, tools, callbacks, &cancel)
                                    .await?;
                            let result = tokio::select! {
                                _ = cancel.cancelled() => {
                                    let cancelled = failed_tool_result(
                                        &call,
                                        "tool_cancelled",
                                        "tool execution was cancelled",
                                    );
                                    callbacks.emit(CoreEvent::ToolCallCompleted {
                                        round,
                                        call: call.clone(),
                                        result: cancelled,
                                    }).await?;
                                    return Err(CoreError::Cancelled);
                                },
                                result = tools.after_tool(&call, result) => match result {
                                    Ok(result) => result,
                                    Err(error) => failed_tool_result(
                                        &call,
                                        "after_tool_failed",
                                        &error.to_string(),
                                    ),
                                },
                            };
                            (call, result)
                        }
                        CoreBeforeToolDecision::Deny { result } => {
                            callbacks
                                .emit(CoreEvent::ToolCallStarted {
                                    round,
                                    call: call.clone(),
                                })
                                .await?;
                            (call, result)
                        }
                    };
                    callbacks
                        .emit(CoreEvent::ToolCallCompleted {
                            round,
                            call: effective_call,
                            result: result.clone(),
                        })
                        .await?;
                    let provider_text = result.text();
                    messages.push(CoreMessage::tool(
                        result.call_id,
                        provider_text,
                        result.is_error,
                    ));
                }
                messages.extend(callbacks.drain_steering(round, false).await?);
            }
            _ => return Err(CoreError::InvalidProviderTerminal),
        }

        round = round.checked_add(1).ok_or_else(|| CoreError::Provider {
            code: "provider_round_counter_overflow".to_owned(),
            message: "provider round counter overflowed".to_owned(),
            retryable: false,
        })?;
    }
}

async fn consume_tool_execution(
    round: u32,
    call: &CoreToolCall,
    tools: &dyn CoreToolCallbacks,
    callbacks: &dyn CoreCallbacks,
    cancel: &CancellationToken,
) -> Result<CoreToolResult, CoreError> {
    let mut stream = match tools.invoke(call.clone()).await {
        Ok(stream) => stream,
        Err(error) => {
            callbacks
                .emit(CoreEvent::ToolCallStarted {
                    round,
                    call: call.clone(),
                })
                .await?;
            return Ok(failed_tool_result(
                call,
                "tool_invocation_failed",
                &error.to_string(),
            ));
        }
    };
    let mut started = false;
    let mut last_update_index = 0_u64;
    loop {
        let event = tokio::select! {
            _ = cancel.cancelled() => {
                if !started {
                    callbacks.emit(CoreEvent::ToolCallStarted {
                        round,
                        call: call.clone(),
                    }).await?;
                }
                return Ok(failed_tool_result(
                    call,
                    "tool_cancelled",
                    "tool execution was cancelled",
                ));
            },
            event = stream.next() => event,
        };
        match event {
            Ok(Some(CoreToolExecutionEvent::Started)) if !started => {
                started = true;
                callbacks
                    .emit(CoreEvent::ToolCallStarted {
                        round,
                        call: call.clone(),
                    })
                    .await?;
            }
            Ok(Some(CoreToolExecutionEvent::Progress {
                update_index,
                update,
            })) if started && last_update_index.checked_add(1) == Some(update_index) => {
                last_update_index = update_index;
                callbacks
                    .emit(CoreEvent::ToolCallProgress {
                        round,
                        call: call.clone(),
                        update_index,
                        update,
                    })
                    .await?;
            }
            Ok(Some(CoreToolExecutionEvent::Completed { result })) if started => {
                return Ok(result);
            }
            Ok(Some(_)) => {
                if !started {
                    callbacks
                        .emit(CoreEvent::ToolCallStarted {
                            round,
                            call: call.clone(),
                        })
                        .await?;
                }
                return Ok(failed_tool_result(
                    call,
                    "tool_stream_protocol_error",
                    "tool execution stream violated started/progress/completed ordering",
                ));
            }
            Ok(None) => {
                if !started {
                    callbacks
                        .emit(CoreEvent::ToolCallStarted {
                            round,
                            call: call.clone(),
                        })
                        .await?;
                }
                return Ok(failed_tool_result(
                    call,
                    "tool_stream_lost",
                    "tool execution stream ended before a terminal result",
                ));
            }
            Err(error) => {
                if !started {
                    callbacks
                        .emit(CoreEvent::ToolCallStarted {
                            round,
                            call: call.clone(),
                        })
                        .await?;
                }
                return Ok(failed_tool_result(
                    call,
                    "tool_stream_failed",
                    &error.to_string(),
                ));
            }
        }
    }
}

async fn close_cancelled_tool(
    round: u32,
    call: &CoreToolCall,
    callbacks: &dyn CoreCallbacks,
) -> Result<(), CoreError> {
    callbacks
        .emit(CoreEvent::ToolCallStarted {
            round,
            call: call.clone(),
        })
        .await?;
    callbacks
        .emit(CoreEvent::ToolCallCompleted {
            round,
            call: call.clone(),
            result: failed_tool_result(call, "tool_cancelled", "tool execution was cancelled"),
        })
        .await
}

fn failed_tool_result(call: &CoreToolCall, code: &str, message: &str) -> CoreToolResult {
    CoreToolResult {
        call_id: call.call_id.clone(),
        content: vec![crate::CoreToolContent::Text {
            text: message.to_owned(),
        }],
        is_error: true,
        details: Some(serde_json::json!({"code": code, "message": message})),
    }
}

fn ensure_not_cancelled(cancel: &CancellationToken) -> Result<(), CoreError> {
    if cancel.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}
