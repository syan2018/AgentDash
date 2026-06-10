use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use agentdash_application::hooks::hook_rule_preset_registry;
use agentdash_application::workflow::{
    ActivityLifecycleCatalogService, LifecycleDispatchService, OrchestrationExecutorLauncher,
    ScriptCompiler, SubmitHumanGateDecisionInput, WorkflowScriptPreflightInput,
    WorkflowScriptPreflightService, lifecycle_run_view_builder,
};
use agentdash_contracts::workflow::{
    AgentProcedureResponse, DeleteAgentProcedureResponse, DeleteHookPresetResponse,
    DeleteWorkflowGraphResponse, HookPresetResponse, HookPresetsResponse, LifecycleRunView,
    PreflightWorkflowScriptRequest, PreflightWorkflowScriptResponse, RegisterHookPresetResponse,
    SubmitOrchestrationHumanDecisionRequest, SubmitOrchestrationHumanDecisionResponse,
    ValidateHookScriptResponse, ValidationSeverity as ContractValidationSeverity,
    WorkflowGraphResponse, WorkflowScriptApiEndpointDto, WorkflowScriptBashCommandDto,
    WorkflowScriptCapabilitySummaryDto, WorkflowScriptHumanGateCapabilityDto,
    WorkflowScriptPlanPreviewDto, WorkflowScriptPlanPreviewNodeDto,
    WorkflowScriptPreflightDiagnosticDto, WorkflowTargetKind,
};
use agentdash_domain::workflow::{
    ActivityExecutorSpec, AgentProcedure, DefinitionSource, ExecutionSource, LifecycleRun,
    LifecycleRunStartIntent, OrchestrationSourceRef, ValidationIssue, ValidationSeverity,
    WorkflowGraph, WorkflowGraphRef, WorkflowScriptCapabilitySummary, WorkflowScriptProvenance,
    WorkflowScriptProvenanceSource, workflow_script_source_digest,
};

use super::lifecycle_contracts::lifecycle_run_view_to_contract;
use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    CreateAgentProcedureRequest, CreateWorkflowGraphRequest, ListWorkflowsQuery,
    RegisterPresetRequest, StartWorkflowRunRequest, ToolCatalogQuery, UpdateAgentProcedureRequest,
    UpdateWorkflowGraphRequest, ValidateAgentProcedureRequest, ValidateScriptRequest,
    ValidateWorkflowGraphRequest, WorkflowValidationResponse,
};
use crate::rpc::ApiError;
use agentdash_application::session::context::normalize_string;

pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<AgentProcedureResponse>>, ApiError> {
    let project_id = parse_project_id_query(query.project_id.as_deref())?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let definitions = state
        .repos
        .agent_procedure_repo
        .list_by_project(project_id)
        .await?;
    Ok(Json(
        definitions
            .into_iter()
            .map(agent_procedure_to_contract_response)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/agent-procedures",
            axum::routing::get(list_workflows).post(create_agent_procedure),
        )
        .route(
            "/workflow-graphs",
            axum::routing::get(list_activity_lifecycles).post(create_workflow_graph),
        )
        .route(
            "/agent-procedures/validate",
            axum::routing::post(validate_agent_procedure),
        )
        .route(
            "/workflow-graphs/validate",
            axum::routing::post(validate_workflow_graph),
        )
        .route(
            "/workflow-scripts/preflight",
            axum::routing::post(preflight_workflow_script),
        )
        .route(
            "/agent-procedures/{id}",
            axum::routing::get(get_agent_procedure)
                .put(update_agent_procedure)
                .delete(delete_agent_procedure),
        )
        .route(
            "/workflow-graphs/{id}",
            axum::routing::get(get_workflow_graph)
                .put(update_workflow_graph)
                .delete(delete_workflow_graph),
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
            "/lifecycle-runs/{id}/orchestration-human-decisions",
            axum::routing::post(submit_orchestration_human_decision),
        )
}

pub async fn list_activity_lifecycles(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<Vec<WorkflowGraphResponse>>, ApiError> {
    let project_id = parse_project_id_query(query.project_id.as_deref())?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let definitions = state
        .repos
        .workflow_graph_repo
        .list_by_project(project_id)
        .await?;
    Ok(Json(
        definitions
            .into_iter()
            .map(workflow_graph_to_contract_response)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

pub async fn create_workflow_graph(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateWorkflowGraphRequest>,
) -> Result<Json<WorkflowGraphResponse>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let definition = WorkflowGraph::new(
        project_id,
        req.key,
        req.name,
        req.description,
        DefinitionSource::UserAuthored,
        req.entry_activity_key,
        req.activities,
        req.transitions,
    )
    .map_err(ApiError::BadRequest)?;
    let service = ActivityLifecycleCatalogService::new(
        state.repos.agent_procedure_repo.as_ref(),
        state.repos.workflow_graph_repo.as_ref(),
    );
    let saved = service.upsert_workflow_graph(definition).await?;
    Ok(Json(workflow_graph_to_contract_response(saved)?))
}

pub async fn get_workflow_graph(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<WorkflowGraphResponse>, ApiError> {
    let id = parse_uuid(&id, "activity_lifecycle_id")?;
    let definition = state
        .repos
        .workflow_graph_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_graph 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(workflow_graph_to_contract_response(definition)?))
}

pub async fn update_workflow_graph(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkflowGraphRequest>,
) -> Result<Json<WorkflowGraphResponse>, ApiError> {
    let id = parse_uuid(&id, "activity_lifecycle_id")?;
    let mut definition = state
        .repos
        .workflow_graph_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_graph 不存在: {id}")))?;
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
        state.repos.agent_procedure_repo.as_ref(),
        state.repos.workflow_graph_repo.as_ref(),
    );
    let saved = service.upsert_workflow_graph(definition).await?;
    Ok(Json(workflow_graph_to_contract_response(saved)?))
}

pub async fn validate_workflow_graph(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ValidateWorkflowGraphRequest>,
) -> Result<Json<WorkflowValidationResponse>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    match WorkflowGraph::new(
        project_id,
        req.key,
        req.name,
        req.description,
        DefinitionSource::UserAuthored,
        req.entry_activity_key,
        req.activities,
        req.transitions,
    ) {
        Ok(definition) => {
            let service = ActivityLifecycleCatalogService::new(
                state.repos.agent_procedure_repo.as_ref(),
                state.repos.workflow_graph_repo.as_ref(),
            );
            let issues = service.validate_workflow_graph(&definition).await?;
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

pub async fn preflight_workflow_script(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<PreflightWorkflowScriptRequest>,
) -> Result<Json<PreflightWorkflowScriptResponse>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let source_digest = workflow_script_source_digest(&req.source_text);
    let source_ref = OrchestrationSourceRef::Inline {
        source_digest: source_digest.clone(),
    };
    let evaluator = agentdash_infrastructure::RhaiWorkflowScriptEvaluator::new();
    let compiler = ScriptCompiler;
    let mut provenance =
        WorkflowScriptProvenance::new(WorkflowScriptProvenanceSource::UserAuthored);
    provenance.created_by = Some(current_user.user_id.clone());
    provenance.runtime_session_id = req.runtime_session_id.clone();

    let output = WorkflowScriptPreflightService::preflight(WorkflowScriptPreflightInput {
        evaluator: &evaluator,
        compiler: &compiler,
        source_text: &req.source_text,
        ctx: workflow_script_eval_context(project_id, &current_user.user_id, req.ctx),
        args: req.args,
        source_ref,
        provenance,
    });

    let valid = !output.has_blocking_diagnostics();
    let source_ref = serde_json::to_value(&output.source_ref).map_err(|error| {
        ApiError::Internal(format!("序列化 workflow script source_ref 失败: {error}"))
    })?;
    let plan_snapshot = match &output.plan_snapshot {
        Some(plan_snapshot) => Some(serde_json::to_value(plan_snapshot).map_err(|error| {
            ApiError::Internal(format!(
                "序列化 workflow script plan_snapshot 失败: {error}"
            ))
        })?),
        None => None,
    };

    Ok(Json(PreflightWorkflowScriptResponse {
        valid,
        source_digest,
        source_ref,
        raw_builder_document: output.raw_builder_document,
        plan_snapshot,
        plan_preview: output
            .plan_preview
            .map(|preview| WorkflowScriptPlanPreviewDto {
                plan_digest: preview.plan_digest,
                node_count: preview.node_count,
                entry_node_ids: preview.entry_node_ids,
                nodes: preview
                    .nodes
                    .into_iter()
                    .map(|node| WorkflowScriptPlanPreviewNodeDto {
                        node_id: node.node_id,
                        node_path: node.node_path,
                        kind: node.kind,
                        label: node.label,
                    })
                    .collect(),
            }),
        capability_summary: workflow_script_capability_summary_dto(output.capability_summary),
        diagnostics: output
            .diagnostics
            .into_iter()
            .map(|diagnostic| WorkflowScriptPreflightDiagnosticDto {
                code: diagnostic.code,
                severity: contract_validation_severity(diagnostic.severity),
                message: diagnostic.message,
                source_path: diagnostic.source_path,
            })
            .collect(),
    }))
}

pub async fn delete_workflow_graph(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteWorkflowGraphResponse>, ApiError> {
    let id = parse_uuid(&id, "activity_lifecycle_id")?;
    let definition = state
        .repos
        .workflow_graph_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("workflow_graph 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    state.repos.workflow_graph_repo.delete(id).await?;
    Ok(Json(DeleteWorkflowGraphResponse { deleted: true }))
}

pub async fn start_lifecycle_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<StartWorkflowRunRequest>,
) -> Result<Json<LifecycleRunView>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let workflow_graph_ref = workflow_graph_ref_from_start_request(project_id, &req)?;
    let dispatch_service = LifecycleDispatchService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.workflow_graph_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.lifecycle_subject_association_repo.as_ref(),
        state.repos.lifecycle_gate_repo.as_ref(),
        state.repos.agent_lineage_repo.as_ref(),
    )
    .with_anchor_repo(state.repos.execution_anchor_repo.as_ref())
    .with_runtime_session_creator(state.repos.runtime_session_creator.as_ref());
    let dispatch_result = dispatch_service
        .start_lifecycle_run(&LifecycleRunStartIntent {
            project_id,
            source: ExecutionSource::Api,
            workflow_graph_ref,
        })
        .await?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(dispatch_result.run_ref)
        .await?
        .ok_or_else(|| {
            ApiError::Internal(format!(
                "Lifecycle dispatch 未持久化 run {}",
                dispatch_result.run_ref
            ))
        })?;
    let launcher = OrchestrationExecutorLauncher::new(state.repos.clone())
        .with_function_runner(state.services.function_runner.clone());
    launcher.drain_ready_nodes(run.id).await?;
    let latest_run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run.id)
        .await?
        .unwrap_or(run);
    Ok(Json(
        lifecycle_run_to_contract_view(&state, &latest_run).await?,
    ))
}

pub async fn get_lifecycle_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
) -> Result<Json<LifecycleRunView>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let run = load_lifecycle_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(lifecycle_run_to_contract_view(&state, &run).await?))
}

pub async fn submit_orchestration_human_decision(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(run_id): Path<String>,
    Json(req): Json<SubmitOrchestrationHumanDecisionRequest>,
) -> Result<Json<SubmitOrchestrationHumanDecisionResponse>, ApiError> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let orchestration_id = parse_uuid(&req.orchestration_id, "orchestration_id")?;
    let run = load_lifecycle_run(&state, run_id).await?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        run.project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let launcher = OrchestrationExecutorLauncher::new(state.repos.clone())
        .with_function_runner(state.services.function_runner.clone());
    let result = launcher
        .submit_human_gate_decision(SubmitHumanGateDecisionInput {
            run_id,
            orchestration_id,
            node_path: req.node_path,
            attempt: req.attempt,
            decision: req.decision,
            resolved_by: req
                .resolved_by
                .unwrap_or_else(|| current_user.user_id.to_string()),
        })
        .await?;
    let view =
        lifecycle_run_view_builder::build_lifecycle_run_view(&state.repos, &result.run).await?;
    Ok(Json(SubmitOrchestrationHumanDecisionResponse {
        run: lifecycle_run_view_to_contract(view),
        gate_id: result.gate_id.to_string(),
    }))
}

pub async fn create_agent_procedure(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateAgentProcedureRequest>,
) -> Result<Json<AgentProcedureResponse>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let definition = AgentProcedure::new(
        project_id,
        req.key,
        req.name,
        req.description,
        DefinitionSource::UserAuthored,
        req.contract,
    )
    .map_err(ApiError::BadRequest)?;
    let saved = upsert_agent_procedure(state.as_ref(), definition).await?;
    Ok(Json(agent_procedure_to_contract_response(saved)?))
}

pub async fn get_agent_procedure(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<AgentProcedureResponse>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let definition = state
        .repos
        .agent_procedure_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("agent_procedure 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(agent_procedure_to_contract_response(definition)?))
}

pub async fn update_agent_procedure(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentProcedureRequest>,
) -> Result<Json<AgentProcedureResponse>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let mut definition = state
        .repos
        .agent_procedure_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("agent_procedure 不存在: {id}")))?;
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
    let saved = upsert_agent_procedure(state.as_ref(), definition).await?;
    Ok(Json(agent_procedure_to_contract_response(saved)?))
}

pub async fn validate_agent_procedure(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ValidateAgentProcedureRequest>,
) -> Result<Json<WorkflowValidationResponse>, ApiError> {
    let project_id = parse_uuid_required(&req.project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    match AgentProcedure::new(
        project_id,
        req.key,
        req.name,
        req.description,
        DefinitionSource::UserAuthored,
        req.contract,
    ) {
        Ok(definition) => {
            let mut issues = definition.validate_full();
            issues.extend(validate_workflow_graph_references(state.as_ref(), &definition).await?);
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

pub async fn delete_agent_procedure(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteAgentProcedureResponse>, ApiError> {
    let id = parse_uuid(&id, "workflow_id")?;
    let definition = state
        .repos
        .agent_procedure_repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("agent_procedure 不存在: {id}")))?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        definition.project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let procedure_key = definition.key.clone();
    let referencing_activities: Vec<String> = state
        .repos
        .workflow_graph_repo
        .list_by_project(definition.project_id)
        .await?
        .into_iter()
        .flat_map(|lifecycle| {
            let lifecycle_key = lifecycle.key.clone();
            let procedure_key = procedure_key.clone();
            lifecycle
                .activities
                .into_iter()
                .filter_map(move |activity| match activity.executor {
                    ActivityExecutorSpec::Agent(agent) if agent.procedure_key == procedure_key => {
                        Some(format!("{lifecycle_key}.{}", activity.key))
                    }
                    _ => None,
                })
        })
        .collect();
    if !referencing_activities.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "workflow `{}` 仍被 Activity Lifecycle 引用，不能删除：{}",
            procedure_key,
            referencing_activities.join("、")
        )));
    }
    state.repos.agent_procedure_repo.delete(id).await?;
    Ok(Json(DeleteAgentProcedureResponse { deleted: true }))
}

fn agent_procedure_to_contract_response(
    definition: AgentProcedure,
) -> Result<AgentProcedureResponse, ApiError> {
    Ok(AgentProcedureResponse {
        id: definition.id.to_string(),
        project_id: definition.project_id.to_string(),
        key: definition.key,
        name: definition.name,
        description: definition.description,
        target_kinds: default_workflow_target_kinds(),
        source: definition_source_to_contract(definition.source),
        installed_source: definition
            .installed_source
            .map(installed_asset_source_to_contract),
        version: definition.version,
        contract: domain_to_contract_value(definition.contract, "agent_procedure.contract")?,
        created_at: definition.created_at.to_rfc3339(),
        updated_at: definition.updated_at.to_rfc3339(),
    })
}

fn workflow_graph_to_contract_response(
    definition: WorkflowGraph,
) -> Result<WorkflowGraphResponse, ApiError> {
    Ok(WorkflowGraphResponse {
        id: definition.id.to_string(),
        project_id: definition.project_id.to_string(),
        key: definition.key,
        name: definition.name,
        description: definition.description,
        target_kinds: default_workflow_target_kinds(),
        source: definition_source_to_contract(definition.source),
        installed_source: definition
            .installed_source
            .map(installed_asset_source_to_contract),
        version: definition.version,
        entry_activity_key: definition.entry_activity_key,
        activities: domain_to_contract_value(definition.activities, "workflow_graph.activities")?,
        transitions: domain_to_contract_value(
            definition.transitions,
            "workflow_graph.transitions",
        )?,
        created_at: definition.created_at.to_rfc3339(),
        updated_at: definition.updated_at.to_rfc3339(),
    })
}

fn default_workflow_target_kinds() -> Vec<WorkflowTargetKind> {
    vec![WorkflowTargetKind::Story]
}

fn definition_source_to_contract(
    source: agentdash_domain::workflow::DefinitionSource,
) -> agentdash_contracts::workflow::DefinitionSource {
    match source {
        agentdash_domain::workflow::DefinitionSource::BuiltinSeed => {
            agentdash_contracts::workflow::DefinitionSource::BuiltinSeed
        }
        agentdash_domain::workflow::DefinitionSource::UserAuthored => {
            agentdash_contracts::workflow::DefinitionSource::UserAuthored
        }
        agentdash_domain::workflow::DefinitionSource::Cloned => {
            agentdash_contracts::workflow::DefinitionSource::Cloned
        }
    }
}

fn installed_asset_source_to_contract(
    source: agentdash_domain::shared_library::InstalledAssetSource,
) -> agentdash_contracts::shared_library::InstalledAssetSourceDto {
    agentdash_contracts::shared_library::InstalledAssetSourceDto {
        library_asset_id: source.library_asset_id.to_string(),
        source_ref: source.source_ref,
        source_version: source.source_version,
        source_digest: source.source_digest,
        installed_at: source.installed_at.to_rfc3339(),
    }
}

fn domain_to_contract_value<T, U>(value: T, field: &str) -> Result<U, ApiError>
where
    T: serde::Serialize,
    U: serde::de::DeserializeOwned,
{
    serde_json::from_value(
        serde_json::to_value(value)
            .map_err(|error| ApiError::Internal(format!("序列化 {field} 失败: {error}")))?,
    )
    .map_err(|error| ApiError::Internal(format!("转换 {field} contract DTO 失败: {error}")))
}

async fn load_lifecycle_run(state: &Arc<AppState>, run_id: Uuid) -> Result<LifecycleRun, ApiError> {
    state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))
}

async fn lifecycle_run_to_contract_view(
    state: &Arc<AppState>,
    run: &LifecycleRun,
) -> Result<LifecycleRunView, ApiError> {
    lifecycle_run_view_builder::build_lifecycle_run_view(&state.repos, run)
        .await
        .map(lifecycle_run_view_to_contract)
        .map_err(ApiError::from)
}

async fn upsert_agent_procedure(
    state: &AppState,
    definition: AgentProcedure,
) -> Result<AgentProcedure, ApiError> {
    let issues = validate_workflow_graph_references(state, &definition).await?;
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
        .agent_procedure_repo
        .get_by_project_and_key(definition.project_id, &definition.key)
        .await?
    {
        let mut updated = definition;
        updated.id = existing.id;
        updated.version = existing.version + 1;
        updated.created_at = existing.created_at;
        updated.updated_at = chrono::Utc::now();
        state.repos.agent_procedure_repo.update(&updated).await?;
        return Ok(updated);
    }

    state.repos.agent_procedure_repo.create(&definition).await?;
    Ok(definition)
}

async fn validate_workflow_graph_references(
    _state: &AppState,
    _definition: &AgentProcedure,
) -> Result<Vec<ValidationIssue>, ApiError> {
    Ok(Vec::new())
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

fn workflow_script_eval_context(project_id: Uuid, user_id: &str, ctx: Option<Value>) -> Value {
    let mut object = match ctx {
        Some(Value::Object(object)) => object,
        Some(value) => {
            let mut object = Map::new();
            object.insert("input".to_string(), value);
            object
        }
        None => Map::new(),
    };
    object
        .entry("project_id".to_string())
        .or_insert_with(|| json!(project_id));
    object
        .entry("user_id".to_string())
        .or_insert_with(|| json!(user_id));
    Value::Object(object)
}

fn contract_validation_severity(severity: ValidationSeverity) -> ContractValidationSeverity {
    match severity {
        ValidationSeverity::Error => ContractValidationSeverity::Error,
        ValidationSeverity::Warning => ContractValidationSeverity::Warning,
    }
}

fn workflow_script_capability_summary_dto(
    summary: WorkflowScriptCapabilitySummary,
) -> WorkflowScriptCapabilitySummaryDto {
    WorkflowScriptCapabilitySummaryDto {
        agent_procedure_keys: summary.agent_procedure_keys,
        function_api_endpoints: summary
            .function_api_endpoints
            .into_iter()
            .map(|endpoint| WorkflowScriptApiEndpointDto {
                method: endpoint.method,
                url: endpoint.url,
            })
            .collect(),
        local_effect_capabilities: summary.local_effect_capabilities,
        bash_commands: summary
            .bash_commands
            .into_iter()
            .map(|command| WorkflowScriptBashCommandDto {
                command: command.command,
                args: command.args,
                working_directory: command.working_directory,
            })
            .collect(),
        human_gates: summary
            .human_gates
            .into_iter()
            .map(|gate| WorkflowScriptHumanGateCapabilityDto {
                name: gate.name,
                form_schema: gate.form_schema,
                decision_port: gate.decision_port,
            })
            .collect(),
    }
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

fn workflow_graph_ref_from_start_request(
    project_id: Uuid,
    req: &StartWorkflowRunRequest,
) -> Result<WorkflowGraphRef, ApiError> {
    let lifecycle_id = parse_optional_uuid(req.lifecycle_id.as_deref(), "lifecycle_id")?;
    let lifecycle_key = req.lifecycle_key.clone().and_then(normalize_string);
    match (lifecycle_id, lifecycle_key) {
        (Some(_), Some(_)) => Err(ApiError::BadRequest(
            "lifecycle_id 与 lifecycle_key 只能提供一个".to_string(),
        )),
        (None, None) => Err(ApiError::BadRequest(
            "必须提供 lifecycle_id 或 lifecycle_key".to_string(),
        )),
        (Some(id), None) => Ok(WorkflowGraphRef::ById(id)),
        (None, Some(key)) => Ok(WorkflowGraphRef::ByKey { project_id, key }),
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
