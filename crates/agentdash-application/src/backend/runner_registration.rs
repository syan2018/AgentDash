use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendShareScopeKind, ProjectBackendAccess, ProjectBackendAccessStatus,
    RunnerRegistrationToken, RunnerRegistrationTokenPlaintext, RunnerRegistrationTokenStatus,
    verify_runner_registration_secret,
};

use crate::ApplicationError;
use crate::backend::{EnsureRunnerProjectRuntimeInput, ensure_runner_project_runtime_record};
use crate::repository_set::RepositorySet;

const DEFAULT_TOKEN_TTL_DAYS: i64 = 30;

#[derive(Debug, Clone)]
pub struct RunnerRegistrationTokenCreateInput {
    pub project_id: Uuid,
    pub name: String,
    pub created_by_user_id: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub default_capability_slot: Option<String>,
    pub machine_policy: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct RunnerRegistrationTokenCreateResult {
    pub token: RunnerRegistrationToken,
    pub registration_token: String,
}

#[derive(Debug, Clone)]
pub struct RunnerRegistrationClaimInput {
    pub registration_token: String,
    pub machine_id: String,
    pub machine_label: Option<String>,
    pub runner_name: Option<String>,
    pub client_version: Option<String>,
    pub device: serde_json::Value,
    pub executor_enabled: bool,
    pub capability_slot: Option<String>,
    pub relay_ws_url: String,
}

#[derive(Debug, Clone)]
pub struct RunnerRegistrationClaimResult {
    pub backend_id: String,
    pub name: String,
    pub relay_ws_url: String,
    pub auth_token: String,
    pub machine_id: String,
    pub machine_label: String,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerRegistrationClaimError {
    #[error("runner registration token 缺失")]
    MissingToken,
    #[error("runner registration token 无效")]
    InvalidToken,
    #[error("runner registration token 已过期")]
    ExpiredToken,
    #[error("runner registration token 已撤销")]
    RevokedToken,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
}

pub async fn create_runner_registration_token(
    repos: &RepositorySet,
    input: RunnerRegistrationTokenCreateInput,
) -> Result<RunnerRegistrationTokenCreateResult, ApplicationError> {
    let name = normalize_required("name", &input.name)?;
    let default_capability_slot = normalize_optional_string(input.default_capability_slot)
        .unwrap_or_else(|| "default".to_string());
    let expires_at = input
        .expires_at
        .unwrap_or_else(|| Utc::now() + Duration::days(DEFAULT_TOKEN_TTL_DAYS));
    if expires_at <= Utc::now() {
        return Err(ApplicationError::BadRequest(
            "expires_at 必须晚于当前时间".to_string(),
        ));
    }
    let issued = RunnerRegistrationToken::new_project_scoped(
        input.project_id,
        name,
        input.created_by_user_id,
        expires_at,
        default_capability_slot,
        normalize_machine_policy(input.machine_policy)?,
    );
    repos
        .runner_registration_token_repo
        .create(&issued.token)
        .await
        .map_err(ApplicationError::from)?;
    Ok(RunnerRegistrationTokenCreateResult {
        token: issued.token,
        registration_token: issued.registration_token,
    })
}

pub async fn list_runner_registration_tokens(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<Vec<RunnerRegistrationToken>, ApplicationError> {
    repos
        .runner_registration_token_repo
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn revoke_runner_registration_token(
    repos: &RepositorySet,
    project_id: Uuid,
    token_id: &str,
) -> Result<RunnerRegistrationToken, ApplicationError> {
    let token = load_project_token(repos, project_id, token_id).await?;
    let revoked_at = Utc::now();
    repos
        .runner_registration_token_repo
        .revoke(&token.id, revoked_at)
        .await
        .map_err(ApplicationError::from)?;
    let mut token = token;
    token.revoked_at = token.revoked_at.or(Some(revoked_at));
    token.updated_at = revoked_at;
    Ok(token)
}

pub async fn rotate_runner_registration_token(
    repos: &RepositorySet,
    project_id: Uuid,
    token_id: &str,
    created_by_user_id: String,
) -> Result<RunnerRegistrationTokenCreateResult, ApplicationError> {
    let old = load_project_token(repos, project_id, token_id).await?;
    repos
        .runner_registration_token_repo
        .revoke(&old.id, Utc::now())
        .await
        .map_err(ApplicationError::from)?;
    create_runner_registration_token(
        repos,
        RunnerRegistrationTokenCreateInput {
            project_id,
            name: old.name,
            created_by_user_id,
            expires_at: Some(
                old.expires_at
                    .max(Utc::now() + Duration::days(DEFAULT_TOKEN_TTL_DAYS)),
            ),
            default_capability_slot: Some(old.default_capability_slot),
            machine_policy: old.machine_policy,
        },
    )
    .await
}

pub async fn claim_runner_registration_token(
    repos: &RepositorySet,
    input: RunnerRegistrationClaimInput,
) -> Result<RunnerRegistrationClaimResult, RunnerRegistrationClaimError> {
    let plaintext = RunnerRegistrationTokenPlaintext::parse(&input.registration_token)
        .ok_or(RunnerRegistrationClaimError::InvalidToken)?;
    let token = repos
        .runner_registration_token_repo
        .get_by_id(&plaintext.token_id)
        .await
        .map_err(claim_internal_from_domain)?
        .ok_or(RunnerRegistrationClaimError::InvalidToken)?;
    if !verify_runner_registration_secret(&plaintext.secret, &token.token_secret_hash) {
        return Err(RunnerRegistrationClaimError::InvalidToken);
    }

    let now = Utc::now();
    match token.status_at(now) {
        RunnerRegistrationTokenStatus::Active => {}
        RunnerRegistrationTokenStatus::Expired => {
            return Err(RunnerRegistrationClaimError::ExpiredToken);
        }
        RunnerRegistrationTokenStatus::Revoked => {
            return Err(RunnerRegistrationClaimError::RevokedToken);
        }
    }

    let project_exists = repos
        .project_repo
        .get_by_id(token.project_id)
        .await
        .map_err(claim_internal_from_domain)?
        .is_some();
    if !project_exists {
        return Err(RunnerRegistrationClaimError::Forbidden(
            "runner registration token scope 不可用".to_string(),
        ));
    }

    let capability_slot = normalize_optional_string(input.capability_slot)
        .unwrap_or_else(|| token.default_capability_slot.clone());
    let ensured = ensure_runner_project_runtime_record(
        repos,
        EnsureRunnerProjectRuntimeInput {
            owner_user_id: token.created_by_user_id.clone(),
            project_id: token.project_id,
            machine_id: input.machine_id,
            machine_label: input.machine_label,
            capability_slot: Some(capability_slot),
            runner_name: input.runner_name,
            executor_enabled: input.executor_enabled,
            client_version: input.client_version,
            device: input.device,
            relay_ws_url: input.relay_ws_url,
        },
    )
    .await
    .map_err(claim_error_from_application)?;

    ensure_active_project_backend_access(
        repos,
        token.project_id,
        &ensured.backend.id,
        &token.created_by_user_id,
    )
    .await
    .map_err(claim_error_from_application)?;

    let claimed_at = Utc::now();
    repos
        .runner_registration_token_repo
        .record_usage(&token.id, &ensured.backend.id, claimed_at)
        .await
        .map_err(claim_internal_from_domain)?;

    Ok(RunnerRegistrationClaimResult {
        backend_id: ensured.backend.id,
        name: ensured.backend.name,
        relay_ws_url: ensured.backend.endpoint,
        auth_token: ensured.auth_token,
        machine_id: ensured.machine_id,
        machine_label: ensured.machine_label,
        share_scope_kind: ensured.backend.share_scope_kind,
        share_scope_id: ensured.share_scope_id,
        capability_slot: ensured.capability_slot,
        claimed_at,
    })
}

async fn load_project_token(
    repos: &RepositorySet,
    project_id: Uuid,
    token_id: &str,
) -> Result<RunnerRegistrationToken, ApplicationError> {
    let token = repos
        .runner_registration_token_repo
        .get_by_id(token_id)
        .await?
        .ok_or_else(|| {
            ApplicationError::NotFound("Runner registration token 不存在".to_string())
        })?;
    if token.project_id != project_id {
        return Err(ApplicationError::NotFound(
            "Runner registration token 不存在".to_string(),
        ));
    }
    Ok(token)
}

async fn ensure_active_project_backend_access(
    repos: &RepositorySet,
    project_id: Uuid,
    backend_id: &str,
    created_by_user_id: &str,
) -> Result<(), ApplicationError> {
    if repos
        .project_backend_access_repo
        .get_active_for_project_backend(project_id, backend_id)
        .await?
        .is_some()
    {
        return Ok(());
    }

    if let Some(existing) = repos
        .project_backend_access_repo
        .list_by_project(project_id)
        .await?
        .into_iter()
        .find(|access| access.backend_id == backend_id)
    {
        if existing.status != ProjectBackendAccessStatus::Active {
            repos
                .project_backend_access_repo
                .set_status(existing.id, ProjectBackendAccessStatus::Active)
                .await?;
        }
        return Ok(());
    }

    let mut access = ProjectBackendAccess::new(
        project_id,
        backend_id.to_string(),
        Some(created_by_user_id.to_string()),
    );
    access.note = Some("runner_registration_token".to_string());
    match repos.project_backend_access_repo.create(&access).await {
        Ok(()) => Ok(()),
        Err(DomainError::Conflict { .. }) => {
            if repos
                .project_backend_access_repo
                .get_active_for_project_backend(project_id, backend_id)
                .await?
                .is_some()
            {
                Ok(())
            } else {
                Err(ApplicationError::Conflict(
                    "ProjectBackendAccess 并发创建冲突".to_string(),
                ))
            }
        }
        Err(error) => Err(ApplicationError::from(error)),
    }
}

fn normalize_required(field: &str, raw: &str) -> Result<String, ApplicationError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApplicationError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value.to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_machine_policy(
    value: serde_json::Value,
) -> Result<serde_json::Value, ApplicationError> {
    match value {
        serde_json::Value::Null => Ok(serde_json::json!({})),
        serde_json::Value::Object(_) => Ok(value),
        _ => Err(ApplicationError::BadRequest(
            "machine_policy 必须是 JSON object 或 null".to_string(),
        )),
    }
}

fn claim_internal_from_domain(error: DomainError) -> RunnerRegistrationClaimError {
    match error {
        DomainError::Conflict { .. } | DomainError::InvalidTransition { .. } => {
            RunnerRegistrationClaimError::Conflict(error.to_string())
        }
        DomainError::Forbidden { .. } => RunnerRegistrationClaimError::Forbidden(error.to_string()),
        DomainError::InvalidConfig(_) | DomainError::Serialization(_) => {
            RunnerRegistrationClaimError::BadRequest(error.to_string())
        }
        DomainError::NotFound { .. } => RunnerRegistrationClaimError::InvalidToken,
        DomainError::Database { .. } => {
            RunnerRegistrationClaimError::Internal("内部数据库错误".to_string())
        }
    }
}

fn claim_error_from_application(error: ApplicationError) -> RunnerRegistrationClaimError {
    match error {
        ApplicationError::BadRequest(message) | ApplicationError::InvalidConfig(message) => {
            RunnerRegistrationClaimError::BadRequest(message)
        }
        ApplicationError::Forbidden(message) => RunnerRegistrationClaimError::Forbidden(message),
        ApplicationError::Conflict(message) => RunnerRegistrationClaimError::Conflict(message),
        ApplicationError::NotFound(_) => RunnerRegistrationClaimError::Forbidden(
            "runner registration token scope 不可用".to_string(),
        ),
        ApplicationError::Unavailable(message) | ApplicationError::Internal(message) => {
            RunnerRegistrationClaimError::Internal(message)
        }
    }
}
