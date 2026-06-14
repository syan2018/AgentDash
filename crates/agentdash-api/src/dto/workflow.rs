use agentdash_domain::workflow::{
    ActivityDefinition, ActivityTransition, AgentProcedureContract, ValidationIssue,
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
}

#[derive(Debug, Deserialize)]
pub struct StartWorkflowRunRequest {
    pub lifecycle_id: Option<String>,
    pub lifecycle_key: Option<String>,
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentProcedureRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub contract: AgentProcedureContract,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentProcedureRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub contract: Option<AgentProcedureContract>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateAgentProcedureRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub contract: AgentProcedureContract,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowGraphRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    #[serde(default)]
    pub transitions: Vec<ActivityTransition>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowGraphRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub entry_activity_key: Option<String>,
    pub activities: Option<Vec<ActivityDefinition>>,
    pub transitions: Option<Vec<ActivityTransition>>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateWorkflowGraphRequest {
    pub project_id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
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
    #[serde(default)]
    pub capabilities: Option<String>,
}
