//! Workflow runtime context transition 的统一应用入口。
//!
//! 这里刻意放在 Hub 层：transition 应用需要同时触碰 live connector、SessionRuntime、
//! persistence event、Hook runtime 与 Bundle sink。调用方只描述“目标上下文是什么”，
//! 不再各自手写事件 JSON 或 hook 触发顺序。

use std::collections::BTreeSet;

use agentdash_agent_types::DynAgentTool;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookInjection, RuntimeEventSource, SetDelta,
    SharedHookRuntime,
};
use uuid::Uuid;

use super::super::assignment_context_frame::build_runtime_assignment_context_frame;
use super::super::context_frame::{self, ContextFramePayload};
use super::super::dimension::{self, DimensionDelta};
use super::SessionRuntimeInner;
use crate::hooks::hook_injection_to_fragment;
use crate::session::{
    CapabilityState, CapabilityStateDelta, PendingCapabilityStateTransition,
    RuntimeCapabilityTransition, RuntimeContextTransition, apply_runtime_capability_transition,
    compute_capability_state_delta,
};

#[derive(Debug, Clone)]
pub(crate) struct LiveRuntimeContextTransitionInput {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub phase_node: String,
    #[allow(dead_code)]
    pub run_id: Option<Uuid>,
    #[allow(dead_code)]
    pub lifecycle_key: Option<String>,
    pub before_state: Option<CapabilityState>,
    pub after_state: CapabilityState,
    pub capability_keys: BTreeSet<String>,
    pub key_delta: SetDelta,
    pub apply_mode: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeContextTransitionOutcome {
    pub capability_delta: Option<SetDelta>,
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
    pub transition: RuntimeCapabilityTransition,
    pub capability_keys: BTreeSet<String>,
    pub source_turn_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PendingRuntimeContextApplication {
    pub context_frames: Vec<ContextFrame>,
}

pub(crate) fn build_initial_capability_state_frame(
    capability_state: &CapabilityState,
    capability_keys: &BTreeSet<String>,
    tools: &[DynAgentTool],
) -> ContextFrame {
    let initial_delta = SetDelta {
        added: capability_keys.iter().cloned().collect(),
        removed: Vec::new(),
    };
    let state_delta = compute_capability_state_delta(None, capability_state, capability_keys);
    build_context_frame(
        "bootstrap",
        Some("initial"),
        "queued_for_transform_context",
        &initial_delta,
        capability_keys,
        Some(&state_delta),
        tools,
        &capability_state.skill.skills,
    )
}

impl SessionRuntimeInner {
    pub(crate) async fn apply_live_runtime_context_transition(
        &self,
        hook_runtime: &SharedHookRuntime,
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

        let delta = hook_runtime.update_capabilities(input.capability_keys.clone());
        let notification_delta = delta.clone().unwrap_or_else(|| input.key_delta.clone());

        let injections = self
            .collect_runtime_context_update_injections(&input.session_id)
            .await;
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );
        let notice = build_live_context_frame(&input, &notification_delta, &state_delta, &tools);
        self.emit_context_frame(&input.session_id, input.turn_id.as_deref(), &notice)
            .await
            .map_err(|error| format!("Phase node runtime context notice 持久化失败: {error}"))?;
        let _ = context_frame::enqueue_context_frame(hook_runtime, &notice);

        // assignment_context 作为独立 frame 一职一责地发出，不再和能力/工具 delta 混装。
        if let Some(workflow_frame) = build_workflow_assignment_context_frame(
            &input.phase_node,
            input.apply_mode,
            &injections,
        ) {
            self.emit_context_frame(&input.session_id, input.turn_id.as_deref(), &workflow_frame)
                .await
                .map_err(|error| format!("Phase node mission context frame 持久化失败: {error}"))?;
            let _ = context_frame::enqueue_context_frame(hook_runtime, &workflow_frame);
        }

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
            input.transition,
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

        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );
        let capability_delta = SetDelta {
            added: state_delta.tool_capabilities.added.clone(),
            removed: state_delta.tool_capabilities.removed.clone(),
        };
        let notice = build_context_frame(
            &input.phase_node,
            Some("pending_next_turn"),
            "deferred_until_next_turn",
            &capability_delta,
            &input.capability_keys,
            Some(&state_delta),
            &[],
            &input.after_state.skill.skills,
        );
        self.emit_context_frame(&input.session_id, input.turn_id.as_deref(), &notice)
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
        _turn_id: &str,
        hook_runtime: Option<&SharedHookRuntime>,
        before_state: CapabilityState,
        final_capability_state: &CapabilityState,
        transitions: &[PendingCapabilityStateTransition],
        tools: &[DynAgentTool],
    ) -> PendingRuntimeContextApplication {
        let mut application = PendingRuntimeContextApplication::default();
        if transitions.is_empty() {
            return application;
        }

        if let Some(hook_runtime) = hook_runtime
            && let Some(last_transition) = transitions.last()
        {
            let _ = hook_runtime.update_capabilities(last_transition.capability_keys.clone());
        }

        let mut pending_event_before_state = before_state;
        for (index, pending) in transitions.iter().enumerate() {
            let pending_after_state = if index + 1 == transitions.len() {
                final_capability_state.clone()
            } else {
                match apply_runtime_capability_transition(
                    &pending_event_before_state,
                    &pending.transition,
                ) {
                    Ok(state) => state,
                    Err(error) => {
                        tracing::warn!(
                            session_id,
                            transition_id = %pending.id,
                            "pending runtime capability transition replay failed before event emission: {error}"
                        );
                        pending_event_before_state.clone()
                    }
                }
            };
            let state_delta = compute_capability_state_delta(
                Some(&pending_event_before_state),
                &pending_after_state,
                &pending.capability_keys,
            );
            let capability_delta = SetDelta {
                added: state_delta.tool_capabilities.added.clone(),
                removed: state_delta.tool_capabilities.removed.clone(),
            };
            let notice = build_context_frame(
                &pending.phase_node,
                Some("applied_on_next_turn"),
                "applied_before_prompt",
                &capability_delta,
                &pending.capability_keys,
                Some(&state_delta),
                tools,
                &pending_after_state.skill.skills,
            );
            application.context_frames.push(notice);

            let injections = self
                .collect_runtime_context_update_injections(session_id)
                .await;
            if let Some(workflow_frame) = build_workflow_assignment_context_frame(
                &pending.phase_node,
                "applied_on_next_turn",
                &injections,
            ) {
                application.context_frames.push(workflow_frame);
            }
            pending_event_before_state = pending_after_state;
        }
        application
    }

    async fn emit_runtime_context_changed_notice(&self, input: &LiveRuntimeContextTransitionInput) {
        let injections = self
            .collect_runtime_context_update_injections(&input.session_id)
            .await;
        if let Some(notice) = build_workflow_assignment_context_frame(
            &input.phase_node,
            input.apply_mode,
            &injections,
        ) {
            let _ = self
                .emit_context_frame(&input.session_id, input.turn_id.as_deref(), &notice)
                .await;
            if let Some(hook_runtime) = self.get_hook_runtime(&input.session_id).await {
                let _ = context_frame::enqueue_context_frame(&hook_runtime, &notice);
            }
        }
    }
}

fn build_live_context_frame(
    input: &LiveRuntimeContextTransitionInput,
    notification_delta: &SetDelta,
    state_delta: &CapabilityStateDelta,
    tools: &[DynAgentTool],
) -> ContextFrame {
    let metadata = RuntimeContextUpdateFrame::new(
        &input.phase_node,
        Some(input.apply_mode),
        "queued_for_transform_context",
        notification_delta,
        &input.capability_keys,
        Some(state_delta),
        tools,
        &input.after_state.skill.skills,
    );
    context_frame::build_context_frame(&metadata)
}

fn build_context_frame(
    phase_node: &str,
    apply_mode: Option<&str>,
    delivery_status: &str,
    capability_delta: &SetDelta,
    effective_capabilities: &BTreeSet<String>,
    state_delta: Option<&CapabilityStateDelta>,
    tools: &[DynAgentTool],
    skill_entries: &[agentdash_spi::context::capability::SkillEntry],
) -> ContextFrame {
    let metadata = RuntimeContextUpdateFrame::new(
        phase_node,
        apply_mode,
        delivery_status,
        capability_delta,
        effective_capabilities,
        state_delta,
        tools,
        skill_entries,
    );
    context_frame::build_context_frame(&metadata)
}

/// 根据 hook injections 构造独立的 `assignment_context` frame。
/// 已从能力更新帧剥离，遵循 frame 一职一责。
///
/// 统一走 HookInjection → ContextFragment → AssignmentContextFrame 单一路径，
/// 复用 `assignment_context_frame.rs` 的渲染逻辑。
fn build_workflow_assignment_context_frame(
    phase_node: &str,
    apply_mode: &str,
    injections: &[HookInjection],
) -> Option<ContextFrame> {
    if injections.is_empty() {
        return None;
    }
    let fragments: Vec<_> = injections
        .iter()
        .cloned()
        .map(hook_injection_to_fragment)
        .collect();
    build_runtime_assignment_context_frame(phase_node, Some(apply_mode), &fragments)
}

struct RuntimeContextUpdateFrame {
    phase_node: String,
    apply_mode: Option<String>,
    delivery_status: String,
    dimensions: Vec<Box<dyn DimensionDelta>>,
}

impl RuntimeContextUpdateFrame {
    fn new(
        phase_node: &str,
        apply_mode: Option<&str>,
        delivery_status: &str,
        capability_delta: &SetDelta,
        effective_capabilities: &BTreeSet<String>,
        state_delta: Option<&CapabilityStateDelta>,
        tools: &[DynAgentTool],
        skill_entries: &[agentdash_spi::context::capability::SkillEntry],
    ) -> Self {
        let mut dimensions: Vec<Box<dyn DimensionDelta>> = Vec::new();

        if let Some(d) = dimension::capability_key::CapabilityKeyDimensionDelta::from_delta(
            capability_delta,
            effective_capabilities,
            state_delta,
        ) {
            dimensions.push(d);
        }
        if let Some(d) = dimension::tool_path::ToolPathDimensionDelta::from_state_delta(state_delta)
        {
            dimensions.push(d);
        }
        if let Some(d) =
            dimension::mcp_server::McpServerDimensionDelta::from_state_delta(state_delta)
        {
            dimensions.push(d);
        }
        if let Some(d) = dimension::vfs::VfsDimensionDelta::from_state_delta(state_delta) {
            dimensions.push(d);
        }
        if let Some(d) =
            dimension::skill::SkillDimensionDelta::from_state_delta(state_delta, skill_entries)
        {
            dimensions.push(d);
        }
        if let Some(d) =
            dimension::tool_schema::ToolSchemaDimensionDelta::from_tools_and_state_delta(
                tools,
                state_delta,
            )
        {
            dimensions.push(d);
        }

        Self {
            phase_node: phase_node.to_string(),
            apply_mode: apply_mode.map(ToString::to_string),
            delivery_status: delivery_status.to_string(),
            dimensions,
        }
    }
}

impl ContextFramePayload for RuntimeContextUpdateFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("runtime-context-{}-{created_at_ms}", self.phase_node)
    }

    fn kind(&self) -> &'static str {
        "capability_state_update"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn phase_node(&self) -> Option<String> {
        Some(self.phase_node.clone())
    }

    fn apply_mode(&self) -> Option<String> {
        self.apply_mode.clone()
    }

    fn delivery_status(&self) -> String {
        self.delivery_status.clone()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        self.dimensions.iter().map(|d| d.to_section()).collect()
    }

    fn rendered_text(&self) -> String {
        let phase = Some(self.phase_node.as_str());
        self.dimensions
            .iter()
            .map(|d| d.render_text(phase))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent_types::{
        AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use tokio_util::sync::CancellationToken;

    use super::*;

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
    fn live_context_frame_includes_tool_schema_delta_only() {
        let input = LiveRuntimeContextTransitionInput {
            session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            phase_node: "apply".to_string(),
            run_id: None,
            lifecycle_key: None,
            before_state: None,
            after_state: CapabilityState::default(),
            capability_keys: BTreeSet::from(["workflow_management".to_string()]),
            key_delta: SetDelta::default(),
            apply_mode: "live",
        };
        let state_delta = CapabilityStateDelta {
            excluded_tool_paths: crate::session::SetDelta {
                added: Vec::new(),
                removed: vec!["workflow_management::upsert_workflow_tool".to_string()],
            },
            ..Default::default()
        };
        let tools: Vec<DynAgentTool> = vec![Arc::new(StubTool)];

        let notice = build_live_context_frame(&input, &SetDelta::default(), &state_delta, &tools);

        assert_eq!(notice.kind, "capability_state_update");
        assert_eq!(notice.phase_node.as_deref(), Some("apply"));
        assert_eq!(notice.apply_mode.as_deref(), Some("live"));
        // TOOL section 只承载 added_tools;路径级变化全部归 CAP。
        let tool_section = notice
            .sections
            .iter()
            .find_map(|section| match section {
                ContextFrameSection::ToolSchemaDelta { added_tools } => Some(added_tools),
                _ => None,
            })
            .expect("tool_schema_delta section should exist with added_tools");
        assert_eq!(tool_section.len(), 1);
        // assignment_context section 不应混入 capability_state_update frame。
        assert!(
            !notice
                .sections
                .iter()
                .any(|section| matches!(section, ContextFrameSection::AssignmentContext { .. }))
        );
        assert!(
            notice
                .rendered_text
                .contains("## Tool Schema Delta — Step Transition: apply")
        );
        // Path-only 提示已归 CAP,TOOL 的 rendered_text 不应再包含 "Restored tool paths"。
        assert!(!notice.rendered_text.contains("Restored tool paths"));
        assert!(
            notice
                .rendered_text
                .contains("mcp_agentdash_workflow_tools_upsert_workflow_tool")
        );
        assert!(notice.rendered_text.contains("创建或更新 Workflow 定义"));
        assert!(notice.rendered_text.contains("参数说明："));
        assert!(notice.rendered_text.contains("`key` (required, string)"));
        assert!(!notice.rendered_text.contains("```json"));
        assert!(notice.rendered_text.contains("Workflow key"));
    }

    #[test]
    fn assignment_context_frame_is_emitted_independently() {
        let injections = vec![HookInjection {
            slot: "workflow".to_string(),
            content: "Active workflow step: apply".to_string(),
            source: "workflow:admin:apply".to_string(),
        }];

        let frame = build_workflow_assignment_context_frame("apply", "live", &injections)
            .expect("assignment_context frame should be emitted when injections present");

        assert_eq!(frame.kind, "assignment_context");
        assert_eq!(frame.phase_node.as_deref(), Some("apply"));
        assert_eq!(frame.apply_mode.as_deref(), Some("live"));
        assert_eq!(frame.sections.len(), 1);
        assert!(matches!(
            frame.sections[0],
            ContextFrameSection::AssignmentContext { .. }
        ));

        // 没有 injection 时不构造 frame。
        assert!(build_workflow_assignment_context_frame("apply", "live", &[]).is_none());
    }
}
