use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_agent_types::{
    AfterToolCallEffects, AfterToolCallInput, AgentRuntimeError, BeforeToolCallInput,
    DynRuntimeToolPolicyDelegate, RuntimeToolPolicyDelegate, ToolCallDecision,
};
use agentdash_application_ports::agent_run_surface::{
    AgentRunAdmissionRequest, AgentRunEffectiveCapabilityPort,
};
use agentdash_spi::hooks::RuntimeToolSchemaEntry;
use agentdash_spi::{ToolCapability, ToolCluster};
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

pub(crate) struct AgentRunAdmissionToolPolicyFacet {
    runtime_session_id: String,
    port: Arc<dyn AgentRunEffectiveCapabilityPort>,
    inner: Option<DynRuntimeToolPolicyDelegate>,
    tools: BTreeMap<String, ToolAdmissionMetadata>,
}

impl AgentRunAdmissionToolPolicyFacet {
    pub(crate) fn wrap(
        runtime_session_id: String,
        port: Arc<dyn AgentRunEffectiveCapabilityPort>,
        inner: Option<DynRuntimeToolPolicyDelegate>,
        tools: BTreeMap<String, ToolAdmissionMetadata>,
    ) -> DynRuntimeToolPolicyDelegate {
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
impl RuntimeToolPolicyDelegate for AgentRunAdmissionToolPolicyFacet {
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

        fn deny() -> Self {
            Self {
                decision: AgentRunAdmissionDecision {
                    allowed: false,
                    reason: Some("denied by test".to_string()),
                },
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

    #[derive(Default)]
    struct RecordingToolPolicy {
        before_calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl RuntimeToolPolicyDelegate for RecordingToolPolicy {
        async fn before_tool_call(
            &self,
            input: BeforeToolCallInput,
            _cancel: CancellationToken,
        ) -> Result<ToolCallDecision, AgentRuntimeError> {
            self.before_calls
                .lock()
                .expect("before calls mutex")
                .push(input.tool_call.name);
            Ok(ToolCallDecision::Allow)
        }

        async fn after_tool_call(
            &self,
            _input: AfterToolCallInput,
            _cancel: CancellationToken,
        ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
            Ok(AfterToolCallEffects::default())
        }
    }

    impl RecordingToolPolicy {
        fn before_call_count(&self) -> usize {
            self.before_calls.lock().expect("before calls mutex").len()
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

    fn workflow_tool_metadata() -> BTreeMap<String, ToolAdmissionMetadata> {
        ToolAdmissionMetadata::from_schema_entries(&[RuntimeToolSchemaEntry {
            name: "workflow_runtime_name".to_string(),
            description: "Workflow mutation".to_string(),
            parameters_schema: serde_json::json!({ "type": "object" }),
            capability_key: Some("workflow_management".to_string()),
            source: Some("platform:workflow".to_string()),
            tool_path: Some("workflow_management::upsert_workflow_tool".to_string()),
            context_usage_kind: None,
        }])
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
    async fn admission_tool_policy_calls_agent_run_admission_before_inner_policy() {
        let port = Arc::new(CapturingAdmissionPort::allow());
        let inner = Arc::new(RecordingToolPolicy::default());
        let delegate = AgentRunAdmissionToolPolicyFacet {
            runtime_session_id: "session-a".to_string(),
            port: port.clone(),
            inner: Some(inner.clone()),
            tools: workflow_tool_metadata(),
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
        assert_eq!(inner.before_call_count(), 1);
    }

    #[tokio::test]
    async fn admission_deny_short_circuits_before_inner_policy() {
        let port = Arc::new(CapturingAdmissionPort::deny());
        let inner = Arc::new(RecordingToolPolicy::default());
        let delegate = AgentRunAdmissionToolPolicyFacet {
            runtime_session_id: "session-a".to_string(),
            port: port.clone(),
            inner: Some(inner.clone()),
            tools: workflow_tool_metadata(),
        };

        let decision = delegate
            .before_tool_call(
                tool_call_input("workflow_runtime_name"),
                CancellationToken::new(),
            )
            .await
            .expect("decision");

        assert!(
            matches!(decision, ToolCallDecision::Deny { reason } if reason == "denied by test")
        );
        assert_eq!(port.requests().len(), 1);
        assert_eq!(inner.before_call_count(), 0);
    }

    #[tokio::test]
    async fn admission_tool_policy_denies_tools_without_metadata() {
        let port = Arc::new(CapturingAdmissionPort::allow());
        let delegate = AgentRunAdmissionToolPolicyFacet {
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
