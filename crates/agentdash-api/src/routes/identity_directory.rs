use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;

use agentdash_contracts::auth::{
    DirectoryGroup, DirectoryResolveRequest, DirectoryResolveResponse, DirectorySearchResponse,
    DirectoryTreeNode, DirectoryUser,
};
use agentdash_domain::identity::{
    DirectorySearchOptions, DirectorySearchResult, Group, User, UserProfile,
};
use agentdash_integration_api::{
    DirectoryGroup as ProviderDirectoryGroup, DirectoryProviderError, DirectorySearchRequest,
    DirectorySearchResponse as ProviderSearchResponse,
    DirectoryTreeNode as ProviderDirectoryTreeNode, DirectoryTreeRequest,
    DirectoryUser as ProviderDirectoryUser,
};

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::rpc::ApiError;

const DEFAULT_DIRECTORY_LIMIT: u32 = 20;
const MAX_DIRECTORY_LIMIT: u32 = 50;
const PROJECTION_SOURCE: &str = "projection";

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DirectoryListQuery {
    pub query: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

impl DirectoryListQuery {
    fn is_legacy_list(&self) -> bool {
        self.limit.is_none()
            && self.cursor.is_none()
            && self
                .query
                .as_deref()
                .is_none_or(|query| query.trim().is_empty())
    }

    fn normalized_limit(&self) -> u32 {
        normalize_limit(self.limit)
    }

    fn search_request(&self) -> DirectorySearchRequest {
        DirectorySearchRequest {
            query: normalized_query(self.query.as_deref()),
            limit: self.normalized_limit(),
            cursor: self.cursor.clone(),
        }
    }

    fn projection_options(&self) -> DirectorySearchOptions {
        DirectorySearchOptions {
            query: normalized_query(self.query.as_deref()),
            limit: self.normalized_limit() as usize,
            cursor: self.cursor.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DirectoryTreeQuery {
    pub parent_id: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

impl DirectoryTreeQuery {
    fn normalized_limit(&self) -> u32 {
        normalize_limit(self.limit)
    }

    fn tree_request(&self) -> DirectoryTreeRequest {
        DirectoryTreeRequest {
            parent_group_id: normalized_query(self.parent_id.as_deref()),
            limit: self.normalized_limit(),
            cursor: self.cursor.clone(),
        }
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/directory/users", axum::routing::get(list_directory_users))
        .route(
            "/directory/groups",
            axum::routing::get(list_directory_groups),
        )
        .route(
            "/directory/groups/tree",
            axum::routing::get(list_directory_group_tree),
        )
        .route(
            "/directory/users/resolve",
            axum::routing::post(resolve_directory_user),
        )
        .route(
            "/directory/groups/resolve",
            axum::routing::post(resolve_directory_group),
        )
}

pub async fn list_directory_users(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<DirectoryListQuery>,
) -> Result<Response, ApiError> {
    if query.is_legacy_list() {
        let users = state.repos.user_directory_repo.list_users().await?;
        let response = users
            .into_iter()
            .map(DirectoryUser::from)
            .collect::<Vec<_>>();
        return Ok(Json(response).into_response());
    }

    if let Some(provider) = &state.identity_directory_provider {
        match provider.search_users(query.search_request()).await {
            Ok(result) => {
                let mut response = provider_user_search_response(result, state.config.auth_mode);
                apply_projected_user_fields(&state, &mut response.items).await;
                return Ok(Json(response).into_response());
            }
            Err(DirectoryProviderError::Unavailable(message)) => {
                tracing::warn!(error = %message, "身份目录 provider 不可用，回退到本地 user projection");
            }
            Err(error) => return Err(map_provider_error(error)),
        }
    }

    let result = state
        .repos
        .user_directory_repo
        .search_users(query.projection_options())
        .await?;
    Ok(Json(projection_user_search_response(result)).into_response())
}

pub async fn list_directory_groups(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<DirectoryListQuery>,
) -> Result<Response, ApiError> {
    if query.is_legacy_list() {
        let groups = state.repos.user_directory_repo.list_groups().await?;
        let response = groups
            .into_iter()
            .map(DirectoryGroup::from)
            .collect::<Vec<_>>();
        return Ok(Json(response).into_response());
    }

    if let Some(provider) = &state.identity_directory_provider {
        match provider.search_groups(query.search_request()).await {
            Ok(result) => {
                let response = provider_group_search_response(result);
                return Ok(Json(response).into_response());
            }
            Err(DirectoryProviderError::Unavailable(message)) => {
                tracing::warn!(error = %message, "身份目录 provider 不可用，回退到本地 group projection");
            }
            Err(error) => return Err(map_provider_error(error)),
        }
    }

    let result = state
        .repos
        .user_directory_repo
        .search_groups(query.projection_options())
        .await?;
    Ok(Json(projection_group_search_response(result)).into_response())
}

pub async fn list_directory_group_tree(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<DirectoryTreeQuery>,
) -> Result<Json<DirectorySearchResponse<DirectoryTreeNode>>, ApiError> {
    if let Some(provider) = &state.identity_directory_provider {
        match provider.list_group_children(query.tree_request()).await {
            Ok(result) => return Ok(Json(provider_tree_response(result))),
            Err(DirectoryProviderError::Unavailable(message)) => {
                tracing::warn!(error = %message, "身份目录 provider 不可用，回退到本地 group projection tree");
            }
            Err(error) => return Err(map_provider_error(error)),
        }
    }

    let response = projection_tree_response(&state, &query).await?;
    Ok(Json(response))
}

pub async fn resolve_directory_user(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Json(request): Json<DirectoryResolveRequest>,
) -> Result<Json<DirectoryResolveResponse<DirectoryUser>>, ApiError> {
    let key = normalize_required_key(&request.key, "user key")?;
    if let Some(user) = state.repos.user_directory_repo.get_user_by_id(&key).await? {
        return Ok(Json(DirectoryResolveResponse {
            item: DirectoryUser::from(user),
            source: Some(PROJECTION_SOURCE.to_string()),
            is_projection_only: true,
        }));
    }

    let Some(provider) = &state.identity_directory_provider else {
        return Err(ApiError::NotFound(format!("目录用户不存在: {key}")));
    };

    let user = provider
        .resolve_user(agentdash_integration_api::DirectoryResolveRequest { key: key.clone() })
        .await
        .map_err(map_provider_error)?;
    let projection = user_projection_from_provider(&user, state.config.auth_mode);
    state
        .repos
        .user_directory_repo
        .upsert_user(&projection)
        .await?;

    let response = DirectoryResolveResponse {
        source: user
            .source
            .clone()
            .or_else(|| user.provider.clone())
            .or_else(|| Some("provider".to_string())),
        item: contract_user_from_provider(user, state.config.auth_mode),
        is_projection_only: false,
    };
    Ok(Json(response))
}

pub async fn resolve_directory_group(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Json(request): Json<DirectoryResolveRequest>,
) -> Result<Json<DirectoryResolveResponse<DirectoryGroup>>, ApiError> {
    let key = normalize_required_key(&request.key, "group key")?;
    if let Some(group) = state
        .repos
        .user_directory_repo
        .get_group_by_id(&key)
        .await?
    {
        return Ok(Json(DirectoryResolveResponse {
            item: DirectoryGroup::from(group),
            source: Some(PROJECTION_SOURCE.to_string()),
            is_projection_only: true,
        }));
    }

    let Some(provider) = &state.identity_directory_provider else {
        return Err(ApiError::NotFound(format!("目录用户组不存在: {key}")));
    };

    let group = provider
        .resolve_group(agentdash_integration_api::DirectoryResolveRequest { key: key.clone() })
        .await
        .map_err(map_provider_error)?;
    state
        .repos
        .user_directory_repo
        .upsert_group(&group_projection_from_provider(&group))
        .await?;

    let response = DirectoryResolveResponse {
        source: group
            .source
            .clone()
            .or_else(|| group.provider.clone())
            .or_else(|| Some("provider".to_string())),
        item: contract_group_from_provider(group),
        is_projection_only: false,
    };
    Ok(Json(response))
}

async fn projection_tree_response(
    state: &AppState,
    query: &DirectoryTreeQuery,
) -> Result<DirectorySearchResponse<DirectoryTreeNode>, ApiError> {
    if query
        .parent_id
        .as_deref()
        .is_some_and(|parent_id| !parent_id.trim().is_empty())
    {
        return Ok(DirectorySearchResponse {
            items: vec![],
            next_cursor: None,
            source: Some(PROJECTION_SOURCE.to_string()),
            is_projection_only: true,
        });
    }

    let result = state
        .repos
        .user_directory_repo
        .search_groups(DirectorySearchOptions {
            query: None,
            limit: query.normalized_limit() as usize,
            cursor: query.cursor.clone(),
        })
        .await?;

    Ok(DirectorySearchResponse {
        items: result
            .items
            .into_iter()
            .map(|group| DirectoryTreeNode {
                group_id: group.group_id,
                display_name: group.display_name,
                path: None,
                has_children: false,
                children: None,
                provider: None,
                source: Some(PROJECTION_SOURCE.to_string()),
            })
            .collect(),
        next_cursor: result.next_cursor,
        source: Some(PROJECTION_SOURCE.to_string()),
        is_projection_only: true,
    })
}

fn projection_user_search_response(
    result: DirectorySearchResult<User>,
) -> DirectorySearchResponse<DirectoryUser> {
    DirectorySearchResponse {
        items: result.items.into_iter().map(DirectoryUser::from).collect(),
        next_cursor: result.next_cursor,
        source: Some(PROJECTION_SOURCE.to_string()),
        is_projection_only: true,
    }
}

fn projection_group_search_response(
    result: DirectorySearchResult<Group>,
) -> DirectorySearchResponse<DirectoryGroup> {
    DirectorySearchResponse {
        items: result.items.into_iter().map(DirectoryGroup::from).collect(),
        next_cursor: result.next_cursor,
        source: Some(PROJECTION_SOURCE.to_string()),
        is_projection_only: true,
    }
}

fn provider_user_search_response(
    result: ProviderSearchResponse<ProviderDirectoryUser>,
    auth_mode: agentdash_integration_api::AuthMode,
) -> DirectorySearchResponse<DirectoryUser> {
    DirectorySearchResponse {
        items: result
            .items
            .into_iter()
            .map(|user| contract_user_from_provider(user, auth_mode))
            .collect(),
        next_cursor: result.next_cursor,
        source: result.source,
        is_projection_only: result.is_projection_only,
    }
}

fn provider_group_search_response(
    result: ProviderSearchResponse<ProviderDirectoryGroup>,
) -> DirectorySearchResponse<DirectoryGroup> {
    DirectorySearchResponse {
        items: result
            .items
            .into_iter()
            .map(contract_group_from_provider)
            .collect(),
        next_cursor: result.next_cursor,
        source: result.source,
        is_projection_only: result.is_projection_only,
    }
}

fn provider_tree_response(
    result: ProviderSearchResponse<ProviderDirectoryTreeNode>,
) -> DirectorySearchResponse<DirectoryTreeNode> {
    DirectorySearchResponse {
        items: result
            .items
            .into_iter()
            .map(contract_tree_node_from_provider)
            .collect(),
        next_cursor: result.next_cursor,
        source: result.source,
        is_projection_only: result.is_projection_only,
    }
}

fn contract_user_from_provider(
    user: ProviderDirectoryUser,
    auth_mode: agentdash_integration_api::AuthMode,
) -> DirectoryUser {
    let now = Utc::now();
    let subject = non_empty_or(user.subject, Some(user.user_id.clone()))
        .unwrap_or_else(|| user.user_id.clone());
    DirectoryUser {
        user_id: user.user_id,
        subject,
        auth_mode: auth_mode.to_string(),
        display_name: user.display_name,
        email: user.email,
        avatar_url: user.avatar_url,
        is_admin: false,
        provider: user.provider,
        source: user.source,
        created_at: now,
        updated_at: now,
    }
}

fn contract_group_from_provider(group: ProviderDirectoryGroup) -> DirectoryGroup {
    let now = Utc::now();
    DirectoryGroup {
        group_id: group.group_id,
        display_name: group.display_name,
        path: group.path,
        provider: group.provider,
        source: group.source,
        created_at: now,
        updated_at: now,
    }
}

fn contract_tree_node_from_provider(node: ProviderDirectoryTreeNode) -> DirectoryTreeNode {
    DirectoryTreeNode {
        group_id: node.group_id,
        display_name: node.display_name,
        path: node.path,
        has_children: node.has_children,
        children: node.children.map(|children| {
            children
                .into_iter()
                .map(contract_tree_node_from_provider)
                .collect()
        }),
        provider: node.provider,
        source: node.source,
    }
}

fn user_projection_from_provider(
    user: &ProviderDirectoryUser,
    auth_mode: agentdash_integration_api::AuthMode,
) -> User {
    User::new(UserProfile {
        user_id: user.user_id.clone(),
        subject: non_empty_or(user.subject.clone(), Some(user.user_id.clone()))
            .unwrap_or_else(|| user.user_id.clone()),
        auth_mode: auth_mode.to_string(),
        display_name: user.display_name.clone(),
        email: user.email.clone(),
        avatar_url: user.avatar_url.clone(),
        is_admin: false,
        provider: user.provider.clone().or_else(|| user.source.clone()),
    })
}

fn group_projection_from_provider(group: &ProviderDirectoryGroup) -> Group {
    Group::new(group.group_id.clone(), group.display_name.clone())
}

fn normalize_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(DEFAULT_DIRECTORY_LIMIT)
        .clamp(1, MAX_DIRECTORY_LIMIT)
}

fn normalized_query(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn non_empty_or(value: String, fallback: Option<String>) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_required_key(value: &str, label: &'static str) -> Result<String, ApiError> {
    let key = value.trim();
    if key.is_empty() {
        return Err(ApiError::BadRequest(format!("{label} 不能为空")));
    }
    Ok(key.to_string())
}

async fn apply_projected_user_fields(state: &AppState, users: &mut [DirectoryUser]) {
    for user in users.iter_mut() {
        if user.email.is_some() && user.avatar_url.is_some() {
            continue;
        }

        let projection = match state
            .repos
            .user_directory_repo
            .get_user_by_id(&user.user_id)
            .await
        {
            Ok(Some(user)) => user,
            Ok(None) => continue,
            Err(error) => {
                tracing::debug!(error = %error, user_id = %user.user_id, "投影用户字段查询失败，跳过补全");
                continue;
            }
        };

        if user.email.is_none() {
            user.email = projection.email;
        }
        if user.avatar_url.is_none() {
            user.avatar_url = projection.avatar_url;
        }
    }
}

fn map_provider_error(error: DirectoryProviderError) -> ApiError {
    match error {
        DirectoryProviderError::BadRequest(message) => ApiError::BadRequest(message),
        DirectoryProviderError::NotFound { kind, key } => {
            ApiError::NotFound(format!("目录主体未找到: {kind} {key}"))
        }
        DirectoryProviderError::Unavailable(message) => ApiError::ServiceUnavailable(message),
        DirectoryProviderError::Internal(message) => {
            tracing::error!(error = %message, "身份目录 provider 内部错误");
            ApiError::Internal("身份目录服务错误".to_string())
        }
    }
}
