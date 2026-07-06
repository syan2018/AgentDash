use std::collections::HashMap;

use agentdash_domain::DomainError;
use agentdash_domain::backend::{
    BackendConfig, BackendRepository, BackendShareScopeKind, ProjectBackendAccess,
    ProjectBackendAccessRepository,
};
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
            BackendPermission::View => ProjectPermission::Use,
            BackendPermission::Manage => ProjectPermission::Configure,
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

pub struct BackendAuthorizationService<'a, B: ?Sized, P: ?Sized, A: ?Sized> {
    backend_repo: &'a B,
    project_repo: &'a P,
    project_backend_access_repo: &'a A,
}

impl<'a, B: ?Sized, P: ?Sized, A: ?Sized> BackendAuthorizationService<'a, B, P, A>
where
    B: BackendRepository,
    P: ProjectRepository,
    A: ProjectBackendAccessRepository,
{
    pub fn new(
        backend_repo: &'a B,
        project_repo: &'a P,
        project_backend_access_repo: &'a A,
    ) -> Self {
        Self {
            backend_repo,
            project_repo,
            project_backend_access_repo,
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

        // 批量预取所有候选 backend 的 active ProjectBackendAccess grant，避免每个
        // User-scoped backend 各查一次（N+1）。owner / scoped-to-user 的 backend
        // 已经命中放行，不依赖 grant，但一次性预取更简单且与单条路径语义一致。
        let candidate_ids: Vec<String> = backends.iter().map(|item| item.id.clone()).collect();
        let grants_by_backend = self.prefetch_active_grants(&candidate_ids).await?;

        let mut visible = Vec::new();
        for backend in backends {
            let grants = grants_by_backend
                .get(&backend.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if self
                .can_access_config_with_grants(identity, &backend, BackendPermission::View, grants)
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

    async fn prefetch_active_grants(
        &self,
        backend_ids: &[String],
    ) -> Result<HashMap<String, Vec<ProjectBackendAccess>>, BackendAuthorizationError> {
        let mut map: HashMap<String, Vec<ProjectBackendAccess>> = HashMap::new();
        if backend_ids.is_empty() {
            return Ok(map);
        }
        let grants = self
            .project_backend_access_repo
            .list_active_by_backends(backend_ids)
            .await?;
        for grant in grants {
            map.entry(grant.backend_id.clone()).or_default().push(grant);
        }
        Ok(map)
    }

    async fn can_access_config(
        &self,
        identity: &AuthIdentity,
        config: &BackendConfig,
        permission: BackendPermission,
    ) -> Result<bool, BackendAuthorizationError> {
        // 单条路径：只有当 User-scoped backend 既非 owner 也非 scoped-to-user 时，
        // 才需要查询该 backend 的 active grant。其它 scope 不依赖 grant。
        if Self::can_manage_global_scope(identity) || backend_owned_by_user(config, identity) {
            return Ok(true);
        }
        if config.share_scope_kind == BackendShareScopeKind::User
            && !backend_scoped_to_user(config, identity)
        {
            let grants = self
                .project_backend_access_repo
                .list_active_by_backend(&config.id)
                .await?;
            return self
                .can_access_config_with_grants(identity, config, permission, &grants)
                .await;
        }
        self.can_access_config_with_grants(identity, config, permission, &[])
            .await
    }

    /// 鉴权核心，grant 由调用方预取传入（列表路径批量预取，单条路径按需取）。
    async fn can_access_config_with_grants(
        &self,
        identity: &AuthIdentity,
        config: &BackendConfig,
        permission: BackendPermission,
        active_grants: &[ProjectBackendAccess],
    ) -> Result<bool, BackendAuthorizationError> {
        if Self::can_manage_global_scope(identity) || backend_owned_by_user(config, identity) {
            return Ok(true);
        }

        match config.share_scope_kind {
            BackendShareScopeKind::User => {
                if backend_scoped_to_user(config, identity) {
                    return Ok(true);
                }
                // 最小放行规则：User-scoped backend 若有指向某 project P 的 active
                // ProjectBackendAccess grant，且 identity 是 P 成员并满足 permission，则放行。
                // 无 grant 时退回 owner-only 行为，desktop 个人 backend 不受影响、不回退。
                self.user_scoped_grant_allows(identity, active_grants, permission)
                    .await
            }
            BackendShareScopeKind::Project => {
                self.project_scope_allows(identity, config, permission)
                    .await
            }
            BackendShareScopeKind::System => Ok(false),
        }
    }

    async fn user_scoped_grant_allows(
        &self,
        identity: &AuthIdentity,
        active_grants: &[ProjectBackendAccess],
        permission: BackendPermission,
    ) -> Result<bool, BackendAuthorizationError> {
        let project_authz = ProjectAuthorizationService::new(self.project_repo);
        let context = project_authorization_context_from_identity(identity);
        for grant in active_grants {
            let Some(project) = self.project_repo.get_by_id(grant.project_id).await? else {
                continue;
            };
            if project_authz
                .can_access_project(&context, &project, permission.project_permission())
                .await
                .map_err(BackendAuthorizationError::from)?
            {
                return Ok(true);
            }
        }
        Ok(false)
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
        BackendType, BackendVisibility, LocalBackendClaim, ViewConfig,
    };
    use agentdash_domain::project::{
        Project, ProjectConfig, ProjectRole, ProjectSubjectGrant, ProjectSubjectType,
        ProjectVisibility,
    };
    use agentdash_spi::platform::auth::{AuthGroup, AuthIdentity, AuthMode};

    use super::*;

    #[derive(Default)]
    struct FixtureBackendStore {
        backends: Mutex<HashMap<String, BackendConfig>>,
    }

    #[async_trait::async_trait]
    impl BackendRepository for FixtureBackendStore {
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
    }

    #[derive(Default)]
    struct FixtureProjectStore {
        projects: Mutex<HashMap<Uuid, Project>>,
        grants: Mutex<HashMap<(Uuid, ProjectSubjectType, String), ProjectSubjectGrant>>,
    }

    #[async_trait::async_trait]
    impl ProjectRepository for FixtureProjectStore {
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

    use agentdash_domain::backend::{
        ProjectBackendAccess, ProjectBackendAccessRepository, ProjectBackendAccessStatus,
    };

    #[derive(Default)]
    struct FixtureAccessStore {
        accesses: Mutex<Vec<ProjectBackendAccess>>,
    }

    impl FixtureAccessStore {
        fn insert(&self, access: ProjectBackendAccess) {
            self.accesses.lock().expect("lock").push(access);
        }
    }

    #[async_trait::async_trait]
    impl ProjectBackendAccessRepository for FixtureAccessStore {
        async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            self.insert(access.clone());
            Ok(())
        }

        async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            let mut accesses = self.accesses.lock().expect("lock");
            if let Some(existing) = accesses.iter_mut().find(|item| item.id == access.id) {
                *existing = access.clone();
            }
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .expect("lock")
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
                .expect("lock")
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
                .accesses
                .lock()
                .expect("lock")
                .iter()
                .filter(|access| access.project_id == project_id && access.is_active())
                .cloned()
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
                .expect("lock")
                .iter()
                .find(|access| {
                    access.project_id == project_id
                        && access.backend_id == backend_id
                        && access.is_active()
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
                .expect("lock")
                .iter()
                .filter(|access| access.backend_id == backend_id && access.is_active())
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
                .expect("lock")
                .iter()
                .filter(|access| {
                    backend_ids.iter().any(|id| id == &access.backend_id) && access.is_active()
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
                .expect("lock")
                .iter_mut()
                .find(|item| item.id == id)
            {
                access.status = status;
            }
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
        let backend_store = FixtureBackendStore::default();
        let project_store = FixtureProjectStore::default();
        backend_store
            .add_backend(&backend("alice-runtime", Some("alice")))
            .await
            .expect("insert alice");
        backend_store
            .add_backend(&backend("bob-runtime", Some("bob")))
            .await
            .expect("insert bob");

        let access_store = FixtureAccessStore::default();
        let service =
            BackendAuthorizationService::new(&backend_store, &project_store, &access_store);
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
        let backend_store = FixtureBackendStore::default();
        let project_store = FixtureProjectStore::default();
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

        let access_store = FixtureAccessStore::default();
        let service =
            BackendAuthorizationService::new(&backend_store, &project_store, &access_store);
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
            FixtureBackendStore,
            FixtureProjectStore,
            FixtureAccessStore,
        >::can_manage_global_scope(&identity(
            "local",
            &[],
            false,
            AuthMode::Personal
        )));
    }

    /// 模拟 runner 身份去 project 化后的落点：User-scoped、owner=token 创建者、Shared。
    fn user_scoped_shared_backend(id: &str, owner: &str) -> BackendConfig {
        let mut config = backend(id, Some(owner));
        config.visibility = BackendVisibility::Shared;
        config.share_scope_kind = BackendShareScopeKind::User;
        config.share_scope_id = Some(owner.to_string());
        config
    }

    /// Grant-based auth 四路：
    /// owner 放行 / 非 owner 无 grant 拒绝 / 非 owner 有 active grant 放行 / grant 撤销后拒绝。
    #[tokio::test]
    async fn user_scoped_backend_grant_auth_covers_four_cases() {
        let backend_store = FixtureBackendStore::default();
        let project_store = FixtureProjectStore::default();
        let access_store = FixtureAccessStore::default();

        // runner backend 归 owner 用户所有（User scope）。
        let runner_backend = user_scoped_shared_backend("runner-backend", "owner-user");
        backend_store
            .add_backend(&runner_backend)
            .await
            .expect("insert runner backend");

        // 一个由 member 用户参与的 project。
        let project = project();
        project_store
            .create(&project)
            .await
            .expect("create project");
        project_store
            .upsert_subject_grant(&ProjectSubjectGrant::new(
                project.id,
                ProjectSubjectType::User,
                "member-user".to_string(),
                ProjectRole::Editor,
                "owner".to_string(),
            ))
            .await
            .expect("project member grant");

        let owner = identity("owner-user", &[], false, AuthMode::Enterprise);
        let member = identity("member-user", &[], false, AuthMode::Enterprise);

        let service =
            BackendAuthorizationService::new(&backend_store, &project_store, &access_store);

        // 1) owner 始终可见（不依赖 grant）。
        assert!(
            service
                .can_access_config(&owner, &runner_backend, BackendPermission::View)
                .await
                .expect("owner check"),
            "owner should always access its own User-scoped backend"
        );

        // 2) 非 owner、无 grant：拒绝（owner-only，不回退）。
        assert!(
            !service
                .can_access_config(&member, &runner_backend, BackendPermission::View)
                .await
                .expect("no-grant check"),
            "non-owner without grant must be denied"
        );

        // 3) 非 owner、有 active grant 且是 project 成员：放行。
        let mut grant = ProjectBackendAccess::new(
            project.id,
            runner_backend.id.clone(),
            Some("owner-user".to_string()),
        );
        access_store.insert(grant.clone());
        assert!(
            service
                .can_access_config(&member, &runner_backend, BackendPermission::View)
                .await
                .expect("active-grant check"),
            "project member with active grant must be allowed"
        );
        // 列表路径（批量预取）应得到一致结果。
        let visible = service
            .filter_backends(&member, backend_store.list_backends().await.expect("list"))
            .await
            .expect("filter with grant");
        assert!(visible.iter().any(|item| item.id == runner_backend.id));

        // 4) grant 撤销后：拒绝。
        grant.status = ProjectBackendAccessStatus::Revoked;
        access_store.update(&grant).await.expect("revoke grant");
        assert!(
            !service
                .can_access_config(&member, &runner_backend, BackendPermission::View)
                .await
                .expect("revoked-grant check"),
            "revoked grant must deny non-owner access"
        );
        let visible_after_revoke = service
            .filter_backends(&member, backend_store.list_backends().await.expect("list"))
            .await
            .expect("filter after revoke");
        assert!(
            !visible_after_revoke
                .iter()
                .any(|item| item.id == runner_backend.id)
        );
    }
}
