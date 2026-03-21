use std::sync::Arc;

use agentdash_agent::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeStopInput, BeforeToolCallInput, StopDecision, ToolCallDecision,
    TransformContextInput, TransformContextOutput, TurnControlDecision,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::hooks::{
    HookConstraint, HookContextFragment, HookEvaluationQuery, HookSessionRuntimeSnapshot,
    HookTrigger, SessionHookRefreshQuery, SharedHookSessionRuntime,
};

pub struct HookRuntimeDelegate {
    hook_session: SharedHookSessionRuntime,
}

impl HookRuntimeDelegate {
    pub fn new(hook_session: SharedHookSessionRuntime) -> Arc<dyn AgentRuntimeDelegate> {
        Arc::new(Self { hook_session })
    }

    async fn evaluate(
        &self,
        trigger: HookTrigger,
        payload: Option<serde_json::Value>,
    ) -> Result<EvaluatedResolution, AgentRuntimeError> {
        let snapshot = self.hook_session.snapshot();
        let resolution = self
            .hook_session
            .evaluate(HookEvaluationQuery {
                session_id: self.hook_session.session_id().to_string(),
                trigger: trigger.clone(),
                turn_id: None,
                tool_name: None,
                tool_call_id: None,
                subagent_type: None,
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
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                })),
            )
            .await?;
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
        let snapshot = self.hook_session.snapshot();
        let resolution = self
            .hook_session
            .evaluate(HookEvaluationQuery {
                session_id: self.hook_session.session_id().to_string(),
                trigger: HookTrigger::BeforeTool,
                turn_id: None,
                tool_name: Some(input.tool_call.name.clone()),
                tool_call_id: Some(input.tool_call.id.clone()),
                subagent_type: None,
                snapshot: Some(snapshot),
                payload: Some(serde_json::json!({
                    "args": input.args,
                })),
            })
            .await
            .map_err(map_runtime_error)?;

        if resolution.refresh_snapshot {
            self.hook_session
                .refresh(SessionHookRefreshQuery {
                    session_id: self.hook_session.session_id().to_string(),
                    turn_id: None,
                    reason: Some(format!("before_tool:{}", input.tool_call.name)),
                })
                .await
                .map_err(map_runtime_error)?;
        }

        if let Some(reason) = resolution.block_reason {
            return Ok(ToolCallDecision::Deny { reason });
        }
        if let Some(args) = resolution.rewritten_tool_input {
            return Ok(ToolCallDecision::Rewrite { args, note: None });
        }
        Ok(ToolCallDecision::Allow)
    }

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
        let snapshot = self.hook_session.snapshot();
        let resolution = self
            .hook_session
            .evaluate(HookEvaluationQuery {
                session_id: self.hook_session.session_id().to_string(),
                trigger: HookTrigger::AfterTool,
                turn_id: None,
                tool_name: Some(input.tool_call.name.clone()),
                tool_call_id: Some(input.tool_call.id.clone()),
                subagent_type: None,
                snapshot: Some(snapshot),
                payload: Some(serde_json::json!({
                    "args": input.args,
                    "result": input.result,
                    "is_error": input.is_error,
                })),
            })
            .await
            .map_err(map_runtime_error)?;

        if resolution.refresh_snapshot {
            self.hook_session
                .refresh(SessionHookRefreshQuery {
                    session_id: self.hook_session.session_id().to_string(),
                    turn_id: None,
                    reason: Some(format!("after_tool:{}", input.tool_call.name)),
                })
                .await
                .map_err(map_runtime_error)?;
        }

        Ok(AfterToolCallEffects {
            refresh_snapshot: resolution.refresh_snapshot,
            diagnostics: resolution
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
                Some(serde_json::json!({
                    "assistant_message": input.message,
                    "tool_results": input.tool_results,
                })),
            )
            .await?;

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
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                })),
            )
            .await?;

        let steering = build_hook_steering_messages(
            &evaluated.snapshot,
            &evaluated.resolution.context_fragments,
            &evaluated.resolution.constraints,
            &evaluated.runtime,
        );

        if steering.is_empty() {
            return Ok(StopDecision::Stop);
        }

        Ok(StopDecision::Continue {
            steering,
            follow_up: Vec::new(),
            reason: Some("hook runtime 注入了额外约束，继续 loop".to_string()),
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
    context_fragments: &[HookContextFragment],
    constraints: &[HookConstraint],
    runtime: &HookSessionRuntimeSnapshot,
) -> Vec<AgentMessage> {
    build_hook_markdown(snapshot, context_fragments, constraints, runtime)
        .map(AgentMessage::user)
        .into_iter()
        .collect()
}

fn build_hook_markdown(
    snapshot: &crate::hooks::SessionHookSnapshot,
    context_fragments: &[HookContextFragment],
    constraints: &[HookConstraint],
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<String> {
    if context_fragments.is_empty() && constraints.is_empty() {
        return None;
    }

    let mut sections = vec![format!(
        "[系统动态 Hook 上下文]\n当前 session_id={}，revision={}",
        snapshot.session_id, runtime.revision
    )];

    if !snapshot.owners.is_empty() {
        sections.push(format!(
            "## 归属对象\n{}",
            snapshot
                .owners
                .iter()
                .map(|owner| {
                    format!(
                        "- {}: {}",
                        owner.owner_type,
                        owner.label.as_deref().unwrap_or(owner.owner_id.as_str())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if !context_fragments.is_empty() {
        sections.push(format!(
            "## 动态注入上下文\n{}",
            context_fragments
                .iter()
                .map(|fragment| { format!("### {}\n{}", fragment.label, fragment.content.trim()) })
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
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

    sections
        .push("以上内容由 Hook Runtime 自动注入，不代表用户新增需求，但必须优先遵守。".to_string());
    Some(sections.join("\n\n"))
}

fn map_runtime_error(error: crate::hooks::HookError) -> AgentRuntimeError {
    AgentRuntimeError::Runtime(error.to_string())
}
