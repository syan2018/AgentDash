use agentdash_agent_types::{DynAgentTool, ToolDefinition};
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTurnStartNotice, RuntimeEventSource,
    RuntimeToolSchemaEntry, SharedHookSessionRuntime,
};
use agentdash_spi::platform::tool_capability::{
    PlatformMcpScope, ToolDescriptor, ToolSource, platform_tool_descriptors,
};

use crate::session::context_frame::ContextFramePayload;
use crate::session::{CapabilityStateDelta, context_frame};

#[derive(Debug, Clone, Copy)]
pub(crate) enum ToolSchemaNoticeKind {
    Initial,
}

impl ToolSchemaNoticeKind {
    fn notice_id_prefix(&self) -> String {
        match self {
            Self::Initial => "runtime-tool-schema-initial".to_string(),
        }
    }

    fn phase_node(&self) -> Option<String> {
        match self {
            Self::Initial => None,
        }
    }
}

#[derive(Debug, Clone)]
struct ToolSurfaceContextFrame {
    kind: ToolSchemaNoticeKind,
    tools: Vec<RuntimeToolSchemaEntry>,
}

impl ToolSurfaceContextFrame {
    fn new(kind: ToolSchemaNoticeKind, tools: Vec<RuntimeToolSchemaEntry>) -> Self {
        Self { kind, tools }
    }
}

impl ContextFramePayload for ToolSurfaceContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("{}-{created_at_ms}", self.kind.notice_id_prefix())
    }

    fn kind(&self) -> &'static str {
        "tool_surface"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn phase_node(&self) -> Option<String> {
        self.kind.phase_node()
    }

    fn delivery_status(&self) -> String {
        "queued_for_transform_context".to_string()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::ToolSchema {
            tools: self.tools.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        render_tool_surface_text(self.kind, &self.tools)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ToolSchemaDeltaMetadata {
    added_tools: Vec<RuntimeToolSchemaEntry>,
    removed_tool_paths: Vec<String>,
    restored_tool_paths: Vec<String>,
    blocked_tool_paths: Vec<String>,
}

impl ToolSchemaDeltaMetadata {
    pub(crate) fn from_tools_and_state_delta(
        tools: &[DynAgentTool],
        state_delta: &CapabilityStateDelta,
    ) -> Option<Self> {
        let mut definitions = tools
            .iter()
            .map(|tool| ToolDefinition::from_tool(tool.as_ref()))
            .collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions.dedup_by(|left, right| left.name == right.name);

        let entries = runtime_tool_schema_entries(definitions);
        let restored_paths = state_delta
            .excluded_tool_paths
            .removed
            .iter()
            .chain(state_delta.included_tool_paths.added.iter())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let added_capabilities = state_delta
            .tool_capabilities
            .added
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();

        let added_tools = entries
            .into_iter()
            .filter(|entry| {
                entry
                    .capability_key
                    .as_ref()
                    .is_some_and(|capability| added_capabilities.contains(capability))
                    || entry
                        .tool_path
                        .as_ref()
                        .is_some_and(|path| restored_paths.contains(path))
            })
            .collect::<Vec<_>>();
        let removed_tool_paths = state_delta.included_tool_paths.removed.clone();
        let blocked_tool_paths = state_delta.excluded_tool_paths.added.clone();
        let restored_tool_paths = restored_paths.into_iter().collect::<Vec<_>>();

        let metadata = Self {
            added_tools,
            removed_tool_paths,
            restored_tool_paths,
            blocked_tool_paths,
        };
        (!metadata.is_empty()).then_some(metadata)
    }

    pub(crate) fn section(&self) -> ContextFrameSection {
        ContextFrameSection::ToolSchemaDelta {
            added_tools: self.added_tools.clone(),
            removed_tool_paths: self.removed_tool_paths.clone(),
            restored_tool_paths: self.restored_tool_paths.clone(),
            blocked_tool_paths: self.blocked_tool_paths.clone(),
        }
    }

    pub(crate) fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![
            tool_schema_delta_title(phase_node),
            "以下只列出本次 capability state delta 影响到的工具；provider 的完整工具集合以实际 tool list 为准。".to_string(),
        ];
        append_named_list(&mut lines, "Restored tool paths", &self.restored_tool_paths);
        append_named_list(&mut lines, "Blocked tool paths", &self.blocked_tool_paths);
        append_named_list(&mut lines, "Removed tool paths", &self.removed_tool_paths);
        if !self.added_tools.is_empty() {
            lines.push("### Added / Restored Tool Schemas".to_string());
            for tool in &self.added_tools {
                lines.push(format_tool_schema_entry(tool));
            }
        }
        lines.join("\n\n")
    }

    fn is_empty(&self) -> bool {
        self.added_tools.is_empty()
            && self.removed_tool_paths.is_empty()
            && self.restored_tool_paths.is_empty()
            && self.blocked_tool_paths.is_empty()
    }
}

pub(crate) fn enqueue_tool_schema_notice(
    hook_session: Option<&SharedHookSessionRuntime>,
    kind: ToolSchemaNoticeKind,
    tools: &[DynAgentTool],
) -> Option<ContextFrame> {
    let Some(hook_session) = hook_session else {
        return None;
    };
    let Some(notice) = build_tool_schema_notice(kind, tools) else {
        return None;
    };
    hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
        id: notice.id.clone(),
        created_at_ms: notice.created_at_ms,
        source: RuntimeEventSource::RuntimeContextUpdate,
        content: notice.rendered_text.clone(),
        context_frame: Some(notice.clone()),
    });
    Some(notice)
}

pub(crate) fn build_tool_schema_notice(
    kind: ToolSchemaNoticeKind,
    tools: &[DynAgentTool],
) -> Option<ContextFrame> {
    if tools.is_empty() {
        return None;
    }

    let mut definitions = tools
        .iter()
        .map(|tool| ToolDefinition::from_tool(tool.as_ref()))
        .collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.name.cmp(&right.name));
    definitions.dedup_by(|left, right| left.name == right.name);
    let entries = runtime_tool_schema_entries(definitions);

    let metadata = ToolSurfaceContextFrame::new(kind, entries);
    Some(context_frame::build_context_frame(&metadata))
}

fn render_tool_surface_text(
    kind: ToolSchemaNoticeKind,
    tools: &[RuntimeToolSchemaEntry],
) -> String {
    if tools.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        tool_schema_title(kind.phase_node().as_deref()),
        "以下是当前 provider request 生效的完整工具 schema。只有这里列出的工具可被本轮模型调用："
            .to_string(),
    ];
    for tool in tools {
        lines.push(format_tool_schema_entry(tool));
    }
    lines.join("\n\n")
}

fn tool_schema_title(phase_node: Option<&str>) -> String {
    match phase_node {
        Some(phase_node) => format!("## Runtime Tool Schema — Step Transition: {phase_node}"),
        None => "## Runtime Tool Schema — Initial".to_string(),
    }
}

fn tool_schema_delta_title(phase_node: Option<&str>) -> String {
    match phase_node {
        Some(phase_node) => format!("## Tool Schema Delta — Step Transition: {phase_node}"),
        None => "## Tool Schema Delta".to_string(),
    }
}

fn format_tool_schema_entry(entry: &RuntimeToolSchemaEntry) -> String {
    let description = entry.description.trim();
    let parameters = serde_json::to_string_pretty(&entry.parameters_schema)
        .unwrap_or_else(|_| entry.parameters_schema.to_string());

    let mut lines = vec![format!("### `{}`", entry.name)];
    let mut meta = Vec::new();
    if let Some(capability_key) = entry.capability_key.as_deref() {
        meta.push(format!("capability: `{capability_key}`"));
    }
    if let Some(source) = entry.source.as_deref() {
        meta.push(format!("source: `{source}`"));
    }
    if let Some(tool_path) = entry.tool_path.as_deref() {
        meta.push(format!("path: `{tool_path}`"));
    }
    if !meta.is_empty() {
        lines.push(meta.join("；"));
    }
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    lines.push("参数 schema：".to_string());
    lines.push(format!("```json\n{parameters}\n```"));
    lines.join("\n\n")
}

#[derive(Debug, Clone)]
struct ToolRuntimeMetadata {
    runtime_name: String,
    capability_key: String,
    source: String,
    tool_path: String,
}

fn runtime_tool_schema_entries(definitions: Vec<ToolDefinition>) -> Vec<RuntimeToolSchemaEntry> {
    let metadata = tool_runtime_metadata(&definitions);
    definitions
        .into_iter()
        .map(|definition| {
            let metadata = metadata_for_tool(&definition.name, &metadata);
            RuntimeToolSchemaEntry {
                name: definition.name,
                description: definition.description,
                parameters_schema: definition.parameters,
                capability_key: metadata
                    .as_ref()
                    .map(|metadata| metadata.capability_key.clone()),
                source: metadata.as_ref().map(|metadata| metadata.source.clone()),
                tool_path: metadata.as_ref().map(|metadata| metadata.tool_path.clone()),
            }
        })
        .collect()
}

fn tool_runtime_metadata(definitions: &[ToolDefinition]) -> Vec<ToolRuntimeMetadata> {
    let runtime_names = definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    platform_tool_descriptors()
        .into_iter()
        .flat_map(|descriptor| {
            runtime_names_for_descriptor(&descriptor).into_iter().map(
                move |(runtime_name, tool_path)| ToolRuntimeMetadata {
                    runtime_name,
                    capability_key: descriptor.capability_key.clone(),
                    source: format_tool_source(&descriptor.source),
                    tool_path,
                },
            )
        })
        .filter(|metadata| runtime_names.contains(metadata.runtime_name.as_str()))
        .collect()
}

fn metadata_for_tool(
    tool_name: &str,
    metadata: &[ToolRuntimeMetadata],
) -> Option<ToolRuntimeMetadata> {
    metadata
        .iter()
        .find(|candidate| candidate.runtime_name == tool_name)
        .cloned()
}

fn runtime_names_for_descriptor(descriptor: &ToolDescriptor) -> Vec<(String, String)> {
    let tool_path = format!("{}::{}", descriptor.capability_key, descriptor.name);
    match descriptor.source {
        ToolSource::Platform { .. } => vec![(descriptor.name.clone(), tool_path)],
        ToolSource::PlatformMcp { scope } => {
            vec![(
                platform_mcp_runtime_name(scope, &descriptor.name),
                tool_path,
            )]
        }
        ToolSource::Mcp { .. } => vec![(descriptor.name.clone(), tool_path)],
    }
}

fn platform_mcp_runtime_name(scope: PlatformMcpScope, tool_name: &str) -> String {
    let server_name = match scope {
        PlatformMcpScope::Relay => "agentdash-relay-tools",
        PlatformMcpScope::Story => "agentdash-story-tools",
        PlatformMcpScope::Task => "agentdash-task-tools",
        PlatformMcpScope::Workflow => "agentdash-workflow-tools",
    };
    format!(
        "mcp_{}_{}",
        sanitize_identifier(server_name),
        sanitize_identifier(tool_name)
    )
}

fn sanitize_identifier(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('_').to_string()
}

fn format_tool_source(source: &ToolSource) -> String {
    match source {
        ToolSource::Platform { cluster } => format!("platform:{}", tool_cluster_key(*cluster)),
        ToolSource::PlatformMcp { scope } => {
            format!("platform_mcp:{}", platform_mcp_scope_key(*scope))
        }
        ToolSource::Mcp { server_name } => format!("mcp:{server_name}"),
    }
}

fn tool_cluster_key(cluster: agentdash_spi::ToolCluster) -> &'static str {
    match cluster {
        agentdash_spi::ToolCluster::Read => "read",
        agentdash_spi::ToolCluster::Write => "write",
        agentdash_spi::ToolCluster::Execute => "execute",
        agentdash_spi::ToolCluster::Workflow => "workflow",
        agentdash_spi::ToolCluster::Collaboration => "collaboration",
        agentdash_spi::ToolCluster::Canvas => "canvas",
    }
}

fn platform_mcp_scope_key(scope: PlatformMcpScope) -> &'static str {
    match scope {
        PlatformMcpScope::Relay => "relay",
        PlatformMcpScope::Story => "story",
        PlatformMcpScope::Task => "task",
        PlatformMcpScope::Workflow => "workflow",
    }
}

fn append_named_list(lines: &mut Vec<String>, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    for value in values {
        lines.push(format!("- `{value}`"));
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
    fn tool_schema_notice_includes_full_parameter_schema() {
        let tools: Vec<DynAgentTool> = vec![Arc::new(StubTool)];

        let notice = build_tool_schema_notice(ToolSchemaNoticeKind::Initial, &tools)
            .expect("notice should be built");

        assert_eq!(notice.sections.len(), 1);
        assert!(
            notice
                .rendered_text
                .contains("## Runtime Tool Schema — Initial")
        );
        assert!(
            notice
                .rendered_text
                .contains("mcp_agentdash_workflow_tools_upsert_workflow_tool")
        );
        assert!(
            notice
                .rendered_text
                .contains("capability: `workflow_management`")
        );
        assert!(
            notice
                .rendered_text
                .contains("source: `platform_mcp:workflow`")
        );
        assert!(
            notice
                .rendered_text
                .contains("path: `workflow_management::upsert_workflow_tool`")
        );
        assert!(notice.rendered_text.contains("创建或更新 Workflow 定义"));
        assert!(notice.rendered_text.contains("\"required\": ["));
        assert!(notice.rendered_text.contains("\"key\""));
        assert!(
            notice
                .rendered_text
                .contains("\"description\": \"Workflow key\"")
        );
    }
}
