use axum::Json;

use agentdash_integration_api::AuthIdentity;

use crate::auth::CurrentUser;
use crate::rpc::ApiError;

/// 当前用户信息，用于前端启动时拉取身份上下文。
pub async fn get_current_user(
    CurrentUser(user): CurrentUser,
) -> Result<Json<AuthIdentity>, ApiError> {
    Ok(Json(user))
}
use std::sync::Arc;

use axum::{Router, routing::get};

use crate::app_state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/me", get(get_current_user))
}
