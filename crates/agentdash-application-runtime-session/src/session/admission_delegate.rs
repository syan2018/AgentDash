use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_application_ports::agent_run_surface::{
    AgentRunAdmissionRequest, AgentRunEffectiveCapabilityPort,
};
use agentdash_spi::hooks::RuntimeToolSchemaEntry;
use agentdash_spi::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallInput,
    CompactionFailureInput, CompactionParams, CompactionResult, DynAgentRuntimeDelegate,
    EvaluateCompactionInput, StopDecision, ToolCallDecision, ToolCapability, ToolCluster,
    TransformContextInput, TransformContextOutput, TurnControlDecision,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolAdmissionMetadata {
    capability_key: String,
    tool_name: String,
    cluster: Option<ToolCluster>,
}

impl ToolAdmissionMetadata {
    pub(crate) fn from_schema_entries(
        entries: &[RuntimeToolSchemaEntry],
    ) -> BTreeMap<String, Self> {
        entries
            .iter()
            .filter_map(|entry| {
                let capability_key = entry.capability_key.as_ref()?.clone();
                let tool_name = entry
                    .tool_path
                    .as_deref()
                    .and_then(|path| path.rsplit_once("::").map(|(_, tool)| tool.to_string()))
                    .unwrap_or_else(|| entry.name.clone());
                let cluster =
                    agentdash_spi::platform::tool_capability::capability_to_tool_clusters(
                        &ToolCapability::new(&capability_key),
                    )
                    .into_iter()
                    .next();
                Some((
                    entry.name.clone(),
                    Self {
                        capability_key,
                        tool_name,
                        cluster,
                    },
                ))
            })
            .collect()
    }
}

pub(crate) struct AgentRunAdmissionRuntimeDelegate {
    runtime_session_id: String,
    port: Arc<dyn AgentRunEffectiveCapabilityPort>,
    inner: Option<DynAgentRuntimeDelegate>,
    tools: BTreeMap<String, ToolAdmissionMetadata>,
}

impl AgentRunAdmissionRuntimeDelegate {
    pub(crate) fn wrap(
        runtime_session_id: String,
        port: Arc<dyn AgentRunEffectiveCapabilityPort>,
        inner: Option<DynAgentRuntimeDelegate>,
        tools: BTreeMap<String, ToolAdmissionMetadata>,
    ) -> DynAgentRuntimeDelegate {
        Arc::new(Self {
            runtime_session_id,
            port,
            inner,
            tools,
        })
    }

    async fn admit_tool_call(
        &self,
        tool_name: &str,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        let Some(metadata) = self.tools.get(tool_name) else {
            return Ok(ToolCallDecision::Deny {
                reason: format!("tool `{tool_name}` has no AgentRun admission metadata"),
            });
        };
        let decision = self
            .port
            .admit_tool(AgentRunAdmissionRequest::tool(
                self.runtime_session_id.clone(),
                metadata.capability_key.clone(),
                metadata.tool_name.clone(),
                metadata.cluster,
            ))
            .await
            .map_err(|error| {
                AgentRuntimeError::Runtime(format!("AgentRun tool admission failed: {error}"))
            })?;
        if decision.allowed {
            Ok(ToolCallDecision::Allow)
        } else {
            Ok(ToolCallDecision::Deny {
                reason: decision.reason.unwrap_or_else(|| {
                    format!(
                        "tool `{}` is not admitted for capability `{}`",
                        metadata.tool_name, metadata.capability_key
                    )
                }),
            })
        }
    }
}

#[async_trait]
impl AgentRuntimeDelegate for AgentRunAdmissionRuntimeDelegate {
    async fn evaluate_compaction(
        &self,
        input: EvaluateCompactionInput,
        cancel: CancellationToken,
    ) -> Result<Option<CompactionParams>, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.evaluate_compaction(input, cancel).await,
            None => Ok(None),
        }
    }

    async fn after_compaction(
        &self,
        result: CompactionResult,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.after_compaction(result, cancel).await,
            None => Ok(()),
        }
    }

    async fn after_compaction_failed(
        &self,
        input: CompactionFailureInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.after_compaction_failed(input, cancel).await,
            None => Ok(()),
        }
    }

    async fn transform_context(
        &self,
        input: TransformContextInput,
        cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.transform_context(input, cancel).await,
            None => Ok(TransformContextOutput {
                steering_messages: input.context.messages,
                blocked: None,
            }),
        }
    }

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        match self.admit_tool_call(&input.tool_call.name).await? {
            ToolCallDecision::Allow => match &self.inner {
                Some(inner) => inner.before_tool_call(input, cancel).await,
                None => Ok(ToolCallDecision::Allow),
            },
            decision => Ok(decision),
        }
    }

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.after_tool_call(input, cancel).await,
            None => Ok(AfterToolCallEffects::default()),
        }
    }

    async fn after_turn(
        &self,
        input: AfterTurnInput,
        cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.after_turn(input, cancel).await,
            None => Ok(TurnControlDecision::default()),
        }
    }

    async fn before_stop(
        &self,
        input: BeforeStopInput,
        cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.before_stop(input, cancel).await,
            None => Ok(StopDecision::Stop),
        }
    }

    async fn on_before_provider_request(
        &self,
        input: BeforeProviderRequestInput,
        cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        match &self.inner {
            Some(inner) => inner.on_before_provider_request(input, cancel).await,
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_ports::agent_run_surface::{
        AgentRunAdmissionDecision, AgentRunEffectiveCapabilityError,
        AgentRunEffectiveCapabilityRequest, AgentRunEffectiveCapabilityView,
    };
    use agentdash_spi::{AgentContext, AgentMessage, ToolCallInfo};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct CapturingAdmissionPort {
        decision: AgentRunAdmissionDecision,
        requests: Mutex<Vec<AgentRunAdmissionRequest>>,
    }

    impl CapturingAdmissionPort {
        fn allow() -> Self {
            Self {
                decision: AgentRunAdmissionDecision::allow(),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<AgentRunAdmissionRequest> {
            self.requests.lock().expect("requests mutex").clone()
        }
    }

    #[async_trait]
    impl AgentRunEffectiveCapabilityPort for CapturingAdmissionPort {
        async fn effective_capability(
            &self,
            _request: AgentRunEffectiveCapabilityRequest,
        ) -> Result<AgentRunEffectiveCapabilityView, AgentRunEffectiveCapabilityError> {
            Err(AgentRunEffectiveCapabilityError::Projection {
                message: "unused in admission delegate tests".to_string(),
            })
        }

        async fn admit_tool(
            &self,
            request: AgentRunAdmissionRequest,
        ) -> Result<AgentRunAdmissionDecision, AgentRunEffectiveCapabilityError> {
            self.requests.lock().expect("requests mutex").push(request);
            Ok(self.decision.clone())
        }
    }

    fn tool_call_input(tool_name: &str) -> BeforeToolCallInput {
        BeforeToolCallInput {
            assistant_message: AgentMessage::user("assistant"),
            tool_call: ToolCallInfo {
                id: "call-1".to_string(),
                call_id: None,
                name: tool_name.to_string(),
                arguments: serde_json::json!({}),
            },
            args: serde_json::json!({}),
            context: AgentContext {
                system_prompt: String::new(),
                messages: Vec::new(),
                message_refs: Vec::new(),
                tools: Vec::new(),
            },
        }
    }

    #[test]
    fn metadata_uses_tool_path_leaf_as_admission_tool_name() {
        let metadata = ToolAdmissionMetadata::from_schema_entries(&[RuntimeToolSchemaEntry {
            name: "mcp_code_analyzer_scan_repo".to_string(),
            description: "Scan repository".to_string(),
            parameters_schema: serde_json::json!({ "type": "object" }),
            capability_key: Some("mcp:code-analyzer".to_string()),
            source: Some("mcp:code-analyzer".to_string()),
            tool_path: Some("mcp:code-analyzer::scan_repo".to_string()),
            context_usage_kind: None,
        }]);

        let entry = metadata
            .get("mcp_code_analyzer_scan_repo")
            .expect("metadata entry");
        assert_eq!(entry.capability_key, "mcp:code-analyzer");
        assert_eq!(entry.tool_name, "scan_repo");
        assert_eq!(entry.cluster, None);
    }

    #[tokio::test]
    async fn delegate_calls_agent_run_admission_before_inner_delegate() {
        let port = Arc::new(CapturingAdmissionPort::allow());
        let metadata = ToolAdmissionMetadata::from_schema_entries(&[RuntimeToolSchemaEntry {
            name: "workflow_runtime_name".to_string(),
            description: "Workflow mutation".to_string(),
            parameters_schema: serde_json::json!({ "type": "object" }),
            capability_key: Some("workflow_management".to_string()),
            source: Some("platform:workflow".to_string()),
            tool_path: Some("workflow_management::upsert_workflow_tool".to_string()),
            context_usage_kind: None,
        }]);
        let delegate = AgentRunAdmissionRuntimeDelegate {
            runtime_session_id: "session-a".to_string(),
            port: port.clone(),
            inner: None,
            tools: metadata,
        };

        let decision = delegate
            .before_tool_call(
                tool_call_input("workflow_runtime_name"),
                CancellationToken::new(),
            )
            .await
            .expect("decision");

        assert!(matches!(decision, ToolCallDecision::Allow));
        let requests = port.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].runtime_session_id, "session-a");
        assert_eq!(requests[0].capability_key, "workflow_management");
        assert_eq!(requests[0].tool_name, "upsert_workflow_tool");
    }

    #[tokio::test]
    async fn delegate_denies_tools_without_admission_metadata() {
        let port = Arc::new(CapturingAdmissionPort::allow());
        let delegate = AgentRunAdmissionRuntimeDelegate {
            runtime_session_id: "session-a".to_string(),
            port: port.clone(),
            inner: None,
            tools: BTreeMap::new(),
        };

        let decision = delegate
            .before_tool_call(tool_call_input("untracked_tool"), CancellationToken::new())
            .await
            .expect("decision");

        assert!(
            matches!(decision, ToolCallDecision::Deny { reason } if reason.contains("no AgentRun admission metadata"))
        );
        assert!(port.requests().is_empty());
    }
}
