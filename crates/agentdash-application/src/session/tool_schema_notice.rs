use agentdash_agent_types::{DynAgentTool, ToolDefinition};
use agentdash_spi::hooks::{
    HookTurnStartNotice, RuntimeContextNotice, RuntimeContextNoticeSection, RuntimeEventSource,
    RuntimeHookInjectionEntry, RuntimeToolSchemaEntry, SharedHookSessionRuntime,
};
use agentdash_spi::platform::tool_capability::{
    PlatformMcpScope, ToolDescriptor, ToolSource, platform_tool_descriptors,
};

use crate::capability::capability_description;
use crate::session::CapabilityStateDelta;

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

pub(crate) fn enqueue_tool_schema_notice(
    hook_session: Option<&SharedHookSessionRuntime>,
    kind: ToolSchemaNoticeKind,
    tools: &[DynAgentTool],
) -> Option<RuntimeContextNotice> {
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
        content: notice.agent_visible_text.clone(),
        runtime_context_notice: Some(notice.clone()),
    });
    Some(notice)
}

pub(crate) fn build_tool_schema_notice(
    kind: ToolSchemaNoticeKind,
    tools: &[DynAgentTool],
) -> Option<RuntimeContextNotice> {
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

    let now = chrono::Utc::now().timestamp_millis();
    let sections = vec![RuntimeContextNoticeSection::ToolSchema { tools: entries }];
    Some(finalize_runtime_context_notice(RuntimeContextNotice {
        id: format!("{}-{now}", kind.notice_id_prefix()),
        source: RuntimeEventSource::RuntimeContextUpdate,
        phase_node: kind.phase_node(),
        apply_mode: None,
        delivery_status: "queued_for_transform_context".to_string(),
        agent_visible_text: String::new(),
        sections,
        created_at_ms: now,
    }))
}

pub(crate) fn build_tool_schema_delta_section(
    tools: &[DynAgentTool],
    state_delta: &CapabilityStateDelta,
) -> Option<RuntimeContextNoticeSection> {
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

    if added_tools.is_empty()
        && removed_tool_paths.is_empty()
        && restored_tool_paths.is_empty()
        && blocked_tool_paths.is_empty()
    {
        return None;
    }

    Some(RuntimeContextNoticeSection::ToolSchemaDelta {
        added_tools,
        removed_tool_paths,
        restored_tool_paths,
        blocked_tool_paths,
    })
}

pub(crate) fn finalize_runtime_context_notice(
    mut notice: RuntimeContextNotice,
) -> RuntimeContextNotice {
    notice.agent_visible_text = render_runtime_context_notice(&notice);
    notice
}

pub(crate) fn render_runtime_context_notice(notice: &RuntimeContextNotice) -> String {
    let mut blocks = Vec::new();
    for section in &notice.sections {
        if let Some(block) = render_notice_section(notice, section) {
            blocks.push(block);
        }
    }
    blocks.join("\n\n")
}

fn render_notice_section(
    notice: &RuntimeContextNotice,
    section: &RuntimeContextNoticeSection,
) -> Option<String> {
    match section {
        RuntimeContextNoticeSection::CapabilityDelta {
            added_capabilities,
            removed_capabilities,
            effective_capabilities,
            blocked_tool_paths,
            unblocked_tool_paths,
            whitelisted_tool_paths,
            removed_whitelist_paths,
            added_mcp_servers,
            removed_mcp_servers,
            changed_mcp_servers,
            vfs_mounts_added,
            vfs_mounts_removed,
            default_mount_before,
            default_mount_after,
        } => Some(render_capability_delta_section(
            notice,
            added_capabilities,
            removed_capabilities,
            effective_capabilities,
            blocked_tool_paths,
            unblocked_tool_paths,
            whitelisted_tool_paths,
            removed_whitelist_paths,
            added_mcp_servers,
            removed_mcp_servers,
            changed_mcp_servers,
            vfs_mounts_added,
            vfs_mounts_removed,
            default_mount_before,
            default_mount_after,
        )),
        RuntimeContextNoticeSection::ToolSchema { tools } => {
            if tools.is_empty() {
                return None;
            }
            let mut lines = vec![
                tool_schema_title(notice),
                "以下是当前 provider request 生效的完整工具 schema。只有这里列出的工具可被本轮模型调用："
                    .to_string(),
            ];
            for tool in tools {
                lines.push(format_tool_schema_entry(tool));
            }
            Some(lines.join("\n\n"))
        }
        RuntimeContextNoticeSection::ToolSchemaDelta {
            added_tools,
            removed_tool_paths,
            restored_tool_paths,
            blocked_tool_paths,
        } => {
            if added_tools.is_empty()
                && removed_tool_paths.is_empty()
                && restored_tool_paths.is_empty()
                && blocked_tool_paths.is_empty()
            {
                return None;
            }
            let mut lines = vec![
                tool_schema_delta_title(notice),
                "以下只列出本次 capability state delta 影响到的工具；provider 的完整工具集合以实际 tool list 为准。".to_string(),
            ];
            append_named_list(&mut lines, "Restored tool paths", restored_tool_paths);
            append_named_list(&mut lines, "Blocked tool paths", blocked_tool_paths);
            append_named_list(&mut lines, "Removed tool paths", removed_tool_paths);
            if !added_tools.is_empty() {
                lines.push("### Added / Restored Tool Schemas".to_string());
                for tool in added_tools {
                    lines.push(format_tool_schema_entry(tool));
                }
            }
            Some(lines.join("\n\n"))
        }
        RuntimeContextNoticeSection::WorkflowContext {
            title,
            summary,
            injections,
        }
        | RuntimeContextNoticeSection::HookInjection {
            title,
            summary,
            injections,
        } => {
            if injections.is_empty() {
                return None;
            }
            let mut lines = vec![format!("[{title}]"), summary.clone()];
            lines.push(format_injection_items(injections));
            Some(lines.join("\n\n"))
        }
        RuntimeContextNoticeSection::SystemNotice {
            title,
            summary,
            body,
        } => {
            let mut lines = vec![format!("[{title}]"), summary.clone()];
            if let Some(body) = body.as_deref()
                && !body.trim().is_empty()
            {
                lines.push(body.trim().to_string());
            }
            Some(lines.join("\n\n"))
        }
    }
}

fn tool_schema_title(notice: &RuntimeContextNotice) -> String {
    match notice.phase_node.as_deref() {
        Some(phase_node) => format!("## Runtime Tool Schema — Step Transition: {phase_node}"),
        None => "## Runtime Tool Schema — Initial".to_string(),
    }
}

fn tool_schema_delta_title(notice: &RuntimeContextNotice) -> String {
    match notice.phase_node.as_deref() {
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

fn render_capability_delta_section(
    notice: &RuntimeContextNotice,
    added_capabilities: &[String],
    removed_capabilities: &[String],
    effective_capabilities: &[String],
    blocked_tool_paths: &[String],
    unblocked_tool_paths: &[String],
    whitelisted_tool_paths: &[String],
    removed_whitelist_paths: &[String],
    added_mcp_servers: &[String],
    removed_mcp_servers: &[String],
    changed_mcp_servers: &[String],
    vfs_mounts_added: &[String],
    vfs_mounts_removed: &[String],
    default_mount_before: &Option<String>,
    default_mount_after: &Option<String>,
) -> String {
    let phase = notice.phase_node.as_deref().unwrap_or("unknown");
    let mut sections = vec![format!("## Capability Update — Step Transition: {phase}")];

    if !added_capabilities.is_empty() {
        let mut block = vec!["### Added Capabilities".to_string()];
        append_capability_lines(&mut block, added_capabilities, false);
        sections.push(block.join("\n"));
    }
    if !removed_capabilities.is_empty() {
        let mut block = vec!["### Removed Capabilities".to_string()];
        append_capability_lines(&mut block, removed_capabilities, true);
        sections.push(block.join("\n"));
    }

    let caps_block = if effective_capabilities.is_empty() {
        "- （无）".to_string()
    } else {
        effective_capabilities
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
        blocked_tool_paths,
        "不再暴露",
    );
    append_path_lines(
        &mut tool_lines,
        "Unblocked tool paths",
        unblocked_tool_paths,
        "重新暴露",
    );
    append_path_lines(
        &mut tool_lines,
        "Whitelisted tool paths",
        whitelisted_tool_paths,
        "进入白名单",
    );
    append_path_lines(
        &mut tool_lines,
        "Removed whitelist paths",
        removed_whitelist_paths,
        "移出白名单",
    );
    append_path_lines(
        &mut tool_lines,
        "Added MCP servers",
        added_mcp_servers,
        "已注入",
    );
    append_path_lines(
        &mut tool_lines,
        "Removed MCP servers",
        removed_mcp_servers,
        "已移除",
    );
    append_path_lines(
        &mut tool_lines,
        "Changed MCP servers",
        changed_mcp_servers,
        "已变更",
    );
    append_path_lines(
        &mut tool_lines,
        "Added VFS mounts",
        vfs_mounts_added,
        "已挂载",
    );
    append_path_lines(
        &mut tool_lines,
        "Removed VFS mounts",
        vfs_mounts_removed,
        "已移除",
    );
    if default_mount_before != default_mount_after {
        tool_lines.push(format!(
            "- Default VFS mount: `{}` → `{}`",
            default_mount_before.as_deref().unwrap_or("none"),
            default_mount_after.as_deref().unwrap_or("none"),
        ));
    }
    if tool_lines.len() > 1 {
        sections.push(tool_lines.join("\n"));
    }

    let has_delta = !added_capabilities.is_empty()
        || !removed_capabilities.is_empty()
        || !blocked_tool_paths.is_empty()
        || !unblocked_tool_paths.is_empty()
        || !whitelisted_tool_paths.is_empty()
        || !removed_whitelist_paths.is_empty()
        || !added_mcp_servers.is_empty()
        || !removed_mcp_servers.is_empty()
        || !changed_mcp_servers.is_empty()
        || !vfs_mounts_added.is_empty()
        || !vfs_mounts_removed.is_empty()
        || default_mount_before != default_mount_after;
    if has_delta {
        sections.push(
            "> 工具状态已按上述 capability 与 tool path 更新；历史对话未被改写。".to_string(),
        );
    } else {
        sections.push("> 本次没有 capability key 或工具级状态变化；历史对话未被改写。".to_string());
    }
    sections.join("\n\n")
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

fn append_named_list(lines: &mut Vec<String>, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    for value in values {
        lines.push(format!("- `{value}`"));
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
    fn tool_schema_notice_includes_full_parameter_schema() {
        let tools: Vec<DynAgentTool> = vec![Arc::new(StubTool)];

        let notice = build_tool_schema_notice(ToolSchemaNoticeKind::Initial, &tools)
            .expect("notice should be built");

        assert_eq!(notice.sections.len(), 1);
        assert!(
            notice
                .agent_visible_text
                .contains("## Runtime Tool Schema — Initial")
        );
        assert!(
            notice
                .agent_visible_text
                .contains("mcp_agentdash_workflow_tools_upsert_workflow_tool")
        );
        assert!(
            notice
                .agent_visible_text
                .contains("capability: `workflow_management`")
        );
        assert!(
            notice
                .agent_visible_text
                .contains("source: `platform_mcp:workflow`")
        );
        assert!(
            notice
                .agent_visible_text
                .contains("path: `workflow_management::upsert_workflow_tool`")
        );
        assert!(
            notice
                .agent_visible_text
                .contains("创建或更新 Workflow 定义")
        );
        assert!(notice.agent_visible_text.contains("\"required\": ["));
        assert!(notice.agent_visible_text.contains("\"key\""));
        assert!(
            notice
                .agent_visible_text
                .contains("\"description\": \"Workflow key\"")
        );
    }
}
