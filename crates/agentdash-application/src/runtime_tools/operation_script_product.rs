use std::sync::Arc;

use agentdash_application_operation_gateway::{
    AgentRunOperationHost, HostOperationScriptProgram, OperationGateway,
};
use agentdash_application_ports::operation_script::{
    OPERATION_SCRIPT_HOST_API_V1, OperationScriptEngine, OperationScriptError,
    OperationScriptLimits, OperationScriptPreflightToken, RHAI_V1_DIALECT,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use agentdash_contracts::workspace_module::WorkspaceModuleOperationRef;
use agentdash_domain::operation::OperationRef;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationScriptArguments {
    #[serde(default = "default_language")]
    language: String,
    #[serde(default = "default_host_api_version")]
    host_api_version: u16,
    source: String,
    #[serde(default)]
    input: Value,
    requested_operations: Vec<WorkspaceModuleOperationRef>,
    #[serde(default)]
    limits: OperationScriptLimits,
    #[serde(default)]
    token: Option<OperationScriptPreflightToken>,
}

fn default_language() -> String {
    RHAI_V1_DIALECT.to_owned()
}

fn default_host_api_version() -> u16 {
    OPERATION_SCRIPT_HOST_API_V1
}

pub fn operation_script_runtime_tool_schema(kind: ProductRuntimeToolKind) -> Value {
    let mut properties = serde_json::Map::from_iter([
        (
            "language".to_owned(),
            json!({
                "type": "string",
                "enum": [RHAI_V1_DIALECT],
                "default": RHAI_V1_DIALECT
            }),
        ),
        (
            "host_api_version".to_owned(),
            json!({
                "type": "integer",
                "enum": [OPERATION_SCRIPT_HOST_API_V1],
                "default": OPERATION_SCRIPT_HOST_API_V1
            }),
        ),
        (
            "source".to_owned(),
            json!({
                "type": "string",
                "description": "Ephemeral Rhai source executed by the trusted OperationScript host."
            }),
        ),
        ("input".to_owned(), json!({})),
        (
            "requested_operations".to_owned(),
            json!({
                "type": "array",
                "minItems": 1,
                "items": operation_ref_schema(),
                "description": "Exact OperationRefs copied from current workspace_module_describe results."
            }),
        ),
        (
            "limits".to_owned(),
            json!({
                "type": "object",
                "description": "Optional bounded execution limits. Omit to use server defaults.",
                "properties": {
                    "timeout_ms": {"type": "integer", "minimum": 1},
                    "max_source_bytes": {"type": "integer", "minimum": 1},
                    "max_input_bytes": {"type": "integer", "minimum": 1},
                    "max_output_bytes": {"type": "integer", "minimum": 1},
                    "max_rhai_operations": {"type": "integer", "minimum": 1},
                    "max_call_levels": {"type": "integer", "minimum": 1},
                    "max_string_size": {"type": "integer", "minimum": 1},
                    "max_array_size": {"type": "integer", "minimum": 1},
                    "max_map_size": {"type": "integer", "minimum": 1},
                    "max_operation_calls": {"type": "integer", "minimum": 1},
                    "max_parallel_operations": {"type": "integer", "minimum": 1}
                },
                "required": [
                    "timeout_ms",
                    "max_source_bytes",
                    "max_input_bytes",
                    "max_output_bytes",
                    "max_rhai_operations",
                    "max_call_levels",
                    "max_string_size",
                    "max_array_size",
                    "max_map_size",
                    "max_operation_calls",
                    "max_parallel_operations"
                ],
                "additionalProperties": false
            }),
        ),
    ]);
    let mut required = vec!["source", "requested_operations"];
    if kind == ProductRuntimeToolKind::OperationScriptRun {
        properties.insert(
            "token".to_owned(),
            json!({
                "type": "object",
                "description": "Unmodified token returned by operation_script_preflight.",
                "properties": {
                    "plan_id": {"type": "string", "format": "uuid"},
                    "binding_digest": {"type": "string"},
                    "issued_at": {"type": "string", "format": "date-time"},
                    "expires_at": {"type": "string", "format": "date-time"},
                    "signature": {"type": "string"}
                },
                "required": [
                    "plan_id",
                    "binding_digest",
                    "issued_at",
                    "expires_at",
                    "signature"
                ],
                "additionalProperties": false
            }),
        );
        required.push("token");
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn operation_ref_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "namespace": {"type": "string"},
            "provider_key": {"type": "string"},
            "operation_key": {"type": "string"},
            "contract_version": {"type": "integer", "minimum": 1}
        },
        "required": [
            "namespace",
            "provider_key",
            "operation_key",
            "contract_version"
        ],
        "additionalProperties": false
    })
}

pub struct ApplicationOperationScriptRuntimeToolService {
    kind: ProductRuntimeToolKind,
    gateway: Arc<OperationGateway>,
    engine: Arc<dyn OperationScriptEngine>,
}

impl ApplicationOperationScriptRuntimeToolService {
    pub fn new(
        kind: ProductRuntimeToolKind,
        gateway: Arc<OperationGateway>,
        engine: Arc<dyn OperationScriptEngine>,
    ) -> Self {
        assert!(matches!(
            kind,
            ProductRuntimeToolKind::OperationScriptPreflight
                | ProductRuntimeToolKind::OperationScriptRun
        ));
        Self {
            kind,
            gateway,
            engine,
        }
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
        let requested_operations = arguments
            .requested_operations
            .into_iter()
            .map(|operation_ref| {
                OperationRef::new(
                    operation_ref.namespace,
                    operation_ref.provider_key,
                    operation_ref.operation_key,
                    operation_ref.contract_version,
                )
                .map_err(|error| {
                    rejected("operation_script_invalid_operation_ref", error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let program = HostOperationScriptProgram {
            language: arguments.language,
            host_api_version: arguments.host_api_version,
            source: arguments.source,
            input: arguments.input,
            requested_operations,
            limits: arguments.limits,
        };
        let host = AgentRunOperationHost::project(
            self.gateway.clone(),
            request.context.target.run_id,
            request.context.target.agent_id,
            request.context.target.project_id,
        )
        .map_err(|error| failed("operation_script_host_binding_failed", error.to_string()))?
        .operation_script(self.engine.clone());
        let result = match self.kind {
            ProductRuntimeToolKind::OperationScriptPreflight => host
                .preflight(program, CancellationToken::new())
                .await
                .and_then(serialize_result),
            ProductRuntimeToolKind::OperationScriptRun => {
                let token = arguments.token.ok_or_else(|| {
                    rejected(
                        "operation_script_token_required",
                        "operation_script_run requires the preflight token",
                    )
                })?;
                host.run(program, token, CancellationToken::new())
                    .await
                    .and_then(serialize_result)
            }
            _ => unreachable!("constructor restricts OperationScript tool kinds"),
        };
        result.map_err(map_script_error)
    }
}

fn serialize_result<T: serde::Serialize>(result: T) -> Result<Value, OperationScriptError> {
    serde_json::to_value(result).map_err(|_| OperationScriptError::Internal {
        code: "result_serialization_failed",
    })
}

#[async_trait]
impl ProductRuntimeToolService for ApplicationOperationScriptRuntimeToolService {
    fn kind(&self) -> ProductRuntimeToolKind {
        self.kind
    }

    fn parameters_schema(&self) -> Value {
        operation_script_runtime_tool_schema(self.kind)
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
        OperationScriptError::InvalidRequest { .. } => "operation_script_invalid_request",
        OperationScriptError::InvalidPlan { .. } => "operation_script_invalid_plan",
        OperationScriptError::TokenExpired => "operation_script_token_expired",
        OperationScriptError::OperationDenied { .. } => "operation_script_operation_denied",
        OperationScriptError::Compile { .. } => "operation_script_compile_failed",
        OperationScriptError::CapacityExceeded => "operation_script_capacity_exceeded",
        OperationScriptError::Cancelled => "operation_script_cancelled",
        OperationScriptError::DeadlineExceeded => "operation_script_deadline_exceeded",
        OperationScriptError::ExecutionInterrupted { .. } => "operation_script_interrupted",
        OperationScriptError::Runtime { .. } => "operation_script_runtime_failed",
        OperationScriptError::CallLimitExceeded { .. } => "operation_script_call_limit_exceeded",
        OperationScriptError::ParallelLimitExceeded { .. } => {
            "operation_script_parallel_limit_exceeded"
        }
        OperationScriptError::OutputLimitExceeded { .. } => {
            "operation_script_output_limit_exceeded"
        }
        OperationScriptError::NestedOperation { .. } => "operation_script_nested_operation_failed",
        OperationScriptError::ExecutionFailed { .. } => "operation_script_execution_failed",
        OperationScriptError::Internal { .. } => "operation_script_internal",
    };
    match error {
        OperationScriptError::InvalidRequest { .. }
        | OperationScriptError::InvalidPlan { .. }
        | OperationScriptError::TokenExpired
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
    use super::*;

    #[test]
    fn run_schema_requires_preflight_token_and_exact_operation_refs() {
        let preflight =
            operation_script_runtime_tool_schema(ProductRuntimeToolKind::OperationScriptPreflight);
        let run = operation_script_runtime_tool_schema(ProductRuntimeToolKind::OperationScriptRun);

        assert!(
            !preflight["required"]
                .as_array()
                .expect("required")
                .iter()
                .any(|field| field == "token")
        );
        assert!(
            run["required"]
                .as_array()
                .expect("required")
                .iter()
                .any(|field| field == "token")
        );
        assert_eq!(
            run["properties"]["requested_operations"]["items"]["additionalProperties"],
            false
        );
    }
}
