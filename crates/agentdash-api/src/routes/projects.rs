use std::sync::Arc;

use agentdash_application::project::{
    CloneProjectInput, CreateProjectInput, ProjectAuthorizationContext,
    ProjectAuthorizationService, ProjectMutationInput, UpdateProjectInput, clone_project_record,
    create_project_record, delete_project_record, load_project_by_id, load_project_detail_facts,
    update_project_record,
};
use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use agentdash_contracts::common_response::DeletedIdResponse;
use agentdash_contracts::project::{
    DeletedProjectSubjectGrantResponse, ProjectRole as ContractProjectRole,
    ProjectSubjectType as ContractProjectSubjectType, RevokeProjectGrantResponse,
};
use agentdash_domain::identity::{Group, User, UserDirectoryRepository, UserProfile};
use agentdash_domain::project::{Project, ProjectRole, ProjectSubjectGrant, ProjectSubjectType};
use agentdash_integration_api::{
    AuthIdentity, AuthMode, DirectoryGroup as ProviderDirectoryGroup, DirectoryProviderError,
    DirectoryResolveRequest, DirectoryUser as ProviderDirectoryUser, IdentityDirectoryProvider,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, require_project_permission};
use crate::dto::{
    CloneProjectRequest, CreateProjectRequest, ProjectAccessSummaryResponse, ProjectDetailResponse,
    ProjectResponse, ProjectSubjectGrantResponse, UpdateProjectRequest, UpsertProjectGrantRequest,
};
use crate::rpc::ApiError;

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<ProjectResponse>>, ApiError> {
    let authz = project_authorization_service(state.as_ref());
    let projects = authz
        .list_accessible_projects(&project_authorization_context(&current_user))
        .await?;
    let mut responses = Vec::with_capacity(projects.len());
    for project in projects {
        responses.push(project_response_for_user(state.as_ref(), &current_user, project).await?);
    }
    Ok(Json(responses))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects",
            axum::routing::get(list_projects).post(create_project),
        )
        .route(
            "/projects/{id}",
            axum::routing::get(get_project)
                .put(update_project)
                .delete(delete_project),
        )
        .route("/projects/{id}/clone", axum::routing::post(clone_project))
        .route(
            "/projects/{id}/grants",
            axum::routing::get(list_project_grants),
        )
        .route(
            "/projects/{id}/grants/users/{user_id}",
            axum::routing::put(grant_project_user).delete(revoke_project_user),
        )
        .route(
            "/projects/{id}/grants/groups/{group_id}",
            axum::routing::put(grant_project_group).delete(revoke_project_group),
        )
}

pub async fn create_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project = create_project_record(
        &state.repos,
        CreateProjectInput {
            creator_user_id: current_user.user_id.clone(),
            name: req.name,
            description: req.description,
            mutation: ProjectMutationInput {
                config: req.config,
                visibility: req.visibility,
                is_template: req.is_template,
                cloned_from_project_id: req.cloned_from_project_id,
                context_containers: req.context_containers,
                ..ProjectMutationInput::default()
            },
        },
    )
    .await?;

    Ok(Json(
        project_response_for_user(state.as_ref(), &current_user, project).await?,
    ))
}

pub async fn get_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ProjectDetailResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        ProjectPermission::View,
    )
    .await?;

    let detail = load_project_detail_facts(&state.repos, project_id).await?;

    let project_response =
        project_response_for_user(state.as_ref(), &current_user, project).await?;
    Ok(Json(ProjectDetailResponse::from_parts(
        project_response,
        detail.workspaces,
        detail.stories,
    )))
}

pub async fn update_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;

    let requires_owner = req.visibility.is_some()
        || req.is_template.is_some()
        || req.cloned_from_project_id.is_some();
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        if requires_owner {
            ProjectPermission::ManageSharing
        } else {
            ProjectPermission::Edit
        },
    )
    .await?;

    let project = update_project_record(
        &state.repos,
        project,
        UpdateProjectInput {
            updated_by_user_id: current_user.user_id.clone(),
            mutation: ProjectMutationInput {
                name: req.name,
                description: req.description,
                config: req.config,
                visibility: req.visibility,
                is_template: req.is_template,
                cloned_from_project_id: req.cloned_from_project_id,
                context_containers: req.context_containers,
            },
        },
    )
    .await?;

    Ok(Json(
        project_response_for_user(state.as_ref(), &current_user, project).await?,
    ))
}

pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    delete_project_record(&state.repos, project_id).await?;
    Ok(Json(DeletedIdResponse { deleted: id }))
}

pub async fn clone_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CloneProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let source_project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &source_project,
        ProjectPermission::View,
    )
    .await?;

    let cloned_project = clone_project_record(
        &state.repos,
        &source_project,
        CloneProjectInput {
            creator_user_id: current_user.user_id.clone(),
            name: req.name,
            description: req.description,
        },
    )
    .await?;

    Ok(Json(
        project_response_for_user(state.as_ref(), &current_user, cloned_project).await?,
    ))
}

pub async fn list_project_grants(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProjectSubjectGrantResponse>>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    let grants = state
        .repos
        .project_repo
        .list_subject_grants(project_id)
        .await?;
    Ok(Json(
        grants
            .into_iter()
            .map(ProjectSubjectGrantResponse::from)
            .collect(),
    ))
}

pub async fn grant_project_user(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, user_id)): Path<(String, String)>,
    Json(req): Json<UpsertProjectGrantRequest>,
) -> Result<Json<ProjectSubjectGrantResponse>, ApiError> {
    upsert_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::User,
        &user_id,
        req.role,
    )
    .await
    .map(Json)
}

pub async fn revoke_project_user(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<Json<RevokeProjectGrantResponse>, ApiError> {
    revoke_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::User,
        &user_id,
    )
    .await
    .map(Json)
}

pub async fn grant_project_group(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, group_id)): Path<(String, String)>,
    Json(req): Json<UpsertProjectGrantRequest>,
) -> Result<Json<ProjectSubjectGrantResponse>, ApiError> {
    upsert_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::Group,
        &group_id,
        req.role,
    )
    .await
    .map(Json)
}

pub async fn revoke_project_group(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, group_id)): Path<(String, String)>,
) -> Result<Json<RevokeProjectGrantResponse>, ApiError> {
    revoke_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::Group,
        &group_id,
    )
    .await
    .map(Json)
}

fn project_authorization_context(current_user: &AuthIdentity) -> ProjectAuthorizationContext {
    ProjectAuthorizationContext::new(
        current_user.user_id.clone(),
        current_user
            .groups
            .iter()
            .map(|group| group.group_id.clone())
            .collect(),
        current_user.is_admin,
    )
}

fn project_authorization_service<'a>(
    state: &'a AppState,
) -> ProjectAuthorizationService<'a, dyn agentdash_domain::project::ProjectRepository> {
    ProjectAuthorizationService::new(state.repos.project_repo.as_ref())
}

async fn load_project_or_not_found(
    state: &AppState,
    project_id: Uuid,
    raw_id: &str,
) -> Result<Project, ApiError> {
    load_project_by_id(&state.repos, project_id, raw_id)
        .await
        .map_err(ApiError::from)
}

async fn resolve_project_access(
    state: &AppState,
    current_user: &AuthIdentity,
    project: &Project,
) -> Result<agentdash_application::project::ProjectAuthorization, ApiError> {
    let authz = project_authorization_service(state);
    authz
        .resolve_project_access(&project_authorization_context(current_user), project)
        .await
        .map_err(ApiError::from)
}

async fn upsert_project_grant(
    state: &AppState,
    current_user: &AuthIdentity,
    raw_project_id: &str,
    subject_type: ProjectSubjectType,
    subject_id: &str,
    role: ProjectRole,
) -> Result<ProjectSubjectGrantResponse, ApiError> {
    let project_id = parse_project_id(raw_project_id)?;
    let project = load_project_or_not_found(state, project_id, raw_project_id).await?;
    require_project_permission(
        state,
        current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    let subject_id = normalize_subject_id(subject_type, subject_id)?;
    let subject_id = ensure_project_subject_exists(
        state.repos.user_directory_repo.as_ref(),
        state.identity_directory_provider.as_deref(),
        state.config.auth_mode,
        subject_type,
        &subject_id,
    )
    .await?;

    let authz = project_authorization_service(state);
    if authz
        .would_leave_project_without_owner(project_id, subject_type, &subject_id, Some(role))
        .await?
    {
        return Err(ApiError::Conflict(
            "Project 至少需要保留一个 owner，当前变更会移除最后一个 owner".into(),
        ));
    }

    let grant = ProjectSubjectGrant::new(
        project_id,
        subject_type,
        subject_id.clone(),
        role,
        current_user.user_id.clone(),
    );
    state
        .repos
        .project_repo
        .upsert_subject_grant(&grant)
        .await?;

    find_project_grant(state, project_id, subject_type, &subject_id)
        .await?
        .map(ProjectSubjectGrantResponse::from)
        .ok_or_else(|| ApiError::Internal("Project grant 写入成功但读取结果缺失".into()))
}

async fn revoke_project_grant(
    state: &AppState,
    current_user: &AuthIdentity,
    raw_project_id: &str,
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<RevokeProjectGrantResponse, ApiError> {
    let project_id = parse_project_id(raw_project_id)?;
    let project = load_project_or_not_found(state, project_id, raw_project_id).await?;
    require_project_permission(
        state,
        current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    let subject_id = normalize_subject_id(subject_type, subject_id)?;
    let existing = find_project_grant(state, project_id, subject_type, &subject_id)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "Project {} 不存在该共享记录: {} {}",
                project_id,
                subject_type.as_str(),
                subject_id
            ))
        })?;

    let authz = project_authorization_service(state);
    if authz
        .would_leave_project_without_owner(project_id, subject_type, &subject_id, None)
        .await?
    {
        return Err(ApiError::Conflict(
            "Project 至少需要保留一个 owner，当前撤销会移除最后一个 owner".into(),
        ));
    }

    state
        .repos
        .project_repo
        .delete_subject_grant(project_id, subject_type, &subject_id)
        .await?;

    Ok(RevokeProjectGrantResponse {
        deleted: DeletedProjectSubjectGrantResponse {
            project_id: project_id.to_string(),
            subject_type: ContractProjectSubjectType::from(existing.subject_type),
            subject_id: existing.subject_id,
        },
    })
}

async fn find_project_grant(
    state: &AppState,
    project_id: Uuid,
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<Option<ProjectSubjectGrant>, ApiError> {
    let grants = state
        .repos
        .project_repo
        .list_subject_grants(project_id)
        .await?;
    Ok(grants
        .into_iter()
        .find(|grant| grant.subject_type == subject_type && grant.subject_id == subject_id))
}

async fn ensure_project_subject_exists(
    user_directory_repo: &dyn UserDirectoryRepository,
    identity_directory_provider: Option<&dyn IdentityDirectoryProvider>,
    auth_mode: AuthMode,
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<String, ApiError> {
    match subject_type {
        ProjectSubjectType::User => {
            if user_directory_repo
                .get_user_by_id(subject_id)
                .await?
                .is_some()
            {
                return Ok(subject_id.to_string());
            }

            let Some(provider) = identity_directory_provider else {
                return Err(ApiError::NotFound(format!(
                    "用户 `{subject_id}` 不存在于身份目录中，暂时无法授权"
                )));
            };

            let user = provider
                .resolve_user(DirectoryResolveRequest {
                    key: subject_id.to_string(),
                })
                .await
                .map_err(|error| map_directory_resolve_error(error, "用户", subject_id))?;
            let canonical_subject_id = normalize_resolved_subject_id(&user.user_id, "user_id")?;
            user_directory_repo
                .upsert_user(&user_projection_from_provider(
                    &user,
                    &canonical_subject_id,
                    auth_mode,
                ))
                .await?;

            Ok(canonical_subject_id)
        }
        ProjectSubjectType::Group => {
            if user_directory_repo
                .get_group_by_id(subject_id)
                .await?
                .is_some()
            {
                return Ok(subject_id.to_string());
            }

            let Some(provider) = identity_directory_provider else {
                return Err(ApiError::NotFound(format!(
                    "用户组 `{subject_id}` 不存在于身份目录中，暂时无法授权"
                )));
            };

            let group = provider
                .resolve_group(DirectoryResolveRequest {
                    key: subject_id.to_string(),
                })
                .await
                .map_err(|error| map_directory_resolve_error(error, "用户组", subject_id))?;
            let canonical_subject_id = normalize_resolved_subject_id(&group.group_id, "group_id")?;
            user_directory_repo
                .upsert_group(&group_projection_from_provider(
                    &group,
                    &canonical_subject_id,
                ))
                .await?;

            Ok(canonical_subject_id)
        }
    }
}

fn user_projection_from_provider(
    user: &ProviderDirectoryUser,
    canonical_user_id: &str,
    auth_mode: AuthMode,
) -> User {
    let subject = non_empty_or(user.subject.clone(), Some(canonical_user_id.to_string()))
        .unwrap_or_else(|| canonical_user_id.to_string());
    User::new(UserProfile {
        user_id: canonical_user_id.to_string(),
        subject,
        auth_mode: auth_mode.to_string(),
        display_name: user.display_name.clone(),
        email: user.email.clone(),
        avatar_url: user.avatar_url.clone(),
        is_admin: false,
        provider: user.provider.clone().or_else(|| user.source.clone()),
    })
}

fn group_projection_from_provider(
    group: &ProviderDirectoryGroup,
    canonical_group_id: &str,
) -> Group {
    Group::new(canonical_group_id.to_string(), group.display_name.clone())
}

fn non_empty_or(value: String, fallback: Option<String>) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_resolved_subject_id(value: &str, label: &'static str) -> Result<String, ApiError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(ApiError::Internal(format!(
            "身份目录 provider 返回了空 {label}"
        )));
    }
    Ok(normalized.to_string())
}

fn map_directory_resolve_error(
    error: DirectoryProviderError,
    subject_label: &'static str,
    subject_id: &str,
) -> ApiError {
    match error {
        DirectoryProviderError::BadRequest(message) => ApiError::BadRequest(message),
        DirectoryProviderError::NotFound { .. } => ApiError::NotFound(format!(
            "{subject_label} `{subject_id}` 不存在于身份目录中，暂时无法授权"
        )),
        DirectoryProviderError::Unavailable(message) => ApiError::ServiceUnavailable(message),
        DirectoryProviderError::Internal(message) => {
            tracing::error!(error = %message, "身份目录 provider 解析 Project 授权主体失败");
            ApiError::Internal("身份目录服务错误".to_string())
        }
    }
}

fn parse_project_id(raw_project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw_project_id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn normalize_subject_id(
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<String, ApiError> {
    let normalized = subject_id.trim();
    if normalized.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "{} 不能为空",
            match subject_type {
                ProjectSubjectType::User => "user_id",
                ProjectSubjectType::Group => "group_id",
            }
        )));
    }

    Ok(normalized.to_string())
}

async fn project_response_for_user(
    state: &AppState,
    current_user: &AuthIdentity,
    project: Project,
) -> Result<ProjectResponse, ApiError> {
    let access = resolve_project_access(state, current_user, &project).await?;
    Ok(ProjectResponse::from_project(
        project,
        project_access_response(access),
    ))
}

fn project_access_response(
    access: agentdash_application::project::ProjectAuthorization,
) -> ProjectAccessSummaryResponse {
    ProjectAccessSummaryResponse {
        role: access.role.map(ContractProjectRole::from),
        can_view: access.can_view_project(),
        can_edit: access.can_edit_project(),
        can_manage_sharing: access.can_manage_project_sharing(),
        via_admin_bypass: access.via_admin_bypass,
        via_template_visibility: access.via_template_visibility,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::identity::{DirectorySearchOptions, DirectorySearchResult};
    use agentdash_integration_api::{
        DirectorySearchRequest, DirectorySearchResponse, DirectoryTreeNode, DirectoryTreeRequest,
    };
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MemoryUserDirectoryRepository {
        users: Mutex<HashMap<String, User>>,
        groups: Mutex<HashMap<String, Group>>,
    }

    impl MemoryUserDirectoryRepository {
        async fn insert_user(&self, user: User) {
            self.users.lock().await.insert(user.user_id.clone(), user);
        }
    }

    #[async_trait]
    impl UserDirectoryRepository for MemoryUserDirectoryRepository {
        async fn upsert_user(&self, user: &User) -> Result<(), DomainError> {
            self.users
                .lock()
                .await
                .insert(user.user_id.clone(), user.clone());
            Ok(())
        }

        async fn upsert_group(&self, group: &Group) -> Result<(), DomainError> {
            self.groups
                .lock()
                .await
                .insert(group.group_id.clone(), group.clone());
            Ok(())
        }

        async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, DomainError> {
            Ok(self.users.lock().await.get(user_id).cloned())
        }

        async fn get_group_by_id(&self, group_id: &str) -> Result<Option<Group>, DomainError> {
            Ok(self.groups.lock().await.get(group_id).cloned())
        }

        async fn list_users(&self) -> Result<Vec<User>, DomainError> {
            Ok(self.users.lock().await.values().cloned().collect())
        }

        async fn list_groups(&self) -> Result<Vec<Group>, DomainError> {
            Ok(self.groups.lock().await.values().cloned().collect())
        }

        async fn search_users(
            &self,
            _options: DirectorySearchOptions,
        ) -> Result<DirectorySearchResult<User>, DomainError> {
            Ok(DirectorySearchResult {
                items: self.list_users().await?,
                next_cursor: None,
            })
        }

        async fn search_groups(
            &self,
            _options: DirectorySearchOptions,
        ) -> Result<DirectorySearchResult<Group>, DomainError> {
            Ok(DirectorySearchResult {
                items: self.list_groups().await?,
                next_cursor: None,
            })
        }

        async fn list_groups_for_user(&self, _user_id: &str) -> Result<Vec<Group>, DomainError> {
            Ok(Vec::new())
        }

        async fn replace_groups_for_user(
            &self,
            _user_id: &str,
            _groups: &[Group],
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Clone)]
    enum ResolveResult<T> {
        Hit(T),
        NotFound,
        Unavailable,
    }

    struct TestIdentityDirectoryProvider {
        user_result: ResolveResult<ProviderDirectoryUser>,
        group_result: ResolveResult<ProviderDirectoryGroup>,
        user_calls: Mutex<Vec<String>>,
        group_calls: Mutex<Vec<String>>,
    }

    impl TestIdentityDirectoryProvider {
        fn with_user(user: ProviderDirectoryUser) -> Self {
            Self {
                user_result: ResolveResult::Hit(user),
                group_result: ResolveResult::NotFound,
                user_calls: Mutex::new(Vec::new()),
                group_calls: Mutex::new(Vec::new()),
            }
        }

        fn with_group(group: ProviderDirectoryGroup) -> Self {
            Self {
                user_result: ResolveResult::NotFound,
                group_result: ResolveResult::Hit(group),
                user_calls: Mutex::new(Vec::new()),
                group_calls: Mutex::new(Vec::new()),
            }
        }

        fn unavailable() -> Self {
            Self {
                user_result: ResolveResult::Unavailable,
                group_result: ResolveResult::Unavailable,
                user_calls: Mutex::new(Vec::new()),
                group_calls: Mutex::new(Vec::new()),
            }
        }

        async fn user_calls(&self) -> Vec<String> {
            self.user_calls.lock().await.clone()
        }

        async fn group_calls(&self) -> Vec<String> {
            self.group_calls.lock().await.clone()
        }
    }

    #[async_trait]
    impl IdentityDirectoryProvider for TestIdentityDirectoryProvider {
        async fn search_users(
            &self,
            _request: DirectorySearchRequest,
        ) -> Result<DirectorySearchResponse<ProviderDirectoryUser>, DirectoryProviderError>
        {
            Ok(DirectorySearchResponse {
                items: Vec::new(),
                next_cursor: None,
                source: Some("test".to_string()),
                is_projection_only: false,
            })
        }

        async fn search_groups(
            &self,
            _request: DirectorySearchRequest,
        ) -> Result<DirectorySearchResponse<ProviderDirectoryGroup>, DirectoryProviderError>
        {
            Ok(DirectorySearchResponse {
                items: Vec::new(),
                next_cursor: None,
                source: Some("test".to_string()),
                is_projection_only: false,
            })
        }

        async fn resolve_user(
            &self,
            request: DirectoryResolveRequest,
        ) -> Result<ProviderDirectoryUser, DirectoryProviderError> {
            self.user_calls.lock().await.push(request.key.clone());
            match &self.user_result {
                ResolveResult::Hit(user) => Ok(user.clone()),
                ResolveResult::NotFound => Err(DirectoryProviderError::NotFound {
                    kind: "user",
                    key: request.key,
                }),
                ResolveResult::Unavailable => Err(DirectoryProviderError::Unavailable(
                    "目录不可用".to_string(),
                )),
            }
        }

        async fn resolve_group(
            &self,
            request: DirectoryResolveRequest,
        ) -> Result<ProviderDirectoryGroup, DirectoryProviderError> {
            self.group_calls.lock().await.push(request.key.clone());
            match &self.group_result {
                ResolveResult::Hit(group) => Ok(group.clone()),
                ResolveResult::NotFound => Err(DirectoryProviderError::NotFound {
                    kind: "group",
                    key: request.key,
                }),
                ResolveResult::Unavailable => Err(DirectoryProviderError::Unavailable(
                    "目录不可用".to_string(),
                )),
            }
        }

        async fn list_group_children(
            &self,
            _request: DirectoryTreeRequest,
        ) -> Result<DirectorySearchResponse<DirectoryTreeNode>, DirectoryProviderError> {
            Ok(DirectorySearchResponse {
                items: Vec::new(),
                next_cursor: None,
                source: Some("test".to_string()),
                is_projection_only: false,
            })
        }
    }

    fn directory_user(user_id: &str, subject: &str) -> ProviderDirectoryUser {
        ProviderDirectoryUser {
            user_id: user_id.to_string(),
            subject: subject.to_string(),
            display_name: Some("测试用户".to_string()),
            email: Some(format!("{user_id}@example.test")),
            avatar_url: None,
            provider: Some("test".to_string()),
            source: Some("fixture".to_string()),
        }
    }

    fn directory_group(group_id: &str) -> ProviderDirectoryGroup {
        ProviderDirectoryGroup {
            group_id: group_id.to_string(),
            display_name: Some("测试用户组".to_string()),
            path: Some("/测试用户组".to_string()),
            provider: Some("test".to_string()),
            source: Some("fixture".to_string()),
        }
    }

    fn projected_user(user_id: &str) -> User {
        User::new(UserProfile {
            user_id: user_id.to_string(),
            subject: user_id.to_string(),
            auth_mode: AuthMode::Enterprise.to_string(),
            display_name: Some("已投影用户".to_string()),
            email: None,
            avatar_url: None,
            is_admin: false,
            provider: Some("projection".to_string()),
        })
    }

    #[tokio::test]
    async fn project_subject_existing_user_projection_skips_provider_resolve() {
        let repo = MemoryUserDirectoryRepository::default();
        repo.insert_user(projected_user("user-1")).await;
        let provider = TestIdentityDirectoryProvider::unavailable();

        let subject_id = ensure_project_subject_exists(
            &repo,
            Some(&provider),
            AuthMode::Enterprise,
            ProjectSubjectType::User,
            "user-1",
        )
        .await
        .expect("existing projection should pass");

        assert_eq!(subject_id, "user-1");
        assert!(provider.user_calls().await.is_empty());
    }

    #[tokio::test]
    async fn project_subject_resolves_user_and_upserts_projection() {
        let repo = MemoryUserDirectoryRepository::default();
        let provider = TestIdentityDirectoryProvider::with_user(directory_user("user-42", "alias"));

        let subject_id = ensure_project_subject_exists(
            &repo,
            Some(&provider),
            AuthMode::Enterprise,
            ProjectSubjectType::User,
            "alias",
        )
        .await
        .expect("provider-resolved user should pass");

        assert_eq!(subject_id, "user-42");
        assert_eq!(provider.user_calls().await, vec!["alias".to_string()]);
        let projected = repo
            .get_user_by_id("user-42")
            .await
            .expect("repo read")
            .expect("resolved user projection");
        assert_eq!(projected.subject, "alias");
        assert_eq!(projected.auth_mode, "enterprise");
    }

    #[tokio::test]
    async fn project_subject_resolves_group_and_upserts_projection() {
        let repo = MemoryUserDirectoryRepository::default();
        let provider = TestIdentityDirectoryProvider::with_group(directory_group("org-7"));

        let subject_id = ensure_project_subject_exists(
            &repo,
            Some(&provider),
            AuthMode::Enterprise,
            ProjectSubjectType::Group,
            "org-alias",
        )
        .await
        .expect("provider-resolved group should pass");

        assert_eq!(subject_id, "org-7");
        assert_eq!(provider.group_calls().await, vec!["org-alias".to_string()]);
        let projected = repo
            .get_group_by_id("org-7")
            .await
            .expect("repo read")
            .expect("resolved group projection");
        assert_eq!(projected.display_name.as_deref(), Some("测试用户组"));
    }

    #[tokio::test]
    async fn project_subject_missing_without_provider_returns_not_found() {
        let repo = MemoryUserDirectoryRepository::default();

        let err = ensure_project_subject_exists(
            &repo,
            None,
            AuthMode::Enterprise,
            ProjectSubjectType::User,
            "missing-user",
        )
        .await
        .expect_err("missing projection without provider should fail");

        assert!(matches!(err, ApiError::NotFound(message) if message.contains("missing-user")));
    }

    #[tokio::test]
    async fn project_subject_provider_unavailable_does_not_upsert() {
        let repo = MemoryUserDirectoryRepository::default();
        let provider = TestIdentityDirectoryProvider::unavailable();

        let err = ensure_project_subject_exists(
            &repo,
            Some(&provider),
            AuthMode::Enterprise,
            ProjectSubjectType::Group,
            "org-404",
        )
        .await
        .expect_err("provider unavailable should fail");

        assert!(matches!(err, ApiError::ServiceUnavailable(message) if message == "目录不可用"));
        assert!(
            repo.get_group_by_id("org-404")
                .await
                .expect("repo read")
                .is_none()
        );
    }
}
