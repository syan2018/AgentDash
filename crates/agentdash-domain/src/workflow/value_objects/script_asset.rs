use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::orchestration::{OrchestrationLimits, OrchestrationSourceRef};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunScriptArtifactStatus {
    Draft,
    Preflighted,
    Approved,
    Rejected,
    Compiled,
    Launched,
}

impl RunScriptArtifactStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Preflighted)
                | (Self::Draft, Self::Rejected)
                | (Self::Preflighted, Self::Draft)
                | (Self::Preflighted, Self::Approved)
                | (Self::Preflighted, Self::Rejected)
                | (Self::Approved, Self::Compiled)
                | (Self::Approved, Self::Rejected)
                | (Self::Compiled, Self::Launched)
        ) || self == next
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowScriptDefinitionStatus {
    Draft,
    Published,
    Archived,
}

impl WorkflowScriptDefinitionStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Published)
                | (Self::Draft, Self::Archived)
                | (Self::Published, Self::Draft)
                | (Self::Published, Self::Archived)
        ) || self == next
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowScriptProvenanceSource {
    ModelGenerated,
    UserAuthored,
    Imported,
    SavedFromRunArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowScriptProvenance {
    pub source: WorkflowScriptProvenanceSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by_agent_run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<DateTime<Utc>>,
}

impl WorkflowScriptProvenance {
    pub fn new(source: WorkflowScriptProvenanceSource) -> Self {
        let now = Utc::now();
        Self {
            source,
            created_by: None,
            generated_by_agent_run_id: None,
            edited_by: None,
            approved_by: None,
            runtime_session_id: None,
            created_at: now,
            updated_at: now,
            approved_at: None,
        }
    }

    pub fn mark_updated(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn mark_approved(&mut self, approved_by: Option<String>) {
        let now = Utc::now();
        self.approved_by = approved_by;
        self.approved_at = Some(now);
        self.updated_at = now;
    }
}

impl Default for WorkflowScriptProvenance {
    fn default() -> Self {
        Self::new(WorkflowScriptProvenanceSource::UserAuthored)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowScriptCapabilitySummary {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_procedure_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_api_endpoints: Vec<WorkflowScriptApiEndpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_effect_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bash_commands: Vec<WorkflowScriptBashCommand>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub human_gates: Vec<WorkflowScriptHumanGateCapability>,
}

impl WorkflowScriptCapabilitySummary {
    pub fn is_empty(&self) -> bool {
        self.agent_procedure_keys.is_empty()
            && self.function_api_endpoints.is_empty()
            && self.local_effect_capabilities.is_empty()
            && self.bash_commands.is_empty()
            && self.human_gates.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkflowScriptApiEndpoint {
    pub method: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkflowScriptBashCommand {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkflowScriptHumanGateCapability {
    pub name: String,
    pub form_schema: String,
    pub decision_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunScriptArtifact {
    pub artifact_id: Uuid,
    pub lifecycle_run_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
    pub status: RunScriptArtifactStatus,
    pub revision: i32,
    pub source_text: String,
    pub source_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default)]
    pub limits: OrchestrationLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_document: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "WorkflowScriptCapabilitySummary::is_empty"
    )]
    pub capability_summary: WorkflowScriptCapabilitySummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiled_plan_digest: Option<String>,
    pub provenance: WorkflowScriptProvenance,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RunScriptArtifact {
    pub fn new_draft(
        lifecycle_run_id: Uuid,
        source_text: impl Into<String>,
        args_schema: Option<Value>,
        args: Option<Value>,
        limits: OrchestrationLimits,
        provenance: WorkflowScriptProvenance,
    ) -> Self {
        let now = Utc::now();
        let source_text = source_text.into();
        Self {
            artifact_id: Uuid::new_v4(),
            lifecycle_run_id,
            runtime_session_id: provenance.runtime_session_id.clone(),
            status: RunScriptArtifactStatus::Draft,
            revision: 1,
            source_digest: workflow_script_source_digest(&source_text),
            source_text,
            args_schema,
            args,
            limits,
            builder_document: None,
            capability_summary: WorkflowScriptCapabilitySummary::default(),
            compiled_plan_digest: None,
            provenance,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn replace_source(&mut self, source_text: impl Into<String>) {
        self.source_text = source_text.into();
        self.source_digest = workflow_script_source_digest(&self.source_text);
        self.revision += 1;
        self.status = RunScriptArtifactStatus::Draft;
        self.builder_document = None;
        self.capability_summary = WorkflowScriptCapabilitySummary::default();
        self.compiled_plan_digest = None;
        self.touch();
    }

    pub fn record_preflight(
        &mut self,
        builder_document: Value,
        capability_summary: WorkflowScriptCapabilitySummary,
    ) -> bool {
        if !self.transition_to(RunScriptArtifactStatus::Preflighted) {
            return false;
        }
        self.builder_document = Some(builder_document);
        self.capability_summary = capability_summary;
        true
    }

    pub fn approve(&mut self, approved_by: Option<String>) -> bool {
        if !self.transition_to(RunScriptArtifactStatus::Approved) {
            return false;
        }
        self.provenance.mark_approved(approved_by);
        self.updated_at = self.provenance.updated_at;
        true
    }

    pub fn record_compiled_plan(&mut self, compiled_plan_digest: impl Into<String>) -> bool {
        if !self.transition_to(RunScriptArtifactStatus::Compiled) {
            return false;
        }
        self.compiled_plan_digest = Some(compiled_plan_digest.into());
        true
    }

    pub fn transition_to(&mut self, next: RunScriptArtifactStatus) -> bool {
        if !self.status.can_transition_to(next) {
            return false;
        }
        self.status = next;
        self.touch();
        true
    }

    pub fn orchestration_source_ref(&self) -> OrchestrationSourceRef {
        OrchestrationSourceRef::RunScriptArtifact {
            artifact_id: self.artifact_id,
            revision: self.revision,
            source_digest: self.source_digest.clone(),
        }
    }

    fn touch(&mut self) {
        let now = Utc::now();
        self.updated_at = now;
        self.provenance.updated_at = now;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowScriptDefinitionScope {
    Project { project_id: Uuid },
    Library { library_asset_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowScriptDefinition {
    pub definition_id: Uuid,
    pub scope: WorkflowScriptDefinitionScope,
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: WorkflowScriptDefinitionStatus,
    pub revision: i32,
    pub source_text: String,
    pub source_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default)]
    pub limits: OrchestrationLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_document: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "WorkflowScriptCapabilitySummary::is_empty"
    )]
    pub capability_summary: WorkflowScriptCapabilitySummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiled_plan_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<Value>,
    pub provenance: WorkflowScriptProvenance,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowScriptDefinition {
    pub fn new_project(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        source_text: impl Into<String>,
        provenance: WorkflowScriptProvenance,
    ) -> Self {
        Self::new(
            WorkflowScriptDefinitionScope::Project { project_id },
            key,
            name,
            source_text,
            provenance,
        )
    }

    pub fn new_library(
        library_asset_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        source_text: impl Into<String>,
        provenance: WorkflowScriptProvenance,
    ) -> Self {
        Self::new(
            WorkflowScriptDefinitionScope::Library { library_asset_id },
            key,
            name,
            source_text,
            provenance,
        )
    }

    pub fn replace_source(&mut self, source_text: impl Into<String>) {
        self.source_text = source_text.into();
        self.source_digest = workflow_script_source_digest(&self.source_text);
        self.revision += 1;
        self.status = WorkflowScriptDefinitionStatus::Draft;
        self.builder_document = None;
        self.capability_summary = WorkflowScriptCapabilitySummary::default();
        self.compiled_plan_digest = None;
        self.touch();
    }

    pub fn record_preflight(
        &mut self,
        builder_document: Value,
        capability_summary: WorkflowScriptCapabilitySummary,
        compiled_plan_digest: Option<String>,
    ) {
        self.builder_document = Some(builder_document);
        self.capability_summary = capability_summary;
        self.compiled_plan_digest = compiled_plan_digest;
        self.touch();
    }

    pub fn transition_to(&mut self, next: WorkflowScriptDefinitionStatus) -> bool {
        if !self.status.can_transition_to(next) {
            return false;
        }
        self.status = next;
        self.touch();
        true
    }

    pub fn orchestration_source_ref(&self) -> OrchestrationSourceRef {
        OrchestrationSourceRef::WorkflowScript {
            script_id: self.definition_id,
            version: self.revision,
        }
    }

    fn new(
        scope: WorkflowScriptDefinitionScope,
        key: impl Into<String>,
        name: impl Into<String>,
        source_text: impl Into<String>,
        provenance: WorkflowScriptProvenance,
    ) -> Self {
        let now = Utc::now();
        let source_text = source_text.into();
        Self {
            definition_id: Uuid::new_v4(),
            scope,
            key: key.into(),
            name: name.into(),
            description: None,
            status: WorkflowScriptDefinitionStatus::Draft,
            revision: 1,
            source_digest: workflow_script_source_digest(&source_text),
            source_text,
            args_schema: None,
            args: None,
            limits: OrchestrationLimits::default(),
            builder_document: None,
            capability_summary: WorkflowScriptCapabilitySummary::default(),
            compiled_plan_digest: None,
            installed_source: None,
            provenance,
            created_at: now,
            updated_at: now,
        }
    }

    fn touch(&mut self) {
        let now = Utc::now();
        self.updated_at = now;
        self.provenance.updated_at = now;
    }
}

pub fn workflow_script_source_digest(source_text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_text.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn run_script_artifact_roundtrips_without_storage_suffix_fields() {
        let mut artifact = RunScriptArtifact::new_draft(
            Uuid::new_v4(),
            "workflow(#{ body: [] })",
            Some(json!({"topic": "string"})),
            Some(json!({"topic": "runtime"})),
            OrchestrationLimits {
                max_agent_runs: Some(4),
                ..OrchestrationLimits::default()
            },
            WorkflowScriptProvenance::new(WorkflowScriptProvenanceSource::ModelGenerated),
        );
        let summary = WorkflowScriptCapabilitySummary {
            agent_procedure_keys: vec!["researcher".to_string()],
            ..WorkflowScriptCapabilitySummary::default()
        };

        assert!(artifact.record_preflight(json!({"kind": "workflow"}), summary));
        assert!(artifact.approve(Some("user-1".to_string())));
        assert!(artifact.record_compiled_plan("sha256:compiled-plan"));

        let value = serde_json::to_value(&artifact).expect("serialize artifact");
        assert!(value.get("args_schema").is_some());
        assert!(value.get("builder_document").is_some());
        assert!(value.get("capability_summary").is_some());
        assert!(value.get("compiled_plan_digest").is_some());
        assert!(value.get("args_schema_json").is_none());
        assert!(value.get("builder_document_json").is_none());

        let restored: RunScriptArtifact =
            serde_json::from_value(value).expect("deserialize artifact");
        assert_eq!(restored, artifact);
        assert_eq!(
            restored.orchestration_source_ref(),
            OrchestrationSourceRef::RunScriptArtifact {
                artifact_id: restored.artifact_id,
                revision: restored.revision,
                source_digest: restored.source_digest.clone(),
            }
        );
    }

    #[test]
    fn workflow_script_definition_roundtrips_reusable_scope() {
        let mut definition = WorkflowScriptDefinition::new_project(
            Uuid::new_v4(),
            "research_review",
            "Research Review",
            "workflow(#{ body: [] })",
            WorkflowScriptProvenance::new(WorkflowScriptProvenanceSource::UserAuthored),
        );
        definition.description = Some("Reusable workflow script".to_string());
        definition.args_schema = Some(json!({"topic": "string"}));
        definition.args = Some(json!({"topic": "default"}));
        definition.limits.max_concurrency = Some(2);
        definition.record_preflight(
            json!({"kind": "workflow"}),
            WorkflowScriptCapabilitySummary {
                human_gates: vec![WorkflowScriptHumanGateCapability {
                    name: "approve".to_string(),
                    form_schema: "workflow.approval".to_string(),
                    decision_port: "decision".to_string(),
                }],
                ..WorkflowScriptCapabilitySummary::default()
            },
            Some("sha256:compiled-plan".to_string()),
        );
        assert!(definition.transition_to(WorkflowScriptDefinitionStatus::Published));

        let json = serde_json::to_string(&definition).expect("serialize definition");
        let restored: WorkflowScriptDefinition =
            serde_json::from_str(&json).expect("deserialize definition");
        assert_eq!(restored, definition);
        assert_eq!(
            restored.orchestration_source_ref(),
            OrchestrationSourceRef::WorkflowScript {
                script_id: restored.definition_id,
                version: restored.revision,
            }
        );
    }

    #[test]
    fn run_script_artifact_digest_and_status_transitions_are_explicit() {
        let mut artifact = RunScriptArtifact::new_draft(
            Uuid::new_v4(),
            "workflow(#{ body: [] })",
            None,
            None,
            OrchestrationLimits::default(),
            WorkflowScriptProvenance::default(),
        );
        let original_digest = artifact.source_digest.clone();

        assert!(original_digest.starts_with("sha256:"));
        assert!(!artifact.transition_to(RunScriptArtifactStatus::Compiled));
        assert_eq!(artifact.status, RunScriptArtifactStatus::Draft);
        assert!(artifact.transition_to(RunScriptArtifactStatus::Preflighted));
        assert!(artifact.transition_to(RunScriptArtifactStatus::Approved));
        assert!(artifact.record_compiled_plan("sha256:compiled-plan"));
        assert!(artifact.transition_to(RunScriptArtifactStatus::Launched));
        assert!(!artifact.transition_to(RunScriptArtifactStatus::Draft));

        artifact.replace_source("workflow(#{ name: \"changed\", body: [] })");
        assert_eq!(artifact.status, RunScriptArtifactStatus::Draft);
        assert_eq!(artifact.revision, 2);
        assert_ne!(artifact.source_digest, original_digest);
        assert!(artifact.compiled_plan_digest.is_none());
    }
}
