use agentdash_agent_types::{AgentTool, AgentToolError, AgentToolResult, ContentPart};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::service::WaitActivityService;
use super::types::{WaitActivityRequest, WaitActivityResult, WaitToolContext};

#[derive(Clone)]
pub(crate) struct WaitTool {
    service: WaitActivityService,
    context: WaitToolContext,
}

impl WaitTool {
    pub(crate) fn new(service: WaitActivityService, context: WaitToolContext) -> Self {
        Self { service, context }
    }
}

#[async_trait]
impl AgentTool for WaitTool {
    fn name(&self) -> &str {
        "wait"
    }

    fn description(&self) -> &str {
        "Wait for AgentRun activities such as exec terminals, human/subagent/companion gates, and mailbox wake messages. Returns bounded status summaries and refs only; timeout does not cancel background activity."
    }

    fn parameters_schema(&self) -> Value {
        schema_value(schemars::schema_for!(WaitActivityRequest))
    }
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(agentdash_agent_types::ToolProtocolProjector::Dynamic { namespace: None })
    }
    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_wait_activity_dynamic_lifecycle".to_string())
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: Value,
        cancel: CancellationToken,
        _on_update: Option<agentdash_agent_types::ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let request: WaitActivityRequest = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;
        let result = self
            .service
            .wait(self.context.clone(), request, cancel)
            .await?;
        Ok(wait_tool_result(result))
    }
}

fn wait_tool_result(result: WaitActivityResult) -> AgentToolResult {
    let mut lines = vec![
        format!("status: {}", result.status),
        format!("timed_out: {}", result.timed_out),
        format!("cursor: {}", result.cursor),
    ];
    if result.items.is_empty() {
        lines.push("items: []".to_string());
    } else {
        lines.push("items:".to_string());
        for item in &result.items {
            let preview = item
                .preview
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("");
            if preview.is_empty() {
                lines.push(format!(
                    "- [{}:{}] {}",
                    item.kind, item.status, item.activity_ref
                ));
            } else {
                lines.push(format!(
                    "- [{}:{}] {} - {}",
                    item.kind, item.status, item.activity_ref, preview
                ));
            }
        }
    }
    AgentToolResult {
        content: vec![ContentPart::text(lines.join("\n"))],
        is_error: false,
        details: Some(json!({
            "type": "wait",
            "status": result.status,
            "timed_out": result.timed_out,
            "cursor": result.cursor,
            "items": result.items,
        })),
    }
}

fn schema_value(schema: schemars::Schema) -> Value {
    serde_json::to_value(schema).unwrap_or_else(|_| json!({ "type": "object" }))
}
