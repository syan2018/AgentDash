use agentdash_domain::workflow::ValidationIssue;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WorkflowValidationResponse {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}
