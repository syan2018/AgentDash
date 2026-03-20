use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::workflow::{
    ActivateWorkflowPhaseCommand, AssignWorkflowCommand, CompleteWorkflowPhaseCommand,
    StartWorkflowRunCommand, WorkflowCatalogService, WorkflowRecordArtifactDraft,
    WorkflowRunService, build_trellis_dev_workflow_definition,
};
use agentdash_domain::workflow::{
    WorkflowAgentRole, WorkflowRecordArtifactType, WorkflowTargetKind,
};

use crate::app_state::AppState;
use crate::dto::{WorkflowAssignmentResponse, WorkflowDefinitionResponse, WorkflowRunResponse};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowsQuery {
    pub target_kind: Option<WorkflowTargetKind>,
    pub enabled_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct BootstrapTrellisWorkflowRequest {
    pub target_kind: WorkflowTargetKind,
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
        (None, true) => state.repos.workflow_definition_repo.list_enabled().await?,
        (None, false) => state.repos.workflow_definition_repo.list_all().await?,
    };

    Ok(Json(
        definitions
            .into_iter()
            .map(WorkflowDefinitionResponse::from)
            .collect(),
    ))
}

pub async fn bootstrap_trellis_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BootstrapTrellisWorkflowRequest>,
) -> Result<Json<WorkflowDefinitionResponse>, ApiError> {
    let service = WorkflowCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_assignment_repo.as_ref(),
    );
    let definition =
        build_trellis_dev_workflow_definition(req.target_kind).map_err(ApiError::BadRequest)?;
    let saved = service.upsert_definition(definition).await?;

    Ok(Json(WorkflowDefinitionResponse::from(saved)))
}

pub async fn list_project_workflow_assignments(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<WorkflowAssignmentResponse>>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    ensure_project_exists(&state, project_id).await?;

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
    Path(project_id): Path<String>,
    Json(req): Json<CreateWorkflowAssignmentRequest>,
) -> Result<Json<WorkflowAssignmentResponse>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    ensure_project_exists(&state, project_id).await?;
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
    Json(req): Json<StartWorkflowRunRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let service = WorkflowRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_run_repo.as_ref(),
    );
    let run = service
        .start_run(StartWorkflowRunCommand {
            workflow_id: parse_optional_uuid(req.workflow_id.as_deref(), "workflow_id")?,
            workflow_key: req.workflow_key.and_then(normalize_optional_string),
            target_kind: req.target_kind,
            target_id: parse_uuid_required(&req.target_id, "target_id")?,
        })
        .await?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

pub async fn get_workflow_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let run = state
        .repos
        .workflow_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_run {run_id} 不存在")))?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

pub async fn list_workflow_runs_by_target(
    State(state): State<Arc<AppState>>,
    Path((target_kind_raw, target_id_raw)): Path<(String, String)>,
) -> Result<Json<Vec<WorkflowRunResponse>>, ApiError> {
    let target_kind = parse_target_kind(&target_kind_raw)?;
    let target_id = parse_uuid(&target_id_raw, "target_id")?;
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
    Path((run_id, phase_key)): Path<(String, String)>,
    Json(req): Json<ActivateWorkflowPhaseRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
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
    Path((run_id, phase_key)): Path<(String, String)>,
    Json(req): Json<CompleteWorkflowPhaseRequest>,
) -> Result<Json<WorkflowRunResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let service = WorkflowRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.workflow_run_repo.as_ref(),
    );
    let run = service
        .complete_phase(CompleteWorkflowPhaseCommand {
            run_id,
            phase_key,
            summary: req.summary.and_then(normalize_optional_string),
            record_artifacts: req
                .record_artifacts
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        })
        .await?;

    Ok(Json(WorkflowRunResponse::from(run)))
}

async fn ensure_project_exists(state: &Arc<AppState>, project_id: Uuid) -> Result<(), ApiError> {
    let exists = state
        .repos
        .project_repo
        .get_by_id(project_id)
        .await?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(ApiError::NotFound(format!("Project {project_id} 不存在")))
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

fn normalize_optional_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
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
