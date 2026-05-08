//! Workflow runtime context transition 的统一应用入口。
//!
//! 这里刻意放在 Hub 层：transition 应用需要同时触碰 live connector、SessionRuntime、
//! persistence event、Hook runtime 与 Bundle sink。调用方只描述“目标上下文是什么”，
//! 不再各自手写事件 JSON 或 hook 触发顺序。

use std::collections::BTreeSet;

use agentdash_agent_types::{DynAgentTool, ToolDefinition};
use agentdash_spi::hooks::{
    CapabilityDelta, HookInjection, HookTurnStartNotice, RuntimeEventSource,
    SharedHookSessionRuntime,
};
use serde_json::Value;
use uuid::Uuid;

use super::super::hook_messages as msg;
use super::SessionHub;
use crate::capability::build_capability_delta_markdown;
use crate::session::{
    CapabilityState, CapabilityStateDelta, PendingCapabilityStateTransition,
    RuntimeContextTransition, compute_capability_state_delta,
};

#[derive(Debug, Clone)]
pub(crate) struct LiveRuntimeContextTransitionInput {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub phase_node: String,
    pub run_id: Option<Uuid>,
    pub lifecycle_key: Option<String>,
    pub before_state: Option<CapabilityState>,
    pub after_state: CapabilityState,
    pub capability_keys: BTreeSet<String>,
    pub key_delta: CapabilityDelta,
    pub apply_mode: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeContextTransitionOutcome {
    pub capability_delta: Option<CapabilityDelta>,
    pub emitted_capability_change: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingRuntimeContextTransitionInput {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub transition_id: String,
    pub phase_node: String,
    pub run_id: Uuid,
    pub lifecycle_key: String,
    pub before_state: Option<CapabilityState>,
    pub after_state: CapabilityState,
    pub capability_keys: BTreeSet<String>,
    pub source_turn_id: Option<String>,
    pub created_at: i64,
}

impl SessionHub {
    pub(crate) async fn apply_live_runtime_context_transition(
        &self,
        hook_session: &SharedHookSessionRuntime,
        input: LiveRuntimeContextTransitionInput,
    ) -> Result<RuntimeContextTransitionOutcome, String> {
        let state_changed = input.before_state.as_ref() != Some(&input.after_state);
        if input.key_delta.is_empty() && !state_changed {
            self.emit_runtime_context_changed_notice(&input).await;
            return Ok(RuntimeContextTransitionOutcome {
                capability_delta: None,
                emitted_capability_change: false,
            });
        }

        let tools = self
            .replace_current_capability_state(&input.session_id, input.after_state.clone())
            .await
            .map_err(|error| format!("Phase node 能力状态热更新失败: {error}"))?;

        let delta = hook_session.update_capabilities(input.capability_keys.clone());
        let notification_delta = delta.clone().unwrap_or_else(|| input.key_delta.clone());
        let steering_delivery = serde_json::json!({
            "status": "queued_for_transform_context"
        });

        let event = RuntimeContextTransition {
            phase_node: &input.phase_node,
            run_id: input.run_id,
            lifecycle_key: input.lifecycle_key.as_deref(),
            apply_mode: input.apply_mode,
            before_state: input.before_state.as_ref(),
            after_state: &input.after_state,
            capability_keys: &input.capability_keys,
            steering_delivery,
            state_changed_override: None,
            steering_capability_delta: Some(&notification_delta),
        }
        .event_payload();

        self.emit_capability_state_changed(
            &input.session_id,
            input.turn_id.as_deref(),
            event.clone(),
        )
        .await
        .map_err(|error| format!("Phase node capability state 事件持久化失败: {error}"))?;

        let injections = self
            .collect_runtime_context_update_injections(&input.session_id)
            .await;
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );
        let notice = build_live_runtime_context_notice(
            &input,
            &notification_delta,
            &state_delta,
            &tools,
            &injections,
        );
        enqueue_runtime_context_notice(hook_session.as_ref(), input.phase_node.as_str(), notice);

        Ok(RuntimeContextTransitionOutcome {
            capability_delta: delta,
            emitted_capability_change: true,
        })
    }

    pub(crate) async fn enqueue_pending_runtime_context_transition(
        &self,
        input: PendingRuntimeContextTransitionInput,
    ) -> Result<(), String> {
        let state_changed = input.before_state.as_ref() != Some(&input.after_state);
        let transition = RuntimeContextTransition {
            phase_node: &input.phase_node,
            run_id: Some(input.run_id),
            lifecycle_key: Some(&input.lifecycle_key),
            apply_mode: "pending_next_turn",
            before_state: input.before_state.as_ref(),
            after_state: &input.after_state,
            capability_keys: &input.capability_keys,
            steering_delivery: serde_json::json!({
                "status": "deferred_until_next_turn"
            }),
            state_changed_override: Some(state_changed),
            steering_capability_delta: None,
        };
        let Some(pending_transition) = transition.to_pending_capability_state_transition(
            input.transition_id,
            input.source_turn_id,
            input.created_at,
        ) else {
            return Err(format!(
                "PhaseNode `{}` pending transition 缺少 run/lifecycle 元数据",
                input.phase_node
            ));
        };

        self.enqueue_pending_capability_state_transition(&input.session_id, pending_transition)
            .await
            .map_err(|error| {
                format!(
                    "PhaseNode `{}` 能力状态 pending transition 写入失败: {error}",
                    input.phase_node
                )
            })?;

        self.emit_capability_state_changed(
            &input.session_id,
            input.turn_id.as_deref(),
            transition.event_payload(),
        )
        .await
        .map_err(|error| {
            format!(
                "PhaseNode `{}` pending 事件持久化失败: {error}",
                input.phase_node
            )
        })?;

        Ok(())
    }

    pub(crate) async fn apply_pending_runtime_context_transitions_on_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        hook_session: Option<&SharedHookSessionRuntime>,
        before_state: CapabilityState,
        transitions: &[PendingCapabilityStateTransition],
    ) {
        if transitions.is_empty() {
            return;
        }

        if let Some(hook_session) = hook_session
            && let Some(last_transition) = transitions.last()
        {
            let _ = hook_session.update_capabilities(last_transition.capability_keys.clone());
        }

        let mut pending_event_before_state = before_state;
        for pending in transitions {
            let payload = RuntimeContextTransition {
                phase_node: &pending.phase_node,
                run_id: Some(pending.run_id),
                lifecycle_key: Some(&pending.lifecycle_key),
                apply_mode: "applied_on_next_turn",
                before_state: Some(&pending_event_before_state),
                after_state: &pending.state,
                capability_keys: &pending.capability_keys,
                steering_delivery: serde_json::json!({ "status": "applied_before_prompt" }),
                state_changed_override: None,
                steering_capability_delta: None,
            }
            .event_payload();
            pending_event_before_state = pending.state.clone();
            let _ = self
                .emit_capability_state_changed(session_id, Some(turn_id), payload.clone())
                .await;
            let _ = payload;
            let injections = self
                .collect_runtime_context_update_injections(session_id)
                .await;
            if !injections.is_empty()
                && let Some(hook_session) = hook_session
            {
                enqueue_runtime_context_notice(
                    hook_session.as_ref(),
                    pending.phase_node.as_str(),
                    build_context_only_notice(&pending.phase_node, &injections),
                );
            }
        }
    }

    async fn emit_runtime_context_changed_notice(&self, input: &LiveRuntimeContextTransitionInput) {
        let injections = self
            .collect_runtime_context_update_injections(&input.session_id)
            .await;
        if !injections.is_empty() {
            let notice = build_context_only_notice(&input.phase_node, &injections);
            if let Some(hook_session) = self.get_hook_session_runtime(&input.session_id).await {
                enqueue_runtime_context_notice(
                    hook_session.as_ref(),
                    input.phase_node.as_str(),
                    notice,
                );
            }
        }
    }
}

fn build_live_runtime_context_notice(
    input: &LiveRuntimeContextTransitionInput,
    notification_delta: &CapabilityDelta,
    state_delta: &CapabilityStateDelta,
    tools: &[DynAgentTool],
    injections: &[HookInjection],
) -> String {
    let mut sections = vec![build_capability_delta_markdown(
        &input.phase_node,
        notification_delta,
        &input.capability_keys,
        Some(state_delta),
    )];
    if let Some(tool_block) = build_tool_definition_update_block(state_delta, tools) {
        sections.push(tool_block);
    }
    if let Some(injection_block) = build_runtime_injection_block(injections) {
        sections.push(injection_block);
    }
    sections.join("\n\n")
}

fn build_context_only_notice(phase_node: &str, injections: &[HookInjection]) -> String {
    let mut sections = vec![format!(
        "## Runtime Context Update — Step Transition: {phase_node}"
    )];
    if let Some(injection_block) = build_runtime_injection_block(injections) {
        sections.push(injection_block);
    }
    sections.join("\n\n")
}

fn enqueue_runtime_context_notice(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
    phase_node: &str,
    content: String,
) {
    hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
        id: format!(
            "runtime-context-{phase_node}-{}",
            chrono::Utc::now().timestamp_millis()
        ),
        created_at_ms: chrono::Utc::now().timestamp_millis(),
        source: RuntimeEventSource::RuntimeContextUpdate,
        content,
    });
}

fn build_runtime_injection_block(injections: &[HookInjection]) -> Option<String> {
    msg::runtime_hook_injection_notification(
        "Workflow Context Update",
        "Workflow 运行上下文已更新。以下内容已在同一运行时边界生效：",
        injections,
    )
}

fn build_tool_definition_update_block(
    state_delta: &CapabilityStateDelta,
    tools: &[DynAgentTool],
) -> Option<String> {
    let raw_tool_names = changed_raw_tool_names(state_delta);
    if raw_tool_names.is_empty() {
        return None;
    }
    let mut definitions = tools
        .iter()
        .filter_map(|tool| {
            let name = tool.name();
            raw_tool_names
                .iter()
                .any(|raw| name == raw || name.ends_with(&format!("_{raw}")))
                .then(|| ToolDefinition::from_tool(tool.as_ref()))
        })
        .collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.name.cmp(&right.name));
    definitions.dedup_by(|left, right| left.name == right.name);
    if definitions.is_empty() {
        return None;
    }

    let mut lines = vec![
        "### Tool Definitions Updated".to_string(),
        "以下工具已进入当前 provider request 的工具 schema，可直接调用：".to_string(),
    ];
    for definition in definitions {
        lines.push(format_tool_definition(&definition));
    }
    Some(lines.join("\n"))
}

fn changed_raw_tool_names(state_delta: &CapabilityStateDelta) -> BTreeSet<String> {
    state_delta
        .excluded_tool_paths
        .removed
        .iter()
        .chain(state_delta.included_tool_paths.added.iter())
        .filter_map(|path| path.rsplit_once("::").map(|(_, tool)| tool.to_string()))
        .collect()
}

fn format_tool_definition(definition: &ToolDefinition) -> String {
    let mut lines = Vec::new();
    let description = definition.description.trim();
    if description.is_empty() {
        lines.push(format!("- **{}**", definition.name));
    } else {
        lines.push(format!("- **{}**: {}", definition.name, description));
    }
    lines.extend(format_parameters_brief(&definition.parameters));
    lines.join("\n")
}

fn format_parameters_brief(schema: &Value) -> Vec<String> {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    if properties.is_empty() {
        return Vec::new();
    }
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    properties
        .iter()
        .map(|(name, spec)| {
            let ty = spec.get("type").and_then(Value::as_str).unwrap_or("any");
            let marker = if required.contains(name.as_str()) {
                ", required"
            } else {
                ""
            };
            let description = spec
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            match description {
                Some(description) => format!("  - `{name}` ({ty}{marker}): {description}"),
                None => format!("  - `{name}` ({ty}{marker})"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent_types::{
        AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
    };
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::session::SetDelta;

    struct StubTool;

    #[async_trait]
    impl AgentTool for StubTool {
        fn name(&self) -> &str {
            "mcp_agentdash_workflow_tools_upsert_workflow_tool"
        }

        fn description(&self) -> &str {
            "创建或更新 Workflow 定义"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Workflow key"
                    }
                },
                "required": ["key"]
            })
        }

        async fn execute(
            &self,
            _tool_call_id: &str,
            _args: Value,
            _cancel: CancellationToken,
            _on_update: Option<ToolUpdateCallback>,
        ) -> Result<AgentToolResult, AgentToolError> {
            Ok(AgentToolResult {
                content: vec![ContentPart::text("ok")],
                is_error: false,
                details: None,
            })
        }
    }

    #[test]
    fn tool_definition_update_block_describes_unblocked_tools() {
        let state_delta = CapabilityStateDelta {
            excluded_tool_paths: SetDelta {
                added: vec![],
                removed: vec!["workflow_management::upsert_workflow_tool".to_string()],
            },
            ..Default::default()
        };
        let tools: Vec<DynAgentTool> = vec![Arc::new(StubTool)];

        let block = build_tool_definition_update_block(&state_delta, &tools)
            .expect("unblocked tool should render definition block");

        assert!(block.contains("### Tool Definitions Updated"));
        assert!(block.contains("mcp_agentdash_workflow_tools_upsert_workflow_tool"));
        assert!(block.contains("创建或更新 Workflow 定义"));
        assert!(block.contains("`key` (string, required): Workflow key"));
    }
}
