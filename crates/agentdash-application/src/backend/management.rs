use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendConfig, BackendRepository, BackendShareScopeKind, BackendType, BackendVisibility,
    LocalBackendClaim,
};
use agentdash_spi::platform::auth::AuthIdentity;
use sha2::{Digest, Sha256};

use crate::ApplicationError;
use crate::backend::{BackendAuthorizationService, BackendPermission};
use crate::repository_set::RepositorySet;

/// registration_source 写入 device 的稳定取值。Desktop 与 Runner 两条 enrollment
/// 路径都必须写入对应来源，诊断 UI / `/backends` 投影据此区分本机执行面归属。
pub const REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN: &str = "desktop_access_token";
pub const REGISTRATION_SOURCE_RUNNER_REGISTRATION_TOKEN: &str = "runner_registration_token";

/// 统一的本机 backend enrollment 入口来源。
///
/// 两个外部认证入口（Desktop user access token / Runner registration token）
/// 在进入 application service 后收束到这一个 source 模型，再走同一条
/// normalize → backend_id → device → LocalBackendClaim → ensure_local_backend → token 流程。
#[derive(Debug, Clone)]
pub enum EnrollmentSource {
    /// 已登录桌面 App 的用户授权来源。挂到 user/personal scope。
    DesktopAccessToken { user_id: String },
    /// 项目级服务器 runner 部署令牌来源。
    ///
    /// 注意：runner backend 的稳定身份按 `owner` 收束到 user scope，
    /// 项目可见性由 `ProjectBackendAccess` 投影承载，因此同一机器在不同
    /// 项目下复用同一 stable backend id。
    RunnerRegistrationToken {
        /// token 所属项目，用于建立 `ProjectBackendAccess` active projection。
        project_id: uuid::Uuid,
        /// token 创建者，作为 runner backend 的 owner / user scope id。
        created_by_user_id: String,
    },
}

impl EnrollmentSource {
    fn registration_source(&self) -> &'static str {
        match self {
            Self::DesktopAccessToken { .. } => REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN,
            Self::RunnerRegistrationToken { .. } => REGISTRATION_SOURCE_RUNNER_REGISTRATION_TOKEN,
        }
    }
}

/// 统一 enrollment 请求的通用字段，与认证来源无关。
#[derive(Debug, Clone)]
pub struct EnrollLocalBackendRequest {
    pub machine_id: String,
    pub machine_label: Option<String>,
    pub capability_slot: Option<String>,
    pub name: Option<String>,
    pub executor_enabled: bool,
    pub client_version: Option<String>,
    pub device: serde_json::Value,
    pub relay_ws_url: String,
    pub rotate_token: bool,
    /// Desktop 路径携带的请求 scope；Runner 路径忽略。
    pub scope: Option<LocalRuntimeScopeInput>,
    /// Desktop 路径携带的 profile id；Runner 路径使用固定 `runner-registration`。
    pub profile_id: Option<String>,
}

/// 统一 enrollment 结果。Desktop ensure 与 Runner claim handler 都从这里投影响应。
#[derive(Debug, Clone)]
pub struct EnrollLocalBackendResult {
    pub backend: BackendConfig,
    pub auth_token: String,
    pub profile_id: String,
    pub machine_id: String,
    pub machine_label: String,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub registration_source: String,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateBackendInput {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub auth_token: Option<String>,
    pub backend_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EnsureLocalRuntimeInput {
    pub current_user_id: String,
    pub machine_id: String,
    pub machine_label: Option<String>,
    pub profile_id: String,
    pub scope: Option<LocalRuntimeScopeInput>,
    pub capability_slot: Option<String>,
    pub name: Option<String>,
    pub executor_enabled: bool,
    pub client_version: Option<String>,
    pub device: serde_json::Value,
    pub rotate_token: bool,
    pub relay_ws_url: String,
}

#[derive(Debug, Clone)]
pub struct LocalRuntimeScopeInput {
    pub kind: BackendShareScopeKind,
    pub id: Option<String>,
}

pub async fn add_backend_record(
    repos: &RepositorySet,
    identity: &AuthIdentity,
    input: CreateBackendInput,
) -> Result<BackendConfig, ApplicationError> {
    let id = normalize_required("backend id", &input.id)?;
    let name = normalize_required("backend name", &input.name)?;
    let endpoint = input.endpoint.trim().to_string();
    let requested_token = normalize_optional_string(input.auth_token);
    let existing = match repos.backend_repo.get_backend(&id).await {
        Ok(config) => Some(config),
        Err(DomainError::NotFound { .. }) => None,
        Err(error) => {
            return Err(ApplicationError::Internal(format!(
                "读取 Backend 配置失败: {error}"
            )));
        }
    };
    if let Some(config) = existing.as_ref() {
        let authz = BackendAuthorizationService::new(
            repos.backend_repo.as_ref(),
            repos.project_repo.as_ref(),
            repos.project_backend_access_repo.as_ref(),
        );
        authz
            .require_config(identity, config, BackendPermission::Manage)
            .await
            .map_err(ApplicationError::from)?;
    }
    let auth_token =
        resolve_backend_auth_token(repos, &id, requested_token, existing.as_ref()).await?;

    let config = BackendConfig {
        id,
        name,
        endpoint,
        auth_token: Some(auth_token),
        enabled: existing.as_ref().map(|item| item.enabled).unwrap_or(true),
        backend_type: match input.backend_type.as_deref() {
            Some("remote") => BackendType::Remote,
            _ => BackendType::Local,
        },
        owner_user_id: match existing.as_ref() {
            Some(item) => item.owner_user_id.clone(),
            None => Some(identity.user_id.clone()),
        },
        profile_id: existing.as_ref().and_then(|item| item.profile_id.clone()),
        device_id: existing.as_ref().and_then(|item| item.device_id.clone()),
        machine_id: existing.as_ref().and_then(|item| item.machine_id.clone()),
        machine_label: existing
            .as_ref()
            .and_then(|item| item.machine_label.clone()),
        visibility: existing
            .as_ref()
            .map(|item| item.visibility)
            .unwrap_or(BackendVisibility::Private),
        share_scope_kind: existing
            .as_ref()
            .map(|item| item.share_scope_kind)
            .unwrap_or(BackendShareScopeKind::User),
        share_scope_id: match existing.as_ref() {
            Some(item) => item.share_scope_id.clone(),
            None => Some(identity.user_id.clone()),
        },
        capability_slot: existing
            .as_ref()
            .map(|item| item.capability_slot.clone())
            .unwrap_or_else(|| "default".to_string()),
        device: existing
            .as_ref()
            .map(|item| item.device.clone())
            .unwrap_or_else(|| serde_json::json!({})),
        last_claimed_at: existing.as_ref().and_then(|item| item.last_claimed_at),
    };
    repos
        .backend_repo
        .add_backend(&config)
        .await
        .map_err(ApplicationError::from)?;
    Ok(config)
}

/// 唯一的本机 backend enrollment use case。
///
/// Desktop ensure（user access token）与 Runner claim（registration token）两条路径
/// 都收束到这里：单一 normalize → backend_id → device → LocalBackendClaim →
/// ensure_local_backend → token 流程。token 统一由 `generate_backend_auth_token()` 生成。
///
/// scope / 身份差异由 `EnrollmentSource` 决定：
/// - Desktop：share_scope=User(owner=user)，visibility=Private，profile 来自请求。
/// - Runner：share_scope=User(owner=token.created_by_user_id)，visibility=Shared，
///   profile="runner-registration"，并由调用方在 backend 落地后建立 `ProjectBackendAccess`。
///
/// 两条路径都把对应的 `registration_source` 写入 `device`。
pub async fn enroll_local_backend(
    backend_repo: &dyn BackendRepository,
    source: EnrollmentSource,
    request: EnrollLocalBackendRequest,
) -> Result<EnrollLocalBackendResult, ApplicationError> {
    let machine_id = normalize_required("machine_id", &request.machine_id)?;
    let machine_label = request
        .machine_label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_machine_label(&machine_id));
    let capability_slot = request
        .capability_slot
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "default".to_string());

    // 身份去项目化：runner backend 的稳定身份按 owner 收束到 user scope，
    // 因此 stable_local_backend_id 不包含 project_id —— 同一机器/能力槽位
    // 跨项目得到同一个 backend id。
    let (owner_user_id, profile_id, share_scope_kind, share_scope_id, visibility) = match &source {
        EnrollmentSource::DesktopAccessToken { user_id } => {
            let profile_id = normalize_required(
                "profile_id",
                request.profile_id.as_deref().unwrap_or_default(),
            )?;
            let (kind, scope_id, visibility) =
                resolve_local_runtime_scope(request.scope.clone(), user_id)?;
            (user_id.clone(), profile_id, kind, scope_id, visibility)
        }
        EnrollmentSource::RunnerRegistrationToken {
            created_by_user_id, ..
        } => (
            created_by_user_id.clone(),
            "runner-registration".to_string(),
            BackendShareScopeKind::User,
            Some(created_by_user_id.clone()),
            BackendVisibility::Shared,
        ),
    };

    let name = request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_local_runtime_name(&machine_label));
    let backend_id = stable_local_backend_id(
        &machine_id,
        share_scope_kind,
        share_scope_id.as_deref(),
        &capability_slot,
    );

    let registration_source = source.registration_source();
    let mut device = normalize_device_payload(request.device)?;
    if let Some(client_version) = normalize_optional_string(request.client_version) {
        device["client_version"] = serde_json::Value::String(client_version);
    }
    device["executor_enabled"] = serde_json::Value::Bool(request.executor_enabled);
    device["registration_source"] = serde_json::Value::String(registration_source.to_string());

    let claim = LocalBackendClaim {
        owner_user_id,
        profile_id: profile_id.clone(),
        machine_id: machine_id.clone(),
        machine_label: machine_label.clone(),
        visibility,
        share_scope_kind,
        share_scope_id: share_scope_id.clone(),
        capability_slot: capability_slot.clone(),
        backend_id,
        name,
        endpoint: request.relay_ws_url,
        auth_token: generate_backend_auth_token(),
        device,
        rotate_token: request.rotate_token,
    };

    let backend = backend_repo
        .ensure_local_backend(&claim)
        .await
        .map_err(ApplicationError::from)?;
    let auth_token = normalize_optional_string(backend.auth_token.clone()).ok_or_else(|| {
        ApplicationError::Internal(format!(
            "本机 backend `{}` 缺少 server 颁发的 relay token",
            backend.id
        ))
    })?;

    Ok(EnrollLocalBackendResult {
        backend,
        auth_token,
        profile_id,
        machine_id,
        machine_label,
        share_scope_kind,
        share_scope_id,
        capability_slot,
        registration_source: registration_source.to_string(),
        claimed_at: chrono::Utc::now(),
    })
}

/// Desktop `/api/local-runtime/ensure` 的薄适配：把 user access token 请求映射到
/// 统一 enrollment use case。
pub async fn ensure_local_runtime_record(
    repos: &RepositorySet,
    input: EnsureLocalRuntimeInput,
) -> Result<EnrollLocalBackendResult, ApplicationError> {
    enroll_local_backend(
        repos.backend_repo.as_ref(),
        EnrollmentSource::DesktopAccessToken {
            user_id: input.current_user_id,
        },
        EnrollLocalBackendRequest {
            machine_id: input.machine_id,
            machine_label: input.machine_label,
            capability_slot: input.capability_slot,
            name: input.name,
            executor_enabled: input.executor_enabled,
            client_version: input.client_version,
            device: input.device,
            relay_ws_url: input.relay_ws_url,
            rotate_token: input.rotate_token,
            scope: input.scope,
            profile_id: Some(input.profile_id),
        },
    )
    .await
}

pub async fn remove_backend_record(
    repos: &RepositorySet,
    identity: &AuthIdentity,
    backend_id: &str,
) -> Result<(), ApplicationError> {
    let authz = BackendAuthorizationService::new(
        repos.backend_repo.as_ref(),
        repos.project_repo.as_ref(),
        repos.project_backend_access_repo.as_ref(),
    );
    authz
        .require_backend(identity, backend_id, BackendPermission::Manage)
        .await
        .map_err(ApplicationError::from)?;
    repos
        .backend_repo
        .remove_backend(backend_id)
        .await
        .map_err(ApplicationError::from)
}

async fn resolve_backend_auth_token(
    repos: &RepositorySet,
    backend_id: &str,
    requested_token: Option<String>,
    existing: Option<&BackendConfig>,
) -> Result<String, ApplicationError> {
    if let Some(token) = requested_token {
        return Ok(token);
    }

    if let Some(config) = existing
        && let Some(token) = normalize_optional_string(config.auth_token.clone())
    {
        return Ok(token);
    }

    match repos.backend_repo.get_backend(backend_id).await {
        Ok(config) => Ok(normalize_optional_string(config.auth_token)
            .unwrap_or_else(generate_backend_auth_token)),
        Err(DomainError::NotFound { .. }) => Ok(generate_backend_auth_token()),
        Err(error) => Err(ApplicationError::Internal(format!(
            "读取 Backend token 失败: {error}"
        ))),
    }
}

fn resolve_local_runtime_scope(
    scope: Option<LocalRuntimeScopeInput>,
    current_user_id: &str,
) -> Result<(BackendShareScopeKind, Option<String>, BackendVisibility), ApplicationError> {
    match scope {
        Some(LocalRuntimeScopeInput {
            kind: BackendShareScopeKind::User,
            id,
        }) => {
            let requested_user = id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(current_user_id);
            if requested_user != current_user_id {
                return Err(ApplicationError::Forbidden(
                    "只能领取当前用户的个人本机 runtime".to_string(),
                ));
            }
            Ok((
                BackendShareScopeKind::User,
                Some(current_user_id.to_string()),
                BackendVisibility::Private,
            ))
        }
        Some(LocalRuntimeScopeInput {
            kind: BackendShareScopeKind::Project | BackendShareScopeKind::System,
            ..
        }) => Err(ApplicationError::BadRequest(
            "共享本机 runtime scope 尚未开放创建入口".to_string(),
        )),
        None => Ok((
            BackendShareScopeKind::User,
            Some(current_user_id.to_string()),
            BackendVisibility::Private,
        )),
    }
}

/// 统一的 backend relay auth token 生成入口。Desktop 与 Runner 两条路径都必须使用它，
/// 不允许各自手搓 `Uuid::new_v4()`，以保证 relay 凭据形态可集中演进。
pub fn generate_backend_auth_token() -> String {
    uuid::Uuid::new_v4().to_string()
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

fn normalize_device_payload(
    value: serde_json::Value,
) -> Result<serde_json::Value, ApplicationError> {
    match value {
        serde_json::Value::Null => Ok(serde_json::json!({})),
        serde_json::Value::Object(_) => Ok(value),
        _ => Err(ApplicationError::BadRequest(
            "device 必须是 JSON object 或 null".to_string(),
        )),
    }
}

fn default_machine_label(machine_id: &str) -> String {
    let suffix = machine_id
        .rsplit([':', '/', '\\'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("desktop");
    format!("Desktop {suffix}")
}

fn default_local_runtime_name(machine_label: &str) -> String {
    machine_label.to_string()
}

pub(crate) fn stable_local_backend_id(
    machine_id: &str,
    share_scope_kind: BackendShareScopeKind,
    share_scope_id: Option<&str>,
    capability_slot: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(machine_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(share_scope_kind.as_str().as_bytes());
    hasher.update(b"\n");
    hasher.update(share_scope_id.unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(capability_slot.as_bytes());
    let digest = hasher.finalize();
    format!("local_{}", hex_prefix(&digest, 24))
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(chars);
    for byte in bytes {
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte >> 4) as usize] as char);
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::backend::{UserPreferences, ViewConfig};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct CaptureBackendRepository {
        backends: Mutex<HashMap<String, BackendConfig>>,
        claims: Mutex<Vec<LocalBackendClaim>>,
    }

    impl CaptureBackendRepository {
        fn claims(&self) -> Vec<LocalBackendClaim> {
            self.claims.lock().expect("lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl BackendRepository for CaptureBackendRepository {
        async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError> {
            self.backends
                .lock()
                .expect("lock")
                .insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
            Ok(self
                .backends
                .lock()
                .expect("lock")
                .values()
                .cloned()
                .collect())
        }

        async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError> {
            self.backends
                .lock()
                .expect("lock")
                .get(id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound {
                    entity: "backend",
                    id: id.to_string(),
                })
        }

        async fn get_backend_by_auth_token(
            &self,
            _token: &str,
        ) -> Result<BackendConfig, DomainError> {
            unreachable!("测试未使用");
        }

        async fn ensure_local_backend(
            &self,
            claim: &LocalBackendClaim,
        ) -> Result<BackendConfig, DomainError> {
            self.claims.lock().expect("lock").push(claim.clone());
            let mut backends = self.backends.lock().expect("lock");
            if let Some(existing) = backends.get(&claim.backend_id) {
                return Ok(existing.clone());
            }
            let config = BackendConfig {
                id: claim.backend_id.clone(),
                name: claim.name.clone(),
                endpoint: claim.endpoint.clone(),
                auth_token: Some(claim.auth_token.clone()),
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
                last_claimed_at: Some(chrono::Utc::now()),
            };
            backends.insert(config.id.clone(), config.clone());
            Ok(config)
        }

        async fn remove_backend(&self, _id: &str) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
            unreachable!("测试未使用");
        }

        async fn save_view(&self, _view: &ViewConfig) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }

        async fn get_preferences(&self) -> Result<UserPreferences, DomainError> {
            unreachable!("测试未使用");
        }

        async fn save_preferences(&self, _prefs: &UserPreferences) -> Result<(), DomainError> {
            unreachable!("测试未使用");
        }
    }

    fn desktop_request() -> EnrollLocalBackendRequest {
        EnrollLocalBackendRequest {
            machine_id: "machine-desktop".to_string(),
            machine_label: Some("Workstation".to_string()),
            capability_slot: None,
            name: None,
            executor_enabled: true,
            client_version: Some("1.2.3".to_string()),
            device: serde_json::json!({ "os": "windows" }),
            relay_ws_url: "wss://cloud.test/ws/backend".to_string(),
            rotate_token: false,
            scope: None,
            profile_id: Some("default".to_string()),
        }
    }

    #[tokio::test]
    async fn desktop_enrollment_writes_desktop_registration_source_and_user_scope() {
        let repo = CaptureBackendRepository::default();
        let result = enroll_local_backend(
            &repo,
            EnrollmentSource::DesktopAccessToken {
                user_id: "alice".to_string(),
            },
            desktop_request(),
        )
        .await
        .expect("desktop enroll should succeed");

        assert_eq!(
            result.registration_source,
            REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN
        );
        assert_eq!(result.share_scope_kind, BackendShareScopeKind::User);
        assert_eq!(result.share_scope_id, Some("alice".to_string()));
        assert_eq!(result.profile_id, "default");
        assert!(!result.auth_token.is_empty());

        let claims = repo.claims();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].visibility, BackendVisibility::Private);
        assert_eq!(
            claims[0].device["registration_source"],
            REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN
        );
        assert_eq!(claims[0].device["client_version"], "1.2.3");
        assert_eq!(claims[0].device["executor_enabled"], true);
        // 统一 token 生成：claim 的 auth_token 是 UUID 形态（36 字符），不为空。
        assert_eq!(claims[0].auth_token.len(), 36);
    }

    #[tokio::test]
    async fn desktop_enrollment_backend_id_excludes_project_and_is_stable() {
        let repo = CaptureBackendRepository::default();
        let first = enroll_local_backend(
            &repo,
            EnrollmentSource::DesktopAccessToken {
                user_id: "alice".to_string(),
            },
            desktop_request(),
        )
        .await
        .expect("first enroll");
        let second = enroll_local_backend(
            &repo,
            EnrollmentSource::DesktopAccessToken {
                user_id: "alice".to_string(),
            },
            desktop_request(),
        )
        .await
        .expect("second enroll");

        assert_eq!(first.backend.id, second.backend.id);
        assert!(first.backend.id.starts_with("local_"));
    }

    #[tokio::test]
    async fn desktop_enrollment_defaults_backend_name_to_machine_label() {
        let repo = CaptureBackendRepository::default();
        let result = enroll_local_backend(
            &repo,
            EnrollmentSource::DesktopAccessToken {
                user_id: "alice".to_string(),
            },
            desktop_request(),
        )
        .await
        .expect("desktop enroll should succeed");

        assert_eq!(result.backend.name, "Workstation");
    }
}
