use std::sync::Arc;

use agentdash_agent_types::{AgentToolError, AgentToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeInvocationError,
    RuntimeInvocationOutput, RuntimeInvocationRequest, RuntimeProvider,
};

pub const MCP_LIST_TOOLS_ACTION: &str = "mcp.list_tools";
pub const MCP_CALL_TOOL_ACTION: &str = "mcp.call_tool";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpListToolsInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpListToolsOutput {
    pub tools: Vec<RuntimeMcpToolDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeMcpToolDescriptor {
    pub runtime_name: String,
    pub server_name: String,
    pub tool_name: String,
    pub uses_relay: bool,
    pub description: String,
    #[serde(default)]
    pub parameters_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpCallToolInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSessionMcpError {
    #[error("session runtime 不可用: {0}")]
    SessionUnavailable(String),
    #[error("MCP 工具不可见或不存在: {0}")]
    ToolUnavailable(String),
    #[error("MCP 工具参数非法: {0}")]
    InvalidArguments(String),
    #[error("MCP 工具发现失败: {0}")]
    DiscoveryFailed(String),
    #[error("MCP 工具执行失败: {0}")]
    ExecutionFailed(String),
}

#[async_trait]
pub trait RuntimeSessionMcpAccess: Send + Sync {
    async fn list_mcp_tools(
        &self,
        session_id: &str,
    ) -> Result<Vec<RuntimeMcpToolDescriptor>, RuntimeSessionMcpError>;

    async fn call_mcp_tool(
        &self,
        session_id: &str,
        input: McpCallToolInput,
    ) -> Result<AgentToolResult, RuntimeSessionMcpError>;
}

pub struct McpListToolsProvider {
    action_key: RuntimeActionKey,
    access: Arc<dyn RuntimeSessionMcpAccess>,
}

impl McpListToolsProvider {
    pub fn new(access: Arc<dyn RuntimeSessionMcpAccess>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(MCP_LIST_TOOLS_ACTION)
                .expect("builtin runtime action key should be valid"),
            access,
        }
    }
}

#[async_trait]
impl RuntimeProvider for McpListToolsProvider {
    fn action_key(&self) -> &RuntimeActionKey {
        &self.action_key
    }

    fn action_kind(&self) -> RuntimeActionKind {
        RuntimeActionKind::SessionRuntime
    }

    fn describe_action(&self) -> RuntimeActionDescriptor {
        RuntimeActionDescriptor {
            action_key: self.action_key.clone(),
            kind: RuntimeActionKind::SessionRuntime,
            description: Some("列出当前 Session 能力策略允许暴露的 MCP 工具".to_string()),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let raw_input = if request.input.is_null() {
            Value::Object(Default::default())
        } else {
            request.input.clone()
        };
        let input = serde_json::from_value::<McpListToolsInput>(raw_input).map_err(|error| {
            RuntimeInvocationError::invalid_request(
                format!("mcp.list_tools 输入必须是 McpListToolsInput: {error}"),
                Some(request.trace.clone()),
            )
        })?;
        let Some(session_id) = request.context.session_id().map(str::to_string) else {
            return Err(RuntimeInvocationError::invalid_request(
                "mcp.list_tools 必须绑定 Session context",
                Some(request.trace.clone()),
            ));
        };

        let mut tools = self
            .access
            .list_mcp_tools(&session_id)
            .await
            .map_err(|error| runtime_mcp_error_to_invocation(error, &request))?;
        if let Some(server_names) = input.server_names {
            tools.retain(|tool| {
                server_names
                    .iter()
                    .any(|server| server == &tool.server_name)
            });
        }

        let output = serde_json::to_value(McpListToolsOutput { tools }).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 mcp.list_tools 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;
        Ok(RuntimeInvocationOutput::new(output))
    }
}

pub struct McpCallToolProvider {
    action_key: RuntimeActionKey,
    access: Arc<dyn RuntimeSessionMcpAccess>,
}

impl McpCallToolProvider {
    pub fn new(access: Arc<dyn RuntimeSessionMcpAccess>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(MCP_CALL_TOOL_ACTION)
                .expect("builtin runtime action key should be valid"),
            access,
        }
    }
}

#[async_trait]
impl RuntimeProvider for McpCallToolProvider {
    fn action_key(&self) -> &RuntimeActionKey {
        &self.action_key
    }

    fn action_kind(&self) -> RuntimeActionKind {
        RuntimeActionKind::SessionRuntime
    }

    fn describe_action(&self) -> RuntimeActionDescriptor {
        RuntimeActionDescriptor {
            action_key: self.action_key.clone(),
            kind: RuntimeActionKind::SessionRuntime,
            description: Some("调用当前 Session 能力策略允许暴露的 MCP 工具".to_string()),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let input =
            serde_json::from_value::<McpCallToolInput>(request.input.clone()).map_err(|error| {
                RuntimeInvocationError::invalid_request(
                    format!("mcp.call_tool 输入必须是 McpCallToolInput: {error}"),
                    Some(request.trace.clone()),
                )
            })?;
        validate_call_input(&input, &request)?;
        let Some(session_id) = request.context.session_id().map(str::to_string) else {
            return Err(RuntimeInvocationError::invalid_request(
                "mcp.call_tool 必须绑定 Session context",
                Some(request.trace.clone()),
            ));
        };

        let result = self
            .access
            .call_mcp_tool(&session_id, input)
            .await
            .map_err(|error| runtime_mcp_error_to_invocation(error, &request))?;
        let output = serde_json::to_value(result).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 mcp.call_tool 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;
        Ok(RuntimeInvocationOutput::new(output))
    }
}

fn validate_call_input(
    input: &McpCallToolInput,
    request: &RuntimeInvocationRequest,
) -> Result<(), RuntimeInvocationError> {
    let has_runtime_name = input
        .runtime_name
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_server_tool = input
        .server_name
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
        && input
            .tool_name
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty());
    if !has_runtime_name && !has_server_tool {
        return Err(RuntimeInvocationError::invalid_request(
            "mcp.call_tool 必须提供 runtime_name，或同时提供 server_name 与 tool_name",
            Some(request.trace.clone()),
        ));
    }
    if let Some(arguments) = &input.arguments
        && !arguments.is_null()
        && !arguments.is_object()
    {
        return Err(RuntimeInvocationError::invalid_request(
            "mcp.call_tool arguments 必须是 JSON object 或 null",
            Some(request.trace.clone()),
        ));
    }
    Ok(())
}

pub(crate) async fn execute_runtime_mcp_tool(
    tool: agentdash_agent_types::DynAgentTool,
    runtime_name: &str,
    arguments: Value,
) -> Result<AgentToolResult, RuntimeSessionMcpError> {
    tool.execute(
        &format!("rt-mcp-{runtime_name}"),
        arguments,
        CancellationToken::new(),
        None,
    )
    .await
    .map_err(runtime_mcp_error_from_tool_error)
}

fn runtime_mcp_error_from_tool_error(error: AgentToolError) -> RuntimeSessionMcpError {
    match error {
        AgentToolError::InvalidArguments(message) => {
            RuntimeSessionMcpError::InvalidArguments(message)
        }
        AgentToolError::ExecutionFailed(message) => {
            RuntimeSessionMcpError::ExecutionFailed(message)
        }
        AgentToolError::Other(error) => RuntimeSessionMcpError::ExecutionFailed(error.to_string()),
    }
}

fn runtime_mcp_error_to_invocation(
    error: RuntimeSessionMcpError,
    request: &RuntimeInvocationRequest,
) -> RuntimeInvocationError {
    match error {
        RuntimeSessionMcpError::SessionUnavailable(message) => {
            RuntimeInvocationError::conflict(message, Some(request.trace.clone()))
        }
        RuntimeSessionMcpError::ToolUnavailable(message) => {
            RuntimeInvocationError::capability_denied(message, Some(request.trace.clone()))
        }
        RuntimeSessionMcpError::InvalidArguments(message) => {
            RuntimeInvocationError::invalid_request(message, Some(request.trace.clone()))
        }
        RuntimeSessionMcpError::DiscoveryFailed(message) => {
            RuntimeInvocationError::provider_failed(message, Some(request.trace.clone()))
        }
        RuntimeSessionMcpError::ExecutionFailed(message) => {
            RuntimeInvocationError::provider_failed(message, Some(request.trace.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_types::{AgentToolResult, ContentPart};
    use serde_json::json;

    use super::*;
    use crate::runtime_gateway::{
        RuntimeActor, RuntimeContext, RuntimeGateway, RuntimeInvocationErrorKind,
    };

    #[derive(Default)]
    struct FakeRuntimeSessionMcpAccess {
        tools: Vec<RuntimeMcpToolDescriptor>,
        call_result: Option<AgentToolResult>,
    }

    #[async_trait]
    impl RuntimeSessionMcpAccess for FakeRuntimeSessionMcpAccess {
        async fn list_mcp_tools(
            &self,
            _session_id: &str,
        ) -> Result<Vec<RuntimeMcpToolDescriptor>, RuntimeSessionMcpError> {
            Ok(self.tools.clone())
        }

        async fn call_mcp_tool(
            &self,
            _session_id: &str,
            input: McpCallToolInput,
        ) -> Result<AgentToolResult, RuntimeSessionMcpError> {
            let _ = input;
            self.call_result.clone().ok_or_else(|| {
                RuntimeSessionMcpError::ToolUnavailable("fake tool missing".to_string())
            })
        }
    }

    fn session_request(action_key: &str, input: Value) -> RuntimeInvocationRequest {
        RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(action_key).expect("valid action key"),
            RuntimeActor::SessionUser {
                session_id: "session-1".to_string(),
                user_id: None,
            },
            RuntimeContext::Session {
                session_id: "session-1".to_string(),
                project_id: None,
                workspace_id: None,
            },
            input,
        )
    }

    #[tokio::test]
    async fn mcp_list_tools_provider_returns_capability_filtered_surface() {
        let access = Arc::new(FakeRuntimeSessionMcpAccess {
            tools: vec![RuntimeMcpToolDescriptor {
                runtime_name: "mcp_code_analyzer_scan".to_string(),
                server_name: "code-analyzer".to_string(),
                tool_name: "scan".to_string(),
                uses_relay: true,
                description: "scan repo".to_string(),
                parameters_schema: json!({ "type": "object" }),
            }],
            ..Default::default()
        });
        let gateway =
            RuntimeGateway::new().with_provider(Arc::new(McpListToolsProvider::new(access)));

        let result = gateway
            .invoke(session_request(MCP_LIST_TOOLS_ACTION, json!({})))
            .await
            .expect("list tools should succeed");

        assert_eq!(
            result.output.output["tools"][0]["runtime_name"],
            "mcp_code_analyzer_scan"
        );
        assert_eq!(
            result.output.output["tools"][0]["server_name"],
            "code-analyzer"
        );
        assert_eq!(result.output.output["tools"][0]["uses_relay"], true);
    }

    #[tokio::test]
    async fn mcp_call_tool_provider_requires_tool_target() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(McpCallToolProvider::new(
            Arc::new(FakeRuntimeSessionMcpAccess::default()),
        )));

        let err = gateway
            .invoke(session_request(
                MCP_CALL_TOOL_ACTION,
                json!({ "arguments": {} }),
            ))
            .await
            .expect_err("missing tool target should fail");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::InvalidRequest);
    }

    #[tokio::test]
    async fn mcp_call_tool_provider_invokes_access() {
        let access = Arc::new(FakeRuntimeSessionMcpAccess {
            call_result: Some(AgentToolResult {
                content: vec![ContentPart::text("ok")],
                is_error: false,
                details: None,
            }),
            ..Default::default()
        });
        let gateway =
            RuntimeGateway::new().with_provider(Arc::new(McpCallToolProvider::new(access)));

        let result = gateway
            .invoke(session_request(
                MCP_CALL_TOOL_ACTION,
                json!({
                    "runtime_name": "mcp_code_analyzer_scan",
                    "arguments": { "path": "." }
                }),
            ))
            .await
            .expect("call tool should succeed");

        assert_eq!(result.output.output["is_error"], false);
        assert_eq!(result.output.output["content"][0]["type"], "text");
    }

    #[tokio::test]
    async fn mcp_call_tool_provider_maps_missing_tool_to_capability_denied() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(McpCallToolProvider::new(
            Arc::new(FakeRuntimeSessionMcpAccess::default()),
        )));

        let err = gateway
            .invoke(session_request(
                MCP_CALL_TOOL_ACTION,
                json!({ "runtime_name": "mcp_unknown_tool", "arguments": {} }),
            ))
            .await
            .expect_err("unknown tool should fail");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }
}
