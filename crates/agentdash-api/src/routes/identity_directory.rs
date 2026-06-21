use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use agentdash_contracts::auth::{DirectoryGroup, DirectoryUser};

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::rpc::ApiError;

pub async fn list_directory_users(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
) -> Result<Json<Vec<DirectoryUser>>, ApiError> {
    let users = state.repos.user_directory_repo.list_users().await?;
    Ok(Json(users.into_iter().map(DirectoryUser::from).collect()))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route("/directory/users", axum::routing::get(list_directory_users))
        .route(
            "/directory/groups",
            axum::routing::get(list_directory_groups),
        )
}

pub async fn list_directory_groups(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
) -> Result<Json<Vec<DirectoryGroup>>, ApiError> {
    let groups = state.repos.user_directory_repo.list_groups().await?;
    Ok(Json(groups.into_iter().map(DirectoryGroup::from).collect()))
}
