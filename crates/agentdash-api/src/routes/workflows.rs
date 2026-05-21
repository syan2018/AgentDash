use std::{collections::HashSet, sync::Arc};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::hooks::hook_rule_preset_registry;
use agentdash_application::workflow::{
    ActivityEvent, ActivityLifecycleCatalogService, ActivityLifecycleRunService,
    AgentActivityExecutorLauncher, AgentActivityLaunchContext, AgentActivityRuntimePort,
    StartActivityLifecycleRunCommand,
};
use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutorSpec, ActivityLifecycleDefinition, ActivityTransition,
    LifecycleRun, ValidationIssue, ValidationSeverity, WorkflowBindingKind, WorkflowContract,
    WorkflowDefinition, WorkflowDefinitionSource, normalize_workflow_binding_kinds,
    workflow_binding_kinds_cover,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::WorkflowValidationResponse;
use crate::rpc::ApiError;
use agentdash_application::session::context::normalize_string;

#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowsQuery {
    pub project_id: Option<String>,
    pub binding_kind: Option<WorkflowBindingKind>,
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

pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<WorkflowDefinition>>, ApiError> {
    let project_id = parse_project_id_query(query.project_id.as_deref())?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let mut definitions = state
        .repos
        .workflow_definition_repo
        .list_by_project(project_id)
        .await?;
    if let Some(binding_kind) = query.binding_kind {
        definitions.retain(|definition| definition.binding_kinds.contains(&binding_kind));
    }
    Ok(Json(definitions))
}

pub async fn list_activity_lifecycles(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<ActivityLifecycleDefinition>>, ApiError> {
    let project_id = parse_project_id_query(query.project_id.as_deref())?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let mut definitions = state
        .repos
        .activity_lifecycle_definition_repo
        .list_by_project(project_id)
        .await?;
    if let Some(binding_kind) = query.binding_kind {
        definitions.retain(|definition| definition.binding_kinds.contains(&binding_kind));
    }
    Ok(Json(definitions))
}

pub async fn create_activity_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateActivityLifecycleDefinitionRequest>,
) -> Result<Json<ActivityLifecycleDefinition>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let definition = ActivityLifecycleDefinition::new(
        project_id,
        req.key,
        req.name,
        req.description,
        req.binding_kinds,
        WorkflowDefinitionSource::UserAuthored,
        req.entry_activity_key,
        req.activities,
        req.transitions,
    )
    .map_err(ApiError::BadRequest)?;
    let service = ActivityLifecycleCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.activity_lifecycle_definition_repo.as_ref(),
    );
    let saved = service
        .upsert_activity_lifecycle_definition(definition)
        .await?;
    Ok(Json(saved))
}

pub async fn get_activity_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ActivityLifecycleDefinition>, ApiError> {
    let id = parse_uuid(&id, "activity_lifecycle_id")?;
    let definition = state
        .repos
        .activity_lifecycle_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("activity_lifecycle_definition 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(definition))
}

pub async fn update_activity_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateActivityLifecycleDefinitionRequest>,
) -> Result<Json<ActivityLifecycleDefinition>, ApiError> {
    let id = parse_uuid(&id, "activity_lifecycle_id")?;
    let mut definition = state
        .repos
        .activity_lifecycle_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("activity_lifecycle_definition 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    if let Some(name) = req.name {
        definition.name = name;
    }
    if let Some(description) = req.description {
        definition.description = description;
    }
    if let Some(binding_kinds) = req.binding_kinds {
        definition.binding_kinds =
            normalize_workflow_binding_kinds(binding_kinds).map_err(ApiError::BadRequest)?;
    }
    if let Some(entry_activity_key) = req.entry_activity_key {
        definition.entry_activity_key = entry_activity_key;
    }
    if let Some(activities) = req.activities {
        definition.activities = activities;
    }
    if let Some(transitions) = req.transitions {
        definition.transitions = transitions;
    }
    let service = ActivityLifecycleCatalogService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.activity_lifecycle_definition_repo.as_ref(),
    );
    let saved = service
        .upsert_activity_lifecycle_definition(definition)
        .await?;
    Ok(Json(saved))
}

pub async fn validate_activity_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ValidateActivityLifecycleDefinitionRequest>,
) -> Result<Json<WorkflowValidationResponse>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    match ActivityLifecycleDefinition::new(
        project_id,
        req.key,
        req.name,
        req.description,
        req.binding_kinds,
        WorkflowDefinitionSource::UserAuthored,
        req.entry_activity_key,
        req.activities,
        req.transitions,
    ) {
        Ok(definition) => {
            let service = ActivityLifecycleCatalogService::new(
                state.repos.workflow_definition_repo.as_ref(),
                state.repos.activity_lifecycle_definition_repo.as_ref(),
            );
            let issues = service
                .validate_activity_lifecycle_definition(&definition)
                .await?;
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
                "activities",
            )],
        })),
    }
}

pub async fn delete_activity_lifecycle_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_uuid(&id, "activity_lifecycle_id")?;
    let definition = state
        .repos
        .activity_lifecycle_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("activity_lifecycle_definition 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    state
        .repos
        .activity_lifecycle_definition_repo
        .delete(id)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
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
    let service = ActivityLifecycleRunService::new(
        state.repos.activity_lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.activity_execution_claim_repo.as_ref(),
    );
    let run = service
        .start_run(StartActivityLifecycleRunCommand {
            project_id,
            lifecycle_id: parse_optional_uuid(req.lifecycle_id.as_deref(), "lifecycle_id")?,
            lifecycle_key: req.lifecycle_key.and_then(normalize_string),
            session_id: req.session_id,
        })
        .await?;
    let launcher = AgentActivityExecutorLauncher::new(
        AgentActivityLaunchContext {
            project_id: run.project_id,
            lifecycle_key: String::new(),
            root_session_id: run.session_id.clone(),
        },
        AgentActivityRuntimePort::new(
            state.services.session_core.clone(),
            state.services.session_launch.clone(),
            state.repos.clone(),
        )
        .with_runtime_context(
            state.services.session_hooks.clone(),
            state.services.session_capability.clone(),
            state.config.platform_config.clone(),
        ),
    );
    service.launch_ready_attempts(run.id, &launcher).await?;

    let latest_run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run.id)
        .await?
        .unwrap_or(run);
    Ok(Json(latest_run))
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
    Ok(Json(run))
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
    Ok(Json(runs))
}

pub async fn submit_human_decision(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, activity_key, attempt)): Path<(String, String, u32)>,
    Json(req): Json<SubmitHumanDecisionRequest>,
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
    let service = ActivityLifecycleRunService::new(
        state.repos.activity_lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.activity_execution_claim_repo.as_ref(),
    );
    let run = service
        .apply_event(
            run_id,
            ActivityEvent::HumanDecisionSubmitted {
                activity_key,
                attempt,
                decision_port: req.decision_port,
                decision: req.decision,
                summary: req.summary.and_then(normalize_string),
            },
        )
        .await?;
    let launcher = AgentActivityExecutorLauncher::new(
        AgentActivityLaunchContext {
            project_id: run.project_id,
            lifecycle_key: String::new(),
            root_session_id: run.session_id.clone(),
        },
        AgentActivityRuntimePort::new(
            state.services.session_core.clone(),
            state.services.session_launch.clone(),
            state.repos.clone(),
        )
        .with_runtime_context(
            state.services.session_hooks.clone(),
            state.services.session_capability.clone(),
            state.config.platform_config.clone(),
        ),
    );
    service.launch_ready_attempts(run.id, &launcher).await?;
    let latest_run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run.id)
        .await?
        .unwrap_or(run);
    Ok(Json(latest_run))
}

pub async fn create_workflow_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateWorkflowDefinitionRequest>,
) -> Result<Json<WorkflowDefinition>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let definition = WorkflowDefinition::new(
        project_id,
        req.key,
        req.name,
        req.description,
        req.binding_kinds,
        WorkflowDefinitionSource::UserAuthored,
        req.contract,
    )
    .map_err(ApiError::BadRequest)?;
    let saved = upsert_workflow_definition(state.as_ref(), definition).await?;
    Ok(Json(saved))
}

pub async fn get_workflow_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<WorkflowDefinition>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let definition = state
        .repos
        .workflow_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_definition 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(definition))
}

pub async fn update_workflow_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
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
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    if let Some(name) = req.name {
        definition.name = name;
    }
    if let Some(description) = req.description {
        definition.description = description;
    }
    if let Some(binding_kinds) = req.binding_kinds {
        definition.binding_kinds =
            normalize_workflow_binding_kinds(binding_kinds).map_err(ApiError::BadRequest)?;
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
    let saved = upsert_workflow_definition(state.as_ref(), definition).await?;
    Ok(Json(saved))
}

pub async fn validate_workflow_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ValidateWorkflowDefinitionRequest>,
) -> Result<Json<WorkflowValidationResponse>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    match WorkflowDefinition::new(
        project_id,
        req.key,
        req.name,
        req.description,
        req.binding_kinds,
        WorkflowDefinitionSource::UserAuthored,
        req.contract,
    ) {
        Ok(definition) => {
            let mut issues = definition.validate_full();
            issues.extend(
                validate_activity_lifecycle_references_for_workflow(state.as_ref(), &definition)
                    .await?,
            );
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

pub async fn delete_workflow_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let definition = state
        .repos
        .workflow_definition_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_definition 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let workflow_key = definition.key.clone();
    let referencing_activities: Vec<String> = state
        .repos
        .activity_lifecycle_definition_repo
        .list_by_project(definition.project_id)
        .await?
        .into_iter()
        .flat_map(|lifecycle| {
            let lifecycle_key = lifecycle.key.clone();
            let workflow_key = workflow_key.clone();
            lifecycle
                .activities
                .into_iter()
                .filter_map(move |activity| match activity.executor {
                    ActivityExecutorSpec::Agent(agent) if agent.workflow_key == workflow_key => {
                        Some(format!("{lifecycle_key}.{}", activity.key))
                    }
                    _ => None,
                })
        })
        .collect();
    if !referencing_activities.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "workflow `{}` 仍被 Activity Lifecycle 引用，不能删除：{}",
            workflow_key,
            referencing_activities.join("、")
        )));
    }
    state.repos.workflow_definition_repo.delete(id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn load_lifecycle_run(state: &Arc<AppState>, run_id: Uuid) -> Result<LifecycleRun, ApiError> {
    state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))
}

async fn upsert_workflow_definition(
    state: &AppState,
    definition: WorkflowDefinition,
) -> Result<WorkflowDefinition, ApiError> {
    let issues = validate_activity_lifecycle_references_for_workflow(state, &definition).await?;
    let errors = issues
        .iter()
        .filter(|item| item.severity == ValidationSeverity::Error)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "校验失败: {}",
            errors
                .iter()
                .map(|item| format!("[{}] {}", item.field_path, item.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    if let Some(existing) = state
        .repos
        .workflow_definition_repo
        .get_by_project_and_key(definition.project_id, &definition.key)
        .await?
    {
        let mut updated = definition;
        updated.id = existing.id;
        updated.version = existing.version + 1;
        updated.created_at = existing.created_at;
        updated.updated_at = chrono::Utc::now();
        state
            .repos
            .workflow_definition_repo
            .update(&updated)
            .await?;
        return Ok(updated);
    }

    state
        .repos
        .workflow_definition_repo
        .create(&definition)
        .await?;
    Ok(definition)
}

async fn validate_activity_lifecycle_references_for_workflow(
    state: &AppState,
    definition: &WorkflowDefinition,
) -> Result<Vec<ValidationIssue>, ApiError> {
    let lifecycles = state
        .repos
        .activity_lifecycle_definition_repo
        .list_by_project(definition.project_id)
        .await?;
    let issues = lifecycles
        .into_iter()
        .filter_map(|lifecycle| {
            let referencing_activities = lifecycle
                .activities
                .iter()
                .filter_map(|activity| match &activity.executor {
                    ActivityExecutorSpec::Agent(agent) if agent.workflow_key == definition.key => {
                        Some(activity.key.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if referencing_activities.is_empty()
                || workflow_binding_kinds_cover(&lifecycle.binding_kinds, &definition.binding_kinds)
            {
                None
            } else {
                Some(ValidationIssue::error(
                    "activity_workflow_binding_kind_mismatch",
                    format!(
                        "workflow `{}` 的 binding_kinds={:?} 未覆盖引用它的 activity lifecycle `{}` {:?}",
                        definition.key,
                        definition.binding_kinds,
                        lifecycle.key,
                        lifecycle.binding_kinds
                    ),
                    format!("activity_lifecycles.{}", referencing_activities.join(",")),
                ))
            }
        })
        .collect::<Vec<_>>();
    Ok(issues)
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

fn parse_project_id_query(raw: Option<&str>) -> Result<Uuid, ApiError> {
    let Some(raw) = raw else {
        return Err(ApiError::BadRequest("project_id 不能为空".to_string()));
    };
    parse_uuid_required(raw, "project_id")
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
        let trigger_key = serde_json::to_value(preset.trigger)
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

// ── Tool Catalog ──

#[derive(Debug, Deserialize)]
pub struct ToolCatalogQuery {
    /// 逗号分隔的 capability keys，如 `file_read,canvas,mcp:code_analyzer`
    pub capabilities: String,
}

pub async fn query_tool_catalog(
    Query(query): Query<ToolCatalogQuery>,
) -> Json<Vec<agentdash_spi::ToolDescriptor>> {
    let keys: Vec<String> = query
        .capabilities
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let catalog = agentdash_application::capability::query_tool_catalog(&keys);
    Json(catalog)
}
