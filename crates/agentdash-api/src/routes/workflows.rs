use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use agentdash_application::hooks::hook_rule_preset_registry;
use agentdash_application::workflow::{
    ActivityEvent, ActivityLifecycleCatalogService, ActivityLifecycleRunService,
    AgentActivityExecutorLauncher, AgentActivityLaunchContext, AgentActivityRuntimePort,
    StartActivityLifecycleRunCommand,
};
use agentdash_contracts::workflow::{
    DeleteActivityLifecycleDefinitionResponse, DeleteHookPresetResponse,
    DeleteWorkflowDefinitionResponse, HookPresetResponse, HookPresetsResponse,
    RegisterHookPresetResponse, ValidateHookScriptResponse,
};
use agentdash_domain::workflow::{
    ActivityExecutorSpec, ActivityLifecycleDefinition, LifecycleRun, ValidationIssue,
    ValidationSeverity, WorkflowDefinition, WorkflowDefinitionSource,
    normalize_workflow_binding_kinds, workflow_binding_kinds_cover,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    CreateActivityLifecycleDefinitionRequest, CreateWorkflowDefinitionRequest, ListWorkflowsQuery,
    RegisterPresetRequest, StartWorkflowRunRequest, SubmitHumanDecisionRequest, ToolCatalogQuery,
    UpdateActivityLifecycleDefinitionRequest, UpdateWorkflowDefinitionRequest,
    ValidateActivityLifecycleDefinitionRequest, ValidateScriptRequest,
    ValidateWorkflowDefinitionRequest, WorkflowValidationResponse,
};
use crate::rpc::ApiError;
use agentdash_application::session::context::normalize_string;

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

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/workflow-definitions",
            axum::routing::get(list_workflows).post(create_workflow_definition),
        )
        .route(
            "/activity-lifecycle-definitions",
            axum::routing::get(list_activity_lifecycles).post(create_activity_lifecycle_definition),
        )
        .route(
            "/workflow-definitions/validate",
            axum::routing::post(validate_workflow_definition),
        )
        .route(
            "/activity-lifecycle-definitions/validate",
            axum::routing::post(validate_activity_lifecycle_definition),
        )
        .route(
            "/workflow-definitions/{id}",
            axum::routing::get(get_workflow_definition)
                .put(update_workflow_definition)
                .delete(delete_workflow_definition),
        )
        .route(
            "/activity-lifecycle-definitions/{id}",
            axum::routing::get(get_activity_lifecycle_definition)
                .put(update_activity_lifecycle_definition)
                .delete(delete_activity_lifecycle_definition),
        )
        .route("/tool-catalog", axum::routing::get(query_tool_catalog))
        .route("/hook-presets", axum::routing::get(list_hook_presets))
        .route(
            "/hook-scripts/validate",
            axum::routing::post(validate_hook_script),
        )
        .route(
            "/hook-presets/custom",
            axum::routing::post(register_hook_preset),
        )
        .route(
            "/hook-presets/custom/{key}",
            axum::routing::delete(delete_hook_preset),
        )
        .route("/lifecycle-runs", axum::routing::post(start_lifecycle_run))
        .route(
            "/lifecycle-runs/{id}",
            axum::routing::get(get_lifecycle_run),
        )
        .route(
            "/lifecycle-runs/by-session/{session_id}",
            axum::routing::get(list_lifecycle_runs_by_session),
        )
        .route(
            "/lifecycle-runs/{id}/activities/{activity_key}/attempts/{attempt}/human-decision",
            axum::routing::post(submit_human_decision),
        )
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
) -> Result<Json<DeleteActivityLifecycleDefinitionResponse>, ApiError> {
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
    Ok(Json(DeleteActivityLifecycleDefinitionResponse {
        deleted: true,
    }))
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
            root_session_id: run.session_id.clone().unwrap_or_default(),
        },
        AgentActivityRuntimePort::new(
            state.services.session_core.clone(),
            state.services.session_launch.clone(),
            state.repos.clone(),
            Arc::new(agentdash_infrastructure::DefaultFunctionRunner::new()),
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
            root_session_id: run.session_id.clone().unwrap_or_default(),
        },
        AgentActivityRuntimePort::new(
            state.services.session_core.clone(),
            state.services.session_launch.clone(),
            state.repos.clone(),
            Arc::new(agentdash_infrastructure::DefaultFunctionRunner::new()),
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
) -> Result<Json<DeleteWorkflowDefinitionResponse>, ApiError> {
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
    Ok(Json(DeleteWorkflowDefinitionResponse { deleted: true }))
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

pub async fn list_hook_presets() -> Result<Json<HookPresetsResponse>, ApiError> {
    let presets = hook_rule_preset_registry();
    let grouped = group_presets_by_trigger(presets)?;
    Ok(Json(HookPresetsResponse { presets: grouped }))
}

fn group_presets_by_trigger(
    presets: &[agentdash_application::hooks::HookRulePreset],
) -> Result<BTreeMap<String, Vec<HookPresetResponse>>, ApiError> {
    let mut groups: BTreeMap<String, Vec<HookPresetResponse>> = BTreeMap::new();
    for preset in presets {
        let trigger = serde_json::to_value(preset.trigger).map_err(|error| {
            ApiError::Internal(format!(
                "序列化 hook preset trigger 失败: key={}, error={error}",
                preset.key
            ))
        })?;
        let trigger_key = trigger.as_str().map(ToString::to_string).ok_or_else(|| {
            ApiError::Internal(format!(
                "hook preset trigger 不是字符串: key={}",
                preset.key
            ))
        })?;
        let source = serde_json::to_value(preset.source).map_err(|error| {
            ApiError::Internal(format!(
                "序列化 hook preset source 失败: key={}, error={error}",
                preset.key
            ))
        })?;
        let source = source.as_str().map(ToString::to_string).ok_or_else(|| {
            ApiError::Internal(format!("hook preset source 不是字符串: key={}", preset.key))
        })?;
        groups
            .entry(trigger_key)
            .or_default()
            .push(HookPresetResponse {
                key: preset.key.to_string(),
                trigger,
                label: preset.label.to_string(),
                description: preset.description.to_string(),
                param_schema: preset
                    .param_schema
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
                script: preset.script.to_string(),
                source,
            });
    }
    Ok(groups)
}

pub async fn validate_hook_script(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateScriptRequest>,
) -> Json<ValidateHookScriptResponse> {
    match state.services.hook_provider.validate_script(&req.script) {
        Ok(()) => Json(ValidateHookScriptResponse {
            valid: true,
            errors: None,
        }),
        Err(errors) => Json(ValidateHookScriptResponse {
            valid: false,
            errors: Some(errors),
        }),
    }
}

pub async fn register_hook_preset(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterPresetRequest>,
) -> Result<Json<RegisterHookPresetResponse>, ApiError> {
    state
        .services
        .hook_provider
        .register_preset(&req.key, &req.script)
        .map_err(|e| ApiError::BadRequest(format!("脚本编译失败: {e}")))?;
    Ok(Json(RegisterHookPresetResponse {
        registered: true,
        key: req.key,
    }))
}

pub async fn delete_hook_preset(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Json<DeleteHookPresetResponse> {
    let removed = state.services.hook_provider.remove_preset(&key);
    Json(DeleteHookPresetResponse { removed, key })
}

// ── Tool Catalog ──

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

// -- Run Links --

use agentdash_contracts::workflow::{AttachRunLinkRequest, LifecycleRunLinkDto, RunLinksResponse};
use agentdash_domain::workflow::{LifecycleRunLink, RunLinkRole, RunLinkSubjectKind};

fn link_to_dto(link: &LifecycleRunLink) -> LifecycleRunLinkDto {
    LifecycleRunLinkDto {
        id: link.id.to_string(),
        run_id: link.run_id.to_string(),
        subject_kind: link.subject_kind.as_str().to_string(),
        subject_id: link.subject_id.to_string(),
        role: link.role.as_str().to_string(),
        metadata: link.metadata.clone(),
        created_at: link.created_at.to_rfc3339(),
    }
}

/// GET /lifecycle-runs/{run_id}/links
pub async fn list_run_links(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
) -> Result<Json<RunLinksResponse>, ApiError> {
    let run_uuid = parse_uuid(&run_id, "run_id")?;
    let run = load_lifecycle_run(&state, run_uuid).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;

    let links = state
        .repos
        .lifecycle_run_link_repo
        .list_by_run(run_uuid)
        .await?;

    Ok(Json(RunLinksResponse {
        run_id,
        links: links.iter().map(link_to_dto).collect(),
    }))
}

/// POST /lifecycle-runs/{run_id}/links
pub async fn attach_run_link(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
    Json(req): Json<AttachRunLinkRequest>,
) -> Result<Json<LifecycleRunLinkDto>, ApiError> {
    let run_uuid = parse_uuid(&run_id, "run_id")?;
    let run = load_lifecycle_run(&state, run_uuid).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let subject_kind = RunLinkSubjectKind::from_str(&req.subject_kind).ok_or_else(|| {
        ApiError::BadRequest(format!("Invalid subject_kind: {}", req.subject_kind))
    })?;
    let subject_id: Uuid = req
        .subject_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("Invalid subject_id: {}", req.subject_id)))?;
    let role = RunLinkRole::from_str(&req.role)
        .ok_or_else(|| ApiError::BadRequest(format!("Invalid role: {}", req.role)))?;

    let mut link = LifecycleRunLink::new(run_uuid, subject_kind, subject_id, role);
    if let Some(metadata) = req.metadata {
        link = link.with_metadata(metadata);
    }

    state.repos.lifecycle_run_link_repo.create(&link).await?;

    Ok(Json(link_to_dto(&link)))
}
