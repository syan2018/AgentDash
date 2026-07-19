use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use async_trait::async_trait;

use crate::lifecycle::{
    AdvanceCurrentNodeResult, AdvanceCurrentNodeStatus, AdvanceCurrentRuntimeThreadActivityInput,
    LifecycleNodeAdvanceOutcome, LifecycleOrchestrator,
};

use super::advance_node::{CompleteLifecycleNodeParams, StepOutcome};

#[async_trait]
impl ProductRuntimeToolService for LifecycleOrchestrator {
    fn kind(&self) -> ProductRuntimeToolKind {
        ProductRuntimeToolKind::CompleteLifecycleNode
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(CompleteLifecycleNodeParams))
            .unwrap_or_else(|_| serde_json::json!({ "type": "object" }))
    }

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        let parameters: CompleteLifecycleNodeParams =
            match serde_json::from_value(request.arguments) {
                Ok(value) => value,
                Err(error) => {
                    return ProductRuntimeToolOutcome::Rejected {
                        code: "invalid_complete_lifecycle_node_arguments".to_owned(),
                        message: error.to_string(),
                    };
                }
            };
        let outcome = match parameters.outcome {
            StepOutcome::Completed => LifecycleNodeAdvanceOutcome::Completed,
            StepOutcome::Failed => LifecycleNodeAdvanceOutcome::Failed,
        };
        match self
            .advance_current_runtime_thread_activity(AdvanceCurrentRuntimeThreadActivityInput {
                runtime_thread_id: request.context.runtime_thread_id.to_string(),
                project_id: request.context.target.project_id,
                run_id: request.context.target.run_id,
                agent_id: request.context.target.agent_id,
                outcome,
                summary: parameters.summary,
            })
            .await
        {
            Ok(result) => ProductRuntimeToolOutcome::Completed {
                output: result_output(result),
            },
            Err(message) => ProductRuntimeToolOutcome::Failed {
                code: "complete_lifecycle_node_failed".to_owned(),
                message,
            },
        }
    }
}

fn result_output(result: AdvanceCurrentNodeResult) -> serde_json::Value {
    let common = serde_json::json!({
        "run_id": result.run.id,
        "orchestration_id": result.orchestration_id,
        "node_path": result.node_path,
        "run_status": format!("{:?}", result.run.status),
        "orchestration_warning": result.orchestration_warning,
    });
    let mut output = common
        .as_object()
        .cloned()
        .expect("lifecycle result object");
    match result.status {
        AdvanceCurrentNodeStatus::Completed => {
            output.insert("status".to_owned(), serde_json::json!("completed"));
        }
        AdvanceCurrentNodeStatus::Failed => {
            output.insert("status".to_owned(), serde_json::json!("failed"));
        }
        AdvanceCurrentNodeStatus::GateRejected {
            gate_collision_count,
            missing_output_keys,
            terminal_failed,
        } => {
            output.insert("status".to_owned(), serde_json::json!("gate_rejected"));
            output.insert(
                "gate_collision_count".to_owned(),
                serde_json::json!(gate_collision_count),
            );
            output.insert(
                "missing_ports".to_owned(),
                serde_json::json!(missing_output_keys),
            );
            output.insert(
                "terminal_failed".to_owned(),
                serde_json::json!(terminal_failed),
            );
        }
    }
    serde_json::Value::Object(output)
}
