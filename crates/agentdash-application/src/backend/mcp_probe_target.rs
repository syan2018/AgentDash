use std::collections::HashSet;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendConfig, BackendRepository, BackendShareScopeKind, BackendType,
    ProjectBackendAccessRepository,
};
use agentdash_domain::project::ProjectRepository;
use agentdash_platform_spi::AuthIdentity;

use super::{
    BackendAuthorizationError, BackendAuthorizationService, BackendPermission,
    REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpProbeBackendTarget {
    DefaultUserLocal,
    Backend { backend_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMcpProbeBackendTarget {
    pub backend_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum McpProbeBackendTargetResolutionError {
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Failed(String),
}

pub async fn resolve_mcp_probe_backend_target(
    backend_repo: &dyn BackendRepository,
    project_repo: &dyn ProjectRepository,
    project_backend_access_repo: &dyn ProjectBackendAccessRepository,
    identity: &AuthIdentity,
    target: &McpProbeBackendTarget,
    online_backend_ids: &[String],
) -> Result<ResolvedMcpProbeBackendTarget, McpProbeBackendTargetResolutionError> {
    match target {
        McpProbeBackendTarget::DefaultUserLocal => {
            resolve_default_user_local_backend(backend_repo, identity, online_backend_ids).await
        }
        McpProbeBackendTarget::Backend { backend_id } => {
            resolve_explicit_backend(
                backend_repo,
                project_repo,
                project_backend_access_repo,
                identity,
                backend_id,
                online_backend_ids,
            )
            .await
        }
    }
}

async fn resolve_default_user_local_backend(
    backend_repo: &dyn BackendRepository,
    identity: &AuthIdentity,
    online_backend_ids: &[String],
) -> Result<ResolvedMcpProbeBackendTarget, McpProbeBackendTargetResolutionError> {
    let online_ids = online_backend_ids.iter().cloned().collect::<HashSet<_>>();
    let mut candidates = backend_repo
        .list_backends()
        .await
        .map_err(|error| {
            McpProbeBackendTargetResolutionError::Failed(format!(
                "读取当前用户本机 runtime 列表失败: {error}"
            ))
        })?
        .into_iter()
        .filter(|backend| {
            is_default_user_local_backend(backend, identity)
                && online_ids.contains(backend.id.as_str())
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .last_claimed_at
            .cmp(&left.last_claimed_at)
            .then_with(|| left.id.cmp(&right.id))
    });

    let Some(backend) = candidates.into_iter().next() else {
        return Err(McpProbeBackendTargetResolutionError::Unavailable(
            "当前用户没有在线的本机 runtime，请在已连接本机 runtime 的客户端中探测".to_string(),
        ));
    };

    Ok(ResolvedMcpProbeBackendTarget {
        backend_id: backend.id,
    })
}

async fn resolve_explicit_backend(
    backend_repo: &dyn BackendRepository,
    project_repo: &dyn ProjectRepository,
    project_backend_access_repo: &dyn ProjectBackendAccessRepository,
    identity: &AuthIdentity,
    backend_id: &str,
    online_backend_ids: &[String],
) -> Result<ResolvedMcpProbeBackendTarget, McpProbeBackendTargetResolutionError> {
    let authz =
        BackendAuthorizationService::new(backend_repo, project_repo, project_backend_access_repo);
    let backend = authz
        .require_backend(identity, backend_id, BackendPermission::View)
        .await
        .map_err(explicit_backend_unavailable)?;
    if !backend.enabled {
        return Err(McpProbeBackendTargetResolutionError::Unavailable(
            "所选 backend 当前未启用".to_string(),
        ));
    }
    if !online_backend_ids.iter().any(|id| id == &backend.id) {
        return Err(McpProbeBackendTargetResolutionError::Unavailable(
            "所选 backend 当前不在线".to_string(),
        ));
    }

    Ok(ResolvedMcpProbeBackendTarget {
        backend_id: backend.id,
    })
}

fn explicit_backend_unavailable(
    error: BackendAuthorizationError,
) -> McpProbeBackendTargetResolutionError {
    match error {
        BackendAuthorizationError::Domain(DomainError::Database { .. }) => {
            McpProbeBackendTargetResolutionError::Failed("读取 backend 授权信息失败".to_string())
        }
        _ => McpProbeBackendTargetResolutionError::Unavailable(
            "所选 backend 不存在或当前用户无权访问".to_string(),
        ),
    }
}

fn is_default_user_local_backend(backend: &BackendConfig, identity: &AuthIdentity) -> bool {
    backend.enabled
        && backend.backend_type == BackendType::Local
        && backend.owner_user_id.as_deref() == Some(identity.user_id.as_str())
        && backend.share_scope_kind == BackendShareScopeKind::User
        && backend.share_scope_id.as_deref() == Some(identity.user_id.as_str())
        && backend
            .device
            .get("registration_source")
            .and_then(|value| value.as_str())
            == Some(REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use agentdash_domain::backend::{
        BackendRepository, LocalBackendClaim, ProjectBackendAccess, ProjectBackendAccessStatus,
        ViewConfig,
    };
    use agentdash_domain::project::{Project, ProjectSubjectGrant, ProjectSubjectType};
    use agentdash_platform_spi::{AuthMode, platform::auth::AuthGroup};
    use chrono::{Duration, Utc};
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct FixtureBackendRepo {
        backends: Mutex<HashMap<String, BackendConfig>>,
    }

    #[async_trait::async_trait]
    impl BackendRepository for FixtureBackendRepo {
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
            Err(DomainError::NotFound {
                entity: "backend",
                id: "auth_token".to_string(),
            })
        }

        async fn ensure_local_backend(
            &self,
            _claim: &LocalBackendClaim,
        ) -> Result<BackendConfig, DomainError> {
            unimplemented!("not needed by probe target tests")
        }

        async fn remove_backend(&self, id: &str) -> Result<(), DomainError> {
            self.backends.lock().expect("lock").remove(id);
            Ok(())
        }

        async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
            Ok(Vec::new())
        }

        async fn save_view(&self, _view: &ViewConfig) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct EmptyProjectRepo;

    #[async_trait::async_trait]
    impl ProjectRepository for EmptyProjectRepo {
        async fn create(&self, _project: &Project) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, _id: Uuid) -> Result<Option<Project>, DomainError> {
            Ok(None)
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
            _project_id: Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(Vec::new())
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
    struct EmptyProjectBackendAccessRepo;

    #[async_trait::async_trait]
    impl ProjectBackendAccessRepository for EmptyProjectBackendAccessRepo {
        async fn create(&self, _access: &ProjectBackendAccess) -> Result<(), DomainError> {
            Ok(())
        }

        async fn update(&self, _access: &ProjectBackendAccess) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, _id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_active_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(Vec::new())
        }

        async fn get_active_for_project_backend(
            &self,
            _project_id: Uuid,
            _backend_id: &str,
        ) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(None)
        }

        async fn list_active_by_backend(
            &self,
            _backend_id: &str,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_active_by_backends(
            &self,
            _backend_ids: &[String],
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(Vec::new())
        }

        async fn set_status(
            &self,
            _id: Uuid,
            _status: ProjectBackendAccessStatus,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn default_user_local_selects_latest_claimed_desktop_backend() {
        let backend_repo = FixtureBackendRepo::default();
        let identity = identity("alice");
        backend_repo
            .add_backend(&backend(
                "old",
                "alice",
                REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN,
                -10,
            ))
            .await
            .expect("add old");
        backend_repo
            .add_backend(&backend(
                "new",
                "alice",
                REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN,
                0,
            ))
            .await
            .expect("add new");
        backend_repo
            .add_backend(&backend(
                "runner",
                "alice",
                super::super::REGISTRATION_SOURCE_RUNNER_REGISTRATION_TOKEN,
                10,
            ))
            .await
            .expect("add runner");

        let resolved = resolve_mcp_probe_backend_target(
            &backend_repo,
            &EmptyProjectRepo,
            &EmptyProjectBackendAccessRepo,
            &identity,
            &McpProbeBackendTarget::DefaultUserLocal,
            &["old".to_string(), "new".to_string(), "runner".to_string()],
        )
        .await
        .expect("resolve");

        assert_eq!(resolved.backend_id, "new");
    }

    #[tokio::test]
    async fn default_user_local_requires_online_desktop_backend() {
        let backend_repo = FixtureBackendRepo::default();
        let identity = identity("alice");
        backend_repo
            .add_backend(&backend(
                "offline",
                "alice",
                REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN,
                0,
            ))
            .await
            .expect("add backend");

        let err = resolve_mcp_probe_backend_target(
            &backend_repo,
            &EmptyProjectRepo,
            &EmptyProjectBackendAccessRepo,
            &identity,
            &McpProbeBackendTarget::DefaultUserLocal,
            &[],
        )
        .await
        .expect_err("offline should be unavailable");

        assert!(matches!(
            err,
            McpProbeBackendTargetResolutionError::Unavailable(_)
        ));
    }

    #[tokio::test]
    async fn explicit_backend_uses_backend_authorization_and_online_state() {
        let backend_repo = FixtureBackendRepo::default();
        let identity = identity("alice");
        backend_repo
            .add_backend(&backend(
                "runner",
                "alice",
                super::super::REGISTRATION_SOURCE_RUNNER_REGISTRATION_TOKEN,
                0,
            ))
            .await
            .expect("add runner");

        let resolved = resolve_mcp_probe_backend_target(
            &backend_repo,
            &EmptyProjectRepo,
            &EmptyProjectBackendAccessRepo,
            &identity,
            &McpProbeBackendTarget::Backend {
                backend_id: "runner".to_string(),
            },
            &["runner".to_string()],
        )
        .await
        .expect("resolve");

        assert_eq!(resolved.backend_id, "runner");
    }

    fn backend(
        id: &str,
        owner_user_id: &str,
        registration_source: &str,
        claimed_offset_minutes: i64,
    ) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: id.to_string(),
            endpoint: "ws://127.0.0.1/relay".to_string(),
            auth_token: None,
            enabled: true,
            backend_type: BackendType::Local,
            owner_user_id: Some(owner_user_id.to_string()),
            profile_id: Some("default".to_string()),
            device_id: None,
            machine_id: Some(format!("machine-{id}")),
            machine_label: Some(id.to_string()),
            visibility: agentdash_domain::backend::BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: Some(owner_user_id.to_string()),
            capability_slot: "default".to_string(),
            device: json!({ "registration_source": registration_source }),
            last_claimed_at: Some(Utc::now() + Duration::minutes(claimed_offset_minutes)),
        }
    }

    fn identity(user_id: &str) -> AuthIdentity {
        AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: user_id.to_string(),
            subject: user_id.to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::<AuthGroup>::new(),
            is_admin: false,
            provider: None,
            extra: serde_json::Value::Null,
        }
    }
}
