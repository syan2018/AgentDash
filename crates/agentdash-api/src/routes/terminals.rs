use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use agentdash_relay::*;

use crate::app_state::AppState;

/// GET /api/sessions/:session_id/terminals
pub async fn list_terminals(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let terminals = state.services.terminal_cache.list_terminals(&session_id);
    Json(terminals)
}

/// POST /api/sessions/:session_id/terminals
#[derive(Deserialize)]
pub struct SpawnTerminalBody {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

pub async fn spawn_terminal(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<SpawnTerminalBody>,
) -> impl IntoResponse {
    let backend_id = match find_backend_for_session(&state, &session_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "no backend available for session" })),
            )
                .into_response();
        }
    };

    let mount_root = match find_mount_root_for_session(&state, &session_id).await {
        Some(root) => root,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "no mount root found for session" })),
            )
                .into_response();
        }
    };

    let terminal_id = RelayMessage::new_id("term");
    let payload = TerminalSpawnPayload {
        terminal_id: terminal_id.clone(),
        session_id: session_id.clone(),
        mount_root_ref: mount_root,
        cwd: body.cwd,
        shell: body.shell,
        cols: body.cols.unwrap_or(80),
        rows: body.rows.unwrap_or(24),
    };

    // 预注册到 cache，避免 event_tx 事件到达时 cache 尚未就绪的 race condition
    state
        .services
        .terminal_cache
        .register_terminal(&session_id, &terminal_id, &backend_id, None);

    match state
        .services
        .backend_registry
        .send_command(
            &backend_id,
            RelayMessage::CommandTerminalSpawn {
                id: RelayMessage::new_id("api-term-spawn"),
                payload,
            },
        )
        .await
    {
        Ok(RelayMessage::ResponseTerminalSpawn {
            payload: Some(resp),
            ..
        }) => {
            if resp.process_id.is_some() {
                state
                    .services
                    .terminal_cache
                    .update_process_id(&resp.terminal_id, resp.process_id);
            }
            Json(serde_json::json!({
                "terminalId": resp.terminal_id,
                "processId": resp.process_id,
            }))
            .into_response()
        }
        Ok(RelayMessage::ResponseTerminalSpawn {
            error: Some(err), ..
        }) => {
            state.services.terminal_cache.remove_terminal(&terminal_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err.message })),
            )
                .into_response()
        }
        _ => {
            state.services.terminal_cache.remove_terminal(&terminal_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "unexpected response" })),
            )
                .into_response()
        }
    }
}

/// POST /api/terminals/:terminal_id/input
#[derive(Deserialize)]
pub struct TerminalInputBody {
    pub data: String,
}

pub async fn terminal_input(
    State(state): State<Arc<AppState>>,
    Path(terminal_id): Path<String>,
    Json(body): Json<TerminalInputBody>,
) -> impl IntoResponse {
    let term_state = match state.services.terminal_cache.get_terminal(&terminal_id) {
        Some(t) => t,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "terminal not found" })),
            )
                .into_response();
        }
    };

    let payload = TerminalInputPayload {
        terminal_id: terminal_id.clone(),
        data: body.data,
    };

    match state
        .services
        .backend_registry
        .send_command(
            &term_state.backend_id,
            RelayMessage::CommandTerminalInput {
                id: RelayMessage::new_id("api-term-input"),
                payload,
            },
        )
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/terminals/:terminal_id/resize
#[derive(Deserialize)]
pub struct TerminalResizeBody {
    pub cols: u16,
    pub rows: u16,
}

pub async fn terminal_resize(
    State(state): State<Arc<AppState>>,
    Path(terminal_id): Path<String>,
    Json(body): Json<TerminalResizeBody>,
) -> impl IntoResponse {
    let term_state = match state.services.terminal_cache.get_terminal(&terminal_id) {
        Some(t) => t,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "terminal not found" })),
            )
                .into_response();
        }
    };

    let payload = TerminalResizePayload {
        terminal_id: terminal_id.clone(),
        cols: body.cols,
        rows: body.rows,
    };

    match state
        .services
        .backend_registry
        .send_command(
            &term_state.backend_id,
            RelayMessage::CommandTerminalResize {
                id: RelayMessage::new_id("api-term-resize"),
                payload,
            },
        )
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// DELETE /api/terminals/:terminal_id
pub async fn terminal_kill(
    State(state): State<Arc<AppState>>,
    Path(terminal_id): Path<String>,
) -> impl IntoResponse {
    let term_state = match state.services.terminal_cache.get_terminal(&terminal_id) {
        Some(t) => t,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "terminal not found" })),
            )
                .into_response();
        }
    };

    let payload = TerminalKillPayload {
        terminal_id: terminal_id.clone(),
        signal: None,
    };

    match state
        .services
        .backend_registry
        .send_command(
            &term_state.backend_id,
            RelayMessage::CommandTerminalKill {
                id: RelayMessage::new_id("api-term-kill"),
                payload,
            },
        )
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn find_backend_for_session(state: &AppState, _session_id: &str) -> Option<String> {
    let backends = state.services.backend_registry.list_online().await;
    backends.first().map(|b| b.backend_id.clone())
}

async fn find_mount_root_for_session(state: &AppState, _session_id: &str) -> Option<String> {
    let backends = state.services.backend_registry.list_online().await;
    backends
        .first()
        .and_then(|b| b.accessible_roots.first().cloned())
}
