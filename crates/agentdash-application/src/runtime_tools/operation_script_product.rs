use std::sync::Arc;

use agentdash_application_operation_gateway::{
    AgentRunOperationHost, HostOperationScriptOperationSet, HostOperationScriptProgram,
    OperationGateway,
};
use agentdash_application_ports::operation_script::{
    OPERATION_SCRIPT_HOST_API_V1, OperationScriptEngine, OperationScriptError,
    OperationScriptLimits, RHAI_V1_DIALECT,
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
        let cancel = CancellationToken::new();
        let preflight = host
            .preflight(program.clone(), cancel.clone())
            .await
            .map_err(map_script_error)?;
        let result = host
            .run(program, preflight.token, cancel)
            .await
            .and_then(serialize_result);
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
        OperationScriptError::InvalidPlan { .. } => "operation_script_invalid_plan".to_string(),
        OperationScriptError::TokenExpired => "operation_script_token_expired".to_string(),
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
    fn schema_exposes_only_source_and_input() {
        let schema = operation_script_runtime_tool_schema();

        let properties = schema["properties"].as_object().expect("properties");
        let mut property_names = properties.keys().map(String::as_str).collect::<Vec<_>>();
        property_names.sort_unstable();
        assert_eq!(property_names, vec!["input", "source"]);
        assert_eq!(schema["required"], json!(["source"]));
    }
}
