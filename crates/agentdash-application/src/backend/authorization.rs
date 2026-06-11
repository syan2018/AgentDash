use agentdash_domain::DomainError;
use agentdash_domain::backend::{BackendConfig, BackendRepository, BackendShareScopeKind};
use agentdash_domain::project::ProjectRepository;
use agentdash_spi::platform::auth::{AuthIdentity, AuthMode};
use thiserror::Error;
use uuid::Uuid;

use crate::project::{
    ProjectAuthorizationService, ProjectPermission, project_authorization_context_from_identity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendPermission {
    View,
    Manage,
}

impl BackendPermission {
    fn project_permission(self) -> ProjectPermission {
        match self {
            BackendPermission::View => ProjectPermission::View,
            BackendPermission::Manage => ProjectPermission::Edit,
        }
    }

    fn action_label(self) -> &'static str {
        match self {
            BackendPermission::View => "访问",
            BackendPermission::Manage => "管理",
        }
    }
}

#[derive(Debug, Error)]
pub enum BackendAuthorizationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("当前用户无权{action} backend `{backend_id}`")]
    Forbidden {
        backend_id: String,
        action: &'static str,
    },
}

pub struct BackendAuthorizationService<'a, B: ?Sized, P: ?Sized> {
    backend_repo: &'a B,
    project_repo: &'a P,
}

impl<'a, B: ?Sized, P: ?Sized> BackendAuthorizationService<'a, B, P>
where
    B: BackendRepository,
    P: ProjectRepository,
{
    pub fn new(backend_repo: &'a B, project_repo: &'a P) -> Self {
        Self {
            backend_repo,
            project_repo,
        }
    }

    pub fn can_manage_global_scope(identity: &AuthIdentity) -> bool {
        can_manage_global_backend_scope(identity)
    }

    pub async fn require_backend(
        &self,
        identity: &AuthIdentity,
        backend_id: &str,
        permission: BackendPermission,
    ) -> Result<BackendConfig, BackendAuthorizationError> {
        let config = self.backend_repo.get_backend(backend_id).await?;
        self.require_config(identity, &config, permission).await?;
        Ok(config)
    }

    pub async fn require_config(
        &self,
        identity: &AuthIdentity,
        config: &BackendConfig,
        permission: BackendPermission,
    ) -> Result<(), BackendAuthorizationError> {
        if self.can_access_config(identity, config, permission).await? {
            return Ok(());
        }

        Err(BackendAuthorizationError::Forbidden {
            backend_id: config.id.clone(),
            action: permission.action_label(),
        })
    }

    pub async fn filter_backends(
        &self,
        identity: &AuthIdentity,
        backends: Vec<BackendConfig>,
    ) -> Result<Vec<BackendConfig>, BackendAuthorizationError> {
        if Self::can_manage_global_scope(identity) {
            return Ok(backends);
        }

        let mut visible = Vec::new();
        for backend in backends {
            if self
                .can_access_config(identity, &backend, BackendPermission::View)
                .await?
            {
                visible.push(backend);
            }
        }
        Ok(visible)
    }

    pub async fn visible_backend_ids(
        &self,
        identity: &AuthIdentity,
    ) -> Result<std::collections::HashSet<String>, BackendAuthorizationError> {
        Ok(self
            .filter_backends(identity, self.backend_repo.list_backends().await?)
            .await?
            .into_iter()
            .map(|backend| backend.id)
            .collect())
    }

    async fn can_access_config(
        &self,
        identity: &AuthIdentity,
        config: &BackendConfig,
        permission: BackendPermission,
    ) -> Result<bool, BackendAuthorizationError> {
        if Self::can_manage_global_scope(identity) || backend_owned_by_user(config, identity) {
            return Ok(true);
        }

        match config.share_scope_kind {
            BackendShareScopeKind::User => Ok(backend_scoped_to_user(config, identity)),
            BackendShareScopeKind::Project => {
                self.project_scope_allows(identity, config, permission)
                    .await
            }
            BackendShareScopeKind::System => Ok(false),
        }
    }

    async fn project_scope_allows(
        &self,
        identity: &AuthIdentity,
        config: &BackendConfig,
        permission: BackendPermission,
    ) -> Result<bool, BackendAuthorizationError> {
        let Some(project_id) = config
            .share_scope_id
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok())
        else {
            return Ok(false);
        };
        let Some(project) = self.project_repo.get_by_id(project_id).await? else {
            return Ok(false);
        };
        let project_authz = ProjectAuthorizationService::new(self.project_repo);
        project_authz
            .can_access_project(
                &project_authorization_context_from_identity(identity),
                &project,
                permission.project_permission(),
            )
            .await
            .map_err(BackendAuthorizationError::from)
    }
}

pub fn can_manage_global_backend_scope(identity: &AuthIdentity) -> bool {
    identity.is_admin || identity.auth_mode == AuthMode::Personal
}

fn backend_owned_by_user(config: &BackendConfig, identity: &AuthIdentity) -> bool {
    config.owner_user_id.as_deref() == Some(identity.user_id.as_str())
}

fn backend_scoped_to_user(config: &BackendConfig, identity: &AuthIdentity) -> bool {
    config.share_scope_id.as_deref() == Some(identity.user_id.as_str())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use agentdash_domain::backend::{
        BackendType, BackendVisibility, LocalBackendClaim, UserPreferences, ViewConfig,
    };
    use agentdash_domain::project::{
        Project, ProjectConfig, ProjectRole, ProjectSubjectGrant, ProjectSubjectType,
        ProjectVisibility,
    };
    use agentdash_spi::platform::auth::{AuthGroup, AuthIdentity, AuthMode};

    use super::*;

    #[derive(Default)]
    struct MemoryBackendStore {
        backends: Mutex<HashMap<String, BackendConfig>>,
    }

    #[async_trait::async_trait]
    impl BackendRepository for MemoryBackendStore {
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
            _claim: &LocalBackendClaim,
        ) -> Result<BackendConfig, DomainError> {
            unreachable!("测试未使用");
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

    #[derive(Default)]
    struct MemoryProjectStore {
        projects: Mutex<HashMap<Uuid, Project>>,
        grants: Mutex<HashMap<(Uuid, ProjectSubjectType, String), ProjectSubjectGrant>>,
    }

    #[async_trait::async_trait]
    impl ProjectRepository for MemoryProjectStore {
        async fn create(&self, project: &Project) -> Result<(), DomainError> {
            self.projects
                .lock()
                .expect("lock")
                .insert(project.id, project.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError> {
            Ok(self.projects.lock().expect("lock").get(&id).cloned())
        }

        async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
            Ok(self
                .projects
                .lock()
                .expect("lock")
                .values()
                .cloned()
                .collect())
        }

        async fn update(&self, project: &Project) -> Result<(), DomainError> {
            self.projects
                .lock()
                .expect("lock")
                .insert(project.id, project.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.projects.lock().expect("lock").remove(&id);
            Ok(())
        }

        async fn list_subject_grants(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .expect("lock")
                .values()
                .filter(|grant| grant.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn upsert_subject_grant(
            &self,
            grant: &ProjectSubjectGrant,
        ) -> Result<(), DomainError> {
            self.grants.lock().expect("lock").insert(
                (
                    grant.project_id,
                    grant.subject_type,
                    grant.subject_id.clone(),
                ),
                grant.clone(),
            );
            Ok(())
        }

        async fn delete_subject_grant(
            &self,
            project_id: Uuid,
            subject_type: ProjectSubjectType,
            subject_id: &str,
        ) -> Result<(), DomainError> {
            self.grants.lock().expect("lock").remove(&(
                project_id,
                subject_type,
                subject_id.to_string(),
            ));
            Ok(())
        }
    }

    fn identity(user_id: &str, groups: &[&str], is_admin: bool, mode: AuthMode) -> AuthIdentity {
        AuthIdentity {
            auth_mode: mode,
            user_id: user_id.to_string(),
            subject: user_id.to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: groups
                .iter()
                .map(|group_id| AuthGroup {
                    group_id: (*group_id).to_string(),
                    display_name: None,
                })
                .collect(),
            is_admin,
            provider: Some("test".to_string()),
            extra: serde_json::Value::Null,
        }
    }

    fn backend(id: &str, owner_user_id: Option<&str>) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: id.to_string(),
            endpoint: String::new(),
            auth_token: None,
            enabled: true,
            backend_type: BackendType::Local,
            owner_user_id: owner_user_id.map(str::to_string),
            profile_id: None,
            device_id: None,
            machine_id: None,
            machine_label: None,
            visibility: BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: owner_user_id.map(str::to_string),
            capability_slot: "default".to_string(),
            device: serde_json::json!({}),
            last_claimed_at: None,
        }
    }

    fn project() -> Project {
        let mut project =
            Project::new_with_creator("Backend Authz".to_string(), String::new(), "owner".into());
        project.visibility = ProjectVisibility::Private;
        project.config = ProjectConfig::default();
        project
    }

    #[tokio::test]
    async fn enterprise_user_only_sees_owned_or_scoped_backends() {
        let backend_store = MemoryBackendStore::default();
        let project_store = MemoryProjectStore::default();
        backend_store
            .add_backend(&backend("alice-runtime", Some("alice")))
            .await
            .expect("insert alice");
        backend_store
            .add_backend(&backend("bob-runtime", Some("bob")))
            .await
            .expect("insert bob");

        let service = BackendAuthorizationService::new(&backend_store, &project_store);
        let visible = service
            .filter_backends(
                &identity("alice", &[], false, AuthMode::Enterprise),
                backend_store.list_backends().await.expect("list"),
            )
            .await
            .expect("filter");

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "alice-runtime");
    }

    #[tokio::test]
    async fn project_editor_can_manage_project_scoped_backend() {
        let backend_store = MemoryBackendStore::default();
        let project_store = MemoryProjectStore::default();
        let project = project();
        project_store
            .create(&project)
            .await
            .expect("create project");
        project_store
            .upsert_subject_grant(&ProjectSubjectGrant::new(
                project.id,
                ProjectSubjectType::Group,
                "eng".to_string(),
                ProjectRole::Editor,
                "owner".to_string(),
            ))
            .await
            .expect("grant");

        let mut shared = backend("project-runtime", None);
        shared.share_scope_kind = BackendShareScopeKind::Project;
        shared.share_scope_id = Some(project.id.to_string());
        backend_store
            .add_backend(&shared)
            .await
            .expect("insert backend");

        let service = BackendAuthorizationService::new(&backend_store, &project_store);
        service
            .require_backend(
                &identity("alice", &["eng"], false, AuthMode::Enterprise),
                "project-runtime",
                BackendPermission::Manage,
            )
            .await
            .expect("project editor can manage");
    }

    #[test]
    fn personal_mode_can_manage_global_backend_scope() {
        assert!(BackendAuthorizationService::<
            MemoryBackendStore,
            MemoryProjectStore,
        >::can_manage_global_scope(&identity(
            "local",
            &[],
            false,
            AuthMode::Personal
        )));
    }
}
