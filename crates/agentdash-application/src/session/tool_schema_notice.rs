use agentdash_agent_types::{DynAgentTool, ToolDefinition};
use agentdash_spi::hooks::{HookTurnStartNotice, RuntimeEventSource, SharedHookSessionRuntime};

#[derive(Debug, Clone, Copy)]
pub(crate) enum ToolSchemaNoticeKind<'a> {
    Initial,
    RuntimeUpdate { phase_node: &'a str },
}

impl<'a> ToolSchemaNoticeKind<'a> {
    fn title(&self) -> String {
        match self {
            Self::Initial => "## Runtime Tool Schema — Initial".to_string(),
            Self::RuntimeUpdate { phase_node } => {
                format!("## Runtime Tool Schema — Step Transition: {phase_node}")
            }
        }
    }

    fn notice_id_prefix(&self) -> String {
        match self {
            Self::Initial => "runtime-tool-schema-initial".to_string(),
            Self::RuntimeUpdate { phase_node } => {
                format!("runtime-tool-schema-{phase_node}")
            }
        }
    }
}

pub(crate) fn enqueue_tool_schema_notice(
    hook_session: Option<&SharedHookSessionRuntime>,
    kind: ToolSchemaNoticeKind<'_>,
    tools: &[DynAgentTool],
) {
    let Some(hook_session) = hook_session else {
        return;
    };
    let Some(content) = build_tool_schema_notice(kind, tools) else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();
    hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
        id: format!("{}-{now}", kind.notice_id_prefix()),
        created_at_ms: now,
        source: RuntimeEventSource::RuntimeContextUpdate,
        content,
    });
}

pub(crate) fn build_tool_schema_notice(
    kind: ToolSchemaNoticeKind<'_>,
    tools: &[DynAgentTool],
) -> Option<String> {
    if tools.is_empty() {
        return None;
    }

    let mut definitions = tools
        .iter()
        .map(|tool| ToolDefinition::from_tool(tool.as_ref()))
        .collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.name.cmp(&right.name));
    definitions.dedup_by(|left, right| left.name == right.name);

    let mut lines = vec![
        kind.title(),
        "以下是当前 provider request 生效的完整工具 schema。只有这里列出的工具可被本轮模型调用："
            .to_string(),
    ];
    for definition in definitions {
        lines.push(format_tool_definition(&definition));
    }
    Some(lines.join("\n\n"))
}

fn format_tool_definition(definition: &ToolDefinition) -> String {
    let description = definition.description.trim();
    let parameters = serde_json::to_string_pretty(&definition.parameters)
        .unwrap_or_else(|_| definition.parameters.to_string());

    let mut lines = vec![format!("### `{}`", definition.name)];
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    lines.push("参数 schema：".to_string());
    lines.push(format!("```json\n{parameters}\n```"));
    lines.join("\n\n")
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

        assert!(notice.contains("## Runtime Tool Schema — Initial"));
        assert!(notice.contains("mcp_agentdash_workflow_tools_upsert_workflow_tool"));
        assert!(notice.contains("创建或更新 Workflow 定义"));
        assert!(notice.contains("\"required\": ["));
        assert!(notice.contains("\"key\""));
        assert!(notice.contains("\"description\": \"Workflow key\""));
    }
}
