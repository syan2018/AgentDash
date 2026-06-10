use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendConfig, BackendShareScopeKind, BackendType, BackendVisibility, LocalBackendClaim,
};
use agentdash_spi::platform::auth::AuthIdentity;
use sha2::{Digest, Sha256};

use crate::ApplicationError;
use crate::backend::{BackendAuthorizationService, BackendPermission};
use crate::repository_set::RepositorySet;

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
    pub workspace_roots: Vec<String>,
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

#[derive(Debug, Clone)]
pub struct EnsureLocalRuntimeResult {
    pub backend: BackendConfig,
    pub auth_token: String,
    pub profile_id: String,
    pub machine_id: String,
    pub machine_label: String,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
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

pub async fn ensure_local_runtime_record(
    repos: &RepositorySet,
    input: EnsureLocalRuntimeInput,
) -> Result<EnsureLocalRuntimeResult, ApplicationError> {
    let profile_id = normalize_required("profile_id", &input.profile_id)?;
    let machine_id = normalize_required("machine_id", &input.machine_id)?;
    let machine_label = input
        .machine_label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_machine_label(&machine_id));
    let capability_slot = input
        .capability_slot
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "default".to_string());
    let (share_scope_kind, share_scope_id, visibility) =
        resolve_local_runtime_scope(input.scope, &input.current_user_id)?;
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_local_runtime_name(&machine_label, share_scope_kind));
    let backend_id = stable_local_backend_id(
        &machine_id,
        share_scope_kind,
        share_scope_id.as_deref(),
        &capability_slot,
    );
    let mut device = normalize_device_payload(input.device)?;
    if let Some(client_version) = normalize_optional_string(input.client_version) {
        device["client_version"] = serde_json::Value::String(client_version);
    }
    device["executor_enabled"] = serde_json::Value::Bool(input.executor_enabled);
    device["workspace_root_count"] =
        serde_json::Value::Number(serde_json::Number::from(input.workspace_roots.len() as u64));

    let claim = LocalBackendClaim {
        owner_user_id: input.current_user_id,
        profile_id: profile_id.clone(),
        machine_id: machine_id.clone(),
        machine_label: machine_label.clone(),
        visibility,
        share_scope_kind,
        share_scope_id: share_scope_id.clone(),
        capability_slot: capability_slot.clone(),
        backend_id,
        name,
        endpoint: input.relay_ws_url,
        auth_token: generate_backend_auth_token(),
        device,
        rotate_token: input.rotate_token,
    };

    let backend = repos
        .backend_repo
        .ensure_local_backend(&claim)
        .await
        .map_err(ApplicationError::from)?;
    let auth_token = normalize_optional_string(backend.auth_token.clone()).ok_or_else(|| {
        ApplicationError::Internal(format!(
            "本机 backend `{}` 缺少 server 颁发的 relay token",
            backend.id
        ))
    })?;
    Ok(EnsureLocalRuntimeResult {
        backend,
        auth_token,
        profile_id,
        machine_id,
        machine_label,
        share_scope_id,
        capability_slot,
    })
}

pub async fn remove_backend_record(
    repos: &RepositorySet,
    identity: &AuthIdentity,
    backend_id: &str,
) -> Result<(), ApplicationError> {
    let authz =
        BackendAuthorizationService::new(repos.backend_repo.as_ref(), repos.project_repo.as_ref());
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

fn generate_backend_auth_token() -> String {
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

fn default_local_runtime_name(
    machine_label: &str,
    share_scope_kind: BackendShareScopeKind,
) -> String {
    let scope_label = match share_scope_kind {
        BackendShareScopeKind::User => "Personal",
        BackendShareScopeKind::Project => "Project Shared",
        BackendShareScopeKind::System => "System Shared",
    };
    format!("{machine_label} / {scope_label}")
}

fn stable_local_backend_id(
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
