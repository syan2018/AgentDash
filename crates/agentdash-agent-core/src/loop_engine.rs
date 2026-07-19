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
    let max_rounds = context.max_provider_rounds.max(1);
    let mut messages = context.history;
    messages.push(input.message);
    let initial_len = messages.len();
    let mut total_usage = CoreTokenUsage::default();

    for round in 1..=max_rounds {
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
                messages.push(CoreMessage::assistant(assistant_text));
                for call in tool_calls {
                    ensure_not_cancelled(&cancel)?;
                    let decision = tokio::select! {
                        _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                        decision = tools.before_tool(call) => decision?,
                    };
                    let result = match decision {
                        CoreBeforeToolDecision::Invoke { call } => {
                            let result = tokio::select! {
                                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                                result = tools.invoke(call.clone()) => result?,
                            };
                            tokio::select! {
                                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                                result = tools.after_tool(&call, result) => result?,
                            }
                        }
                        CoreBeforeToolDecision::Deny { result } => result,
                    };
                    callbacks
                        .emit(CoreEvent::ToolCallCompleted {
                            round,
                            result: result.clone(),
                        })
                        .await?;
                    messages.push(CoreMessage::tool(result.call_id, result.content));
                }
            }
            _ => return Err(CoreError::InvalidProviderTerminal),
        }
    }

    Err(CoreError::ProviderRoundLimit { max_rounds })
}

fn ensure_not_cancelled(cancel: &CancellationToken) -> Result<(), CoreError> {
    if cancel.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}
