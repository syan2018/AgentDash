use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::workflow::{
    ActivateLifecycleStepCommand, AppendLifecycleStepArtifactsCommand, AssignLifecycleCommand,
    CompleteLifecycleStepCommand, LifecycleRunService, StartLifecycleRunCommand,
    WorkflowCatalogService, WorkflowRecordArtifactDraft, build_builtin_workflow_bundle,
    list_builtin_workflow_templates,
};
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, ValidationSeverity,
    WorkflowBindingKind, WorkflowBindingRole, WorkflowContextBindingKind, WorkflowContract,
    WorkflowDefinition, WorkflowDefinitionSource, WorkflowDefinitionStatus,
    WorkflowRecordArtifactType,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    LifecycleDefinitionResponse, WorkflowAssignmentResponse, WorkflowDefinitionResponse,
    WorkflowRunResponse, WorkflowTemplateResponse, WorkflowValidationResponse,
};
use crate::rpc::ApiError;
use agentdash_application::session_context::normalize_string;

#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowsQuery {
    pub binding_kind: Option<WorkflowBindingKind>,
    pub enabled_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowAssignmentRequest {
    pub lifecycle_id: String,
    pub role: WorkflowBindingRole,
    pub enabled: Option<bool>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartWorkflowRunRequest {
    pub lifecycle_id: Option<String>,
    pub lifecycle_key: Option<String>,
    pub binding_kind: WorkflowBindingKind,
    pub binding_id: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct CompleteWorkflowStepRequest {
    pub summary: Option<String>,
    pub record_artifacts: Option<Vec<WorkflowRecordArtifactDraftRequest>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct AppendWorkflowStepArtifactsRequest {
    pub record_artifacts: Option<Vec<WorkflowRecordArtifactDraftRequest>>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowRecordArtifactDraftRequest {
    pub artifact_type: WorkflowRecordArtifactType,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowDefinitionRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub contract: WorkflowContract,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowDefinitionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub recommended_binding_roles: Option<Vec<WorkflowBindingRole>>,
    pub contract: Option<WorkflowContract>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateWorkflowDefinitionRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub contract: WorkflowContract,
}

#[derive(Debug, Deserialize)]
pub struct CreateLifecycleDefinitionRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub entry_step_key: String,
    pub steps: Vec<LifecycleStepDefinition>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLifecycleDefinitionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub recommended_binding_roles: Option<Vec<WorkflowBindingRole>>,
    pub entry_step_key: Option<String>,
    pub steps: Option<Vec<LifecycleStepDefinition>>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateLifecycleDefinitionRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub entry_step_key: String,
    pub steps: Vec<LifecycleStepDefinition>,
}

pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<WorkflowDefinitionResponse>>, ApiError> {
    let definitions = match (query.binding_kind, query.enabled_only.unwrap_or(false)) {
        (Some(binding_kind), _) => {
            state
                .repos
                .workflow_definition_repo
                .list_by_binding_kind(binding_kind)
                .await?
        }
        (None, true) => {
            state
                .repos
                .workflow_definition_repo
                .list_by_status(WorkflowDefinitionStatus::Active)
                .await?
        }
        (None, false) => state.repos.workflow_definition_repo.list_all().await?,
    };
    Ok(Json(definitions.into_iter().map(Into::into).collect()))
}

pub async fn list_lifecycles(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<LifecycleDefinitionResponse>>, ApiError> {
    let definitions = match (query.binding_kind, query.enabled_only.unwrap_or(false)) {
        (Some(binding_kind), _) => {
            state
                .repos
                .lifecycle_definition_repo
                .list_by_binding_kind(binding_kind)
                .await?
        }
        (None, true) => {
            state
                .repos
                .lifecycle_definition_repo
                .list_by_status(WorkflowDefinitionStatus::Active)
                .await?
        }
        (None, false) => state.repos.lifecycle_definition_repo.list_all().await?,
    };
    Ok(Json(definitions.into_iter().map(Into::into).collect()))
}

pub async fn create_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLifecycleDefinitionRequest>,
) -> Result<Json<LifecycleDefinitionResponse>, ApiError> {
    let mut definition = LifecycleDefinition::new(
        req.key,
        req.name,
        req.description,
        req.binding_kind,
        WorkflowDefinitionSource::UserAuthored,
        req.entry_step_key,
        req.steps,
    )
    .map_err(ApiError::BadRequest)?;
    definition.recommended_binding_roles = req.recommended_binding_roles;
    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let saved = service.upsert_lifecycle_definition(definition).await?;
    Ok(Json(saved.into()))
}

pub async fn list_workflow_templates() -> Result<Json<Vec<WorkflowTemplateResponse>>, ApiError> {
    Ok(Json(
        list_builtin_workflow_templates()
            .map_err(ApiError::BadRequest)?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

pub async fn bootstrap_workflow_template(
    State(state): State<Arc<AppState>>,
    Path(builtin_key): Path<String>,
) -> Result<Json<LifecycleDefinitionResponse>, ApiError> {
    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let bundle = build_builtin_workflow_bundle(&builtin_key).map_err(ApiError::BadRequest)?;
    let saved = service.upsert_bundle(bundle).await?;
    Ok(Json(saved.lifecycle.into()))
}

pub async fn list_project_workflow_assignments(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<WorkflowAssignmentResponse>>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let assignments = state
        .repos
        .workflow_assignment_repo
        .list_by_project(project_id)
        .await?;
    Ok(Json(assignments.into_iter().map(Into::into).collect()))
}

pub async fn create_project_workflow_assignment(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateWorkflowAssignmentRequest>,
) -> Result<Json<WorkflowAssignmentResponse>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let lifecycle_id = parse_uuid_required(&req.lifecycle_id, "lifecycle_id")?;
    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let assignment = service
        .assign_to_project(AssignLifecycleCommand {
            project_id,
            lifecycle_id,
            role: req.role,
            enabled: req.enabled.unwrap_or(true),
            is_default: req.is_default.unwrap_or(false),
        })
        .await?;
    Ok(Json(assignment.into()))
}

pub async fn start_lifecycle_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<StartWorkflowRunRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let binding_id = parse_uuid_required(&req.binding_id, "binding_id")?;
    let project_id =
        resolve_project_id_for_workflow_binding(&state, req.binding_kind, binding_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let service = LifecycleRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    );
    let run = service
        .start_run(StartLifecycleRunCommand {
            project_id,
            lifecycle_id: parse_optional_uuid(req.lifecycle_id.as_deref(), "lifecycle_id")?,
            lifecycle_key: req.lifecycle_key.and_then(normalize_string),
            binding_kind: req.binding_kind,
            binding_id,
        })
        .await?;
    Ok(Json(run.into()))
}

pub async fn get_lifecycle_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let run = load_lifecycle_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(run.into()))
}

pub async fn list_lifecycle_runs_by_binding(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((binding_kind_raw, binding_id_raw)): Path<(String, String)>,
) -> Result<Json<Vec<WorkflowRunResponse>>, ApiError> {
    let binding_kind = parse_binding_kind(&binding_kind_raw)?;
    let binding_id = parse_uuid(&binding_id_raw, "binding_id")?;
    let project_id =
        resolve_project_id_for_workflow_binding(&state, binding_kind, binding_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let runs = state
        .repos
        .lifecycle_run_repo
        .list_by_binding(binding_kind, binding_id)
        .await?;
    Ok(Json(runs.into_iter().map(Into::into).collect()))
}

pub async fn activate_workflow_step(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, step_key)): Path<(String, String)>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let existing_run = load_lifecycle_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        existing_run.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let service = LifecycleRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    );
    let run = service
        .activate_step(ActivateLifecycleStepCommand { run_id, step_key })
        .await?;
    Ok(Json(run.into()))
}

pub async fn complete_workflow_step(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, step_key)): Path<(String, String)>,
    Json(req): Json<CompleteWorkflowStepRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let existing_run = load_lifecycle_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        existing_run.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let service = LifecycleRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    );
    let run = service
        .complete_step(CompleteLifecycleStepCommand {
            run_id,
            step_key,
            summary: req.summary.and_then(normalize_string),
            record_artifacts: req
                .record_artifacts
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        })
        .await?;
    Ok(Json(run.into()))
}

pub async fn append_workflow_step_artifacts(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, step_key)): Path<(String, String)>,
    Json(req): Json<AppendWorkflowStepArtifactsRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let existing_run = load_lifecycle_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        existing_run.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let artifacts = req
        .record_artifacts
        .unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    if artifacts.is_empty() {
        return Err(ApiError::BadRequest(
            "record_artifacts 不能为空".to_string(),
        ));
    }
    let service = LifecycleRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    );
    let run = service
        .append_step_artifacts(AppendLifecycleStepArtifactsCommand {
            run_id,
            step_key,
            artifacts,
        })
        .await?;
    Ok(Json(run.into()))
}

pub async fn create_workflow_definition(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateWorkflowDefinitionRequest>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let mut definition = WorkflowDefinition::new(
        req.key,
        req.name,
        req.description,
        req.binding_kind,
        WorkflowDefinitionSource::UserAuthored,
        req.contract,
    )
    .map_err(ApiError::BadRequest)?;
    definition.recommended_binding_roles = req.recommended_binding_roles;
    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let saved = service.upsert_workflow_definition(definition).await?;
    Ok(Json(saved.into()))
}

pub async fn get_workflow_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let definition = state
        .repos
        .workflow_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_definition 不存在: {id}")))?;
    Ok(Json(definition.into()))
}

pub async fn get_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<LifecycleDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "lifecycle_id")?;
    let definition = state
        .repos
        .lifecycle_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_definition 不存在: {id}")))?;
    Ok(Json(definition.into()))
}

pub async fn update_workflow_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkflowDefinitionRequest>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let mut definition = state
        .repos
        .workflow_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_definition 不存在: {id}")))?;
    if let Some(name) = req.name {
        definition.name = name;
    }
    if let Some(description) = req.description {
        definition.description = description;
    }
    if let Some(roles) = req.recommended_binding_roles {
        definition.recommended_binding_roles = roles;
    }
    if let Some(contract) = req.contract {
        definition.contract = contract;
    }
    let issues = definition.validate_full();
    if issues
        .iter()
        .any(|item| item.severity == ValidationSeverity::Error)
    {
        return Err(ApiError::BadRequest(format!(
            "校验失败: {}",
            issues
                .iter()
                .filter(|item| item.severity == ValidationSeverity::Error)
                .map(|item| format!("[{}] {}", item.field_path, item.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    definition.version += 1;
    definition.updated_at = chrono::Utc::now();
    state
        .repos
        .workflow_definition_repo
        .update(&definition)
        .await?;
    Ok(Json(definition.into()))
}

pub async fn update_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateLifecycleDefinitionRequest>,
) -> Result<Json<LifecycleDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "lifecycle_id")?;
    let mut definition = state
        .repos
        .lifecycle_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_definition 不存在: {id}")))?;
    if let Some(name) = req.name {
        definition.name = name;
    }
    if let Some(description) = req.description {
        definition.description = description;
    }
    if let Some(roles) = req.recommended_binding_roles {
        definition.recommended_binding_roles = roles;
    }
    if let Some(entry_step_key) = req.entry_step_key {
        definition.entry_step_key = entry_step_key;
    }
    if let Some(steps) = req.steps {
        definition.steps = steps;
    }
    let issues = definition.validate_full();
    if issues
        .iter()
        .any(|item| item.severity == ValidationSeverity::Error)
    {
        return Err(ApiError::BadRequest(format!(
            "校验失败: {}",
            issues
                .iter()
                .filter(|item| item.severity == ValidationSeverity::Error)
                .map(|item| format!("[{}] {}", item.field_path, item.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    definition.version += 1;
    definition.updated_at = chrono::Utc::now();
    state
        .repos
        .lifecycle_definition_repo
        .update(&definition)
        .await?;
    Ok(Json(definition.into()))
}

pub async fn validate_workflow_definition(
    Json(req): Json<ValidateWorkflowDefinitionRequest>,
) -> Result<Json<WorkflowValidationResponse>, ApiError> {
    match WorkflowDefinition::new(
        req.key,
        req.name,
        req.description,
        req.binding_kind,
        WorkflowDefinitionSource::UserAuthored,
        req.contract,
    ) {
        Ok(mut definition) => {
            definition.recommended_binding_roles = req.recommended_binding_roles;
            let issues = definition.validate_full();
            Ok(Json(WorkflowValidationResponse {
                valid: !issues
                    .iter()
                    .any(|item| item.severity == ValidationSeverity::Error),
                issues,
            }))
        }
        Err(error) => Ok(Json(WorkflowValidationResponse {
            valid: false,
            issues: vec![agentdash_domain::workflow::ValidationIssue::error(
                "construction_error",
                error,
                "contract",
            )],
        })),
    }
}

pub async fn validate_lifecycle_definition(
    Json(req): Json<ValidateLifecycleDefinitionRequest>,
) -> Result<Json<WorkflowValidationResponse>, ApiError> {
    match LifecycleDefinition::new(
        req.key,
        req.name,
        req.description,
        req.binding_kind,
        WorkflowDefinitionSource::UserAuthored,
        req.entry_step_key,
        req.steps,
    ) {
        Ok(mut definition) => {
            definition.recommended_binding_roles = req.recommended_binding_roles;
            let issues = definition.validate_full();
            Ok(Json(WorkflowValidationResponse {
                valid: !issues
                    .iter()
                    .any(|item| item.severity == ValidationSeverity::Error),
                issues,
            }))
        }
        Err(error) => Ok(Json(WorkflowValidationResponse {
            valid: false,
            issues: vec![agentdash_domain::workflow::ValidationIssue::error(
                "construction_error",
                error,
                "steps",
            )],
        })),
    }
}

pub async fn enable_workflow_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let mut definition = state
        .repos
        .workflow_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_definition 不存在: {id}")))?;
    let issues = definition.validate_full();
    if issues
        .iter()
        .any(|item| item.severity == ValidationSeverity::Error)
    {
        return Err(ApiError::BadRequest(format!(
            "definition 存在校验错误，不能激活: {}",
            issues
                .iter()
                .filter(|item| item.severity == ValidationSeverity::Error)
                .map(|item| format!("[{}] {}", item.field_path, item.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    definition.status = WorkflowDefinitionStatus::Active;
    definition.updated_at = chrono::Utc::now();
    state
        .repos
        .workflow_definition_repo
        .update(&definition)
        .await?;
    Ok(Json(definition.into()))
}

pub async fn enable_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<LifecycleDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "lifecycle_id")?;
    let mut definition = state
        .repos
        .lifecycle_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_definition 不存在: {id}")))?;
    let issues = definition.validate_full();
    if issues
        .iter()
        .any(|item| item.severity == ValidationSeverity::Error)
    {
        return Err(ApiError::BadRequest(format!(
            "definition 存在校验错误，不能激活: {}",
            issues
                .iter()
                .filter(|item| item.severity == ValidationSeverity::Error)
                .map(|item| format!("[{}] {}", item.field_path, item.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    definition.status = WorkflowDefinitionStatus::Active;
    definition.updated_at = chrono::Utc::now();
    state
        .repos
        .lifecycle_definition_repo
        .update(&definition)
        .await?;
    Ok(Json(definition.into()))
}

pub async fn disable_workflow_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let mut definition = state
        .repos
        .workflow_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_definition 不存在: {id}")))?;
    definition.status = WorkflowDefinitionStatus::Disabled;
    definition.updated_at = chrono::Utc::now();
    state
        .repos
        .workflow_definition_repo
        .update(&definition)
        .await?;
    Ok(Json(definition.into()))
}

pub async fn disable_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<LifecycleDefinitionResponse>, ApiError> {
    let id = parse_uuid(&id, "lifecycle_id")?;
    let mut definition = state
        .repos
        .lifecycle_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_definition 不存在: {id}")))?;
    definition.status = WorkflowDefinitionStatus::Disabled;
    definition.updated_at = chrono::Utc::now();
    state
        .repos
        .lifecycle_definition_repo
        .update(&definition)
        .await?;
    Ok(Json(definition.into()))
}

pub async fn delete_workflow_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    state.repos.workflow_definition_repo.delete(id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn delete_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_uuid(&id, "lifecycle_id")?;
    state.repos.lifecycle_definition_repo.delete(id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn list_binding_metadata() -> Result<Json<Vec<BindingKindMetadata>>, ApiError> {
    Ok(Json(build_binding_metadata()))
}

#[derive(Debug, Serialize)]
pub struct BindingKindMetadata {
    pub kind: WorkflowContextBindingKind,
    pub label: String,
    pub description: String,
    pub locator_options: Vec<BindingLocatorOption>,
}

#[derive(Debug, Serialize)]
pub struct BindingLocatorOption {
    pub locator: String,
    pub label: String,
    pub description: String,
    pub applicable_binding_kinds: Vec<WorkflowBindingKind>,
}

fn build_binding_metadata() -> Vec<BindingKindMetadata> {
    vec![
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::DocumentPath,
            label: "文档路径".to_string(),
            description: "从工作空间读取指定相对路径的文件内容".to_string(),
            locator_options: vec![BindingLocatorOption {
                locator: ".trellis/workflow.md".to_string(),
                label: "Workflow 文档".to_string(),
                description: "Trellis 开发工作流文档".to_string(),
                applicable_binding_kinds: vec![
                    WorkflowBindingKind::Task,
                    WorkflowBindingKind::Story,
                ],
            }],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::RuntimeContext,
            label: "运行时上下文".to_string(),
            description: "注入运行时动态上下文信息".to_string(),
            locator_options: vec![
                BindingLocatorOption {
                    locator: "project_session_context".to_string(),
                    label: "项目会话上下文".to_string(),
                    description: "项目级上下文配置快照".to_string(),
                    applicable_binding_kinds: vec![
                        WorkflowBindingKind::Project,
                        WorkflowBindingKind::Story,
                        WorkflowBindingKind::Task,
                    ],
                },
                BindingLocatorOption {
                    locator: "story_prd".to_string(),
                    label: "Story PRD".to_string(),
                    description: "Story 的产品需求文档".to_string(),
                    applicable_binding_kinds: vec![
                        WorkflowBindingKind::Story,
                        WorkflowBindingKind::Task,
                    ],
                },
                BindingLocatorOption {
                    locator: "story_context_snapshot".to_string(),
                    label: "Story 上下文快照".to_string(),
                    description: "Story 的结构化上下文".to_string(),
                    applicable_binding_kinds: vec![
                        WorkflowBindingKind::Story,
                        WorkflowBindingKind::Task,
                    ],
                },
                BindingLocatorOption {
                    locator: "task_execution_context".to_string(),
                    label: "Task 执行上下文".to_string(),
                    description: "Task 的执行配置快照".to_string(),
                    applicable_binding_kinds: vec![WorkflowBindingKind::Task],
                },
            ],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::Checklist,
            label: "检查清单".to_string(),
            description: "结构化检查清单，用于 lifecycle checks".to_string(),
            locator_options: vec![
                BindingLocatorOption {
                    locator: "task_review_checklist".to_string(),
                    label: "Task Review Checklist".to_string(),
                    description: "Task 级检查清单".to_string(),
                    applicable_binding_kinds: vec![WorkflowBindingKind::Task],
                },
                BindingLocatorOption {
                    locator: "story_review_checklist".to_string(),
                    label: "Story Review Checklist".to_string(),
                    description: "Story 级检查清单".to_string(),
                    applicable_binding_kinds: vec![WorkflowBindingKind::Story],
                },
            ],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::JournalTarget,
            label: "日志目标".to_string(),
            description: "指定记录沉淀目录".to_string(),
            locator_options: vec![BindingLocatorOption {
                locator: "workspace_journal".to_string(),
                label: "工作区日志".to_string(),
                description: "Trellis 工作区日志目录".to_string(),
                applicable_binding_kinds: vec![
                    WorkflowBindingKind::Task,
                    WorkflowBindingKind::Story,
                    WorkflowBindingKind::Project,
                ],
            }],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::ActionRef,
            label: "动作引用".to_string(),
            description: "引用可执行动作或建议动作".to_string(),
            locator_options: vec![BindingLocatorOption {
                locator: "workflow_archive_action".to_string(),
                label: "归档动作".to_string(),
                description: "触发 workflow 任务归档".to_string(),
                applicable_binding_kinds: vec![WorkflowBindingKind::Task],
            }],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::ArtifactRef,
            label: "产物引用".to_string(),
            description: "引用上游已生成的记录产物".to_string(),
            locator_options: vec![BindingLocatorOption {
                locator: "latest_checklist_evidence".to_string(),
                label: "最新检查证据".to_string(),
                description: "最近一次 checklist evidence".to_string(),
                applicable_binding_kinds: vec![
                    WorkflowBindingKind::Task,
                    WorkflowBindingKind::Story,
                ],
            }],
        },
    ]
}

impl From<WorkflowRecordArtifactDraftRequest> for WorkflowRecordArtifactDraft {
    fn from(value: WorkflowRecordArtifactDraftRequest) -> Self {
        Self {
            artifact_type: value.artifact_type,
            title: value.title,
            content: value.content,
        }
    }
}

async fn resolve_project_id_for_workflow_binding(
    state: &Arc<AppState>,
    binding_kind: WorkflowBindingKind,
    binding_id: Uuid,
) -> Result<Uuid, ApiError> {
    let project_id = match binding_kind {
        WorkflowBindingKind::Project => state
            .repos
            .project_repo
            .get_by_id(binding_id)
            .await?
            .map(|project| project.id),
        WorkflowBindingKind::Story => state
            .repos
            .story_repo
            .get_by_id(binding_id)
            .await?
            .map(|story| story.project_id),
        WorkflowBindingKind::Task => state
            .repos
            .task_repo
            .get_by_id(binding_id)
            .await?
            .map(|task| task.project_id),
    };
    project_id.ok_or_else(|| {
        ApiError::NotFound(format!(
            "workflow 绑定对象不存在: kind={binding_kind:?}, id={binding_id}"
        ))
    })
}

async fn load_lifecycle_run(state: &Arc<AppState>, run_id: Uuid) -> Result<LifecycleRun, ApiError> {
    state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}

fn parse_uuid_required(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    parse_uuid(trimmed, field)
}

fn parse_optional_uuid(raw: Option<&str>, field: &str) -> Result<Option<Uuid>, ApiError> {
    match raw.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }) {
        Some(value) => parse_uuid(value, field).map(Some),
        None => Ok(None),
    }
}

fn parse_binding_kind(raw: &str) -> Result<WorkflowBindingKind, ApiError> {
    WorkflowBindingKind::from_binding_scope(raw)
        .ok_or_else(|| ApiError::BadRequest(format!("无效的 binding_kind: {raw}")))
}
