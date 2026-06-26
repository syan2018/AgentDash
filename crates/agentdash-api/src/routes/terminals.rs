use agentdash_diagnostics::{diag, Subsystem};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use agentdash_application_runtime_session::session::terminal_cache::TerminalState;
use agentdash_relay::*;

use crate::agent_run_runtime_surface::resolve_terminal_launch_target_for_api;
use crate::auth::{CurrentUser, ProjectPermission};
use crate::dto::{SpawnTerminalBody, TerminalInputBody, TerminalResizeBody};
use crate::routes::sessions::ensure_session_permission;
use crate::{app_state::AppState, rpc::ApiError};

/// GET /api/sessions/:session_id/terminals
pub async fn list_terminals(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<TerminalState>>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::View,
    )
    .await?;
    let terminals = state.services.terminal_cache.list_terminals(&session_id);
    Ok(Json(terminals))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/sessions/{id}/terminals",
            axum::routing::get(list_terminals).post(spawn_terminal),
        )
        .route("/terminals/{id}/input", axum::routing::post(terminal_input))
        .route(
            "/terminals/{id}/resize",
            axum::routing::post(terminal_resize),
        )
        .route("/terminals/{id}", axum::routing::delete(terminal_kill))
}

pub async fn spawn_terminal(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(body): Json<SpawnTerminalBody>,
) -> Result<Response, ApiError> {
    let target = resolve_terminal_launch_target_for_api(&state, &current_user, &session_id).await?;
    if !state
        .services
        .backend_registry
        .is_online(&target.backend_id)
        .await
    {
        return Err(ApiError::Conflict(format!(
            "Session 默认 workspace 所属 Backend 当前不在线: {}",
            target.backend_id
        )));
    }

    let terminal_id = RelayMessage::new_id("term");
    let payload = TerminalSpawnPayload {
        terminal_id: terminal_id.clone(),
        session_id: session_id.clone(),
        mount_root_ref: target.mount_root_ref,
        cwd: body.cwd,
        shell: body.shell,
        cols: body.cols.unwrap_or(80),
        rows: body.rows.unwrap_or(24),
    };

    // 预注册到 cache，避免 event_tx 事件到达时 cache 尚未就绪的 race condition
    state.services.terminal_cache.register_terminal(
        &session_id,
        &terminal_id,
        &target.backend_id,
        None,
    );

    match state
        .services
        .backend_registry
        .send_command(
            &target.backend_id,
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
            Ok(Json(serde_json::json!({
                "terminal_id": resp.terminal_id,
                "process_id": resp.process_id,
            }))
            .into_response())
        }
        Ok(RelayMessage::ResponseTerminalSpawn {
            error: Some(err), ..
        }) => {
            state.services.terminal_cache.remove_terminal(&terminal_id);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err.message })),
            )
                .into_response())
        }
        _ => {
            state.services.terminal_cache.remove_terminal(&terminal_id);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "unexpected response" })),
            )
                .into_response())
        }
    }
}

pub async fn terminal_input(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(terminal_id): Path<String>,
    Json(body): Json<TerminalInputBody>,
) -> Result<Response, ApiError> {
    let term_state = load_terminal_for_user(&state, &current_user, &terminal_id).await?;

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
        Ok(_) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => {
            diag!(Error, Subsystem::Api,
        error = %e, terminal_id, "terminal input relay command failed");
            Err(ApiError::ServiceUnavailable(String::from(
                "终端输入命令发送失败",
            )))
        }
    }
}

pub async fn terminal_resize(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(terminal_id): Path<String>,
    Json(body): Json<TerminalResizeBody>,
) -> Result<Response, ApiError> {
    let term_state = load_terminal_for_user(&state, &current_user, &terminal_id).await?;

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
        Ok(_) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => {
            diag!(Error, Subsystem::Api,
        error = %e, terminal_id, "terminal resize relay command failed");
            Err(ApiError::ServiceUnavailable(String::from(
                "终端尺寸调整命令发送失败",
            )))
        }
    }
}

/// DELETE /api/terminals/:terminal_id
pub async fn terminal_kill(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(terminal_id): Path<String>,
) -> Result<Response, ApiError> {
    let term_state = load_terminal_for_user(&state, &current_user, &terminal_id).await?;

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
        Ok(_) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => {
            diag!(Error, Subsystem::Api,
        error = %e, terminal_id, "terminal kill relay command failed");
            Err(ApiError::ServiceUnavailable(String::from(
                "终端结束命令发送失败",
            )))
        }
    }
}

async fn load_terminal_for_user(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    terminal_id: &str,
) -> Result<TerminalState, ApiError> {
    let term_state = state
        .services
        .terminal_cache
        .get_terminal(terminal_id)
        .ok_or_else(|| ApiError::NotFound("terminal not found".to_string()))?;
    ensure_session_permission(
        state.as_ref(),
        current_user,
        &term_state.session_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(term_state)
}
