use agentdash_domain::workflow::{
    ActivityDefinition, ActivityTransition, ValidationIssue, WorkflowBindingKind, WorkflowContract,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct WorkflowValidationResponse {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowsQuery {
    pub project_id: Option<String>,
    pub binding_kind: Option<WorkflowBindingKind>,
}

#[derive(Debug, Deserialize)]
pub struct StartWorkflowRunRequest {
    pub lifecycle_id: Option<String>,
    pub lifecycle_key: Option<String>,
    pub session_id: String,
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitHumanDecisionRequest {
    pub decision_port: String,
    pub decision: serde_json::Value,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowDefinitionRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kinds: Vec<WorkflowBindingKind>,
    pub contract: WorkflowContract,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowDefinitionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub binding_kinds: Option<Vec<WorkflowBindingKind>>,
    pub contract: Option<WorkflowContract>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateWorkflowDefinitionRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kinds: Vec<WorkflowBindingKind>,
    pub contract: WorkflowContract,
}

#[derive(Debug, Deserialize)]
pub struct CreateActivityLifecycleDefinitionRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kinds: Vec<WorkflowBindingKind>,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    #[serde(default)]
    pub transitions: Vec<ActivityTransition>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateActivityLifecycleDefinitionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub binding_kinds: Option<Vec<WorkflowBindingKind>>,
    pub entry_activity_key: Option<String>,
    pub activities: Option<Vec<ActivityDefinition>>,
    pub transitions: Option<Vec<ActivityTransition>>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateActivityLifecycleDefinitionRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kinds: Vec<WorkflowBindingKind>,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    #[serde(default)]
    pub transitions: Vec<ActivityTransition>,
}

#[derive(Deserialize)]
pub struct ValidateScriptRequest {
    pub script: String,
}

#[derive(Deserialize)]
pub struct RegisterPresetRequest {
    pub key: String,
    pub script: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolCatalogQuery {
    pub capabilities: String,
}
