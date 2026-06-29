use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendRepository, BackendShareScopeKind, ProjectBackendAccessRepository,
    RunnerRegistrationToken, RunnerRegistrationTokenPlaintext, RunnerRegistrationTokenRepository,
    RunnerRegistrationTokenStatus, verify_runner_registration_secret,
};
use agentdash_domain::project::ProjectRepository;

use crate::ApplicationError;
use crate::backend::{
    EnrollLocalBackendRequest, EnrollmentSource, EnsureProjectBackendAccessGrantInput,
    ProjectBackendAccessGrantSource, enroll_local_backend, ensure_project_backend_access_grant,
};
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
    pub registration_source: String,
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
    claim_runner_registration_token_with_ports(
        repos.runner_registration_token_repo.as_ref(),
        repos.project_repo.as_ref(),
        repos.backend_repo.as_ref(),
        repos.project_backend_access_repo.as_ref(),
        input,
    )
    .await
}

async fn claim_runner_registration_token_with_ports(
    runner_registration_token_repo: &dyn RunnerRegistrationTokenRepository,
    project_repo: &dyn ProjectRepository,
    backend_repo: &dyn BackendRepository,
    project_backend_access_repo: &dyn ProjectBackendAccessRepository,
    input: RunnerRegistrationClaimInput,
) -> Result<RunnerRegistrationClaimResult, RunnerRegistrationClaimError> {
    let plaintext = RunnerRegistrationTokenPlaintext::parse(&input.registration_token)
        .ok_or(RunnerRegistrationClaimError::InvalidToken)?;
    let token = runner_registration_token_repo
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

    let project_exists = project_repo
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

    // 收束到统一 enrollment use case：runner 身份去项目化，share_scope=User(owner=token 创建者)，
    // 项目可见性靠下面的 ProjectBackendAccess active projection 承载。
    let enrolled = enroll_local_backend(
        backend_repo,
        EnrollmentSource::RunnerRegistrationToken {
            project_id: token.project_id,
            created_by_user_id: token.created_by_user_id.clone(),
        },
        EnrollLocalBackendRequest {
            machine_id: input.machine_id,
            machine_label: input.machine_label,
            capability_slot: Some(capability_slot),
            name: input.runner_name,
            executor_enabled: input.executor_enabled,
            client_version: input.client_version,
            device: input.device,
            relay_ws_url: input.relay_ws_url,
            rotate_token: false,
            scope: None,
            profile_id: None,
        },
    )
    .await
    .map_err(claim_error_from_application)?;

    ensure_project_backend_access_grant(
        project_backend_access_repo,
        EnsureProjectBackendAccessGrantInput {
            project_id: token.project_id,
            backend_id: enrolled.backend.id.clone(),
            source: ProjectBackendAccessGrantSource::RunnerRegistrationToken,
            created_by_user_id: Some(token.created_by_user_id.clone()),
            priority: None,
            root_policy: None,
            capability_policy: None,
            note: None,
        },
    )
    .await
    .map_err(claim_error_from_application)?;

    let claimed_at = Utc::now();
    runner_registration_token_repo
        .record_usage(&token.id, &enrolled.backend.id, claimed_at)
        .await
        .map_err(claim_internal_from_domain)?;

    Ok(RunnerRegistrationClaimResult {
        backend_id: enrolled.backend.id,
        name: enrolled.backend.name,
        relay_ws_url: enrolled.backend.endpoint,
        auth_token: enrolled.auth_token,
        machine_id: enrolled.machine_id,
        machine_label: enrolled.machine_label,
        share_scope_kind: enrolled.share_scope_kind,
        share_scope_id: enrolled.share_scope_id,
        capability_slot: enrolled.capability_slot,
        registration_source: enrolled.registration_source,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    use agentdash_domain::backend::{
        BackendConfig, BackendType, BackendVisibility, LocalBackendClaim, ProjectBackendAccess,
        ProjectBackendAccessStatus, UserPreferences, ViewConfig,
    };
    use agentdash_domain::project::{
        Project, ProjectRole, ProjectSubjectGrant, ProjectSubjectType,
    };
    use tokio::sync::Mutex;

    struct ClaimHarness {
        project_id: Uuid,
        token_repo: InMemoryRunnerRegistrationTokenRepository,
        project_repo: InMemoryProjectRepository,
        backend_repo: InMemoryBackendRepository,
        access_repo: InMemoryProjectBackendAccessRepository,
    }

    impl ClaimHarness {
        fn new() -> Self {
            let project_id = Uuid::new_v4();
            Self {
                project_id,
                token_repo: InMemoryRunnerRegistrationTokenRepository::default(),
                project_repo: InMemoryProjectRepository::new(project_id),
                backend_repo: InMemoryBackendRepository::default(),
                access_repo: InMemoryProjectBackendAccessRepository::default(),
            }
        }

        async fn issue_token(&self) -> RunnerRegistrationTokenIssuedForTest {
            let issued = RunnerRegistrationToken::new_project_scoped(
                self.project_id,
                "CI runner".to_string(),
                "user-owner".to_string(),
                Utc::now() + Duration::hours(1),
                "build".to_string(),
                serde_json::json!({}),
            );
            self.token_repo.insert(issued.token.clone()).await;
            RunnerRegistrationTokenIssuedForTest {
                token: issued.token,
                registration_token: issued.registration_token,
            }
        }

        async fn claim(
            &self,
            registration_token: String,
        ) -> Result<RunnerRegistrationClaimResult, RunnerRegistrationClaimError> {
            claim_runner_registration_token_with_ports(
                &self.token_repo,
                &self.project_repo,
                &self.backend_repo,
                &self.access_repo,
                RunnerRegistrationClaimInput {
                    registration_token,
                    machine_id: "machine-001".to_string(),
                    machine_label: Some("Builder 1".to_string()),
                    runner_name: Some("Linux Builder".to_string()),
                    client_version: Some("0.2.0".to_string()),
                    device: serde_json::json!({ "os": "linux" }),
                    executor_enabled: true,
                    capability_slot: None,
                    relay_ws_url: "wss://cloud.test/ws/backend".to_string(),
                },
            )
            .await
        }
    }

    struct RunnerRegistrationTokenIssuedForTest {
        token: RunnerRegistrationToken,
        registration_token: String,
    }

    #[derive(Default)]
    struct InMemoryRunnerRegistrationTokenRepository {
        tokens: Mutex<HashMap<String, RunnerRegistrationToken>>,
        usage_records: Mutex<Vec<(String, String, DateTime<Utc>)>>,
        fail_get_with_database: AtomicBool,
        fail_record_usage_with_database: AtomicBool,
    }

    impl InMemoryRunnerRegistrationTokenRepository {
        async fn insert(&self, token: RunnerRegistrationToken) {
            self.tokens.lock().await.insert(token.id.clone(), token);
        }

        async fn usage_count(&self) -> usize {
            self.usage_records.lock().await.len()
        }

        async fn token(&self, id: &str) -> RunnerRegistrationToken {
            self.tokens
                .lock()
                .await
                .get(id)
                .cloned()
                .expect("token should exist")
        }
    }

    #[async_trait::async_trait]
    impl RunnerRegistrationTokenRepository for InMemoryRunnerRegistrationTokenRepository {
        async fn create(&self, token: &RunnerRegistrationToken) -> Result<(), DomainError> {
            self.insert(token.clone()).await;
            Ok(())
        }

        async fn update(&self, token: &RunnerRegistrationToken) -> Result<(), DomainError> {
            self.insert(token.clone()).await;
            Ok(())
        }

        async fn get_by_id(
            &self,
            id: &str,
        ) -> Result<Option<RunnerRegistrationToken>, DomainError> {
            if self.fail_get_with_database.load(Ordering::SeqCst) {
                return Err(database_error("runner_registration_tokens.get_by_id"));
            }
            Ok(self.tokens.lock().await.get(id).cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<RunnerRegistrationToken>, DomainError> {
            Ok(self
                .tokens
                .lock()
                .await
                .values()
                .filter(|token| token.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn revoke(&self, id: &str, revoked_at: DateTime<Utc>) -> Result<(), DomainError> {
            if let Some(token) = self.tokens.lock().await.get_mut(id) {
                token.revoked_at = Some(revoked_at);
                token.updated_at = revoked_at;
            }
            Ok(())
        }

        async fn record_usage(
            &self,
            id: &str,
            backend_id: &str,
            used_at: DateTime<Utc>,
        ) -> Result<(), DomainError> {
            if self.fail_record_usage_with_database.load(Ordering::SeqCst) {
                return Err(database_error("runner_registration_tokens.record_usage"));
            }
            self.usage_records
                .lock()
                .await
                .push((id.to_string(), backend_id.to_string(), used_at));
            if let Some(token) = self.tokens.lock().await.get_mut(id) {
                token.last_used_at = Some(used_at);
                token.last_claimed_backend_id = Some(backend_id.to_string());
                token.updated_at = used_at;
            }
            Ok(())
        }
    }

    struct InMemoryProjectRepository {
        project_id: Uuid,
        exists: AtomicBool,
        fail_get_with_database: AtomicBool,
    }

    impl InMemoryProjectRepository {
        fn new(project_id: Uuid) -> Self {
            Self {
                project_id,
                exists: AtomicBool::new(true),
                fail_get_with_database: AtomicBool::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl ProjectRepository for InMemoryProjectRepository {
        async fn create(&self, _project: &Project) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError> {
            if self.fail_get_with_database.load(Ordering::SeqCst) {
                return Err(database_error("projects.get_by_id"));
            }
            if id != self.project_id || !self.exists.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let mut project = Project::new_with_creator(
                "Runner Project".to_string(),
                String::new(),
                "user-owner".to_string(),
            );
            project.id = self.project_id;
            Ok(Some(project))
        }

        async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
            Ok(Vec::new())
        }

        async fn update(&self, _project: &Project) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }

        async fn list_subject_grants(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(vec![ProjectSubjectGrant::new(
                project_id,
                ProjectSubjectType::User,
                "user-owner".to_string(),
                ProjectRole::Owner,
                "user-owner".to_string(),
            )])
        }

        async fn upsert_subject_grant(
            &self,
            _grant: &ProjectSubjectGrant,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_subject_grant(
            &self,
            _project_id: Uuid,
            _subject_type: ProjectSubjectType,
            _subject_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryBackendRepository {
        backends: Mutex<HashMap<String, BackendConfig>>,
        claims: Mutex<Vec<LocalBackendClaim>>,
        fail_ensure_with_database: AtomicBool,
        return_missing_auth_token: AtomicBool,
    }

    impl InMemoryBackendRepository {
        async fn claims(&self) -> Vec<LocalBackendClaim> {
            self.claims.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl BackendRepository for InMemoryBackendRepository {
        async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError> {
            self.backends
                .lock()
                .await
                .insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
            Ok(self.backends.lock().await.values().cloned().collect())
        }

        async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError> {
            self.backends
                .lock()
                .await
                .get(id)
                .cloned()
                .ok_or_else(|| not_found("backend", id))
        }

        async fn get_backend_by_auth_token(
            &self,
            token: &str,
        ) -> Result<BackendConfig, DomainError> {
            self.backends
                .lock()
                .await
                .values()
                .find(|backend| backend.auth_token.as_deref() == Some(token))
                .cloned()
                .ok_or_else(|| not_found("backend_auth_token", "mock"))
        }

        async fn ensure_local_backend(
            &self,
            claim: &LocalBackendClaim,
        ) -> Result<BackendConfig, DomainError> {
            if self.fail_ensure_with_database.load(Ordering::SeqCst) {
                return Err(database_error("backends.ensure_local_backend"));
            }
            self.claims.lock().await.push(claim.clone());
            let mut backends = self.backends.lock().await;
            if let Some(existing) = backends.get(&claim.backend_id) {
                return Ok(existing.clone());
            }
            let config = BackendConfig {
                id: claim.backend_id.clone(),
                name: claim.name.clone(),
                endpoint: claim.endpoint.clone(),
                auth_token: if self.return_missing_auth_token.load(Ordering::SeqCst) {
                    None
                } else {
                    Some(claim.auth_token.clone())
                },
                enabled: true,
                backend_type: BackendType::Local,
                owner_user_id: Some(claim.owner_user_id.clone()),
                profile_id: Some(claim.profile_id.clone()),
                device_id: None,
                machine_id: Some(claim.machine_id.clone()),
                machine_label: Some(claim.machine_label.clone()),
                visibility: claim.visibility,
                share_scope_kind: claim.share_scope_kind,
                share_scope_id: claim.share_scope_id.clone(),
                capability_slot: claim.capability_slot.clone(),
                device: claim.device.clone(),
                last_claimed_at: Some(Utc::now()),
            };
            backends.insert(config.id.clone(), config.clone());
            Ok(config)
        }

        async fn remove_backend(&self, id: &str) -> Result<(), DomainError> {
            self.backends.lock().await.remove(id);
            Ok(())
        }

        async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
            Ok(Vec::new())
        }

        async fn save_view(&self, _view: &ViewConfig) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_preferences(&self) -> Result<UserPreferences, DomainError> {
            Ok(UserPreferences::default())
        }

        async fn save_preferences(&self, _prefs: &UserPreferences) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryProjectBackendAccessRepository {
        accesses: Mutex<Vec<ProjectBackendAccess>>,
        create_mode: Mutex<AccessCreateMode>,
    }

    #[derive(Default)]
    enum AccessCreateMode {
        #[default]
        Ok,
        ConflictAfterInsert,
        ConflictWithoutInsert,
        Database,
    }

    impl InMemoryProjectBackendAccessRepository {
        async fn set_create_mode(&self, mode: AccessCreateMode) {
            *self.create_mode.lock().await = mode;
        }

        async fn accesses(&self) -> Vec<ProjectBackendAccess> {
            self.accesses.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl ProjectBackendAccessRepository for InMemoryProjectBackendAccessRepository {
        async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            match &*self.create_mode.lock().await {
                AccessCreateMode::Ok => {
                    self.accesses.lock().await.push(access.clone());
                    Ok(())
                }
                AccessCreateMode::ConflictAfterInsert => {
                    self.accesses.lock().await.push(access.clone());
                    Err(conflict_error("project_backend_access"))
                }
                AccessCreateMode::ConflictWithoutInsert => {
                    Err(conflict_error("project_backend_access"))
                }
                AccessCreateMode::Database => Err(database_error("project_backend_access.create")),
            }
        }

        async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            let mut accesses = self.accesses.lock().await;
            if let Some(existing) = accesses.iter_mut().find(|item| item.id == access.id) {
                *existing = access.clone();
            }
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .find(|access| access.id == id)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .filter(|access| access.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_active_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .list_by_project(project_id)
                .await?
                .into_iter()
                .filter(ProjectBackendAccess::is_active)
                .collect())
        }

        async fn get_active_for_project_backend(
            &self,
            project_id: Uuid,
            backend_id: &str,
        ) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .find(|access| {
                    access.project_id == project_id
                        && access.backend_id == backend_id
                        && access.status == ProjectBackendAccessStatus::Active
                })
                .cloned())
        }

        async fn list_active_by_backend(
            &self,
            backend_id: &str,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .filter(|access| {
                    access.backend_id == backend_id
                        && access.status == ProjectBackendAccessStatus::Active
                })
                .cloned()
                .collect())
        }

        async fn list_active_by_backends(
            &self,
            backend_ids: &[String],
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .iter()
                .filter(|access| {
                    backend_ids.iter().any(|id| id == &access.backend_id)
                        && access.status == ProjectBackendAccessStatus::Active
                })
                .cloned()
                .collect())
        }

        async fn set_status(
            &self,
            id: Uuid,
            status: ProjectBackendAccessStatus,
        ) -> Result<(), DomainError> {
            if let Some(access) = self
                .accesses
                .lock()
                .await
                .iter_mut()
                .find(|item| item.id == id)
            {
                access.status = status;
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn claim_success_creates_project_scoped_backend_access_and_usage_metadata() {
        let harness = ClaimHarness::new();
        let issued = harness.issue_token().await;

        let result = harness
            .claim(issued.registration_token.clone())
            .await
            .expect("claim should succeed");

        assert_eq!(result.machine_id, "machine-001");
        assert_eq!(result.machine_label, "Builder 1");
        // 身份去 project 化：runner backend 落 User scope，owner = token 创建者，
        // share_scope_id 不再是 project_id。
        assert_eq!(result.share_scope_kind, BackendShareScopeKind::User);
        assert_eq!(result.share_scope_id, Some("user-owner".to_string()));
        assert_eq!(result.capability_slot, "build");
        assert_eq!(result.relay_ws_url, "wss://cloud.test/ws/backend");
        assert_eq!(result.registration_source, "runner_registration_token");
        assert!(!result.auth_token.is_empty());

        let claims = harness.backend_repo.claims().await;
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].visibility, BackendVisibility::Shared);
        assert_eq!(claims[0].share_scope_kind, BackendShareScopeKind::User);
        assert_eq!(claims[0].share_scope_id, Some("user-owner".to_string()));
        assert_eq!(claims[0].owner_user_id, "user-owner");
        assert_eq!(claims[0].profile_id, "runner-registration");
        assert_eq!(
            claims[0].device["registration_source"],
            "runner_registration_token"
        );
        assert_eq!(claims[0].device["client_version"], "0.2.0");
        assert_eq!(claims[0].device["executor_enabled"], true);

        let accesses = harness.access_repo.accesses().await;
        assert_eq!(accesses.len(), 1);
        assert_eq!(accesses[0].project_id, harness.project_id);
        assert_eq!(accesses[0].backend_id, result.backend_id);
        assert_eq!(accesses[0].status, ProjectBackendAccessStatus::Active);
        assert_eq!(
            accesses[0].note.as_deref(),
            Some("runner_registration_token")
        );

        let updated_token = harness.token_repo.token(&issued.token.id).await;
        assert_eq!(
            updated_token.last_claimed_backend_id.as_deref(),
            Some(result.backend_id.as_str())
        );
        assert!(updated_token.last_used_at.is_some());
        assert_eq!(harness.token_repo.usage_count().await, 1);
    }

    #[tokio::test]
    async fn repeated_claim_is_idempotent_for_same_machine_project_and_slot() {
        let harness = ClaimHarness::new();
        let issued = harness.issue_token().await;

        let first = harness
            .claim(issued.registration_token.clone())
            .await
            .expect("first claim should succeed");
        let second = harness
            .claim(issued.registration_token)
            .await
            .expect("second claim should succeed");

        assert_eq!(second.backend_id, first.backend_id);
        assert_eq!(second.auth_token, first.auth_token);
        assert_eq!(harness.access_repo.accesses().await.len(), 1);
        assert_eq!(harness.token_repo.usage_count().await, 2);
    }

    #[tokio::test]
    async fn same_machine_across_projects_resolves_to_same_backend_id() {
        // 两个不同 project 的 token，但同一 owner / machine / capability slot。
        // 身份去 project 化后必须解析到同一 stable backend_id。
        let project_a = ClaimHarness::new();
        let token_a = project_a.issue_token().await;
        let project_b = ClaimHarness::new();
        let token_b = project_b.issue_token().await;
        assert_ne!(project_a.project_id, project_b.project_id);

        let result_a = project_a
            .claim(token_a.registration_token)
            .await
            .expect("claim in project A");
        let result_b = project_b
            .claim(token_b.registration_token)
            .await
            .expect("claim in project B");

        assert_eq!(
            result_a.backend_id, result_b.backend_id,
            "same machine + owner + slot must yield one stable backend id across projects"
        );
        assert_eq!(result_a.share_scope_kind, BackendShareScopeKind::User);
        assert_eq!(result_b.share_scope_kind, BackendShareScopeKind::User);
        // 每个 project 各自落一行 active ProjectBackendAccess（项目归属的唯一权威）。
        let access_a = project_a.access_repo.accesses().await;
        let access_b = project_b.access_repo.accesses().await;
        assert_eq!(access_a.len(), 1);
        assert_eq!(access_b.len(), 1);
        assert_eq!(access_a[0].project_id, project_a.project_id);
        assert_eq!(access_b[0].project_id, project_b.project_id);
    }

    #[tokio::test]
    async fn claim_rejects_invalid_expired_and_revoked_tokens() {
        let harness = ClaimHarness::new();
        let issued = harness.issue_token().await;

        let malformed = harness
            .claim("not-a-runner-token".to_string())
            .await
            .expect_err("malformed token should fail");
        assert!(matches!(
            malformed,
            RunnerRegistrationClaimError::InvalidToken
        ));

        let wrong_secret = RunnerRegistrationTokenPlaintext {
            token_id: issued.token.id.clone(),
            secret: "wrong-secret".to_string(),
        }
        .format();
        let mismatch = harness
            .claim(wrong_secret)
            .await
            .expect_err("hash mismatch should fail");
        assert!(matches!(
            mismatch,
            RunnerRegistrationClaimError::InvalidToken
        ));

        let mut expired = issued.token.clone();
        expired.id = "rtok_expired".to_string();
        expired.token_prefix = "adrt_rtok_expired".to_string();
        expired.expires_at = Utc::now() - Duration::minutes(1);
        let expired_plaintext = RunnerRegistrationTokenPlaintext {
            token_id: expired.id.clone(),
            secret: "expired-secret".to_string(),
        };
        expired.token_secret_hash =
            agentdash_domain::backend::hash_runner_registration_secret(&expired_plaintext.secret);
        harness.token_repo.insert(expired).await;
        let expired_error = harness
            .claim(expired_plaintext.format())
            .await
            .expect_err("expired token should fail");
        assert!(matches!(
            expired_error,
            RunnerRegistrationClaimError::ExpiredToken
        ));

        let mut revoked = issued.token;
        revoked.id = "rtok_revoked".to_string();
        revoked.token_prefix = "adrt_rtok_revoked".to_string();
        revoked.revoked_at = Some(Utc::now());
        let revoked_plaintext = RunnerRegistrationTokenPlaintext {
            token_id: revoked.id.clone(),
            secret: "revoked-secret".to_string(),
        };
        revoked.token_secret_hash =
            agentdash_domain::backend::hash_runner_registration_secret(&revoked_plaintext.secret);
        harness.token_repo.insert(revoked).await;
        let revoked_error = harness
            .claim(revoked_plaintext.format())
            .await
            .expect_err("revoked token should fail");
        assert!(matches!(
            revoked_error,
            RunnerRegistrationClaimError::RevokedToken
        ));
    }

    #[tokio::test]
    async fn claim_maps_scope_payload_and_database_failures_to_stable_error_classes() {
        let harness = ClaimHarness::new();
        let issued = harness.issue_token().await;

        harness.project_repo.exists.store(false, Ordering::SeqCst);
        let missing_project = harness
            .claim(issued.registration_token.clone())
            .await
            .expect_err("missing project scope should fail");
        assert!(matches!(
            missing_project,
            RunnerRegistrationClaimError::Forbidden(_)
        ));
        harness.project_repo.exists.store(true, Ordering::SeqCst);

        let bad_device = claim_runner_registration_token_with_ports(
            &harness.token_repo,
            &harness.project_repo,
            &harness.backend_repo,
            &harness.access_repo,
            RunnerRegistrationClaimInput {
                registration_token: issued.registration_token.clone(),
                machine_id: "machine-001".to_string(),
                machine_label: None,
                runner_name: None,
                client_version: None,
                device: serde_json::json!("not-object"),
                executor_enabled: true,
                capability_slot: None,
                relay_ws_url: "ws://localhost/ws/backend".to_string(),
            },
        )
        .await
        .expect_err("invalid device payload should fail");
        assert!(matches!(
            bad_device,
            RunnerRegistrationClaimError::BadRequest(_)
        ));

        harness
            .token_repo
            .fail_get_with_database
            .store(true, Ordering::SeqCst);
        let database_error = harness
            .claim(issued.registration_token)
            .await
            .expect_err("database failure should fail");
        assert!(matches!(
            database_error,
            RunnerRegistrationClaimError::Internal(message) if message == "内部数据库错误"
        ));
    }

    #[tokio::test]
    async fn claim_distinguishes_retryable_and_fatal_project_access_conflicts() {
        let retryable = ClaimHarness::new();
        let retryable_token = retryable.issue_token().await;
        retryable
            .access_repo
            .set_create_mode(AccessCreateMode::ConflictAfterInsert)
            .await;
        let result = retryable
            .claim(retryable_token.registration_token)
            .await
            .expect("concurrent conflict with active access should be retried");
        assert_eq!(retryable.access_repo.accesses().await.len(), 1);
        assert!(!result.backend_id.is_empty());

        let fatal = ClaimHarness::new();
        let fatal_token = fatal.issue_token().await;
        fatal
            .access_repo
            .set_create_mode(AccessCreateMode::ConflictWithoutInsert)
            .await;
        let fatal_error = fatal
            .claim(fatal_token.registration_token)
            .await
            .expect_err("conflict without active access should surface");
        assert!(matches!(
            fatal_error,
            RunnerRegistrationClaimError::Conflict(_)
        ));

        let internal = ClaimHarness::new();
        let internal_token = internal.issue_token().await;
        internal
            .access_repo
            .set_create_mode(AccessCreateMode::Database)
            .await;
        let internal_error = internal
            .claim(internal_token.registration_token)
            .await
            .expect_err("repository database failure should be internal");
        assert!(matches!(
            internal_error,
            RunnerRegistrationClaimError::Internal(message) if message == "内部数据库错误"
        ));
    }

    fn not_found(entity: &'static str, id: &str) -> DomainError {
        DomainError::NotFound {
            entity,
            id: id.to_string(),
        }
    }

    fn conflict_error(entity: &'static str) -> DomainError {
        DomainError::Conflict {
            entity,
            constraint: "test",
            message: "conflict".to_string(),
        }
    }

    fn database_error(operation: &'static str) -> DomainError {
        DomainError::Database {
            operation,
            message: "database unavailable".to_string(),
        }
    }
}
