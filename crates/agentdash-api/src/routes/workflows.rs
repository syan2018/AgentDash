use std::{collections::HashSet, sync::Arc};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::hooks::hook_rule_preset_registry;
use agentdash_application::workflow::{
    ActivateLifecycleStepCommand, AppendLifecycleStepArtifactsCommand, AssignLifecycleCommand,
    BuiltinWorkflowTemplateBundle, CompleteLifecycleStepCommand, LifecycleOrchestrator,
    LifecycleRunService, StartLifecycleRunCommand, WorkflowCatalogService,
    WorkflowRecordArtifactDraft, build_builtin_workflow_bundle, list_builtin_workflow_templates,
};
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleEdge, LifecycleRun, LifecycleStepDefinition, ValidationSeverity,
    WorkflowAssignment, WorkflowBindingKind, WorkflowBindingRole, WorkflowContract,
    WorkflowDefinition, WorkflowDefinitionSource, WorkflowDefinitionStatus,
    WorkflowRecordArtifactType,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::WorkflowValidationResponse;
use crate::rpc::ApiError;
use agentdash_application::session::context::normalize_string;
use tracing::warn;

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
    /// 父 session ID — lifecycle run 直接关联 session。
    pub session_id: String,
    /// project_id 显式传入，因为 session 本身不直接携带 project 信息。
    pub project_id: String,
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
    #[serde(default)]
    pub edges: Vec<LifecycleEdge>,
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
    #[serde(default)]
    pub edges: Vec<LifecycleEdge>,
}

pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<WorkflowDefinition>>, ApiError> {
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
) -> Result<Json<Vec<LifecycleDefinition>>, ApiError> {
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
) -> Result<Json<LifecycleDefinition>, ApiError> {
    let mut definition = LifecycleDefinition::new(
        req.key,
        req.name,
        req.description,
        req.binding_kind,
        WorkflowDefinitionSource::UserAuthored,
        req.entry_step_key,
        req.steps,
        req.edges,
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

pub async fn list_workflow_templates() -> Result<Json<Vec<BuiltinWorkflowTemplateBundle>>, ApiError>
{
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
) -> Result<Json<LifecycleDefinition>, ApiError> {
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
) -> Result<Json<Vec<WorkflowAssignment>>, ApiError> {
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
) -> Result<Json<WorkflowAssignment>, ApiError> {
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
) -> Result<Json<LifecycleRun>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
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
            session_id: req.session_id,
        })
        .await?;
    let orchestrator = LifecycleOrchestrator::new(
        state.services.session_hub.clone(),
        state.repos.session_binding_repo.clone(),
        state.repos.workflow_definition_repo.clone(),
        state.repos.lifecycle_definition_repo.clone(),
        state.repos.lifecycle_run_repo.clone(),
    );
    if let Err(error) = orchestrator
        .after_node_advanced(run.id, run.project_id)
        .await
    {
        warn!(
            run_id = %run.id,
            project_id = %run.project_id,
            error = %error,
            "start_lifecycle_run 已创建 run，但触发首批 node 编排失败"
        );
    }

    let latest_run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run.id)
        .await?
        .unwrap_or(run);
    Ok(Json(latest_run.into()))
}

pub async fn get_lifecycle_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
) -> Result<Json<LifecycleRun>, ApiError> {
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

/// 按 session_id 查询关联的 lifecycle runs。
pub async fn list_lifecycle_runs_by_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<LifecycleRun>>, ApiError> {
    let runs = state
        .repos
        .lifecycle_run_repo
        .list_by_session(&session_id)
        .await?;
    let mut checked_projects = HashSet::new();
    for run in &runs {
        if checked_projects.insert(run.project_id) {
            load_project_with_permission(
                state.as_ref(),
                &current_user,
                run.project_id,
                ProjectPermission::View,
            )
            .await?;
        }
    }
    Ok(Json(runs.into_iter().map(Into::into).collect()))
}

pub async fn activate_workflow_step(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, step_key)): Path<(String, String)>,
) -> Result<Json<LifecycleRun>, ApiError> {
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
) -> Result<Json<LifecycleRun>, ApiError> {
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
) -> Result<Json<LifecycleRun>, ApiError> {
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
        .ok_or_else(|| ApiError::BadRequest("record_artifacts 不能为空".to_string()))?
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
) -> Result<Json<WorkflowDefinition>, ApiError> {
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
) -> Result<Json<WorkflowDefinition>, ApiError> {
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
) -> Result<Json<LifecycleDefinition>, ApiError> {
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
) -> Result<Json<WorkflowDefinition>, ApiError> {
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
) -> Result<Json<LifecycleDefinition>, ApiError> {
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
        req.edges,
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
) -> Result<Json<WorkflowDefinition>, ApiError> {
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
) -> Result<Json<LifecycleDefinition>, ApiError> {
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
) -> Result<Json<WorkflowDefinition>, ApiError> {
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
) -> Result<Json<LifecycleDefinition>, ApiError> {
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

impl From<WorkflowRecordArtifactDraftRequest> for WorkflowRecordArtifactDraft {
    fn from(value: WorkflowRecordArtifactDraftRequest) -> Self {
        Self {
            artifact_type: value.artifact_type,
            title: value.title,
            content: value.content,
        }
    }
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

pub async fn list_hook_presets() -> Result<Json<serde_json::Value>, ApiError> {
    let presets = hook_rule_preset_registry();
    let grouped = group_presets_by_trigger(presets)?;
    Ok(Json(serde_json::json!({ "presets": grouped })))
}

fn group_presets_by_trigger(
    presets: &[agentdash_application::hooks::HookRulePreset],
) -> Result<serde_json::Value, ApiError> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for preset in presets {
        let trigger_key = serde_json::to_value(&preset.trigger)
            .map_err(|error| {
                ApiError::Internal(format!(
                    "序列化 hook preset trigger 失败: key={}, error={error}",
                    preset.key
                ))
            })?
            .as_str()
            .map(ToString::to_string)
            .ok_or_else(|| {
                ApiError::Internal(format!(
                    "hook preset trigger 不是字符串: key={}",
                    preset.key
                ))
            })?;
        groups
            .entry(trigger_key)
            .or_default()
            .push(serde_json::json!({
                "key": preset.key,
                "trigger": preset.trigger,
                "label": preset.label,
                "description": preset.description,
                "param_schema": preset.param_schema,
                "script": preset.script,
                "source": preset.source,
            }));
    }
    serde_json::to_value(groups)
        .map_err(|error| ApiError::Internal(format!("序列化 hook preset 分组失败: {error}")))
}

#[derive(Deserialize)]
pub struct ValidateScriptRequest {
    pub script: String,
}

pub async fn validate_hook_script(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateScriptRequest>,
) -> Json<serde_json::Value> {
    match state.services.hook_provider.validate_script(&req.script) {
        Ok(()) => Json(serde_json::json!({ "valid": true })),
        Err(errors) => Json(serde_json::json!({ "valid": false, "errors": errors })),
    }
}

#[derive(Deserialize)]
pub struct RegisterPresetRequest {
    pub key: String,
    pub script: String,
}

pub async fn register_hook_preset(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterPresetRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .services
        .hook_provider
        .register_preset(&req.key, &req.script)
        .map_err(|e| ApiError::BadRequest(format!("脚本编译失败: {e}")))?;
    Ok(Json(
        serde_json::json!({ "registered": true, "key": req.key }),
    ))
}

pub async fn delete_hook_preset(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.services.hook_provider.remove_preset(&key);
    Json(serde_json::json!({ "removed": removed, "key": key }))
}
