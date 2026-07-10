use std::sync::Arc;

use agentdash_application_ports::operation_script::{
    OperationScriptLimits, OperationScriptPreflightToken,
};
use agentdash_application_runtime_gateway::{
    BoundOperationHost, HostInvocationOptions, HostOperationInvocation, HostOperationScriptProgram,
    OperationReadiness, UserWorkshopOperationHost,
};
use agentdash_contracts::interaction::{
    InteractionOperationRefDto, OperationScriptLimitsDto, OperationScriptPreflightTokenDto,
    OperationScriptProgramDto, OperationWorkshopContextDto, OperationWorkshopDescriptorDto,
    OperationWorkshopInvokeRequestDto, OperationWorkshopInvokeResponseDto,
    OperationWorkshopScriptPreflightRequestDto, OperationWorkshopScriptPreflightResponseDto,
    OperationWorkshopScriptRunRequestDto, OperationWorkshopScriptRunResponseDto,
    OperationWorkshopSurfaceDto, OperationWorkshopSurfaceRequestDto,
};
use agentdash_domain::interaction::InteractionOwner;
use agentdash_domain::operation::{OperationEffect, OperationRef, OperationReplayPolicy};
use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

#[derive(Debug, serde::Deserialize)]
struct ProjectWorkshopPath {
    project_id: String,
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/operation-workshop/surface",
            axum::routing::post(surface),
        )
        .route(
            "/projects/{project_id}/operation-workshop/invoke",
            axum::routing::post(invoke),
        )
        .route(
            "/projects/{project_id}/operation-workshop/scripts/preflight",
            axum::routing::post(script_preflight),
        )
        .route(
            "/projects/{project_id}/operation-workshop/scripts/run",
            axum::routing::post(script_run),
        )
}

async fn surface(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(path): Path<ProjectWorkshopPath>,
    Json(request): Json<OperationWorkshopSurfaceRequestDto>,
) -> Result<Json<OperationWorkshopSurfaceDto>, ApiError> {
    let project_id = project_id(&state, &user, &path.project_id).await?;
    let host = resolve_host(&state, &user, project_id, request.context).await?;
    let surface = host
        .discover(CancellationToken::new())
        .await
        .map_err(|error| ApiError::Forbidden(error.to_string()))?;
    Ok(Json(OperationWorkshopSurfaceDto {
        authority_revision: surface.authority_revision,
        operations: surface
            .catalog
            .descriptors()
            .into_iter()
            .map(|descriptor| {
                let (ready, unavailable_reason) = match &descriptor.readiness {
                    OperationReadiness::Ready => (true, None),
                    OperationReadiness::Unavailable { message, .. } => {
                        (false, Some(message.clone()))
                    }
                };
                OperationWorkshopDescriptorDto {
                    operation_ref: operation_ref_to_dto(&descriptor.operation_ref),
                    title: descriptor.title.clone(),
                    description: descriptor.description.clone(),
                    input_schema: descriptor.input_schema.clone(),
                    output_schema: descriptor.output_schema.clone(),
                    effect: match descriptor.effect {
                        OperationEffect::Read => "read",
                        OperationEffect::LocalMutation => "local_mutation",
                        OperationEffect::ExternalSideEffect => "external_side_effect",
                    }
                    .to_string(),
                    replay_policy: match descriptor.replay_policy {
                        OperationReplayPolicy::NonReplayable => "non_replayable",
                        OperationReplayPolicy::Idempotent => "idempotent",
                        OperationReplayPolicy::ReplaySafe => "replay_safe",
                    }
                    .to_string(),
                    required_capabilities: descriptor
                        .required_capabilities
                        .iter()
                        .cloned()
                        .collect(),
                    ready,
                    unavailable_reason,
                }
            })
            .collect(),
    }))
}

async fn invoke(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(path): Path<ProjectWorkshopPath>,
    Json(request): Json<OperationWorkshopInvokeRequestDto>,
) -> Result<Json<OperationWorkshopInvokeResponseDto>, ApiError> {
    let project_id = project_id(&state, &user, &path.project_id).await?;
    let host = resolve_host(&state, &user, project_id, request.context).await?;
    let result = host
        .invoke(
            HostOperationInvocation {
                operation_ref: operation_ref_from_dto(request.operation_ref)?,
                input: request.input,
                idempotency_key: request.idempotency_key,
            },
            HostInvocationOptions::default(),
            CancellationToken::new(),
        )
        .await
        .map_err(|error| ApiError::BadRequestWithCode {
            message: error.to_string(),
            error_code: error.code().to_string(),
        })?;
    Ok(Json(OperationWorkshopInvokeResponseDto {
        result: serde_json::to_value(result)
            .map_err(|error| ApiError::Internal(error.to_string()))?,
    }))
}

async fn script_preflight(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(path): Path<ProjectWorkshopPath>,
    Json(request): Json<OperationWorkshopScriptPreflightRequestDto>,
) -> Result<Json<OperationWorkshopScriptPreflightResponseDto>, ApiError> {
    let project_id = project_id(&state, &user, &path.project_id).await?;
    let host = resolve_host(&state, &user, project_id, request.context)
        .await?
        .operation_script(state.services.operation_script_engine.clone());
    let output = host
        .preflight(program_from_dto(request.program)?, CancellationToken::new())
        .await
        .map_err(script_error)?;
    Ok(Json(OperationWorkshopScriptPreflightResponseDto {
        token: token_to_dto(output.token),
        source_digest: output.source_digest,
        manifest_digest: output.manifest_digest,
    }))
}

async fn script_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path(path): Path<ProjectWorkshopPath>,
    Json(request): Json<OperationWorkshopScriptRunRequestDto>,
) -> Result<Json<OperationWorkshopScriptRunResponseDto>, ApiError> {
    let project_id = project_id(&state, &user, &path.project_id).await?;
    let host = resolve_host(&state, &user, project_id, request.context)
        .await?
        .operation_script(state.services.operation_script_engine.clone());
    let output = host
        .run(
            program_from_dto(request.program)?,
            token_from_dto(request.token)?,
            CancellationToken::new(),
        )
        .await
        .map_err(script_error)?;
    Ok(Json(OperationWorkshopScriptRunResponseDto {
        outcome: serde_json::to_value(output)
            .map_err(|error| ApiError::Internal(error.to_string()))?,
    }))
}

async fn project_id(
    state: &Arc<AppState>,
    user: &agentdash_integration_api::AuthIdentity,
    raw: &str,
) -> Result<Uuid, ApiError> {
    let project_id = parse_uuid(raw, "project_id")?;
    load_project_with_permission(state, user, project_id, ProjectPermission::Use).await?;
    Ok(project_id)
}

async fn resolve_host(
    state: &Arc<AppState>,
    user: &agentdash_integration_api::AuthIdentity,
    project_id: Uuid,
    context: OperationWorkshopContextDto,
) -> Result<BoundOperationHost, ApiError> {
    let gateway = state.services.operation_gateway.clone();
    match context {
        OperationWorkshopContextDto::Project =>
            UserWorkshopOperationHost::project(gateway, user.clone(), project_id),
        OperationWorkshopContextDto::Canvas { definition_id } => {
            let definition_id = parse_uuid(&definition_id, "definition_id")?;
            let definition = state.repos.interaction_definition_repo.get(definition_id).await?
                .ok_or_else(|| ApiError::NotFound("Canvas definition 不存在".into()))?;
            if definition.project_id != project_id
                || matches!(definition.owner, InteractionOwner::User(ref owner) if owner != &user.user_id)
            {
                return Err(ApiError::Forbidden("Canvas definition 不属于当前 actor surface".into()));
            }
            UserWorkshopOperationHost::canvas(gateway, user.clone(), project_id, definition_id)
        }
        OperationWorkshopContextDto::Interaction { instance_id } => {
            let instance_id = parse_uuid(&instance_id, "instance_id")?;
            let instance = state.repos.interaction_instance_repo.get(instance_id).await?
                .ok_or_else(|| ApiError::NotFound("Interaction instance 不存在".into()))?;
            let revision = state.repos.interaction_definition_repo
                .get_revision(instance.definition_revision_id).await?
                .ok_or_else(|| ApiError::NotFound("Interaction definition revision 不存在".into()))?;
            if revision.project_id != project_id
                || matches!(instance.owner, InteractionOwner::User(ref owner) if owner != &user.user_id)
            {
                return Err(ApiError::Forbidden("Interaction instance 不属于当前 actor surface".into()));
            }
            UserWorkshopOperationHost::interaction(gateway, user.clone(), instance_id)
        }
        OperationWorkshopContextDto::ExtensionPanel { installation_id } => {
            let installation_id = parse_uuid(&installation_id, "installation_id")?;
            let installation = state.repos.project_extension_installation_repo
                .get_by_project_and_id(project_id, installation_id).await?
                .filter(|installation| installation.enabled)
                .ok_or_else(|| ApiError::NotFound("Extension installation 不存在".into()))?;
            UserWorkshopOperationHost::extension_panel(
                gateway, user.clone(), project_id, installation.id,
            )
        }
    }.map_err(|error| ApiError::BadRequest(error.to_string()))
}

fn program_from_dto(
    dto: OperationScriptProgramDto,
) -> Result<HostOperationScriptProgram, ApiError> {
    Ok(HostOperationScriptProgram {
        language: dto.language,
        host_api_version: dto.host_api_version,
        source: dto.source,
        input: dto.input,
        requested_operations: dto
            .requested_operations
            .into_iter()
            .map(operation_ref_from_dto)
            .collect::<Result<Vec<_>, _>>()?,
        limits: limits_from_dto(dto.limits)?,
    })
}

fn limits_from_dto(dto: OperationScriptLimitsDto) -> Result<OperationScriptLimits, ApiError> {
    let checked = |field: &'static str, value: u32| {
        usize::try_from(value).map_err(|_| ApiError::BadRequest(format!("{field} 超出范围")))
    };
    Ok(OperationScriptLimits {
        timeout_ms: u64::from(dto.timeout_ms),
        max_source_bytes: checked("max_source_bytes", dto.max_source_bytes)?,
        max_input_bytes: checked("max_input_bytes", dto.max_input_bytes)?,
        max_output_bytes: checked("max_output_bytes", dto.max_output_bytes)?,
        max_rhai_operations: u64::from(dto.max_rhai_operations),
        max_call_levels: checked("max_call_levels", dto.max_call_levels)?,
        max_string_size: checked("max_string_size", dto.max_string_size)?,
        max_array_size: checked("max_array_size", dto.max_array_size)?,
        max_map_size: checked("max_map_size", dto.max_map_size)?,
        max_operation_calls: checked("max_operation_calls", dto.max_operation_calls)?,
        max_parallel_operations: checked("max_parallel_operations", dto.max_parallel_operations)?,
    })
}

fn operation_ref_from_dto(dto: InteractionOperationRefDto) -> Result<OperationRef, ApiError> {
    OperationRef::new(
        dto.namespace,
        dto.provider_key,
        dto.operation_key,
        dto.contract_version,
    )
    .map_err(|error| ApiError::BadRequest(error.to_string()))
}

fn operation_ref_to_dto(value: &OperationRef) -> InteractionOperationRefDto {
    InteractionOperationRefDto {
        namespace: value.provider.namespace.clone(),
        provider_key: value.provider.provider_key.clone(),
        operation_key: value.operation_key.clone(),
        contract_version: value.contract_version,
    }
}

fn token_to_dto(token: OperationScriptPreflightToken) -> OperationScriptPreflightTokenDto {
    OperationScriptPreflightTokenDto {
        plan_id: token.plan_id.to_string(),
        binding_digest: token.binding_digest,
        issued_at: token.issued_at.to_rfc3339(),
        expires_at: token.expires_at.to_rfc3339(),
        signature: token.signature,
    }
}

fn token_from_dto(
    dto: OperationScriptPreflightTokenDto,
) -> Result<OperationScriptPreflightToken, ApiError> {
    Ok(OperationScriptPreflightToken {
        plan_id: parse_uuid(&dto.plan_id, "plan_id")?,
        binding_digest: dto.binding_digest,
        issued_at: parse_time(&dto.issued_at, "issued_at")?,
        expires_at: parse_time(&dto.expires_at, "expires_at")?,
        signature: dto.signature,
    })
}

fn parse_time(raw: &str, field: &'static str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| ApiError::BadRequest(format!("{field} 必须为 RFC3339")))
}

fn parse_uuid(raw: &str, field: &'static str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("{field} 必须为 UUID")))
}

fn script_error(
    error: agentdash_application_ports::operation_script::OperationScriptError,
) -> ApiError {
    use agentdash_application_ports::operation_script::OperationScriptError;
    match error {
        OperationScriptError::CapacityExceeded => ApiError::ServiceUnavailable(error.to_string()),
        OperationScriptError::TokenExpired | OperationScriptError::InvalidPlan { .. } => {
            ApiError::Conflict(error.to_string())
        }
        _ => ApiError::BadRequest(error.to_string()),
    }
}
