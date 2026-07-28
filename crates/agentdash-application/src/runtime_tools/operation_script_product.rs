use std::sync::Arc;

use agentdash_application_operation_gateway::{
    AgentRunOperationHost, HostOperationScriptOperationSet, HostOperationScriptProgram,
    OperationGateway,
};
use agentdash_application_ports::operation_script::{
    OPERATION_SCRIPT_HOST_API_V1, OperationScriptCallEvidence, OperationScriptCallStatus,
    OperationScriptEngine, OperationScriptError, OperationScriptLimits, OperationScriptOutcome,
    OperationScriptResultValue, RHAI_V1_DIALECT,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationScriptArguments {
    source: String,
    #[serde(default)]
    input: Value,
}

pub fn operation_script_runtime_tool_schema() -> Value {
    let properties = serde_json::Map::from_iter([
        (
            "source".to_owned(),
            json!({
                "type": "string",
                "description": "Ephemeral Rhai source. Use exact operation strings obtained from the latest workspace_module_describe results."
            }),
        ),
        ("input".to_owned(), json!({})),
    ]);
    json!({
        "type": "object",
        "properties": properties,
        "required": ["source"],
        "additionalProperties": false
    })
}

pub struct ApplicationOperationScriptRuntimeToolService {
    gateway: Arc<OperationGateway>,
    engine: Arc<dyn OperationScriptEngine>,
}

impl ApplicationOperationScriptRuntimeToolService {
    pub fn new(gateway: Arc<OperationGateway>, engine: Arc<dyn OperationScriptEngine>) -> Self {
        Self { gateway, engine }
    }

    async fn execute_inner(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> Result<Value, ProductRuntimeToolOutcome> {
        let arguments: OperationScriptArguments = serde_json::from_value(request.arguments)
            .map_err(|error| {
                rejected(
                    "operation_script_invalid_arguments",
                    format!("invalid OperationScript arguments: {error}"),
                )
            })?;
        let program = HostOperationScriptProgram {
            language: RHAI_V1_DIALECT.to_owned(),
            host_api_version: OPERATION_SCRIPT_HOST_API_V1,
            source: arguments.source,
            input: arguments.input,
            operation_set: HostOperationScriptOperationSet::CurrentActorSurface,
            limits: OperationScriptLimits::default(),
        };
        let host = AgentRunOperationHost::project(
            self.gateway.clone(),
            request.context.target.run_id,
            request.context.target.agent_id,
            request.context.target.project_id,
        )
        .map_err(|error| failed("operation_script_host_binding_failed", error.to_string()))?
        .operation_script(self.engine.clone());
        let result = host
            .execute(program, CancellationToken::new())
            .await
            .and_then(serialize_agent_result);
        result.map_err(map_script_error)
    }
}

fn serialize_agent_result(outcome: OperationScriptOutcome) -> Result<Value, OperationScriptError> {
    let value = match outcome.value {
        OperationScriptResultValue::Inline { value } => value,
        OperationScriptResultValue::Ref { result_ref } => json!({ "result_ref": result_ref }),
    };
    let call_summary = summarize_calls(&outcome.calls);
    let mut output = json!({
        "value": value,
        "calls": call_summary,
    });
    let has_failed_call = outcome
        .calls
        .iter()
        .any(|call| call.status != OperationScriptCallStatus::Succeeded);
    if outcome.outcome_unknown || has_failed_call {
        output["partial"] = json!(outcome.partial);
        output["outcome_unknown"] = json!(outcome.outcome_unknown);
        output["execution_id"] = json!(outcome.execution_id);
    }
    Ok(output)
}

fn summarize_calls(calls: &[OperationScriptCallEvidence]) -> Value {
    let failed = calls
        .iter()
        .filter(|call| call.status != OperationScriptCallStatus::Succeeded)
        .map(|call| {
            json!({
                "index": call.call_index,
                "operation": operation_ref_label(&call.operation_ref),
                "status": call.status,
                "error_code": call.error_code,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "total": calls.len(),
        "failed": failed,
    })
}

fn operation_ref_label(operation_ref: &agentdash_domain::operation::OperationRef) -> String {
    format!(
        "{}:{}:{}:v{}",
        operation_ref.provider.namespace,
        operation_ref.provider.provider_key,
        operation_ref.operation_key,
        operation_ref.contract_version
    )
}

#[async_trait]
impl ProductRuntimeToolService for ApplicationOperationScriptRuntimeToolService {
    fn kind(&self) -> ProductRuntimeToolKind {
        ProductRuntimeToolKind::OperationScript
    }

    fn parameters_schema(&self) -> Value {
        operation_script_runtime_tool_schema()
    }

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        match self.execute_inner(request).await {
            Ok(output) => ProductRuntimeToolOutcome::Completed { output },
            Err(outcome) => outcome,
        }
    }
}

fn map_script_error(error: OperationScriptError) -> ProductRuntimeToolOutcome {
    let code = match &error {
        OperationScriptError::InvalidRequest { .. } => {
            "operation_script_invalid_request".to_string()
        }
        OperationScriptError::OperationDenied { .. } => {
            "operation_script_operation_denied".to_string()
        }
        OperationScriptError::SurfaceUnavailable { code, .. } => code.clone(),
        OperationScriptError::Compile { .. } => "operation_script_compile_failed".to_string(),
        OperationScriptError::CapacityExceeded => "operation_script_capacity_exceeded".to_string(),
        OperationScriptError::Cancelled => "operation_script_cancelled".to_string(),
        OperationScriptError::DeadlineExceeded => "operation_script_deadline_exceeded".to_string(),
        OperationScriptError::ExecutionInterrupted { .. } => {
            "operation_script_interrupted".to_string()
        }
        OperationScriptError::Runtime { .. } => "operation_script_runtime_failed".to_string(),
        OperationScriptError::CallLimitExceeded { .. } => {
            "operation_script_call_limit_exceeded".to_string()
        }
        OperationScriptError::ParallelLimitExceeded { .. } => {
            "operation_script_parallel_limit_exceeded".to_string()
        }
        OperationScriptError::OutputLimitExceeded { .. } => {
            "operation_script_output_limit_exceeded".to_string()
        }
        OperationScriptError::NestedOperation { .. } => {
            "operation_script_nested_operation_failed".to_string()
        }
        OperationScriptError::ExecutionFailed { .. } => {
            "operation_script_execution_failed".to_string()
        }
        OperationScriptError::Internal { .. } => "operation_script_internal".to_string(),
    };
    match error {
        OperationScriptError::InvalidRequest { .. }
        | OperationScriptError::OperationDenied { .. }
        | OperationScriptError::Compile { .. } => rejected(code, error.to_string()),
        _ => failed(code, error.to_string()),
    }
}

fn rejected(code: impl Into<String>, message: impl Into<String>) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Rejected {
        code: code.into(),
        message: message.into(),
    }
}

fn failed(code: impl Into<String>, message: impl Into<String>) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Failed {
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_application_operation_gateway::{
        EphemeralOperationResultStore, OperationActorKind, OperationAuthorityGrant,
        OperationAuthorityResolver, OperationAuthorizationScope, OperationDescriptor,
        OperationDispatch, OperationExecutionError, OperationExecutionPolicy,
        OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
        OperationProvenance, OperationProvider, OperationReadiness, TracingOperationAuditSink,
    };
    use agentdash_application_ports::product_runtime_tool::{
        ProductRuntimeToolContext, ProductRuntimeToolTarget,
    };
    use agentdash_domain::operation::{
        OperationEffect, OperationProviderRef, OperationRef, OperationReplayPolicy,
    };
    use agentdash_infrastructure::{RhaiOperationScriptConfig, RhaiOperationScriptEngine};

    use super::*;

    struct AllowAuthority;

    #[async_trait]
    impl OperationAuthorityResolver for AllowAuthority {
        async fn resolve(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<
            OperationAuthorityGrant,
            agentdash_application_operation_gateway::OperationExecutionError,
        > {
            Ok(OperationAuthorityGrant {
                authority_revision: "test-revision".to_owned(),
                capabilities: BTreeSet::new(),
            })
        }
    }

    struct FixtureProvider {
        provider_ref: OperationProviderRef,
        operation_ref: OperationRef,
        output: Value,
    }

    #[async_trait]
    impl OperationProvider for FixtureProvider {
        fn provider_ref(&self) -> &OperationProviderRef {
            &self.provider_ref
        }

        async fn discover(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
            Ok(vec![OperationDescriptor {
                operation_ref: self.operation_ref.clone(),
                title: self.operation_ref.operation_key.clone(),
                description: None,
                input_schema: json!({"type": "object"}),
                output_schema: json!({}),
                effect: OperationEffect::Read,
                replay_policy: OperationReplayPolicy::ReplaySafe,
                required_capabilities: BTreeSet::new(),
                actor_visibility: BTreeSet::from([OperationActorKind::Agent]),
                execution_policy: OperationExecutionPolicy::default(),
                readiness: OperationReadiness::Ready,
                provenance: OperationProvenance {
                    source: "product-operation-script-test".to_owned(),
                    artifact_digest: None,
                },
                dispatch: OperationDispatch {
                    provider: self.provider_ref.clone(),
                    route: self.operation_ref.operation_key.clone(),
                },
            }])
        }

        async fn resolve_placement(
            &self,
            _: &OperationDescriptor,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<OperationPlacement, OperationExecutionError> {
            Ok(OperationPlacement::Cloud)
        }

        async fn invoke(
            &self,
            _: &OperationDescriptor,
            _: OperationInvocationEnvelope,
            _: CancellationToken,
        ) -> Result<Value, OperationExecutionError> {
            Ok(self.output.clone())
        }
    }

    fn provider(
        provider_key: &str,
        operation_key: &str,
        output: Value,
    ) -> Arc<dyn OperationProvider> {
        let operation_ref =
            OperationRef::new("platform", provider_key, operation_key, 1).expect("operation ref");
        Arc::new(FixtureProvider {
            provider_ref: operation_ref.provider.clone(),
            operation_ref,
            output,
        })
    }

    fn gateway(providers: Vec<Arc<dyn OperationProvider>>) -> Arc<OperationGateway> {
        Arc::new(
            OperationGateway::try_new(
                Arc::new(AllowAuthority),
                providers,
                [],
                Arc::new(EphemeralOperationResultStore::default()),
                Arc::new(TracingOperationAuditSink),
            )
            .expect("gateway"),
        )
    }

    fn request(source: &str) -> ProductRuntimeToolRequest {
        ProductRuntimeToolRequest {
            context: ProductRuntimeToolContext {
                runtime_thread_id: RuntimeThreadId::new("runtime-thread").expect("runtime thread"),
                target: ProductRuntimeToolTarget {
                    project_id: uuid::Uuid::new_v4(),
                    run_id: uuid::Uuid::new_v4(),
                    agent_id: uuid::Uuid::new_v4(),
                },
                turn_id: "turn-1".to_owned(),
                item_id: Some("item-1".to_owned()),
                effect_id: "effect-1".to_owned(),
                invocation_id: "invocation-1".to_owned(),
                deadline_at_ms: u64::MAX,
            },
            arguments: json!({"source": source}),
        }
    }

    #[test]
    fn schema_exposes_only_source_and_input() {
        let schema = operation_script_runtime_tool_schema();

        let properties = schema["properties"].as_object().expect("properties");
        let mut property_names = properties.keys().map(String::as_str).collect::<Vec<_>>();
        property_names.sort_unstable();
        assert_eq!(property_names, vec!["input", "source"]);
        assert_eq!(schema["required"], json!(["source"]));
    }

    #[tokio::test]
    async fn composes_current_surface_operations_through_the_product_tool() {
        let engine = Arc::new(
            RhaiOperationScriptEngine::new(RhaiOperationScriptConfig::default()).expect("engine"),
        );
        let service = ApplicationOperationScriptRuntimeToolService::new(
            gateway(vec![
                provider("vfs", "mounts_list", json!(["workspace"])),
                provider("task", "task_read", json!({"status": "ready"})),
            ]),
            engine,
        );

        let outcome = service
            .execute(request(
                r#"let results = ops.invoke_all([
                    #{ operation: "platform:vfs:mounts_list:v1", input: #{} },
                    #{ operation: "platform:task:task_read:v1", input: #{ mode: "overview" } }
                ]);
                #{ mounts: results[0], tasks: results[1] }"#,
            ))
            .await;

        let ProductRuntimeToolOutcome::Completed { output } = outcome else {
            panic!("unexpected outcome: {outcome:?}");
        };
        assert_eq!(
            output["value"],
            json!({
                "mounts": ["workspace"],
                "tasks": {"status": "ready"}
            })
        );
        assert_eq!(output["calls"], json!({"total": 2, "failed": []}));
        assert!(output.get("result_access").is_none());
        assert!(output.get("execution_id").is_none());
    }
}
