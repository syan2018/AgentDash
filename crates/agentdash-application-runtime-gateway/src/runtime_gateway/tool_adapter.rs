use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeGateway, RuntimeInvocationError,
    RuntimeInvocationErrorKind, RuntimeInvocationRequest, RuntimeInvocationResult, RuntimeTarget,
};

#[derive(Clone)]
pub struct RuntimeActionToolSpec {
    pub tool_name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub action_key: RuntimeActionKey,
    pub actor: RuntimeActor,
    pub context: RuntimeContext,
    pub target: Option<RuntimeTarget>,
    pub metadata: BTreeMap<String, Value>,
}

impl RuntimeActionToolSpec {
    pub fn runtime_session(
        tool_name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: Value,
        action_key: RuntimeActionKey,
        session_id: impl Into<String>,
        agent_id: Option<String>,
    ) -> Self {
        let session_id = session_id.into();
        Self {
            tool_name: tool_name.into(),
            description: description.into(),
            parameters_schema,
            action_key,
            actor: RuntimeActor::AgentSession {
                session_id: session_id.clone(),
                agent_id,
            },
            context: RuntimeContext::Session {
                session_id,
                project_id: None,
                workspace_id: None,
            },
            target: None,
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct RuntimeActionToolAdapter {
    gateway: Arc<RuntimeGateway>,
    spec: RuntimeActionToolSpec,
}

impl RuntimeActionToolAdapter {
    pub fn new(gateway: Arc<RuntimeGateway>, spec: RuntimeActionToolSpec) -> Self {
        Self { gateway, spec }
    }
}

#[async_trait]
impl AgentTool for RuntimeActionToolAdapter {
    fn name(&self) -> &str {
        &self.spec.tool_name
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn parameters_schema(&self) -> Value {
        self.spec.parameters_schema.clone()
    }
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(
            agentdash_agent_types::ToolProtocolProjector::RuntimeAction {
                action_key: self.spec.action_key.to_string(),
            },
        )
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: Value,
        cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        if cancel.is_cancelled() {
            return Err(AgentToolError::ExecutionFailed(
                "Runtime Action 调用已取消".to_string(),
            ));
        }

        let mut request = RuntimeInvocationRequest::new(
            self.spec.action_key.clone(),
            self.spec.actor.clone(),
            self.spec.context.clone(),
            args,
        );
        request.target = self.spec.target.clone();
        request.metadata = self.spec.metadata.clone();

        let result = self
            .gateway
            .invoke(request)
            .await
            .map_err(runtime_error_to_tool_error)?;
        Ok(invocation_result_to_tool_result(result))
    }
}

fn invocation_result_to_tool_result(result: RuntimeInvocationResult) -> AgentToolResult {
    if let Ok(mut tool_result) =
        serde_json::from_value::<AgentToolResult>(result.output.output.clone())
    {
        tool_result.details = Some(runtime_tool_details(result, tool_result.details.take()));
        return tool_result;
    }

    let rendered = serde_json::to_string_pretty(&result.output.output)
        .unwrap_or_else(|_| result.output.output.to_string());
    AgentToolResult {
        content: vec![ContentPart::text(rendered)],
        is_error: false,
        details: Some(runtime_tool_details(result, None)),
    }
}

fn runtime_tool_details(result: RuntimeInvocationResult, provider_details: Option<Value>) -> Value {
    json!({
        "runtime_action": result.action_key,
        "runtime_trace": result.trace,
        "provider_details": provider_details,
    })
}

fn runtime_error_to_tool_error(error: RuntimeInvocationError) -> AgentToolError {
    match error.kind() {
        RuntimeInvocationErrorKind::InvalidRequest => {
            AgentToolError::InvalidArguments(error.to_string())
        }
        RuntimeInvocationErrorKind::CapabilityDenied
        | RuntimeInvocationErrorKind::Conflict
        | RuntimeInvocationErrorKind::ProviderUnavailable
        | RuntimeInvocationErrorKind::ProviderFailed
        | RuntimeInvocationErrorKind::Timeout => AgentToolError::ExecutionFailed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;
    use tokio::sync::Mutex as TokioMutex;

    use super::*;
    use crate::runtime_gateway::{RuntimeActionKind, RuntimeInvocationOutput, RuntimeProvider};

    struct EchoProvider {
        action_key: RuntimeActionKey,
        captured_input: Arc<TokioMutex<Option<Value>>>,
    }

    impl EchoProvider {
        fn new(action_key: &str, captured_input: Arc<TokioMutex<Option<Value>>>) -> Self {
            Self {
                action_key: RuntimeActionKey::parse(action_key).expect("valid action key"),
                captured_input,
            }
        }
    }

    #[async_trait]
    impl RuntimeProvider for EchoProvider {
        fn action_key(&self) -> &RuntimeActionKey {
            &self.action_key
        }

        fn action_kind(&self) -> RuntimeActionKind {
            RuntimeActionKind::SessionRuntime
        }

        async fn invoke(
            &self,
            request: RuntimeInvocationRequest,
        ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
            *self.captured_input.lock().await = Some(request.input.clone());
            let output = AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "echo: {}",
                    request.input["message"].as_str().unwrap_or_default()
                ))],
                is_error: false,
                details: Some(json!({ "provider": "echo" })),
            };
            Ok(RuntimeInvocationOutput::new(
                serde_json::to_value(output).expect("serialize output"),
            ))
        }
    }

    #[tokio::test]
    async fn runtime_action_tool_adapter_invokes_gateway() {
        let captured_input = Arc::new(TokioMutex::new(None));
        let gateway = Arc::new(
            RuntimeGateway::new().with_provider(Arc::new(EchoProvider::new(
                "runtime.echo",
                captured_input.clone(),
            ))),
        );
        let adapter = RuntimeActionToolAdapter::new(
            gateway,
            RuntimeActionToolSpec::runtime_session(
                "runtime_echo",
                "Echo through Runtime Gateway",
                json!({ "type": "object" }),
                RuntimeActionKey::parse("runtime.echo").expect("valid action key"),
                "session-1",
                Some("agent-1".to_string()),
            ),
        );

        let result = adapter
            .execute(
                "tool-call-1",
                json!({ "message": "hello" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("adapter should invoke gateway");

        assert_eq!(result.content[0].extract_text(), Some("echo: hello"));
        assert_eq!(
            *captured_input.lock().await,
            Some(json!({ "message": "hello" }))
        );
        assert_eq!(
            result.details.as_ref().unwrap()["runtime_action"],
            "runtime.echo"
        );
        assert_eq!(
            result.details.as_ref().unwrap()["provider_details"]["provider"],
            "echo"
        );
    }

    #[tokio::test]
    async fn runtime_action_tool_adapter_maps_invalid_request_to_tool_argument_error() {
        let captured_input = Arc::new(TokioMutex::new(None));
        let gateway = Arc::new(
            RuntimeGateway::new()
                .with_provider(Arc::new(EchoProvider::new("runtime.echo", captured_input))),
        );
        let mut spec = RuntimeActionToolSpec::runtime_session(
            "runtime_echo",
            "Echo through Runtime Gateway",
            json!({ "type": "object" }),
            RuntimeActionKey::parse("runtime.echo").expect("valid action key"),
            "session-1",
            None,
        );
        spec.context = RuntimeContext::Setup {
            project_id: None,
            workspace_id: None,
            backend_id: None,
            root_ref: None,
        };
        let adapter = RuntimeActionToolAdapter::new(gateway, spec);

        let error = adapter
            .execute("tool-call-1", json!({}), CancellationToken::new(), None)
            .await
            .expect_err("missing provider should fail");

        assert!(matches!(error, AgentToolError::InvalidArguments(_)));
    }
}
