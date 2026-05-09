//! Workflow runtime context transition 的统一应用入口。
//!
//! 这里刻意放在 Hub 层：transition 应用需要同时触碰 live connector、SessionRuntime、
//! persistence event、Hook runtime 与 Bundle sink。调用方只描述“目标上下文是什么”，
//! 不再各自手写事件 JSON 或 hook 触发顺序。

use std::collections::BTreeSet;

use agentdash_agent_types::DynAgentTool;
use agentdash_spi::hooks::{
    CapabilityDelta, ContextFrame, ContextFrameSection, HookInjection, HookTurnStartNotice,
    RuntimeEventSource, RuntimeHookInjectionEntry, SharedHookSessionRuntime,
};
use uuid::Uuid;

use super::super::context_frame::{self, ContextFramePayload};
use super::super::tool_schema_notice::ToolSchemaDeltaMetadata;
use super::SessionHub;
use crate::capability::capability_description;
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
        let notice = build_live_context_frame(
            &input,
            &notification_delta,
            &state_delta,
            &tools,
            &injections,
        );
        self.emit_context_frame(&input.session_id, input.turn_id.as_deref(), &notice)
            .await
            .map_err(|error| format!("Phase node runtime context notice 持久化失败: {error}"))?;
        enqueue_context_frame(hook_session.as_ref(), notice);

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
        tools: &[DynAgentTool],
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
            let _ = self
                .emit_capability_state_changed(session_id, Some(turn_id), payload.clone())
                .await;
            let _ = payload;
            let injections = self
                .collect_runtime_context_update_injections(session_id)
                .await;
            if let Some(hook_session) = hook_session {
                let state_delta = compute_capability_state_delta(
                    Some(&pending_event_before_state),
                    &pending.state,
                    &pending.capability_keys,
                );
                let capability_delta = CapabilityDelta {
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
                    &injections,
                );
                let _ = self
                    .emit_context_frame(session_id, Some(turn_id), &notice)
                    .await;
                enqueue_context_frame(hook_session.as_ref(), notice);
            }
            pending_event_before_state = pending.state.clone();
        }
    }

    async fn emit_runtime_context_changed_notice(&self, input: &LiveRuntimeContextTransitionInput) {
        let injections = self
            .collect_runtime_context_update_injections(&input.session_id)
            .await;
        if !injections.is_empty() {
            let notice =
                build_context_only_notice(&input.phase_node, input.apply_mode, &injections);
            let _ = self
                .emit_context_frame(&input.session_id, input.turn_id.as_deref(), &notice)
                .await;
            if let Some(hook_session) = self.get_hook_session_runtime(&input.session_id).await {
                enqueue_context_frame(hook_session.as_ref(), notice);
            }
        }
    }
}

fn build_live_context_frame(
    input: &LiveRuntimeContextTransitionInput,
    notification_delta: &CapabilityDelta,
    state_delta: &CapabilityStateDelta,
    tools: &[DynAgentTool],
    injections: &[HookInjection],
) -> ContextFrame {
    let metadata = RuntimeContextUpdateFrame::new(
        &input.phase_node,
        Some(input.apply_mode),
        "queued_for_transform_context",
        notification_delta,
        &input.capability_keys,
        Some(state_delta),
        tools,
        injections,
    );
    context_frame::build_context_frame(&metadata)
}

fn build_context_frame(
    phase_node: &str,
    apply_mode: Option<&str>,
    delivery_status: &str,
    capability_delta: &CapabilityDelta,
    effective_capabilities: &BTreeSet<String>,
    state_delta: Option<&CapabilityStateDelta>,
    tools: &[DynAgentTool],
    injections: &[HookInjection],
) -> ContextFrame {
    let metadata = RuntimeContextUpdateFrame::new(
        phase_node,
        apply_mode,
        delivery_status,
        capability_delta,
        effective_capabilities,
        state_delta,
        tools,
        injections,
    );
    context_frame::build_context_frame(&metadata)
}

fn build_context_only_notice(
    phase_node: &str,
    apply_mode: &str,
    injections: &[HookInjection],
) -> ContextFrame {
    let metadata = WorkflowContextFrame::new(
        phase_node,
        apply_mode,
        "queued_for_transform_context",
        injections,
    );
    context_frame::build_context_frame(&metadata)
}

fn enqueue_context_frame(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
    frame: ContextFrame,
) {
    hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
        id: frame.id.clone(),
        created_at_ms: frame.created_at_ms,
        source: RuntimeEventSource::RuntimeContextUpdate,
        content: frame.rendered_text.clone(),
        context_frame: Some(frame),
    });
}

#[derive(Debug, Clone)]
struct RuntimeContextUpdateFrame {
    phase_node: String,
    apply_mode: Option<String>,
    delivery_status: String,
    capability_delta: CapabilityDeltaFrameMetadata,
    tool_schema_delta: Option<ToolSchemaDeltaMetadata>,
    workflow_context: Option<WorkflowContextMetadata>,
}

impl RuntimeContextUpdateFrame {
    fn new(
        phase_node: &str,
        apply_mode: Option<&str>,
        delivery_status: &str,
        capability_delta: &CapabilityDelta,
        effective_capabilities: &BTreeSet<String>,
        state_delta: Option<&CapabilityStateDelta>,
        tools: &[DynAgentTool],
        injections: &[HookInjection],
    ) -> Self {
        Self {
            phase_node: phase_node.to_string(),
            apply_mode: apply_mode.map(ToString::to_string),
            delivery_status: delivery_status.to_string(),
            capability_delta: CapabilityDeltaFrameMetadata::from_delta(
                capability_delta,
                effective_capabilities,
                state_delta,
            ),
            tool_schema_delta: state_delta.and_then(|delta| {
                ToolSchemaDeltaMetadata::from_tools_and_state_delta(tools, delta)
            }),
            workflow_context: WorkflowContextMetadata::from_hook_injections(injections),
        }
    }
}

impl ContextFramePayload for RuntimeContextUpdateFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("runtime-context-{}-{created_at_ms}", self.phase_node)
    }

    fn kind(&self) -> &'static str {
        "runtime_context_update"
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
        let mut sections = vec![self.capability_delta.section()];
        if let Some(tool_schema_delta) = &self.tool_schema_delta {
            sections.push(tool_schema_delta.section());
        }
        if let Some(workflow_context) = &self.workflow_context {
            sections.push(workflow_context.section());
        }
        sections
    }

    fn rendered_text(&self) -> String {
        let mut blocks = vec![self.capability_delta.render_text(&self.phase_node)];
        if let Some(tool_schema_delta) = &self.tool_schema_delta {
            blocks.push(tool_schema_delta.render_text(Some(&self.phase_node)));
        }
        if let Some(workflow_context) = &self.workflow_context {
            blocks.push(workflow_context.render_text());
        }
        blocks.join("\n\n")
    }
}

#[derive(Debug, Clone)]
struct WorkflowContextFrame {
    phase_node: String,
    apply_mode: String,
    delivery_status: String,
    workflow_context: Option<WorkflowContextMetadata>,
}

impl WorkflowContextFrame {
    fn new(
        phase_node: &str,
        apply_mode: &str,
        delivery_status: &str,
        injections: &[HookInjection],
    ) -> Self {
        Self {
            phase_node: phase_node.to_string(),
            apply_mode: apply_mode.to_string(),
            delivery_status: delivery_status.to_string(),
            workflow_context: WorkflowContextMetadata::from_hook_injections(injections),
        }
    }
}

impl ContextFramePayload for WorkflowContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("runtime-context-{}-{created_at_ms}", self.phase_node)
    }

    fn kind(&self) -> &'static str {
        "workflow_context"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn phase_node(&self) -> Option<String> {
        Some(self.phase_node.clone())
    }

    fn apply_mode(&self) -> Option<String> {
        Some(self.apply_mode.clone())
    }

    fn delivery_status(&self) -> String {
        self.delivery_status.clone()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        self.workflow_context
            .as_ref()
            .map(|metadata| vec![metadata.section()])
            .unwrap_or_default()
    }

    fn rendered_text(&self) -> String {
        self.workflow_context
            .as_ref()
            .map(WorkflowContextMetadata::render_text)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
struct CapabilityDeltaFrameMetadata {
    added_capabilities: Vec<String>,
    removed_capabilities: Vec<String>,
    effective_capabilities: Vec<String>,
    blocked_tool_paths: Vec<String>,
    unblocked_tool_paths: Vec<String>,
    whitelisted_tool_paths: Vec<String>,
    removed_whitelist_paths: Vec<String>,
    added_mcp_servers: Vec<String>,
    removed_mcp_servers: Vec<String>,
    changed_mcp_servers: Vec<String>,
    vfs_mounts_added: Vec<String>,
    vfs_mounts_removed: Vec<String>,
    default_mount_before: Option<String>,
    default_mount_after: Option<String>,
}

impl CapabilityDeltaFrameMetadata {
    fn from_delta(
        capability_delta: &CapabilityDelta,
        effective_capabilities: &BTreeSet<String>,
        state_delta: Option<&CapabilityStateDelta>,
    ) -> Self {
        Self {
            added_capabilities: capability_delta.added.clone(),
            removed_capabilities: capability_delta.removed.clone(),
            effective_capabilities: effective_capabilities.iter().cloned().collect(),
            blocked_tool_paths: state_delta
                .map(|delta| delta.excluded_tool_paths.added.clone())
                .unwrap_or_default(),
            unblocked_tool_paths: state_delta
                .map(|delta| delta.excluded_tool_paths.removed.clone())
                .unwrap_or_default(),
            whitelisted_tool_paths: state_delta
                .map(|delta| delta.included_tool_paths.added.clone())
                .unwrap_or_default(),
            removed_whitelist_paths: state_delta
                .map(|delta| delta.included_tool_paths.removed.clone())
                .unwrap_or_default(),
            added_mcp_servers: state_delta
                .map(|delta| delta.mcp_servers.added.clone())
                .unwrap_or_default(),
            removed_mcp_servers: state_delta
                .map(|delta| delta.mcp_servers.removed.clone())
                .unwrap_or_default(),
            changed_mcp_servers: state_delta
                .map(|delta| delta.mcp_servers.changed.clone())
                .unwrap_or_default(),
            vfs_mounts_added: state_delta
                .map(|delta| delta.vfs.mounts.added.clone())
                .unwrap_or_default(),
            vfs_mounts_removed: state_delta
                .map(|delta| delta.vfs.mounts.removed.clone())
                .unwrap_or_default(),
            default_mount_before: state_delta
                .and_then(|delta| delta.vfs.default_mount.before.clone()),
            default_mount_after: state_delta
                .and_then(|delta| delta.vfs.default_mount.after.clone()),
        }
    }

    fn section(&self) -> ContextFrameSection {
        ContextFrameSection::CapabilityDelta {
            added_capabilities: self.added_capabilities.clone(),
            removed_capabilities: self.removed_capabilities.clone(),
            effective_capabilities: self.effective_capabilities.clone(),
            blocked_tool_paths: self.blocked_tool_paths.clone(),
            unblocked_tool_paths: self.unblocked_tool_paths.clone(),
            whitelisted_tool_paths: self.whitelisted_tool_paths.clone(),
            removed_whitelist_paths: self.removed_whitelist_paths.clone(),
            added_mcp_servers: self.added_mcp_servers.clone(),
            removed_mcp_servers: self.removed_mcp_servers.clone(),
            changed_mcp_servers: self.changed_mcp_servers.clone(),
            vfs_mounts_added: self.vfs_mounts_added.clone(),
            vfs_mounts_removed: self.vfs_mounts_removed.clone(),
            default_mount_before: self.default_mount_before.clone(),
            default_mount_after: self.default_mount_after.clone(),
        }
    }

    fn render_text(&self, phase_node: &str) -> String {
        let mut sections = vec![format!(
            "## Capability Update — Step Transition: {phase_node}"
        )];

        if !self.added_capabilities.is_empty() {
            let mut block = vec!["### Added Capabilities".to_string()];
            append_capability_lines(&mut block, &self.added_capabilities, false);
            sections.push(block.join("\n"));
        }
        if !self.removed_capabilities.is_empty() {
            let mut block = vec!["### Removed Capabilities".to_string()];
            append_capability_lines(&mut block, &self.removed_capabilities, true);
            sections.push(block.join("\n"));
        }

        let caps_block = if self.effective_capabilities.is_empty() {
            "- （无）".to_string()
        } else {
            self.effective_capabilities
                .iter()
                .map(|key| format!("- `{key}`"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        sections.push(format!("### Effective Capabilities\n{caps_block}"));

        let mut tool_lines = vec!["### Tool State Changes".to_string()];
        append_path_lines(
            &mut tool_lines,
            "Blocked tool paths",
            &self.blocked_tool_paths,
            "不再暴露",
        );
        append_path_lines(
            &mut tool_lines,
            "Unblocked tool paths",
            &self.unblocked_tool_paths,
            "重新暴露",
        );
        append_path_lines(
            &mut tool_lines,
            "Whitelisted tool paths",
            &self.whitelisted_tool_paths,
            "进入白名单",
        );
        append_path_lines(
            &mut tool_lines,
            "Removed whitelist paths",
            &self.removed_whitelist_paths,
            "移出白名单",
        );
        append_path_lines(
            &mut tool_lines,
            "Added MCP servers",
            &self.added_mcp_servers,
            "已注入",
        );
        append_path_lines(
            &mut tool_lines,
            "Removed MCP servers",
            &self.removed_mcp_servers,
            "已移除",
        );
        append_path_lines(
            &mut tool_lines,
            "Changed MCP servers",
            &self.changed_mcp_servers,
            "已变更",
        );
        append_path_lines(
            &mut tool_lines,
            "Added VFS mounts",
            &self.vfs_mounts_added,
            "已挂载",
        );
        append_path_lines(
            &mut tool_lines,
            "Removed VFS mounts",
            &self.vfs_mounts_removed,
            "已移除",
        );
        if self.default_mount_before != self.default_mount_after {
            tool_lines.push(format!(
                "- Default VFS mount: `{}` -> `{}`",
                self.default_mount_before.as_deref().unwrap_or("none"),
                self.default_mount_after.as_deref().unwrap_or("none"),
            ));
        }
        if tool_lines.len() > 1 {
            sections.push(tool_lines.join("\n"));
        }

        if self.has_delta() {
            sections.push(
                "> 工具状态已按上述 capability 与 tool path 更新；历史对话未被改写。".to_string(),
            );
        } else {
            sections
                .push("> 本次没有 capability key 或工具级状态变化；历史对话未被改写。".to_string());
        }
        sections.join("\n\n")
    }

    fn has_delta(&self) -> bool {
        !self.added_capabilities.is_empty()
            || !self.removed_capabilities.is_empty()
            || !self.blocked_tool_paths.is_empty()
            || !self.unblocked_tool_paths.is_empty()
            || !self.whitelisted_tool_paths.is_empty()
            || !self.removed_whitelist_paths.is_empty()
            || !self.added_mcp_servers.is_empty()
            || !self.removed_mcp_servers.is_empty()
            || !self.changed_mcp_servers.is_empty()
            || !self.vfs_mounts_added.is_empty()
            || !self.vfs_mounts_removed.is_empty()
            || self.default_mount_before != self.default_mount_after
    }
}

#[derive(Debug, Clone)]
struct WorkflowContextMetadata {
    title: String,
    summary: String,
    injections: Vec<RuntimeHookInjectionEntry>,
}

impl WorkflowContextMetadata {
    fn from_hook_injections(injections: &[HookInjection]) -> Option<Self> {
        if injections.is_empty() {
            return None;
        }
        Some(Self {
            title: "Workflow Context Update".to_string(),
            summary: "Workflow 运行上下文已更新。以下内容已在同一运行时边界生效：".to_string(),
            injections: injections
                .iter()
                .map(|injection| RuntimeHookInjectionEntry {
                    slot: injection.slot.clone(),
                    source: injection.source.clone(),
                    content: injection.content.clone(),
                })
                .collect(),
        })
    }

    fn section(&self) -> ContextFrameSection {
        ContextFrameSection::WorkflowContext {
            title: self.title.clone(),
            summary: self.summary.clone(),
            injections: self.injections.clone(),
        }
    }

    fn render_text(&self) -> String {
        let mut lines = vec![format!("[{}]", self.title), self.summary.clone()];
        lines.push(format_injection_items(&self.injections));
        lines.join("\n\n")
    }
}

fn append_capability_lines(lines: &mut Vec<String>, values: &[String], removed: bool) {
    for key in values {
        let desc = capability_description(key);
        if desc.is_empty() {
            lines.push(format!("- **{key}**"));
        } else if removed {
            lines.push(format!("- **{key}**: {desc}（不再可用）"));
        } else {
            lines.push(format!("- **{key}**: {desc}"));
        }
    }
}

fn append_path_lines(lines: &mut Vec<String>, title: &str, values: &[String], suffix: &str) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("- {title}:"));
    for value in values {
        lines.push(format!("  - `{value}` — {suffix}"));
    }
}

fn format_injection_items(injections: &[RuntimeHookInjectionEntry]) -> String {
    injections
        .iter()
        .map(|injection| {
            let source = if injection.source.trim().is_empty() {
                "unknown"
            } else {
                injection.source.trim()
            };
            let content = injection.content.trim();
            if content.is_empty() {
                format!("- [{}] {source}", injection.slot)
            } else {
                format!("- [{}] {source}: {content}", injection.slot)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
            key_delta: CapabilityDelta::default(),
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

        let notice = build_live_context_frame(
            &input,
            &CapabilityDelta::default(),
            &state_delta,
            &tools,
            &[],
        );

        assert_eq!(notice.phase_node.as_deref(), Some("apply"));
        assert_eq!(notice.apply_mode.as_deref(), Some("live"));
        assert!(
            notice
                .sections
                .iter()
                .any(|section| matches!(section, ContextFrameSection::ToolSchemaDelta { .. }))
        );
        assert!(
            notice
                .rendered_text
                .contains("## Tool Schema Delta — Step Transition: apply")
        );
        assert!(notice.rendered_text.contains("Restored tool paths"));
        assert!(
            notice
                .rendered_text
                .contains("mcp_agentdash_workflow_tools_upsert_workflow_tool")
        );
        assert!(notice.rendered_text.contains("创建或更新 Workflow 定义"));
        assert!(notice.rendered_text.contains("\"required\": ["));
        assert!(
            notice
                .rendered_text
                .contains("\"description\": \"Workflow key\"")
        );
    }
}
