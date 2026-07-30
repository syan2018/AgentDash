use std::sync::Arc;

use agentdash_agent_runtime::{
    PlatformToolBroker, RuntimeToolBrokerError, RuntimeToolResolvedContext, RuntimeToolUpdateSink,
};
use agentdash_agent_runtime_contract::{
    AgentHostCallbackError, AgentToolExecutionEvent, AgentToolExecutionStream, AgentToolResult,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::mpsc;

use crate::{CompleteAgentToolHandler, ResolvedCompleteAgentToolCallback};

pub struct RuntimePlatformToolHandler {
    broker: Arc<PlatformToolBroker>,
}

impl RuntimePlatformToolHandler {
    pub fn new(broker: Arc<PlatformToolBroker>) -> Self {
        Self { broker }
    }
}

#[async_trait]
impl CompleteAgentToolHandler for RuntimePlatformToolHandler {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentToolCallback,
    ) -> Result<Box<dyn AgentToolExecutionStream>, AgentHostCallbackError> {
        let context = callback.context;
        let broker = self.broker.clone();
        let (sender, receiver) = mpsc::channel(32);
        let progress_sender = sender.clone();
        let update_index = Arc::new(AtomicU64::new(0));
        let progress_overflow = Arc::new(AtomicBool::new(false));
        let progress_index = update_index.clone();
        let overflow = progress_overflow.clone();
        let updates: RuntimeToolUpdateSink = Arc::new(move |output| {
            let update_index = progress_index.fetch_add(1, Ordering::Relaxed) + 1;
            if progress_sender
                .try_send(AgentToolExecutionEvent::Progress {
                    update_index,
                    output,
                })
                .is_err()
            {
                overflow.store(true, Ordering::Relaxed);
            }
        });
        sender
            .try_send(AgentToolExecutionEvent::Started)
            .map_err(|_| {
                AgentHostCallbackError::new(
                    agentdash_agent_runtime_contract::AgentHostCallbackErrorCode::Internal,
                    "failed to initialize tool execution stream",
                    false,
                )
            })?;
        tokio::spawn(async move {
            let result = match broker
                .invoke_with_updates(
                    RuntimeToolResolvedContext {
                        runtime_thread_id: context.runtime_thread_id,
                        host_binding_generation: Some(context.binding_generation),
                        applied_surface_revision: context.applied_surface_revision,
                        turn_id: callback.invocation.meta.turn_id,
                        item_id: callback.invocation.meta.item_id,
                        effect_id: callback.invocation.meta.effect_id,
                        invocation_id: callback.invocation.meta.idempotency_key.as_str().to_owned(),
                        deadline_at_ms: callback.invocation.meta.deadline_at_ms,
                    },
                    callback.invocation.tool,
                    callback.invocation.arguments,
                    Some(updates),
                )
                .await
            {
                Ok(result) => result,
                Err(error) => rejected_result(error),
            };
            let result = if progress_overflow.load(Ordering::Relaxed) {
                AgentToolResult::Failed {
                    code: "tool_progress_backpressure".to_owned(),
                    message: "tool progress exceeded the bounded callback stream".to_owned(),
                }
            } else {
                result
            };
            let _ = sender
                .send(AgentToolExecutionEvent::Completed { result })
                .await;
        });
        Ok(Box::new(ChannelAgentToolExecutionStream { receiver }))
    }
}

struct ChannelAgentToolExecutionStream {
    receiver: mpsc::Receiver<AgentToolExecutionEvent>,
}

#[async_trait]
impl AgentToolExecutionStream for ChannelAgentToolExecutionStream {
    async fn next(&mut self) -> Result<Option<AgentToolExecutionEvent>, AgentHostCallbackError> {
        Ok(self.receiver.recv().await)
    }
}

fn rejected_result(error: RuntimeToolBrokerError) -> AgentToolResult {
    let code = match &error {
        RuntimeToolBrokerError::EmptyCatalog => "empty_runtime_tool_catalog",
        RuntimeToolBrokerError::UnknownTool(_) => "unknown_runtime_tool",
        RuntimeToolBrokerError::DuplicateTool(_) => "duplicate_runtime_tool",
        RuntimeToolBrokerError::PermissionDenied { .. } => "runtime_tool_permission_denied",
        RuntimeToolBrokerError::EffectMismatch { .. } => "runtime_tool_effect_mismatch",
        RuntimeToolBrokerError::AuthorizationDenied { code, message } => {
            return AgentToolResult::Rejected {
                code: code.clone(),
                message: message.clone(),
            };
        }
    };
    AgentToolResult::Rejected {
        code: code.to_owned(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_authorization_rejection_preserves_its_typed_code() {
        assert_eq!(
            rejected_result(RuntimeToolBrokerError::AuthorizationDenied {
                code: "stale_product_surface".to_owned(),
                message: "surface revision does not match".to_owned(),
            }),
            AgentToolResult::Rejected {
                code: "stale_product_surface".to_owned(),
                message: "surface revision does not match".to_owned(),
            }
        );
    }
}
