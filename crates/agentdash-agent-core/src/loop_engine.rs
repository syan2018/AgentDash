use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreBeforeToolDecision, CoreCallbacks, CoreContext, CoreError, CoreEvent, CoreInput,
    CoreMessage, CoreOutput, CoreProvider, CoreTokenUsage, CoreToolCall, CoreToolCallbacks,
    FinishReason, ProviderEvent, ProviderRequest,
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
                    callbacks
                        .emit(CoreEvent::ToolCallRequested {
                            round,
                            call: call.clone(),
                        })
                        .await?;
                    tool_calls.push(call);
                }
                ProviderEvent::Completed {
                    finish_reason,
                    usage,
                } => {
                    terminal = Some((finish_reason, usage));
                    break;
                }
            }
        }

        let Some((finish_reason, usage)) = terminal else {
            return Err(CoreError::ProviderStreamDisconnected);
        };
        total_usage.accumulate(usage);
        callbacks
            .emit(CoreEvent::ProviderRoundCompleted {
                round,
                finish_reason,
            })
            .await?;

        match finish_reason {
            FinishReason::Stop if tool_calls.is_empty() => {
                let assistant_message = CoreMessage::assistant(assistant_text);
                messages.push(assistant_message.clone());
                return Ok(CoreOutput {
                    assistant_message,
                    transcript_delta: messages.split_off(initial_len),
                    usage: total_usage,
                    provider_rounds: round,
                });
            }
            FinishReason::ToolCalls if !tool_calls.is_empty() => {
                messages.push(CoreMessage::assistant_with_tool_calls(
                    assistant_text,
                    tool_calls.clone(),
                ));
                for call in tool_calls {
                    ensure_not_cancelled(&cancel)?;
                    let decision = tokio::select! {
                        _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                        decision = tools.before_tool(call.clone()) => decision?,
                    };
                    let (effective_call, result) = match decision {
                        CoreBeforeToolDecision::Invoke { call } => {
                            let result = tokio::select! {
                                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                                result = tools.invoke(call.clone()) => result?,
                            };
                            let result = tokio::select! {
                                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                                result = tools.after_tool(&call, result) => result?,
                            };
                            (call, result)
                        }
                        CoreBeforeToolDecision::Deny { result } => (call, result),
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

fn ensure_not_cancelled(cancel: &CancellationToken) -> Result<(), CoreError> {
    if cancel.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}
