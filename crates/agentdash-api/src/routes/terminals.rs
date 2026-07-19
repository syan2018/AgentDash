use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeBindingRepository, AgentRunTerminalAvailability,
    AgentRunTerminalCapability, AgentRunTerminalControlRoutingRepository, AgentRunTerminalId,
    AgentRunTerminalLifecycleState, AgentRunTerminalOutputProjection,
    AgentRunTerminalOutputSequence, AgentRunTerminalOwnerEpochId, AgentRunTerminalOwnerFence,
    AgentRunTerminalProjection, AgentRunTerminalSourceSequence,
};
use agentdash_application_ports::agent_run_surface::AgentRunTerminalLaunchTarget;
use agentdash_relay::*;

use crate::auth::{CurrentUser, ProjectPermission};
use crate::dto::{SpawnTerminalBody, TerminalInputBody, TerminalResizeBody};
use crate::relay::registry::BackendCommandError;
use crate::agent_run_runtime_surface::resolve_current_runtime_surface_with_backend_for_agent_run_for_api;
use agentdash_application_ports::agent_run_surface::RuntimeSurfaceQueryPurpose;
use crate::{app_state::AppState, rpc::ApiError};

const TERMINAL_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals",
            axum::routing::post(spawn_terminal),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/{id}/input",
            axum::routing::post(terminal_input),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/{id}/resize",
            axum::routing::post(terminal_resize),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/{id}",
            axum::routing::delete(terminal_kill),
        )
}

pub async fn spawn_terminal(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<SpawnTerminalBody>,
) -> Result<Response, ApiError> {
    let runtime = resolve_current_runtime_surface_with_backend_for_agent_run_for_api(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
        RuntimeSurfaceQueryPurpose::new("terminal_spawn"),
        "Terminal",
    )
    .await?;
    let backend_id = runtime.runtime_backend_anchor.backend_id.clone();
    let mount_root_ref = runtime
        .runtime_backend_anchor
        .root_ref
        .clone()
        .ok_or_else(|| ApiError::Conflict("Terminal Runtime backend anchor 缺少 root_ref".into()))?;
    spawn_terminal_for_runtime_thread(
        &state,
        &runtime.runtime_thread_id,
        &run_id,
        &agent_id,
        AgentRunTerminalLaunchTarget {
            backend_id,
            mount_root_ref,
        },
        body,
    )
    .await
}

pub(crate) async fn spawn_terminal_for_runtime_thread(
    state: &Arc<AppState>,
    runtime_thread_id: &str,
    run_id: &str,
    agent_id: &str,
    target: AgentRunTerminalLaunchTarget,
    body: SpawnTerminalBody,
) -> Result<Response, ApiError> {
    if !state
        .services
        .backend_registry
        .is_online(&target.backend_id)
        .await
    {
        return Err(ApiError::Conflict(format!(
            "runtime trace 默认 workspace 所属 Backend 当前不在线: {}",
            target.backend_id
        )));
    }

    let terminal_id = RelayMessage::new_id("term");
    let backend_id = target.backend_id.clone();
    let terminal_cwd = body.cwd.clone();
    let payload = TerminalSpawnPayload {
        terminal_id: terminal_id.clone(),
        session_id: runtime_thread_id.to_string(),
        mount_root_ref: target.mount_root_ref,
        cwd: body.cwd,
        shell: body.shell,
        cols: body.cols.unwrap_or(80),
        rows: body.rows.unwrap_or(24),
        max_output_bytes: TERMINAL_MAX_OUTPUT_BYTES,
    };

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
            let target_ref = agentdash_domain::agent_run_target::AgentRunTarget {
                run_id: uuid::Uuid::parse_str(run_id)
                    .map_err(|_| ApiError::BadRequest("无效的 run_id".into()))?,
                agent_id: uuid::Uuid::parse_str(agent_id)
                    .map_err(|_| ApiError::BadRequest("无效的 agent_id".into()))?,
            };
            let binding = state
                .services
                .agent_run_product_runtime_bindings
                .load_product_binding(&target_ref)
                .await
                .map_err(ApiError::Internal)?
                .ok_or_else(|| ApiError::Conflict("Terminal 缺少 Product Runtime binding".into()))?;
            let latest_source_sequence = u64::max(resp.latest_source_sequence, 1);
            state
                .services
                .terminal_projection_producer
                .register_spawned(
                    AgentRunTerminalProjection {
                        terminal_id: AgentRunTerminalId::new(resp.terminal_id.clone())
                            .map_err(|error| ApiError::Internal(error.to_string()))?,
                        owner: AgentRunTerminalOwnerFence {
                            terminal_owner_epoch_id: AgentRunTerminalOwnerEpochId::new(
                                resp.terminal_owner_epoch_id.clone(),
                            )
                            .map_err(|error| ApiError::Internal(error.to_string()))?,
                            target: target_ref,
                            runtime_thread_id: binding.runtime_thread_id,
                            source_binding: binding.source_binding,
                            backend_id,
                        },
                        mount_id: None,
                        cwd: terminal_cwd,
                        capability: AgentRunTerminalCapability::Interactive,
                        max_output_bytes: u64::try_from(resp.max_output_bytes).unwrap_or(u64::MAX),
                        state: AgentRunTerminalLifecycleState::Starting,
                        availability: AgentRunTerminalAvailability::Online,
                        latest_source_sequence: AgentRunTerminalSourceSequence(
                            latest_source_sequence,
                        ),
                        exit_code: None,
                        process_id: resp.process_id,
                        created_at_ms: now_ms(),
                        exited_at_ms: None,
                        output: AgentRunTerminalOutputProjection {
                            next_sequence: AgentRunTerminalOutputSequence(0),
                            retained_output: String::new(),
                            truncated: false,
                            omitted_bytes: 0,
                        },
                    },
                    &format!(
                        "terminal-spawn:{}:{}",
                        resp.terminal_owner_epoch_id, latest_source_sequence
                    ),
                )
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            Ok(Json(serde_json::json!({
                "terminal_id": resp.terminal_id,
                "runtime_thread_id": runtime_thread_id,
                "terminal_owner_epoch_id": resp.terminal_owner_epoch_id,
                "latest_source_sequence": resp.latest_source_sequence,
                "max_output_bytes": resp.max_output_bytes,
                "process_id": resp.process_id,
            }))
            .into_response())
        }
        Ok(RelayMessage::ResponseTerminalSpawn {
            error: Some(err), ..
        }) => {
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err.message })),
            )
                .into_response())
        }
        _ => {
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
    Path((run_id, agent_id, terminal_id)): Path<(String, String, String)>,
    Json(body): Json<TerminalInputBody>,
) -> Result<Response, ApiError> {
    let term_state =
        load_terminal_for_user(&state, &current_user, &run_id, &agent_id, &terminal_id).await?;

    let payload = TerminalInputPayload {
        terminal_id: terminal_id.clone(),
        data: body.data,
    };

    let response = state
        .services
        .backend_registry
        .send_command(
            &term_state.owner.backend_id,
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
    Path((run_id, agent_id, terminal_id)): Path<(String, String, String)>,
    Json(body): Json<TerminalResizeBody>,
) -> Result<Response, ApiError> {
    let term_state =
        load_terminal_for_user(&state, &current_user, &run_id, &agent_id, &terminal_id).await?;

    let payload = TerminalResizePayload {
        terminal_id: terminal_id.clone(),
        cols: body.cols,
        rows: body.rows,
    };

    let response = state
        .services
        .backend_registry
        .send_command(
            &term_state.owner.backend_id,
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
    Path((run_id, agent_id, terminal_id)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let term_state =
        load_terminal_for_user(&state, &current_user, &run_id, &agent_id, &terminal_id).await?;

    let payload = TerminalKillPayload {
        terminal_id: terminal_id.clone(),
        signal: None,
    };

    let response = state
        .services
        .backend_registry
        .send_command(
            &term_state.owner.backend_id,
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
    run_id: &str,
    agent_id: &str,
    terminal_id: &str,
) -> Result<agentdash_application_agentrun::agent_run::AgentRunTerminalControlRoute, ApiError> {
    let target = agentdash_domain::agent_run_target::AgentRunTarget {
        run_id: uuid::Uuid::parse_str(run_id)
            .map_err(|_| ApiError::BadRequest("无效的 run_id".into()))?,
        agent_id: uuid::Uuid::parse_str(agent_id)
            .map_err(|_| ApiError::BadRequest("无效的 agent_id".into()))?,
    };
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(target.run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("AgentRun 不存在".into()))?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(target.agent_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("AgentRun Agent 不存在".into()))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict("AgentRun target 不一致".into()));
    }
    crate::auth::load_project_with_permission(
        state.as_ref(),
        current_user,
        run.project_id,
        ProjectPermission::Use,
    )
    .await?;
    let terminal_id =
        AgentRunTerminalId::new(terminal_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let route = state
        .services
        .terminal_projections
        .resolve_control_route(&target, &terminal_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound("terminal not found".to_string()))?;
    if route.availability != AgentRunTerminalAvailability::Online {
        return Err(ApiError::Conflict("terminal backend is not online".into()));
    }
    Ok(route)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
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
