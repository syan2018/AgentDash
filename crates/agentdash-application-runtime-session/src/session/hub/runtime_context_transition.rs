//! Workflow runtime context transition 的统一应用入口。
//!
//! 这里刻意放在 Hub 层：transition 应用需要同时触碰 live connector、SessionRuntime、
//! persistence event、Hook runtime 与 Bundle sink。调用方只描述“目标上下文是什么”，
//! 不再各自手写事件 JSON 或 hook 触发顺序。

use std::collections::BTreeSet;

use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookInjection, RuntimeEventSource, RuntimeToolSchemaEntry,
    SetDelta, SharedHookRuntime,
};
use agentdash_spi::platform::tool_capability::CAP_COLLABORATION;

use super::super::assignment_context_frame::build_runtime_assignment_context_frame;
use super::super::context_frame::{self, ContextFramePayload};
use super::super::dimension::{self, DimensionDelta};
use super::SessionRuntimeInner;
use crate::hooks::hook_injection_to_fragment;
use crate::session::runtime_capability::apply_runtime_capability_transition;
use crate::session::{CapabilityState, PendingCapabilityStateTransition};
use agentdash_spi::{CapabilityStateDelta, compute_capability_state_delta};

#[derive(Debug, Clone)]
pub(crate) struct LiveRuntimeContextTransitionInput {
    pub delivery_runtime_session_id: String,
    pub turn_id: Option<String>,
    pub phase_node: String,
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

#[derive(Debug, Clone, Default)]
pub(crate) struct PendingRuntimeContextApplication {
    pub context_frames: Vec<ContextFrame>,
}

pub(crate) fn build_initial_capability_state_frame(
    capability_state: &CapabilityState,
    capability_keys: &BTreeSet<String>,
    tool_schemas: &[RuntimeToolSchemaEntry],
) -> ContextFrame {
    let initial_delta = SetDelta {
        added: capability_keys.iter().cloned().collect(),
        removed: Vec::new(),
    };
    let state_delta = compute_capability_state_delta(None, capability_state, capability_keys);
    build_context_frame(RuntimeContextUpdateFrameInput {
        phase_node: "bootstrap",
        apply_mode: Some("initial"),
        delivery_status: "queued_for_transform_context",
        capability_delta: &initial_delta,
        effective_capabilities: capability_keys,
        state_delta: Some(&state_delta),
        tool_schemas,
        skill_entries: &capability_state.skill.skills,
        companion_agents: &capability_state.companion.agents,
    })
}

impl SessionRuntimeInner {
    pub(crate) async fn emit_adopted_runtime_context_transition(
        &self,
        hook_runtime: &SharedHookRuntime,
        input: LiveRuntimeContextTransitionInput,
        tool_schemas: &[RuntimeToolSchemaEntry],
    ) -> Result<RuntimeContextTransitionOutcome, String> {
        let state_changed = input.before_state.as_ref() != Some(&input.after_state);
        if input.key_delta.is_empty() && !state_changed {
            self.emit_runtime_context_changed_notice(hook_runtime, &input)
                .await;
            return Ok(RuntimeContextTransitionOutcome {
                capability_delta: None,
                emitted_capability_change: false,
            });
        }

        self.emit_runtime_context_transition_notifications(hook_runtime, &input, tool_schemas)
            .await
    }

    async fn emit_runtime_context_transition_notifications(
        &self,
        effective_hook_runtime: &SharedHookRuntime,
        input: &LiveRuntimeContextTransitionInput,
        tool_schemas: &[RuntimeToolSchemaEntry],
    ) -> Result<RuntimeContextTransitionOutcome, String> {
        let injections = self
            .collect_runtime_context_update_injections(
                &input.delivery_runtime_session_id,
                effective_hook_runtime,
            )
            .await;
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );
        let notification_delta = SetDelta {
            added: state_delta.tool_capabilities.added.clone(),
            removed: state_delta.tool_capabilities.removed.clone(),
        };
        let _cache_delta =
            effective_hook_runtime.update_capabilities(input.capability_keys.clone());
        let notice =
            build_live_context_frame(input, &notification_delta, &state_delta, tool_schemas);
        self.emit_context_frame(
            &input.delivery_runtime_session_id,
            input.turn_id.as_deref(),
            &notice,
        )
        .await
        .map_err(|error| format!("Phase node runtime context notice 持久化失败: {error}"))?;
        let _ = context_frame::enqueue_context_frame(effective_hook_runtime, &notice);

        // assignment_context 作为独立 frame 一职一责地发出，不再和能力/工具 delta 混装。
        if let Some(workflow_frame) = build_workflow_assignment_context_frame(
            &input.phase_node,
            input.apply_mode,
            &injections,
        ) {
            self.emit_context_frame(
                &input.delivery_runtime_session_id,
                input.turn_id.as_deref(),
                &workflow_frame,
            )
            .await
            .map_err(|error| format!("Phase node mission context frame 持久化失败: {error}"))?;
            let _ = context_frame::enqueue_context_frame(effective_hook_runtime, &workflow_frame);
        }

        Ok(RuntimeContextTransitionOutcome {
            capability_delta: if notification_delta.is_empty() {
                None
            } else {
                Some(notification_delta)
            },
            emitted_capability_change: true,
        })
    }

    pub(crate) async fn apply_pending_runtime_context_transitions_on_turn(
        &self,
        input: ApplyPendingRuntimeContextTransitionInput<'_>,
    ) -> PendingRuntimeContextApplication {
        let mut application = PendingRuntimeContextApplication::default();
        if input.transitions.is_empty() {
            return application;
        }

        if let Some(hook_runtime) = input.hook_runtime
            && let Some(last_transition) = input.transitions.last()
        {
            let _ = hook_runtime.update_capabilities(last_transition.capability_keys.clone());
        }

        let mut pending_event_before_state = input.before_state;
        for (index, pending) in input.transitions.iter().enumerate() {
            let pending_after_state = if index + 1 == input.transitions.len() {
                input.final_capability_state.clone()
            } else {
                match apply_runtime_capability_transition(
                    &pending_event_before_state,
                    &pending.transition,
                ) {
                    Ok(state) => state,
                    Err(error) => {
                        tracing::warn!(
                            input.session_id,
                            frame_transition_id = %pending.id,
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
            let notice = build_context_frame(RuntimeContextUpdateFrameInput {
                phase_node: &pending.phase_node,
                apply_mode: Some("applied_on_next_turn"),
                delivery_status: "applied_before_prompt",
                capability_delta: &capability_delta,
                effective_capabilities: &pending.capability_keys,
                state_delta: Some(&state_delta),
                tool_schemas: input.tool_schemas,
                skill_entries: &pending_after_state.skill.skills,
                companion_agents: &pending_after_state.companion.agents,
            });
            application.context_frames.push(notice);

            let injections = if let Some(hook_runtime) = input.hook_runtime {
                self.collect_runtime_context_update_injections(input.session_id, hook_runtime)
                    .await
            } else {
                Vec::new()
            };
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

    async fn emit_runtime_context_changed_notice(
        &self,
        hook_runtime: &SharedHookRuntime,
        input: &LiveRuntimeContextTransitionInput,
    ) {
        let injections = self
            .collect_runtime_context_update_injections(
                &input.delivery_runtime_session_id,
                hook_runtime,
            )
            .await;
        if let Some(notice) = build_workflow_assignment_context_frame(
            &input.phase_node,
            input.apply_mode,
            &injections,
        ) {
            let _ = self
                .emit_context_frame(
                    &input.delivery_runtime_session_id,
                    input.turn_id.as_deref(),
                    &notice,
                )
                .await;
            let _ = context_frame::enqueue_context_frame(hook_runtime, &notice);
        }
    }
}

pub(crate) struct ApplyPendingRuntimeContextTransitionInput<'a> {
    pub session_id: &'a str,
    pub hook_runtime: Option<&'a SharedHookRuntime>,
    pub before_state: CapabilityState,
    pub final_capability_state: &'a CapabilityState,
    pub transitions: &'a [PendingCapabilityStateTransition],
    pub tool_schemas: &'a [RuntimeToolSchemaEntry],
}

fn build_live_context_frame(
    input: &LiveRuntimeContextTransitionInput,
    notification_delta: &SetDelta,
    state_delta: &CapabilityStateDelta,
    tool_schemas: &[RuntimeToolSchemaEntry],
) -> ContextFrame {
    let metadata = RuntimeContextUpdateFrame::new(RuntimeContextUpdateFrameInput {
        phase_node: &input.phase_node,
        apply_mode: Some(input.apply_mode),
        delivery_status: "queued_for_transform_context",
        capability_delta: notification_delta,
        effective_capabilities: &input.capability_keys,
        state_delta: Some(state_delta),
        tool_schemas,
        skill_entries: &input.after_state.skill.skills,
        companion_agents: &input.after_state.companion.agents,
    });
    context_frame::build_context_frame(&metadata)
}

fn build_context_frame(input: RuntimeContextUpdateFrameInput<'_>) -> ContextFrame {
    let metadata = RuntimeContextUpdateFrame::new(input);
    context_frame::build_context_frame(&metadata)
}

struct RuntimeContextUpdateFrameInput<'a> {
    phase_node: &'a str,
    apply_mode: Option<&'a str>,
    delivery_status: &'a str,
    capability_delta: &'a SetDelta,
    effective_capabilities: &'a BTreeSet<String>,
    state_delta: Option<&'a CapabilityStateDelta>,
    tool_schemas: &'a [RuntimeToolSchemaEntry],
    skill_entries: &'a [agentdash_spi::context::capability::SkillEntry],
    companion_agents: &'a [agentdash_spi::context::capability::CompanionAgentEntry],
}

impl RuntimeContextUpdateFrame {
    fn new(input: RuntimeContextUpdateFrameInput<'_>) -> Self {
        let mut dimensions: Vec<Box<dyn DimensionDelta>> = Vec::new();

        if let Some(d) = dimension::capability_key::CapabilityKeyDimensionDelta::from_delta(
            input.capability_delta,
            input.effective_capabilities,
            input.state_delta,
        ) {
            dimensions.push(d);
        }
        if let Some(d) =
            dimension::tool_path::ToolPathDimensionDelta::from_state_delta(input.state_delta)
        {
            dimensions.push(d);
        }
        if let Some(d) =
            dimension::mcp_server::McpServerDimensionDelta::from_state_delta(input.state_delta)
        {
            dimensions.push(d);
        }
        if let Some(d) = dimension::companion_agent::CompanionAgentDimensionDelta::from_state_delta(
            input.state_delta,
            input.companion_agents,
            should_include_companion_state_section(
                input.apply_mode,
                input.effective_capabilities,
                input.state_delta,
                input.companion_agents,
            ),
        ) {
            dimensions.push(d);
        }
        if let Some(d) = dimension::vfs::VfsDimensionDelta::from_state_delta(input.state_delta) {
            dimensions.push(d);
        }
        if let Some(d) = dimension::skill::SkillDimensionDelta::from_state_delta(
            input.state_delta,
            input.skill_entries,
        ) {
            dimensions.push(d);
        }
        if let Some(d) =
            dimension::tool_schema::ToolSchemaDimensionDelta::from_schema_entries_and_state_delta(
                input.tool_schemas,
                input.state_delta,
            )
        {
            dimensions.push(d);
        }

        Self {
            phase_node: input.phase_node.to_string(),
            apply_mode: input.apply_mode.map(str::to_string),
            delivery_status: input.delivery_status.to_string(),
            dimensions,
        }
    }
}

fn should_include_companion_state_section(
    apply_mode: Option<&str>,
    effective_capabilities: &BTreeSet<String>,
    state_delta: Option<&CapabilityStateDelta>,
    companion_agents: &[agentdash_spi::context::capability::CompanionAgentEntry],
) -> bool {
    let collaboration_enabled = effective_capabilities.contains(CAP_COLLABORATION);
    let collaboration_changed = state_delta.is_some_and(|delta| {
        delta
            .tool_capabilities
            .added
            .iter()
            .chain(delta.tool_capabilities.removed.iter())
            .any(|capability| capability == CAP_COLLABORATION)
    });
    let roster_changed = state_delta.is_some_and(|delta| !delta.companion_agents.is_empty());
    let initial_snapshot = matches!(apply_mode, Some("initial"));

    roster_changed
        || (collaboration_enabled && (initial_snapshot || collaboration_changed))
        || (!companion_agents.is_empty() && initial_snapshot)
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

impl ContextFramePayload for RuntimeContextUpdateFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("runtime-context-{}-{created_at_ms}", self.phase_node)
    }

    fn kind(&self) -> &'static str {
        "capability_state_delta"
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
        AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
    };
    use agentdash_spi::{
        SkillContextExposure, ToolCapability, context::capability::SkillEntry, context_usage_kind,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use tokio_util::sync::CancellationToken;

    use crate::session::dimension::tool_schema::runtime_tool_schema_entries_from_tools;

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

    fn external_skill_entry() -> SkillEntry {
        SkillEntry {
            name: "jira-issue-lookup".to_string(),
            capability_key: "external-integration/jira-issue-lookup".to_string(),
            provider_key: "external-integration".to_string(),
            local_name: "jira-issue-lookup".to_string(),
            display_name: Some("Jira Issue Lookup".to_string()),
            description: "Look up linked Jira issues.".to_string(),
            file_path: "external-integration://skills/jira-issue-lookup/SKILL.md".to_string(),
            base_dir: Some("external-integration://skills/jira-issue-lookup".to_string()),
            exposure: SkillContextExposure::DefaultExposed,
            disable_model_invocation: false,
        }
    }

    #[test]
    fn live_context_frame_includes_tool_schema_delta_only() {
        let input = LiveRuntimeContextTransitionInput {
            delivery_runtime_session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            phase_node: "apply".to_string(),
            before_state: None,
            after_state: CapabilityState::default(),
            capability_keys: BTreeSet::from(["workflow_management".to_string()]),
            key_delta: SetDelta::default(),
            apply_mode: "live",
        };
        let state_delta = CapabilityStateDelta {
            excluded_tool_paths: agentdash_spi::SetDelta {
                added: Vec::new(),
                removed: vec!["workflow_management::upsert_workflow_tool".to_string()],
            },
            ..Default::default()
        };
        let tools: Vec<DynAgentTool> = vec![Arc::new(StubTool)];
        let tool_schemas = runtime_tool_schema_entries_from_tools(&tools);

        let notice =
            build_live_context_frame(&input, &SetDelta::default(), &state_delta, &tool_schemas);

        assert_eq!(notice.kind, "capability_state_delta");
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
        // assignment_context section 不应混入 capability_state_delta frame。
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
    fn live_context_frame_omits_empty_capability_key_section_for_skill_only_delta() {
        let before_state = CapabilityState::default();
        let mut after_state = CapabilityState::default();
        after_state.skill.skills = vec![external_skill_entry()];
        let input = LiveRuntimeContextTransitionInput {
            delivery_runtime_session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            phase_node: "canvas-bind".to_string(),
            before_state: Some(before_state),
            after_state,
            capability_keys: BTreeSet::new(),
            key_delta: SetDelta::default(),
            apply_mode: "live",
        };
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );

        let notice = build_live_context_frame(&input, &SetDelta::default(), &state_delta, &[]);

        assert_eq!(notice.kind, "capability_state_delta");
        assert!(
            !notice
                .sections
                .iter()
                .any(|section| matches!(section, ContextFrameSection::CapabilityKeyDelta { .. }))
        );
        let added_skills = notice
            .sections
            .iter()
            .find_map(|section| match section {
                ContextFrameSection::SkillDelta { added_skills, .. } => Some(added_skills),
                _ => None,
            })
            .expect("skill-only semantic delta should still render a skill section");
        assert_eq!(added_skills.len(), 1);
        assert_eq!(
            added_skills[0].capability_key,
            "external-integration/jira-issue-lookup"
        );
        assert!(notice.rendered_text.contains("## Skill Delta"));
        assert!(!notice.rendered_text.contains("Capability State Update"));
    }

    #[test]
    fn live_context_frame_includes_companion_agent_roster_delta() {
        let companion_agent = agentdash_spi::context::capability::CompanionAgentEntry {
            name: "reviewer".to_string(),
            executor: "PI_AGENT".to_string(),
            display_name: "Review Agent".to_string(),
        };
        let mut after_state = CapabilityState::default();
        after_state.companion.agents = vec![companion_agent];
        let input = LiveRuntimeContextTransitionInput {
            delivery_runtime_session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            phase_node: "apply".to_string(),
            before_state: Some(CapabilityState::default()),
            after_state,
            capability_keys: BTreeSet::new(),
            key_delta: SetDelta::default(),
            apply_mode: "live",
        };
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );

        let notice = build_live_context_frame(&input, &SetDelta::default(), &state_delta, &[]);

        assert_eq!(notice.kind, "capability_state_delta");
        let (added_agents, effective_agents) = notice
            .sections
            .iter()
            .find_map(|section| match section {
                ContextFrameSection::CompanionAgentRosterDelta {
                    added_agents,
                    effective_agents,
                    ..
                } => Some((added_agents, effective_agents)),
                _ => None,
            })
            .expect("companion roster delta section should exist");
        assert_eq!(added_agents.len(), 1);
        assert_eq!(added_agents[0].agent_key, "reviewer");
        assert_eq!(effective_agents[0].display_name, "Review Agent");
        assert!(
            notice
                .rendered_text
                .contains("## Companion Agent Roster Delta — Step Transition: apply")
        );
        assert!(notice.rendered_text.contains("agent_key: `reviewer`"));
    }

    #[test]
    fn capability_frame_includes_empty_companion_roster_when_collaboration_enabled() {
        let input = LiveRuntimeContextTransitionInput {
            delivery_runtime_session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            phase_node: "bootstrap".to_string(),
            before_state: None,
            after_state: CapabilityState::default(),
            capability_keys: BTreeSet::from([CAP_COLLABORATION.to_string()]),
            key_delta: SetDelta::default(),
            apply_mode: "initial",
        };
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );

        let notice = build_live_context_frame(&input, &SetDelta::default(), &state_delta, &[]);

        assert_eq!(notice.kind, "capability_state_delta");
        let effective_agents = notice
            .sections
            .iter()
            .find_map(|section| match section {
                ContextFrameSection::CompanionAgentRosterDelta {
                    effective_agents, ..
                } => Some(effective_agents),
                _ => None,
            })
            .expect("collaboration capability should surface companion roster state");
        assert!(effective_agents.is_empty());
        assert!(
            notice
                .rendered_text
                .contains("## Companion Agent Roster Delta")
        );
        assert!(notice.rendered_text.contains("- （无）"));
    }

    #[test]
    fn initial_context_frame_includes_project_mcp_schema_as_regular_delta() {
        let mut capability_state = CapabilityState::default();
        capability_state
            .tool
            .capabilities
            .insert(ToolCapability::custom_mcp("code-analyzer"));
        capability_state.tool.mcp_servers = vec![agentdash_spi::RuntimeMcpServer {
            name: "code-analyzer".to_string(),
            transport: agentdash_spi::McpTransportConfig::Http {
                url: "http://127.0.0.1:9999/mcp".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
        }];
        let capability_keys = capability_state.capability_keys();
        let tool_schemas = vec![RuntimeToolSchemaEntry {
            name: "mcp_code_analyzer_scan_repo".to_string(),
            description: "扫描仓库结构".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "root": {
                        "type": "string",
                        "description": "扫描根目录"
                    }
                },
                "required": ["root"]
            }),
            capability_key: Some("mcp:code-analyzer".to_string()),
            source: Some("mcp:code-analyzer".to_string()),
            tool_path: Some("mcp:code-analyzer::scan_repo".to_string()),
            context_usage_kind: Some(context_usage_kind::MCP_TOOLS.to_string()),
        }];

        let notice = build_initial_capability_state_frame(
            &capability_state,
            &capability_keys,
            &tool_schemas,
        );

        assert_eq!(notice.kind, "capability_state_delta");
        assert_eq!(notice.apply_mode.as_deref(), Some("initial"));
        let tool_section = notice
            .sections
            .iter()
            .find_map(|section| match section {
                ContextFrameSection::ToolSchemaDelta { added_tools } => Some(added_tools),
                _ => None,
            })
            .expect("project MCP tool schema should be visible on initial delta");
        assert_eq!(tool_section.len(), 1);
        assert_eq!(
            tool_section[0].capability_key.as_deref(),
            Some("mcp:code-analyzer")
        );
        assert_eq!(
            tool_section[0].tool_path.as_deref(),
            Some("mcp:code-analyzer::scan_repo")
        );
        assert!(notice.rendered_text.contains("mcp_code_analyzer_scan_repo"));
        assert!(notice.rendered_text.contains("source: `mcp:code-analyzer`"));
        assert!(notice.rendered_text.contains("`root` (required, string)"));
    }

    #[test]
    fn live_transition_frame_includes_project_mcp_schema_from_application_surface() {
        let before_state = CapabilityState::default();
        let mut after_state = CapabilityState::default();
        after_state
            .tool
            .capabilities
            .insert(ToolCapability::custom_mcp("code-analyzer"));
        after_state.tool.mcp_servers = vec![agentdash_spi::RuntimeMcpServer {
            name: "code-analyzer".to_string(),
            transport: agentdash_spi::McpTransportConfig::Http {
                url: "http://127.0.0.1:9999/mcp".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
        }];
        let input = LiveRuntimeContextTransitionInput {
            delivery_runtime_session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            phase_node: "enable-mcp".to_string(),
            before_state: Some(before_state),
            after_state,
            capability_keys: BTreeSet::from(["mcp:code-analyzer".to_string()]),
            key_delta: SetDelta::default(),
            apply_mode: "live",
        };
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );
        let tool_schemas = vec![RuntimeToolSchemaEntry {
            name: "mcp_code_analyzer_scan_repo".to_string(),
            description: "扫描仓库结构".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "root": {
                        "type": "string",
                        "description": "扫描根目录"
                    }
                },
                "required": ["root"]
            }),
            capability_key: Some("mcp:code-analyzer".to_string()),
            source: Some("mcp:code-analyzer".to_string()),
            tool_path: Some("mcp:code-analyzer::scan_repo".to_string()),
            context_usage_kind: Some(context_usage_kind::MCP_TOOLS.to_string()),
        }];

        let notice =
            build_live_context_frame(&input, &SetDelta::default(), &state_delta, &tool_schemas);

        assert_eq!(notice.kind, "capability_state_delta");
        let tool_section = notice
            .sections
            .iter()
            .find_map(|section| match section {
                ContextFrameSection::ToolSchemaDelta { added_tools } => Some(added_tools),
                _ => None,
            })
            .expect("project MCP tool schema should be visible on live transition");
        assert_eq!(tool_section.len(), 1);
        assert_eq!(tool_section[0].source.as_deref(), Some("mcp:code-analyzer"));
        assert_eq!(
            tool_section[0].tool_path.as_deref(),
            Some("mcp:code-analyzer::scan_repo")
        );
        assert!(notice.rendered_text.contains("mcp_code_analyzer_scan_repo"));
        assert!(
            notice
                .rendered_text
                .contains("path: `mcp:code-analyzer::scan_repo`")
        );
        assert!(notice.rendered_text.contains("扫描根目录"));
    }

    #[test]
    fn live_capability_frame_does_not_repeat_unchanged_empty_companion_roster() {
        let mut before_state = CapabilityState::default();
        before_state
            .tool
            .capabilities
            .insert(agentdash_spi::ToolCapability::new(CAP_COLLABORATION));
        let after_state = before_state.clone();
        let input = LiveRuntimeContextTransitionInput {
            delivery_runtime_session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            phase_node: "apply".to_string(),
            before_state: Some(before_state),
            after_state,
            capability_keys: BTreeSet::from([CAP_COLLABORATION.to_string()]),
            key_delta: SetDelta::default(),
            apply_mode: "live",
        };
        let state_delta = compute_capability_state_delta(
            input.before_state.as_ref(),
            &input.after_state,
            &input.capability_keys,
        );

        let notice = build_live_context_frame(&input, &SetDelta::default(), &state_delta, &[]);

        assert!(!notice.sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::CompanionAgentRosterDelta { .. }
        )));
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
