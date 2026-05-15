//! Agent prompt / cancel / discover 命令处理

use agentdash_relay::*;
use tokio::sync::mpsc;

use agentdash_application::session::{LaunchCommand, SessionHub, UserPromptInput};

use super::CommandHandler;
use super::relay_mcp_servers::parse_relay_mcp_servers;

impl CommandHandler {
    pub(super) async fn handle_prompt(
        &self,
        id: String,
        payload: CommandPromptPayload,
    ) -> RelayMessage {
        let hub = match &self.session_hub {
            Some(hub) => hub.clone(),
            None => {
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("SessionHub 未初始化")),
                };
            }
        };

        let session_id = payload.session_id.clone();
        let follow_up = payload.follow_up_session_id.clone();
        let mount_root_ref = payload.mount_root_ref.trim();
        if mount_root_ref.is_empty() {
            return RelayMessage::ResponsePrompt {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(
                    "command.prompt 缺少 mount_root_ref",
                )),
            };
        }

        let executor_config = payload.executor_config.map(|c| {
            let mut cfg = agentdash_spi::AgentConfig::new(c.executor);
            cfg.provider_id = c.provider_id;
            cfg.model_id = c.model_id;
            cfg.agent_id = c.agent_id;
            cfg.thinking_level = c
                .thinking_level
                .and_then(|value| serde_json::from_value(serde_json::Value::String(value)).ok());
            cfg.permission_policy = c.permission_policy;
            cfg
        });

        let workspace_root = match self.tool_executor.validate_workspace_root(mount_root_ref) {
            Ok(path) => path,
            Err(error) => {
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(format!(
                        "mount_root_ref 校验失败: {error}"
                    ))),
                };
            }
        };

        if follow_up.is_none() {
            let prepare_result = tokio::task::spawn_blocking({
                let workspace_root = workspace_root.clone();
                let workspace_identity_kind = payload.workspace_identity_kind.clone();
                let workspace_identity_payload = payload.workspace_identity_payload.clone();
                let workspace_contract_config = self.workspace_contract_config.clone();
                move || {
                    crate::workspace_prepare::prepare_workspace(
                        &workspace_root,
                        workspace_identity_kind,
                        workspace_identity_payload.as_ref(),
                        &workspace_contract_config,
                    )
                }
            })
            .await;

            match prepare_result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    return RelayMessage::ResponsePrompt {
                        id,
                        payload: None,
                        error: Some(RelayError::runtime_error(format!(
                            "workspace prepare 失败: {error}"
                        ))),
                    };
                }
                Err(error) => {
                    return RelayMessage::ResponsePrompt {
                        id,
                        payload: None,
                        error: Some(RelayError::runtime_error(format!(
                            "workspace prepare 任务失败: {error}"
                        ))),
                    };
                }
            }
        }

        let command = LaunchCommand::local_relay_prompt_input(
            UserPromptInput {
                prompt_blocks: payload.prompt_blocks.map(|v| {
                    if let serde_json::Value::Array(arr) = v {
                        arr
                    } else {
                        vec![v]
                    }
                }),
                env: payload.env,
                executor_config,
            },
            parse_relay_mcp_servers(&payload.mcp_servers),
            workspace_root,
        )
        .with_follow_up(follow_up.clone());

        tracing::info!(
            session_id = %session_id,
            mount_root_ref = mount_root_ref,
            "收到 command.prompt，启动 Agent 执行"
        );

        let event_tx = self.event_tx.clone();

        match hub.launch_command(&session_id, command).await {
            Ok(turn_id) => {
                let hub_clone = hub.clone();
                let sid = session_id.clone();
                let tid = turn_id.clone();

                tokio::spawn(async move {
                    forward_session_notifications(hub_clone, &sid, &tid, event_tx).await;
                });

                RelayMessage::ResponsePrompt {
                    id,
                    payload: Some(ResponsePromptPayload {
                        turn_id,
                        status: "started".to_string(),
                    }),
                    error: None,
                }
            }
            Err(e) => {
                tracing::error!(session_id = %session_id, error = %e, "Agent 启动失败");
                RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(e.to_string())),
                }
            }
        }
    }

    pub(super) async fn handle_cancel(
        &self,
        id: String,
        payload: CommandCancelPayload,
    ) -> RelayMessage {
        let hub = match &self.session_hub {
            Some(hub) => hub,
            None => {
                return RelayMessage::ResponseCancel {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("SessionHub 未初始化")),
                };
            }
        };

        tracing::info!(session_id = %payload.session_id, "收到 command.cancel");
        match hub.cancel(&payload.session_id).await {
            Ok(()) => RelayMessage::ResponseCancel {
                id,
                payload: Some(ResponseCancelPayload {
                    status: "cancelled".to_string(),
                }),
                error: None,
            },
            Err(e) => RelayMessage::ResponseCancel {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(e.to_string())),
            },
        }
    }

    pub(super) async fn handle_discover(&self, id: String) -> RelayMessage {
        let executors = self.list_executors();
        RelayMessage::ResponseDiscover {
            id,
            payload: Some(ResponseDiscoverPayload { executors }),
            error: None,
        }
    }

    pub(super) async fn handle_discover_options(
        &self,
        id: String,
        payload: CommandDiscoverOptionsPayload,
    ) -> RelayMessage {
        tracing::debug!(
            executor = %payload.executor,
            "收到 command.discover_options，但本机 relay 尚未实现该流式能力"
        );
        RelayMessage::Error {
            id,
            error: RelayError::runtime_error(
                "本机 relay 尚未实现 command.discover_options，请改走云端直连 discovery 管线",
            ),
        }
    }
}

/// 订阅 SessionHub 的通知流并通过事件通道转发到 WebSocket
async fn forward_session_notifications(
    hub: SessionHub,
    session_id: &str,
    _turn_id: &str,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
) {
    let mut rx = hub.ensure_session(session_id).await;

    loop {
        match rx.recv().await {
            Ok(persisted_event) => {
                let envelope_json = serde_json::to_value(&persisted_event.notification)
                    .unwrap_or(serde_json::Value::Null);

                let relay_msg = RelayMessage::EventSessionNotification {
                    id: RelayMessage::new_id("evt"),
                    payload: SessionNotificationPayload {
                        session_id: session_id.to_string(),
                        notification: envelope_json,
                    },
                };

                if event_tx.send(relay_msg).is_err() {
                    tracing::warn!(
                        session_id = %session_id,
                        "事件通道已关闭，停止通知转发"
                    );
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(
                    session_id = %session_id,
                    skipped = n,
                    "通知流落后，跳过部分消息"
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::debug!(session_id = %session_id, "通知流关闭");
                break;
            }
        }
    }
}
