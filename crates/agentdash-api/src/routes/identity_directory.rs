use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::dto::{DirectoryGroupResponse, DirectoryUserResponse};
use crate::rpc::ApiError;

pub async fn list_directory_users(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
) -> Result<Json<Vec<DirectoryUserResponse>>, ApiError> {
    let users = state.repos.user_directory_repo.list_users().await?;
    Ok(Json(
        users.into_iter().map(DirectoryUserResponse::from).collect(),
    ))
}

pub async fn list_directory_groups(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
) -> Result<Json<Vec<DirectoryGroupResponse>>, ApiError> {
    let groups = state.repos.user_directory_repo.list_groups().await?;
    Ok(Json(
        groups
            .into_iter()
            .map(DirectoryGroupResponse::from)
            .collect(),
    ))
}
