use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
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
use crate::relay::registry::BackendCommandError;
use crate::routes::sessions::ensure_session_permission;
use crate::{app_state::AppState, rpc::ApiError};

/// Internal diagnostics: GET /api/sessions/:session_id/terminals
pub async fn list_terminals(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<TerminalState>>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
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

    let response = state
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
        .map_err(|e| {
            terminal_command_send_error(e, TerminalCommandResponseKind::Input, &terminal_id)
        })?;
    validate_terminal_command_response(response, TerminalCommandResponseKind::Input, &terminal_id)?;
    Ok(StatusCode::NO_CONTENT.into_response())
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

    let response = state
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
        .map_err(|e| {
            terminal_command_send_error(e, TerminalCommandResponseKind::Resize, &terminal_id)
        })?;
    validate_terminal_command_response(
        response,
        TerminalCommandResponseKind::Resize,
        &terminal_id,
    )?;
    Ok(StatusCode::NO_CONTENT.into_response())
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

    let response = state
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
        .map_err(|e| {
            terminal_command_send_error(e, TerminalCommandResponseKind::Kill, &terminal_id)
        })?;
    validate_terminal_command_response(response, TerminalCommandResponseKind::Kill, &terminal_id)?;
    Ok(StatusCode::NO_CONTENT.into_response())
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
        ProjectPermission::Use,
    )
    .await?;
    Ok(term_state)
}

#[derive(Debug, Clone, Copy)]
enum TerminalCommandResponseKind {
    Input,
    Resize,
    Kill,
}

impl TerminalCommandResponseKind {
    fn action_label(self) -> &'static str {
        match self {
            Self::Input => "终端输入",
            Self::Resize => "终端尺寸调整",
            Self::Kill => "终端结束",
        }
    }

    fn command_name(self) -> &'static str {
        match self {
            Self::Input => "terminal input",
            Self::Resize => "terminal resize",
            Self::Kill => "terminal kill",
        }
    }

    fn success_response_matches(self, response: &RelayMessage) -> bool {
        matches!(
            (self, response),
            (
                Self::Input,
                RelayMessage::ResponseTerminalInput {
                    payload: Some(_),
                    error: None,
                    ..
                }
            ) | (
                Self::Resize,
                RelayMessage::ResponseTerminalResize {
                    payload: Some(_),
                    error: None,
                    ..
                }
            ) | (
                Self::Kill,
                RelayMessage::ResponseTerminalKill {
                    payload: Some(_),
                    error: None,
                    ..
                }
            )
        )
    }
}

fn validate_terminal_command_response(
    response: RelayMessage,
    expected: TerminalCommandResponseKind,
    terminal_id: &str,
) -> Result<(), ApiError> {
    if expected.success_response_matches(&response) {
        return Ok(());
    }

    match (expected, response) {
        (
            TerminalCommandResponseKind::Input,
            RelayMessage::ResponseTerminalInput {
                error: Some(error), ..
            },
        )
        | (
            TerminalCommandResponseKind::Resize,
            RelayMessage::ResponseTerminalResize {
                error: Some(error), ..
            },
        )
        | (
            TerminalCommandResponseKind::Kill,
            RelayMessage::ResponseTerminalKill {
                error: Some(error), ..
            },
        ) => {
            let context = DiagnosticErrorContext::new("terminal.command", "relay_error_response");
            diag_error!(
                Warn,
                Subsystem::Api,
                context = &context,
                error = &error,
                terminal_id = %terminal_id,
                command = expected.command_name(),
                relay_error_code = ?error.code,
                "terminal relay command returned an error response"
            );
            Err(api_error_from_terminal_relay_error(error, expected))
        }
        (_, other) => {
            diag!(Error, Subsystem::Api,
                operation = "terminal.command",
                stage = "unexpected_response_type",
                terminal_id = %terminal_id,
                command = expected.command_name(),
                response_id = %other.id(),
                "terminal relay command returned an unexpected response type"
            );
            Err(ApiError::Internal(format!(
                "{}返回了意外响应类型",
                expected.action_label()
            )))
        }
    }
}

fn api_error_from_terminal_relay_error(
    error: RelayError,
    expected: TerminalCommandResponseKind,
) -> ApiError {
    let message = format!("{}失败: {}", expected.action_label(), error.message);
    match error.code {
        RelayErrorCode::AuthFailed | RelayErrorCode::Forbidden => ApiError::Forbidden(message),
        RelayErrorCode::NotFound => ApiError::NotFound(message),
        RelayErrorCode::Conflict | RelayErrorCode::SessionBusy => ApiError::Conflict(message),
        RelayErrorCode::InvalidMessage => ApiError::BadRequest(message),
        RelayErrorCode::Timeout
        | RelayErrorCode::ExecutorNotFound
        | RelayErrorCode::ExecutorUnavailable => ApiError::ServiceUnavailable(message),
        RelayErrorCode::SpawnFailed | RelayErrorCode::RuntimeError | RelayErrorCode::IoError => {
            ApiError::Internal(message)
        }
    }
}

fn terminal_command_send_error(
    error: BackendCommandError,
    expected: TerminalCommandResponseKind,
    terminal_id: &str,
) -> ApiError {
    let context = DiagnosticErrorContext::new("terminal.command", "send_command");
    diag_error!(
        Error,
        Subsystem::Api,
        context = &context,
        error = &error,
        terminal_id = %terminal_id,
        command = expected.command_name(),
        "terminal relay command failed"
    );
    ApiError::ServiceUnavailable(format!("{}命令发送失败", expected.action_label()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_input_response_error_is_not_success() {
        let error = validate_terminal_command_response(
            RelayMessage::ResponseTerminalInput {
                id: "resp-1".to_string(),
                payload: None,
                error: Some(RelayError::runtime_error("terminal missing")),
            },
            TerminalCommandResponseKind::Input,
            "term-1",
        )
        .expect_err("relay error must fail");

        assert!(
            matches!(error, ApiError::Internal(message) if message.contains("terminal missing"))
        );
    }

    #[test]
    fn terminal_resize_requires_matching_response_type() {
        let error = validate_terminal_command_response(
            RelayMessage::ResponseTerminalInput {
                id: "resp-1".to_string(),
                payload: Some(TerminalInputResponse {
                    terminal_id: "term-1".to_string(),
                }),
                error: None,
            },
            TerminalCommandResponseKind::Resize,
            "term-1",
        )
        .expect_err("wrong response type must fail");

        assert!(matches!(error, ApiError::Internal(message) if message.contains("意外响应类型")));
    }

    #[test]
    fn terminal_kill_requires_success_payload() {
        let error = validate_terminal_command_response(
            RelayMessage::ResponseTerminalKill {
                id: "resp-1".to_string(),
                payload: None,
                error: None,
            },
            TerminalCommandResponseKind::Kill,
            "term-1",
        )
        .expect_err("missing payload must fail");

        assert!(matches!(error, ApiError::Internal(message) if message.contains("意外响应类型")));
    }

    #[test]
    fn terminal_kill_matching_success_is_ok() {
        validate_terminal_command_response(
            RelayMessage::ResponseTerminalKill {
                id: "resp-1".to_string(),
                payload: Some(TerminalKillResponse {
                    terminal_id: "term-1".to_string(),
                    status: "killed".to_string(),
                }),
                error: None,
            },
            TerminalCommandResponseKind::Kill,
            "term-1",
        )
        .expect("matching success response");
    }
}
