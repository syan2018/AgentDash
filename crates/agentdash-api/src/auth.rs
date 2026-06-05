use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use agentdash_application::project::{
    ProjectAuthorizationContext, ProjectAuthorizationService,
    project_authorization_context_from_identity,
};
use agentdash_domain::DomainError;
use agentdash_domain::identity::{Group, User};
use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::task::Task;
use agentdash_domain::workspace::Workspace;
use agentdash_integration_api::{AuthError, AuthIdentity, AuthRequest};
use axum::extract::{FromRef, FromRequestParts, Request, State};
use axum::http::{HeaderMap, request::Parts};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::rpc::ApiError;

/// 已通过中间件认证的请求身份。
#[derive(Debug, Clone)]
pub struct RequestIdentity(pub AuthIdentity);

impl RequestIdentity {
    pub fn into_inner(self) -> AuthIdentity {
        self.0
    }
}

impl Deref for RequestIdentity {
    type Target = AuthIdentity;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// 面向业务路由的当前用户抽象。
#[derive(Debug, Clone)]
pub struct CurrentUser(pub AuthIdentity);

impl CurrentUser {
    pub fn into_inner(self) -> AuthIdentity {
        self.0
    }
}

impl Deref for CurrentUser {
    type Target = AuthIdentity;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for RequestIdentity
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(identity) = parts.extensions.get::<RequestIdentity>().cloned() {
            return Ok(identity);
        }

        let app_state = Arc::<AppState>::from_ref(state);
        if app_state.auth_provider.is_some() {
            return Err(ApiError::Unauthorized(
                "当前请求缺少有效认证身份".to_string(),
            ));
        }

        Err(ApiError::Unauthorized(
            "当前服务未配置认证提供者".to_string(),
        ))
    }
}

impl<S> FromRequestParts<S> for CurrentUser
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = RequestIdentity::from_request_parts(parts, state).await?;
        Ok(Self(identity.into_inner()))
    }
}

pub use agentdash_application::project::ProjectPermission;

/// 对业务 API 请求执行统一认证，并把身份注入 request extensions。
pub async fn authenticate_request(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let Some(provider) = state.auth_provider.clone() else {
        tracing::error!("业务 API 请求进入时缺少 AuthProvider");
        return Err(ApiError::ServiceUnavailable(
            "服务端认证能力未初始化".to_string(),
        ));
    };

    let auth_request = build_auth_request(&request);
    let identity = match provider.authenticate(&auth_request).await {
        Ok(identity) => identity,
        Err(err) => {
            let token = extract_token(&auth_request);
            if let (Some(raw_token), AuthError::InvalidCredentials) = (token, &err) {
                match state
                    .services
                    .auth_session_service
                    .resolve_identity_by_token(raw_token)
                    .await
                {
                    Ok(Some(identity)) => {
                        tracing::debug!(
                            method = %auth_request.method,
                            path = %auth_request.path,
                            user_id = %identity.user_id,
                            "认证 provider 失败，已通过数据库会话回源恢复身份"
                        );
                        identity
                    }
                    Ok(None) => {
                        log_auth_failure(&auth_request, &err);
                        return Err(map_auth_error(err));
                    }
                    Err(store_err) => {
                        tracing::error!(
                            method = %auth_request.method,
                            path = %auth_request.path,
                            error = %store_err,
                            "认证 provider 失败且数据库回源异常"
                        );
                        return Err(ApiError::ServiceUnavailable(
                            "认证会话服务不可用".to_string(),
                        ));
                    }
                }
            } else {
                log_auth_failure(&auth_request, &err);
                return Err(map_auth_error(err));
            }
        }
    };

    authorize_authenticated_request(provider.as_ref(), &identity, &auth_request).await?;

    request.extensions_mut().insert(identity.clone());
    request.extensions_mut().insert(RequestIdentity(identity));
    Ok(next.run(request).await)
}

fn build_auth_request(request: &Request) -> AuthRequest {
    AuthRequest {
        headers: normalize_headers(request.headers()),
        query_params: parse_query_params(request.uri().query()),
        path: request.uri().path().to_string(),
        method: request.method().as_str().to_string(),
    }
}

fn extract_token(req: &AuthRequest) -> Option<&str> {
    req.header("authorization")
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .or_else(|| req.query_param("token"))
}

fn normalize_headers(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_ascii_lowercase(), value.to_string()))
        })
        .collect()
}

fn parse_query_params(raw_query: Option<&str>) -> HashMap<String, String> {
    let mut params = HashMap::new();

    for pair in raw_query.unwrap_or_default().split('&') {
        if pair.is_empty() {
            continue;
        }

        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or_default().trim();
        if key.is_empty() {
            continue;
        }

        let value = parts.next().unwrap_or_default().trim();
        params.insert(key.to_string(), value.to_string());
    }

    params
}

pub(crate) fn map_auth_error(err: AuthError) -> ApiError {
    match err {
        AuthError::InvalidCredentials => ApiError::Unauthorized("认证失败或凭证已过期".to_string()),
        AuthError::Forbidden(message) => ApiError::Forbidden(message),
        AuthError::ServiceUnavailable(message) => ApiError::ServiceUnavailable(message),
        AuthError::BadRequest(message) => ApiError::BadRequest(message),
    }
}

async fn authorize_authenticated_request(
    provider: &dyn agentdash_integration_api::AuthProvider,
    identity: &AuthIdentity,
    request: &AuthRequest,
) -> Result<(), ApiError> {
    match provider
        .authorize(identity, request.path.as_str(), request.method.as_str())
        .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err(ApiError::Forbidden("认证提供者拒绝访问该资源".to_string())),
        Err(err) => {
            log_auth_failure(request, &err);
            Err(map_auth_error(err))
        }
    }
}

pub async fn persist_identity_snapshot_or_service_unavailable(
    state: &AppState,
    identity: &AuthIdentity,
) -> Result<(), ApiError> {
    persist_identity_snapshot(state, identity)
        .await
        .map_err(|err| {
            tracing::error!(
                user_id = %identity.user_id,
                auth_mode = %identity.auth_mode,
                error = %err,
                "写入用户身份投影失败"
            );
            ApiError::ServiceUnavailable("用户身份目录不可用".to_string())
        })
}

pub fn project_authorization_context(current_user: &AuthIdentity) -> ProjectAuthorizationContext {
    project_authorization_context_from_identity(current_user)
}

pub async fn require_project_permission(
    state: &AppState,
    current_user: &AuthIdentity,
    project: &Project,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    let authz = ProjectAuthorizationService::new(state.repos.project_repo.as_ref());
    let allowed = authz
        .can_access_project(
            &project_authorization_context(current_user),
            project,
            permission,
        )
        .await?;

    if allowed {
        return Ok(());
    }

    let action = match permission {
        ProjectPermission::View => "查看",
        ProjectPermission::Edit => "编辑",
        ProjectPermission::ManageSharing => "管理共享",
    };
    Err(ApiError::Forbidden(format!(
        "当前用户无权{action} Project {}",
        project.id
    )))
}

pub async fn load_project_with_permission(
    state: &AppState,
    current_user: &AuthIdentity,
    project_id: Uuid,
    permission: ProjectPermission,
) -> Result<Project, ApiError> {
    let project = state
        .repos
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} 不存在")))?;
    require_project_permission(state, current_user, &project, permission).await?;
    Ok(project)
}

pub async fn load_story_and_project_with_permission(
    state: &AppState,
    current_user: &AuthIdentity,
    story_id: Uuid,
    permission: ProjectPermission,
) -> Result<(Story, Project), ApiError> {
    let story = state
        .repos
        .story_repo
        .get_by_id(story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Story {story_id} 不存在")))?;
    let project =
        load_project_with_permission(state, current_user, story.project_id, permission).await?;
    Ok((story, project))
}

pub async fn load_task_story_project_with_permission(
    state: &AppState,
    current_user: &AuthIdentity,
    task_id: Uuid,
    permission: ProjectPermission,
) -> Result<(Task, Story, Project), ApiError> {
    // M1-b：Task 查询经 Story aggregate；`find_by_task_id` 一次性拿到 Story + Task
    let story = state
        .repos
        .story_repo
        .find_by_task_id(task_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))?;
    let task = story
        .find_task(task_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))?;
    let project =
        load_project_with_permission(state, current_user, task.project_id, permission).await?;
    Ok((task, story, project))
}

pub async fn load_workspace_and_project_with_permission(
    state: &AppState,
    current_user: &AuthIdentity,
    workspace_id: Uuid,
    permission: ProjectPermission,
) -> Result<(Workspace, Project), ApiError> {
    let workspace = state
        .repos
        .workspace_repo
        .get_by_id(workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Workspace {workspace_id} 不存在")))?;
    let project =
        load_project_with_permission(state, current_user, workspace.project_id, permission).await?;
    Ok((workspace, project))
}

fn log_auth_failure(request: &AuthRequest, err: &AuthError) {
    match err {
        AuthError::InvalidCredentials | AuthError::Forbidden(_) | AuthError::BadRequest(_) => {
            tracing::warn!(
                method = %request.method,
                path = %request.path,
                error = %err,
                "请求认证失败"
            );
        }
        AuthError::ServiceUnavailable(_) => {
            tracing::error!(
                method = %request.method,
                path = %request.path,
                error = %err,
                "认证服务不可用"
            );
        }
    }
}

pub async fn persist_identity_snapshot(
    state: &AppState,
    identity: &AuthIdentity,
) -> Result<(), DomainError> {
    let user = User::new(
        identity.user_id.clone(),
        identity.subject.clone(),
        identity.auth_mode.to_string(),
        identity.display_name.clone(),
        identity.email.clone(),
        identity.avatar_url.clone(),
        identity.is_admin,
        identity.provider.clone(),
    );

    let groups = identity
        .groups
        .iter()
        .map(|group| Group::new(group.group_id.clone(), group.display_name.clone()))
        .collect::<Vec<_>>();

    state.repos.user_directory_repo.upsert_user(&user).await?;
    state
        .repos
        .user_directory_repo
        .replace_groups_for_user(&user.user_id, &groups)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_integration_api::{AuthError, AuthProvider};
    use agentdash_spi::platform::auth::AuthMode;

    struct StaticAuthorizeProvider {
        result: Result<bool, AuthError>,
    }

    #[async_trait::async_trait]
    impl AuthProvider for StaticAuthorizeProvider {
        async fn authenticate(&self, _req: &AuthRequest) -> Result<AuthIdentity, AuthError> {
            Ok(identity())
        }

        async fn authorize(
            &self,
            _identity: &AuthIdentity,
            _resource: &str,
            _action: &str,
        ) -> Result<bool, AuthError> {
            match &self.result {
                Ok(value) => Ok(*value),
                Err(AuthError::Forbidden(message)) => Err(AuthError::Forbidden(message.clone())),
                Err(AuthError::ServiceUnavailable(message)) => {
                    Err(AuthError::ServiceUnavailable(message.clone()))
                }
                Err(AuthError::BadRequest(message)) => Err(AuthError::BadRequest(message.clone())),
                Err(AuthError::InvalidCredentials) => Err(AuthError::InvalidCredentials),
            }
        }
    }

    fn identity() -> AuthIdentity {
        AuthIdentity {
            auth_mode: AuthMode::Enterprise,
            user_id: "alice".to_string(),
            subject: "alice".to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: Some("test".to_string()),
            extra: serde_json::Value::Null,
        }
    }

    fn auth_request() -> AuthRequest {
        AuthRequest {
            headers: HashMap::new(),
            query_params: HashMap::new(),
            path: "/api/projects".to_string(),
            method: "GET".to_string(),
        }
    }

    #[tokio::test]
    async fn authorize_authenticated_request_maps_false_to_forbidden() {
        let provider = StaticAuthorizeProvider { result: Ok(false) };
        let err = authorize_authenticated_request(&provider, &identity(), &auth_request())
            .await
            .expect_err("provider deny should become forbidden");

        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[tokio::test]
    async fn authorize_authenticated_request_preserves_auth_error_mapping() {
        let provider = StaticAuthorizeProvider {
            result: Err(AuthError::ServiceUnavailable("down".to_string())),
        };
        let err = authorize_authenticated_request(&provider, &identity(), &auth_request())
            .await
            .expect_err("provider error should be mapped");

        assert!(matches!(err, ApiError::ServiceUnavailable(_)));
    }
}
