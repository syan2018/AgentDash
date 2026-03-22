use std::sync::Arc;

use agentdash_agent::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeStopInput, BeforeToolCallInput, StopDecision, ToolCallDecision,
    TransformContextInput, TransformContextOutput, TurnControlDecision,
};
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::hooks::{
    HookConstraint, HookContextFragment, HookEvaluationQuery, HookSessionRuntimeSnapshot,
    HookTraceEntry, HookTrigger, SessionHookRefreshQuery, SharedHookSessionRuntime,
};

pub struct HookRuntimeDelegate {
    hook_session: SharedHookSessionRuntime,
    trace_event_tx: Option<UnboundedSender<HookTraceEntry>>,
}

impl HookRuntimeDelegate {
    pub fn new(hook_session: SharedHookSessionRuntime) -> Arc<dyn AgentRuntimeDelegate> {
        Self::new_with_trace_events(hook_session, None)
    }

    pub fn new_with_trace_events(
        hook_session: SharedHookSessionRuntime,
        trace_event_tx: Option<UnboundedSender<HookTraceEntry>>,
    ) -> Arc<dyn AgentRuntimeDelegate> {
        Arc::new(Self {
            hook_session,
            trace_event_tx,
        })
    }

    async fn evaluate(
        &self,
        trigger: HookTrigger,
        tool_name: Option<String>,
        tool_call_id: Option<String>,
        subagent_type: Option<String>,
        payload: Option<serde_json::Value>,
    ) -> Result<EvaluatedResolution, AgentRuntimeError> {
        let snapshot = self.hook_session.snapshot();
        let resolution = self
            .hook_session
            .evaluate(HookEvaluationQuery {
                session_id: self.hook_session.session_id().to_string(),
                trigger: trigger.clone(),
                turn_id: None,
                tool_name,
                tool_call_id,
                subagent_type,
                snapshot: Some(snapshot.clone()),
                payload,
            })
            .await
            .map_err(map_runtime_error)?;

        if resolution.refresh_snapshot {
            self.hook_session
                .refresh(SessionHookRefreshQuery {
                    session_id: self.hook_session.session_id().to_string(),
                    turn_id: None,
                    reason: Some(format!("trigger:{:?}", trigger)),
                })
                .await
                .map_err(map_runtime_error)?;
        }

        Ok(EvaluatedResolution {
            snapshot: self.hook_session.snapshot(),
            resolution,
            runtime: self.hook_session.runtime_snapshot(),
        })
    }

    fn record_trace(
        &self,
        trigger: HookTrigger,
        decision: impl Into<String>,
        tool_name: Option<String>,
        tool_call_id: Option<String>,
        subagent_type: Option<String>,
        evaluated: &EvaluatedResolution,
    ) {
        let trace = HookTraceEntry {
            sequence: self.hook_session.next_trace_sequence(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            revision: evaluated.runtime.revision,
            trigger,
            decision: decision.into(),
            tool_name,
            tool_call_id,
            subagent_type,
            matched_rule_keys: evaluated.resolution.matched_rule_keys.clone(),
            refresh_snapshot: evaluated.resolution.refresh_snapshot,
            block_reason: evaluated.resolution.block_reason.clone(),
            completion: evaluated.resolution.completion.clone(),
            diagnostics: evaluated.resolution.diagnostics.clone(),
        };
        self.hook_session.append_trace(trace.clone());
        if let Some(sender) = &self.trace_event_tx {
            let _ = sender.send(trace);
        }
    }
}

#[async_trait]
impl AgentRuntimeDelegate for HookRuntimeDelegate {
    async fn transform_context(
        &self,
        input: TransformContextInput,
        _cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError> {
        let evaluated = self
            .evaluate(
                HookTrigger::UserPromptSubmit,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                })),
            )
            .await?;
        self.record_trace(
            HookTrigger::UserPromptSubmit,
            if evaluated.resolution.context_fragments.is_empty()
                && evaluated.resolution.constraints.is_empty()
            {
                "noop"
            } else {
                "context_injected"
            },
            None,
            None,
            None,
            &evaluated,
        );
        let mut messages = input.context.messages;
        if let Some(message) = build_hook_injection_message(
            &evaluated.snapshot,
            &evaluated.resolution,
            &evaluated.runtime,
        ) {
            messages.push(message);
        }
        Ok(TransformContextOutput { messages })
    }

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        let tool_name = input.tool_call.name.clone();
        let tool_call_id = input.tool_call.id.clone();
        let evaluated = self
            .evaluate(
                HookTrigger::BeforeTool,
                Some(tool_name.clone()),
                Some(tool_call_id.clone()),
                None,
                Some(serde_json::json!({
                    "args": input.args,
                })),
            )
            .await?;

        if let Some(reason) = evaluated.resolution.block_reason.clone() {
            self.record_trace(
                HookTrigger::BeforeTool,
                "deny",
                Some(tool_name),
                Some(tool_call_id),
                None,
                &evaluated,
            );
            return Ok(ToolCallDecision::Deny { reason });
        }
        if let Some(approval_request) = evaluated.resolution.approval_request.clone() {
            self.record_trace(
                HookTrigger::BeforeTool,
                "ask",
                Some(tool_name),
                Some(tool_call_id),
                None,
                &evaluated,
            );
            return Ok(ToolCallDecision::Ask {
                reason: approval_request.reason,
                args: evaluated.resolution.rewritten_tool_input.clone(),
                details: approval_request.details,
            });
        }
        if let Some(args) = evaluated.resolution.rewritten_tool_input.clone() {
            self.record_trace(
                HookTrigger::BeforeTool,
                "rewrite",
                Some(tool_name),
                Some(tool_call_id),
                None,
                &evaluated,
            );
            return Ok(ToolCallDecision::Rewrite { args, note: None });
        }
        self.record_trace(
            HookTrigger::BeforeTool,
            "allow",
            Some(tool_name),
            Some(tool_call_id),
            None,
            &evaluated,
        );
        Ok(ToolCallDecision::Allow)
    }

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
        let tool_name = input.tool_call.name.clone();
        let tool_call_id = input.tool_call.id.clone();
        let evaluated = self
            .evaluate(
                HookTrigger::AfterTool,
                Some(tool_name.clone()),
                Some(tool_call_id.clone()),
                None,
                Some(serde_json::json!({
                    "args": input.args,
                    "result": input.result,
                    "is_error": input.is_error,
                })),
            )
            .await?;
        self.record_trace(
            HookTrigger::AfterTool,
            if evaluated.resolution.refresh_snapshot {
                "refresh_requested"
            } else {
                "effects_applied"
            },
            Some(tool_name),
            Some(tool_call_id),
            None,
            &evaluated,
        );

        Ok(AfterToolCallEffects {
            refresh_snapshot: evaluated.resolution.refresh_snapshot,
            diagnostics: evaluated
                .resolution
                .diagnostics
                .into_iter()
                .map(|entry| entry.summary)
                .collect(),
            ..AfterToolCallEffects::default()
        })
    }

    async fn after_turn(
        &self,
        input: AfterTurnInput,
        _cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError> {
        let evaluated = self
            .evaluate(
                HookTrigger::AfterTurn,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "assistant_message": input.message,
                    "tool_results": input.tool_results,
                })),
            )
            .await?;
        self.record_trace(
            HookTrigger::AfterTurn,
            if evaluated.resolution.context_fragments.is_empty()
                && evaluated.resolution.constraints.is_empty()
            {
                "noop"
            } else {
                "steering_injected"
            },
            None,
            None,
            None,
            &evaluated,
        );

        let steering = build_hook_steering_messages(
            &evaluated.snapshot,
            &evaluated.resolution.context_fragments,
            &evaluated.resolution.constraints,
            &evaluated.runtime,
        );

        Ok(TurnControlDecision {
            steering,
            follow_up: Vec::new(),
            refresh_snapshot: evaluated.resolution.refresh_snapshot,
            diagnostics: evaluated
                .resolution
                .diagnostics
                .into_iter()
                .map(|entry| entry.summary)
                .collect(),
        })
    }

    async fn before_stop(
        &self,
        input: BeforeStopInput,
        _cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError> {
        let evaluated = self
            .evaluate(
                HookTrigger::BeforeStop,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                    "last_assistant_text": input
                        .context
                        .messages
                        .iter()
                        .rev()
                        .find_map(|message| match message {
                            agentdash_agent::AgentMessage::Assistant { .. } => {
                                message.first_text().map(ToString::to_string)
                            }
                            _ => None,
                        }),
                })),
            )
            .await?;

        let steering = build_hook_steering_messages(
            &evaluated.snapshot,
            &evaluated.resolution.context_fragments,
            &evaluated.resolution.constraints,
            &evaluated.runtime,
        );
        let completion_satisfied = evaluated
            .resolution
            .completion
            .as_ref()
            .is_some_and(|completion| completion.satisfied);

        if steering.is_empty() && completion_satisfied {
            self.record_trace(
                HookTrigger::BeforeStop,
                "stop",
                None,
                None,
                None,
                &evaluated,
            );
            return Ok(StopDecision::Stop);
        }

        self.record_trace(
            HookTrigger::BeforeStop,
            "continue",
            None,
            None,
            None,
            &evaluated,
        );
        Ok(StopDecision::Continue {
            steering,
            follow_up: Vec::new(),
            reason: Some(if completion_satisfied {
                "hook runtime 尚有额外约束待处理，继续 loop".to_string()
            } else {
                "hook runtime 尚未满足 stop gate，继续 loop".to_string()
            }),
        })
    }
}

struct EvaluatedResolution {
    snapshot: crate::hooks::SessionHookSnapshot,
    resolution: crate::hooks::HookResolution,
    runtime: HookSessionRuntimeSnapshot,
}

fn build_hook_injection_message(
    snapshot: &crate::hooks::SessionHookSnapshot,
    resolution: &crate::hooks::HookResolution,
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<AgentMessage> {
    let content = build_hook_markdown(
        snapshot,
        &resolution.context_fragments,
        &resolution.constraints,
        runtime,
    )?;
    Some(AgentMessage::user(content))
}

fn build_hook_steering_messages(
    snapshot: &crate::hooks::SessionHookSnapshot,
    fragments: &[HookContextFragment],
    constraints: &[HookConstraint],
    runtime: &HookSessionRuntimeSnapshot,
) -> Vec<AgentMessage> {
    build_hook_markdown(snapshot, fragments, constraints, runtime)
        .map(|content| vec![AgentMessage::user(content)])
        .unwrap_or_default()
}

fn build_hook_markdown(
    snapshot: &crate::hooks::SessionHookSnapshot,
    fragments: &[HookContextFragment],
    constraints: &[HookConstraint],
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<String> {
    if fragments.is_empty() && constraints.is_empty() {
        return None;
    }

    let mut sections = Vec::new();

    sections.push(format!(
        "[系统动态 Hook 上下文]\n当前 session_id={}，revision={}",
        snapshot.session_id, runtime.revision
    ));

    if !snapshot.owners.is_empty() {
        sections.push(format!(
            "## 归属对象\n{}",
            snapshot
                .owners
                .iter()
                .map(|owner| format!("- {}: {} {}", owner.owner_type, owner.label.as_deref().unwrap_or("??"), owner.owner_id))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    let mut fragment_lines = Vec::new();
    if !fragments.is_empty() {
        fragment_lines.push("## 动态注入上下文".to_string());
        for fragment in fragments {
            fragment_lines.push(format!("### {}", fragment.label));
            fragment_lines.push(fragment.content.clone());
            fragment_lines.push(String::new());
        }
    }
    if !fragment_lines.is_empty() {
        while fragment_lines.last().is_some_and(|line| line.is_empty()) {
            fragment_lines.pop();
        }
        sections.push(fragment_lines.join("\n"));
    }

    if !constraints.is_empty() {
        sections.push(format!(
            "## 必须遵守的流程约束\n{}",
            constraints
                .iter()
                .map(|constraint| format!("- {}", constraint.description))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    sections.push("以上内容由 Hook Runtime 自动注入，不代表用户新增需求，但必须优先遵守。".to_string());

    Some(sections.join("\n\n"))
}

fn map_runtime_error(error: crate::hooks::HookError) -> AgentRuntimeError {
    AgentRuntimeError::Runtime(error.to_string())
}
