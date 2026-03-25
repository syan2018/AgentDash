use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::workflow::{
    ActivateWorkflowPhaseCommand, AppendWorkflowPhaseArtifactsCommand, AssignWorkflowCommand,
    CompleteWorkflowPhaseCommand, StartWorkflowRunCommand, WorkflowCatalogService,
    WorkflowRecordArtifactDraft, WorkflowRunService, build_builtin_workflow_definition,
    list_builtin_workflow_templates,
};
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_domain::workflow::{
    ValidationSeverity, WorkflowAgentRole, WorkflowContextBinding, WorkflowContextBindingKind,
    WorkflowDefinition, WorkflowDefinitionSource, WorkflowDefinitionStatus,
    WorkflowPhaseCompletionMode, WorkflowPhaseDefinition, WorkflowRecordArtifactType,
    WorkflowRecordPolicy, WorkflowRun, WorkflowTargetKind,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    WorkflowAssignmentResponse, WorkflowDefinitionResponse, WorkflowRunResponse,
    WorkflowTemplateResponse, WorkflowValidationResponse,
};
use crate::rpc::ApiError;
use crate::session_context::normalize_string;

#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowsQuery {
    pub target_kind: Option<WorkflowTargetKind>,
    pub enabled_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowAssignmentRequest {
    pub workflow_id: String,
    pub role: WorkflowAgentRole,
    pub enabled: Option<bool>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartWorkflowRunRequest {
    pub workflow_id: Option<String>,
    pub workflow_key: Option<String>,
    pub target_kind: WorkflowTargetKind,
    pub target_id: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ActivateWorkflowPhaseRequest {
    pub session_binding_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CompleteWorkflowPhaseRequest {
    pub summary: Option<String>,
    pub record_artifacts: Option<Vec<WorkflowRecordArtifactDraftRequest>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct AppendWorkflowPhaseArtifactsRequest {
    pub record_artifacts: Option<Vec<WorkflowRecordArtifactDraftRequest>>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowRecordArtifactDraftRequest {
    pub artifact_type: WorkflowRecordArtifactType,
    pub title: String,
    pub content: String,
}

pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<WorkflowDefinitionResponse>>, ApiError> {
    let definitions = match (query.target_kind, query.enabled_only.unwrap_or(false)) {
        (Some(target_kind), _) => {
            state
                .repos
                .workflow_definition_repo
                .list_by_target_kind(target_kind)
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

    Ok(Json(
        definitions
            .into_iter()
            .map(WorkflowDefinitionResponse::from)
            .collect(),
    ))
}

pub async fn list_workflow_templates() -> Result<Json<Vec<WorkflowTemplateResponse>>, ApiError> {
    let templates = list_builtin_workflow_templates().map_err(ApiError::BadRequest)?;
    Ok(Json(
        templates
            .into_iter()
            .map(WorkflowTemplateResponse::from)
            .collect(),
    ))
}

pub async fn bootstrap_workflow_template(
    State(state): State<Arc<AppState>>,
    Path(builtin_key): Path<String>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let definition =
        build_builtin_workflow_definition(&builtin_key).map_err(ApiError::BadRequest)?;
    let saved = service.upsert_definition(definition).await?;

    Ok(Json(WorkflowDefinitionResponse::from(saved)))
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

    Ok(Json(
        assignments
            .into_iter()
            .map(WorkflowAssignmentResponse::from)
            .collect(),
    ))
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
    let workflow_id = parse_uuid_required(&req.workflow_id, "workflow_id")?;

    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let assignment = service
        .assign_to_project(AssignWorkflowCommand {
            project_id,
            workflow_id,
            role: req.role,
            enabled: req.enabled.unwrap_or(true),
            is_default: req.is_default.unwrap_or(false),
        })
        .await?;

    Ok(Json(WorkflowAssignmentResponse::from(assignment)))
}

pub async fn start_workflow_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<StartWorkflowRunRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let target_id = parse_uuid_required(&req.target_id, "target_id")?;
    let project_id =
        resolve_project_id_for_workflow_target(&state, req.target_kind, target_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let service = WorkflowRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_run_repo.as_ref(),
    );
    let run = service
        .start_run(StartWorkflowRunCommand {
            project_id,
            workflow_id: parse_optional_uuid(req.workflow_id.as_deref(), "workflow_id")?,
            workflow_key: req.workflow_key.and_then(normalize_string),
            target_kind: req.target_kind,
            target_id,
        })
        .await?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

pub async fn get_workflow_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let run = state
        .repos
        .workflow_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_run {run_id} 不存在")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

pub async fn list_workflow_runs_by_target(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((target_kind_raw, target_id_raw)): Path<(String, String)>,
) -> Result<Json<Vec<WorkflowRunResponse>>, ApiError> {
    let target_kind = parse_target_kind(&target_kind_raw)?;
    let target_id = parse_uuid(&target_id_raw, "target_id")?;
    let project_id = resolve_project_id_for_workflow_target(&state, target_kind, target_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let runs = state
        .repos
        .workflow_run_repo
        .list_by_target(target_kind, target_id)
        .await?;

    Ok(Json(
        runs.into_iter().map(WorkflowRunResponse::from).collect(),
    ))
}

pub async fn activate_workflow_phase(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, phase_key)): Path<(String, String)>,
    Json(req): Json<ActivateWorkflowPhaseRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let existing_run = load_workflow_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        existing_run.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    if let Some(binding_id) =
        parse_optional_uuid(req.session_binding_id.as_deref(), "session_binding_id")?
    {
        ensure_session_binding_matches_run(&state, binding_id, &existing_run).await?;
    }
    let service = WorkflowRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_run_repo.as_ref(),
    );
    let run = service
        .activate_phase(ActivateWorkflowPhaseCommand {
            run_id,
            phase_key,
            session_binding_id: parse_optional_uuid(
                req.session_binding_id.as_deref(),
                "session_binding_id",
            )?,
        })
        .await?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

pub async fn complete_workflow_phase(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, phase_key)): Path<(String, String)>,
    Json(req): Json<CompleteWorkflowPhaseRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let existing_run = load_workflow_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        existing_run.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    if let Some(phase_state) = existing_run
        .phase_states
        .iter()
        .find(|item| item.phase_key == phase_key)
    {
        if let Some(binding_id) = phase_state.session_binding_id {
            ensure_session_binding_matches_run(&state, binding_id, &existing_run).await?;
        }
    }
    let service = WorkflowRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_run_repo.as_ref(),
    );
    let run = service
        .complete_phase(CompleteWorkflowPhaseCommand {
            run_id,
            phase_key,
            summary: req.summary.and_then(normalize_string),
            record_artifacts: req
                .record_artifacts
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            completed_by: Some(
                agentdash_domain::workflow::WorkflowProgressionSource::ManualOverride,
            ),
        })
        .await?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

pub async fn append_workflow_phase_artifacts(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, phase_key)): Path<(String, String)>,
    Json(req): Json<AppendWorkflowPhaseArtifactsRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let existing_run = load_workflow_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        existing_run.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    if let Some(phase_state) = existing_run
        .phase_states
        .iter()
        .find(|item| item.phase_key == phase_key)
    {
        if let Some(binding_id) = phase_state.session_binding_id {
            ensure_session_binding_matches_run(&state, binding_id, &existing_run).await?;
        }
    }
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

    let service = WorkflowRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_run_repo.as_ref(),
    );
    let run = service
        .append_phase_artifacts(AppendWorkflowPhaseArtifactsCommand {
            run_id,
            phase_key,
            artifacts,
        })
        .await?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

async fn resolve_project_id_for_workflow_target(
    state: &Arc<AppState>,
    target_kind: WorkflowTargetKind,
    target_id: Uuid,
) -> Result<Uuid, ApiError> {
    let project_id = match target_kind {
        WorkflowTargetKind::Project => state
            .repos
            .project_repo
            .get_by_id(target_id)
            .await?
            .map(|project| project.id),
        WorkflowTargetKind::Story => state
            .repos
            .story_repo
            .get_by_id(target_id)
            .await?
            .map(|story| story.project_id),
        WorkflowTargetKind::Task => state
            .repos
            .task_repo
            .get_by_id(target_id)
            .await?
            .map(|task| task.project_id),
    };

    project_id.ok_or_else(|| {
        ApiError::NotFound(format!(
            "workflow target 不存在: kind={target_kind:?}, id={target_id}"
        ))
    })
}

async fn load_workflow_run(state: &Arc<AppState>, run_id: Uuid) -> Result<WorkflowRun, ApiError> {
    state
        .repos
        .workflow_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_run 不存在: {run_id}")))
}

async fn ensure_session_binding_matches_run(
    state: &Arc<AppState>,
    binding_id: Uuid,
    run: &WorkflowRun,
) -> Result<(), ApiError> {
    let binding = state
        .repos
        .session_binding_repo
        .list_by_owner(target_owner_type(run.target_kind), run.target_id)
        .await?
        .into_iter()
        .find(|item| item.id == binding_id)
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "session_binding `{binding_id}` 不属于当前 workflow target"
            ))
        })?;

    if binding.owner_id != run.target_id || binding.owner_type != target_owner_type(run.target_kind)
    {
        return Err(ApiError::BadRequest(format!(
            "session_binding `{binding_id}` 与 workflow target 不匹配"
        )));
    }

    Ok(())
}

fn target_owner_type(target_kind: WorkflowTargetKind) -> SessionOwnerType {
    match target_kind {
        WorkflowTargetKind::Project => SessionOwnerType::Project,
        WorkflowTargetKind::Story => SessionOwnerType::Story,
        WorkflowTargetKind::Task => SessionOwnerType::Task,
    }
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
    match raw.and_then(normalize_optional_str_ref) {
        Some(value) => parse_uuid(value, field).map(Some),
        None => Ok(None),
    }
}

fn normalize_optional_str_ref(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn parse_target_kind(raw: &str) -> Result<WorkflowTargetKind, ApiError> {
    match raw.trim() {
        "project" => Ok(WorkflowTargetKind::Project),
        "story" => Ok(WorkflowTargetKind::Story),
        "task" => Ok(WorkflowTargetKind::Task),
        _ => Err(ApiError::BadRequest(format!("无效的 target_kind: {raw}"))),
    }
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

// ---- Workflow Definition CRUD / Validate / Preview / Enable / Disable ----

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowDefinitionRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    pub recommended_role: Option<WorkflowAgentRole>,
    pub phases: Vec<WorkflowPhaseDefinitionRequest>,
    #[serde(default)]
    pub record_policy: Option<WorkflowRecordPolicy>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowDefinitionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub recommended_role: Option<WorkflowAgentRole>,
    pub phases: Option<Vec<WorkflowPhaseDefinitionRequest>>,
    pub record_policy: Option<WorkflowRecordPolicy>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowPhaseDefinitionRequest {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub agent_instructions: Vec<String>,
    #[serde(default)]
    pub context_bindings: Vec<WorkflowContextBindingRequest>,
    #[serde(default)]
    pub requires_session: bool,
    pub completion_mode: WorkflowPhaseCompletionMode,
    pub default_artifact_type: Option<WorkflowRecordArtifactType>,
    pub default_artifact_title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowContextBindingRequest {
    pub kind: WorkflowContextBindingKind,
    pub locator: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default = "default_true")]
    pub required: bool,
    pub title: Option<String>,
}

fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct ValidateWorkflowDefinitionRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    pub recommended_role: Option<WorkflowAgentRole>,
    pub phases: Vec<WorkflowPhaseDefinitionRequest>,
    #[serde(default)]
    pub record_policy: Option<WorkflowRecordPolicy>,
}

pub async fn create_workflow_definition(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateWorkflowDefinitionRequest>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let phases = req.phases.into_iter().map(Into::into).collect::<Vec<WorkflowPhaseDefinition>>();
    let mut definition = WorkflowDefinition::new(
        req.key,
        req.name,
        req.description,
        req.target_kind,
        WorkflowDefinitionSource::UserAuthored,
        phases,
    )
    .map_err(ApiError::BadRequest)?;

    definition.recommended_role = req.recommended_role;
    if let Some(policy) = req.record_policy {
        definition.record_policy = policy;
    }

    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let saved = service.upsert_definition(definition).await?;
    Ok(Json(WorkflowDefinitionResponse::from(saved)))
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
    Ok(Json(WorkflowDefinitionResponse::from(definition)))
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
    if let Some(role) = req.recommended_role {
        definition.recommended_role = Some(role);
    }
    if let Some(phases) = req.phases {
        definition.phases = phases.into_iter().map(Into::into).collect();
    }
    if let Some(policy) = req.record_policy {
        definition.record_policy = policy;
    }

    let issues = definition.validate_full();
    let has_errors = issues.iter().any(|i| i.severity == ValidationSeverity::Error);
    if has_errors {
        return Err(ApiError::BadRequest(format!(
            "校验失败: {}",
            issues
                .iter()
                .filter(|i| i.severity == ValidationSeverity::Error)
                .map(|i| format!("[{}] {}", i.field_path, i.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    definition.version += 1;
    definition.updated_at = chrono::Utc::now();
    state.repos.workflow_definition_repo.update(&definition).await?;
    Ok(Json(WorkflowDefinitionResponse::from(definition)))
}

pub async fn validate_workflow_definition(
    Json(req): Json<ValidateWorkflowDefinitionRequest>,
) -> Result<Json<WorkflowValidationResponse>, ApiError> {
    let phases: Vec<WorkflowPhaseDefinition> = req.phases.into_iter().map(Into::into).collect();
    let definition = WorkflowDefinition::new(
        req.key,
        req.name,
        req.description,
        req.target_kind,
        WorkflowDefinitionSource::UserAuthored,
        phases,
    );
    match definition {
        Ok(def) => {
            let issues = def.validate_full();
            let valid = !issues.iter().any(|i| i.severity == ValidationSeverity::Error);
            Ok(Json(WorkflowValidationResponse { valid, issues }))
        }
        Err(e) => {
            Ok(Json(WorkflowValidationResponse {
                valid: false,
                issues: vec![agentdash_domain::workflow::ValidationIssue::error(
                    "construction_error",
                    e,
                    "",
                )],
            }))
        }
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
    let has_errors = issues.iter().any(|i| i.severity == ValidationSeverity::Error);
    if has_errors {
        return Err(ApiError::BadRequest(format!(
            "definition 存在校验错误，不能激活: {}",
            issues
                .iter()
                .filter(|i| i.severity == ValidationSeverity::Error)
                .map(|i| format!("[{}] {}", i.field_path, i.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    definition.status = WorkflowDefinitionStatus::Active;
    definition.updated_at = chrono::Utc::now();
    state.repos.workflow_definition_repo.update(&definition).await?;
    Ok(Json(WorkflowDefinitionResponse::from(definition)))
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
    state.repos.workflow_definition_repo.update(&definition).await?;
    Ok(Json(WorkflowDefinitionResponse::from(definition)))
}

pub async fn delete_workflow_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    state.repos.workflow_definition_repo.delete(id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Binding 元数据 registry：返回所有可用的 binding kind 和合法 locator。
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
    pub applicable_target_kinds: Vec<WorkflowTargetKind>,
}

fn build_binding_metadata() -> Vec<BindingKindMetadata> {
    vec![
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::DocumentPath,
            label: "文档路径".to_string(),
            description: "从工作空间读取指定相对路径的文件内容".to_string(),
            locator_options: vec![
                BindingLocatorOption {
                    locator: ".trellis/workflow.md".to_string(),
                    label: "Workflow 文档".to_string(),
                    description: "Trellis 开发工作流文档".to_string(),
                    applicable_target_kinds: vec![WorkflowTargetKind::Task, WorkflowTargetKind::Story],
                },
            ],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::RuntimeContext,
            label: "运行时上下文".to_string(),
            description: "注入运行时动态上下文信息".to_string(),
            locator_options: vec![
                BindingLocatorOption { locator: "project_session_context".to_string(), label: "项目会话上下文".to_string(), description: "项目级上下文配置快照".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Project, WorkflowTargetKind::Story, WorkflowTargetKind::Task] },
                BindingLocatorOption { locator: "story_prd".to_string(), label: "Story PRD".to_string(), description: "Story 的产品需求文档".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Story, WorkflowTargetKind::Task] },
                BindingLocatorOption { locator: "story_context_snapshot".to_string(), label: "Story 上下文快照".to_string(), description: "Story 的结构化上下文".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Story, WorkflowTargetKind::Task] },
                BindingLocatorOption { locator: "task_execution_context".to_string(), label: "Task 执行上下文".to_string(), description: "Task 的执行配置快照".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Task] },
            ],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::Checklist,
            label: "检查清单".to_string(),
            description: "结构化检查清单，用于 checklist_passed 完成模式".to_string(),
            locator_options: vec![
                BindingLocatorOption { locator: "code_quality_checklist".to_string(), label: "代码质量清单".to_string(), description: "代码质量自检清单".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Task] },
                BindingLocatorOption { locator: "review_checklist".to_string(), label: "评审清单".to_string(), description: "代码评审检查清单".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Task] },
            ],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::JournalTarget,
            label: "日志目标".to_string(),
            description: "指定 Trellis 日志写入目录".to_string(),
            locator_options: vec![
                BindingLocatorOption { locator: "trellis_workspace_journal".to_string(), label: "工作区日志".to_string(), description: "Trellis 工作区日志目录".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Task, WorkflowTargetKind::Story] },
            ],
        },
        BindingKindMetadata {
            kind: WorkflowContextBindingKind::ActionRef,
            label: "动作引用".to_string(),
            description: "引用可执行的动作，如归档".to_string(),
            locator_options: vec![
                BindingLocatorOption { locator: "workflow_archive_action".to_string(), label: "归档动作".to_string(), description: "触发 workflow 任务归档".to_string(), applicable_target_kinds: vec![WorkflowTargetKind::Task] },
            ],
        },
    ]
}

impl From<WorkflowPhaseDefinitionRequest> for WorkflowPhaseDefinition {
    fn from(value: WorkflowPhaseDefinitionRequest) -> Self {
        Self {
            key: value.key,
            title: value.title,
            description: value.description,
            agent_instructions: value.agent_instructions,
            context_bindings: value.context_bindings.into_iter().map(Into::into).collect(),
            requires_session: value.requires_session,
            completion_mode: value.completion_mode,
            default_artifact_type: value.default_artifact_type,
            default_artifact_title: value.default_artifact_title,
        }
    }
}

impl From<WorkflowContextBindingRequest> for WorkflowContextBinding {
    fn from(value: WorkflowContextBindingRequest) -> Self {
        Self {
            kind: value.kind,
            locator: value.locator,
            reason: value.reason,
            required: value.required,
            title: value.title,
        }
    }
}
