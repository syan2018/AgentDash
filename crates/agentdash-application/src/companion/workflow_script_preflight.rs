use std::sync::Arc;

use agentdash_application_workflow::{
    ScriptCompiler, WorkflowScriptPreflightInput, WorkflowScriptPreflightOutput,
    WorkflowScriptPreflightService,
};
use agentdash_domain::workflow::{
    OrchestrationSourceRef, WorkflowScriptProvenance, WorkflowScriptProvenanceSource,
    workflow_script_source_digest,
};
use agentdash_platform_spi::WorkflowScriptEvaluator;
use async_trait::async_trait;
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CompanionWorkflowScriptPreflightRequest {
    pub project_id: Uuid,
    pub user_id: Option<String>,
    pub source_text: String,
    pub args: Option<Value>,
    pub ctx: Option<Value>,
    pub runtime_thread_id: Option<String>,
}

#[async_trait]
pub trait CompanionWorkflowScriptPreflightPort: Send + Sync {
    async fn preflight_workflow_script(
        &self,
        request: CompanionWorkflowScriptPreflightRequest,
    ) -> Result<WorkflowScriptPreflightOutput, String>;
}

#[derive(Clone)]
pub struct ApplicationWorkflowScriptPreflightAdapter {
    evaluator: Arc<dyn WorkflowScriptEvaluator>,
}

impl ApplicationWorkflowScriptPreflightAdapter {
    pub fn new(evaluator: Arc<dyn WorkflowScriptEvaluator>) -> Self {
        Self { evaluator }
    }
}

#[async_trait]
impl CompanionWorkflowScriptPreflightPort for ApplicationWorkflowScriptPreflightAdapter {
    async fn preflight_workflow_script(
        &self,
        request: CompanionWorkflowScriptPreflightRequest,
    ) -> Result<WorkflowScriptPreflightOutput, String> {
        let source_digest = workflow_script_source_digest(&request.source_text);
        let source_ref = OrchestrationSourceRef::Inline {
            source_digest: source_digest.clone(),
        };
        let mut provenance =
            WorkflowScriptProvenance::new(WorkflowScriptProvenanceSource::UserAuthored);
        provenance.created_by = request.user_id.clone();
        provenance.runtime_thread_id = request.runtime_thread_id.clone();

        let compiler = ScriptCompiler;
        Ok(WorkflowScriptPreflightService::preflight(
            WorkflowScriptPreflightInput {
                evaluator: self.evaluator.as_ref(),
                compiler: &compiler,
                source_text: &request.source_text,
                ctx: workflow_script_eval_context(request.project_id, request.user_id, request.ctx),
                args: request.args,
                source_ref,
                provenance,
            },
        ))
    }
}

fn workflow_script_eval_context(
    project_id: Uuid,
    user_id: Option<String>,
    ctx: Option<Value>,
) -> Value {
    let mut object = match ctx {
        Some(Value::Object(object)) => object,
        Some(value) => {
            let mut object = Map::new();
            object.insert("input".to_string(), value);
            object
        }
        None => Map::new(),
    };
    object.insert("project_id".to_string(), serde_json::json!(project_id));
    if let Some(user_id) = user_id {
        object.insert("user_id".to_string(), serde_json::json!(user_id));
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_script_eval_context_overwrites_trusted_identity_fields() {
        let project_id = Uuid::new_v4();

        let ctx = workflow_script_eval_context(
            project_id,
            Some("user-1".to_string()),
            Some(serde_json::json!({
                "project_id": "model-project",
                "user_id": "model-user",
                "workspace": "demo"
            })),
        );

        assert_eq!(ctx["project_id"], serde_json::json!(project_id));
        assert_eq!(ctx["user_id"], serde_json::json!("user-1"));
        assert_eq!(ctx["workspace"], serde_json::json!("demo"));
    }
}
