pub mod backends;
pub mod stories;
pub mod health;
pub mod acp_sessions;

use std::sync::Arc;

use axum::{Router, routing::{get, post}};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use crate::stream;

pub fn create_router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/health", get(health::health_check))
        // 后端管理
        .route("/backends", get(backends::list_backends).post(backends::add_backend))
        .route("/backends/{id}", get(backends::get_backend).delete(backends::remove_backend))
        // Story 管理
        .route("/stories", get(stories::list_stories).post(stories::create_story))
        .route("/stories/{id}/tasks", get(stories::list_tasks))
        // 执行与 ACP 会话流
        .route("/sessions/{id}/prompt", post(acp_sessions::prompt_session))
        .route("/sessions/{id}/cancel", post(acp_sessions::cancel_session))
        // ACP 会话流（Streaming HTTP / SSE）
        .route("/acp/sessions/{id}/stream", get(acp_sessions::acp_session_stream_sse))
        .route("/acp/sessions/{id}/stream/ndjson", get(acp_sessions::acp_session_stream_ndjson))
        // 兼容：旧版 WebSocket 流
        .route("/acp/sessions/{id}/ws", get(acp_sessions::acp_session_ws))
        // 全局事件流（SSE + NDJSON）
        .route("/events/stream", get(stream::event_stream))
        .route("/events/stream/ndjson", get(stream::event_stream_ndjson))
        // Resume: 获取指定 ID 之后的变更
        .route("/events/since/{since_id}", get(stream::get_events_since));

    Router::new()
        .nest("/api", api)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
