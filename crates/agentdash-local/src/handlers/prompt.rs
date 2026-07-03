//! Agent prompt / cancel / discover 命令处理

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use agentdash_relay::*;
use tokio::sync::{Mutex, mpsc};

use agentdash_agent_protocol::UserInputBlock;
use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput, LaunchPromptInput};
use agentdash_application_runtime_session::session::{
    SessionRuntimeServices, SessionTurnSteerCommand,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::AgentConnector;

use super::CommandDispatchPlan;
use super::relay_mcp_servers::relay_mcp_servers_to_runtime;
use crate::local_backend_config::WorkspaceContractRuntimeConfig;
use crate::tool_executor::ToolExecutor;

#[derive(Clone)]
pub(super) struct PromptCommandHandler {
    session_runtime: Option<SessionRuntimeServices>,
    connector: Option<Arc<dyn AgentConnector>>,
    tool_executor: ToolExecutor,
    workspace_contract_config: WorkspaceContractRuntimeConfig,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
    session_forwarders: Arc<Mutex<HashSet<String>>>,
}

pub(super) struct PromptCommandHandlerConfig {
    pub session_runtime: Option<SessionRuntimeServices>,
    pub connector: Option<Arc<dyn AgentConnector>>,
    pub tool_executor: ToolExecutor,
    pub workspace_contract_config: WorkspaceContractRuntimeConfig,
    pub event_tx: mpsc::UnboundedSender<RelayMessage>,
    pub session_forwarders: Arc<Mutex<HashSet<String>>>,
}

impl PromptCommandHandler {
    pub(super) fn new(config: PromptCommandHandlerConfig) -> Self {
        Self {
            session_runtime: config.session_runtime,
            connector: config.connector,
            tool_executor: config.tool_executor,
            workspace_contract_config: config.workspace_contract_config,
            event_tx: config.event_tx,
            session_forwarders: config.session_forwarders,
        }
    }

    pub(super) fn list_executors(&self) -> Vec<AgentInfoRelay> {
        match &self.connector {
            Some(connector) => connector
                .list_executors()
                .into_iter()
                .map(|info| AgentInfoRelay {
                    id: info.id,
                    name: info.name,
                    variants: info.variants,
                    available: info.available,
                })
                .collect(),
            None => vec![],
        }
    }

    pub(super) fn dispatch_plan(msg: &RelayMessage) -> Option<CommandDispatchPlan> {
        match msg {
            RelayMessage::CommandPrompt { .. }
            | RelayMessage::CommandCancel { .. }
            | RelayMessage::CommandSteer { .. }
            | RelayMessage::CommandDiscover { .. }
            | RelayMessage::CommandDiscoverOptions { .. } => Some(CommandDispatchPlan::INLINE),
            _ => None,
        }
    }

    pub(super) async fn handle_prompt(
        &self,
        id: String,
        payload: CommandPromptPayload,
    ) -> RelayMessage {
        let session_runtime = match &self.session_runtime {
            Some(session_runtime) => session_runtime.clone(),
            None => {
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("Session runtime 未初始化")),
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

        let mcp_servers = match relay_mcp_servers_to_runtime(&payload.mcp_servers) {
            Ok(servers) => servers,
            Err(error) => {
                let context = DiagnosticErrorContext::new(
                    "local_runtime.prompt.handle",
                    "mcp_servers_decode",
                );
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    command_id = %id,
                    session_id = %session_id,
                    mcp_server_count = payload.mcp_servers.len(),
                    "command.prompt MCP server 配置非法"
                );
                return RelayMessage::ResponsePrompt {
                    id,
                    payload: None,
                    error: Some(RelayError::invalid_message(format!(
                        "mcp_servers 配置非法: {error}"
                    ))),
                };
            }
        };

        let workspace_root = match self.tool_executor.validate_workspace_root(mount_root_ref) {
            Ok(path) => path,
            Err(error) => {
                let context = DiagnosticErrorContext::new(
                    "local_runtime.prompt.handle",
                    "workspace_root_validate",
                );
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    command_id = %id,
                    session_id = %session_id,
                    mount_root_ref_len = mount_root_ref.len(),
                    "command.prompt workspace root 校验失败"
                );
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
                let workspace_identity_kind =
                    payload
                        .workspace_identity_kind
                        .clone()
                        .map(|kind| match kind {
                            WorkspaceIdentityKindRelay::GitRepo => {
                                agentdash_domain::workspace::WorkspaceIdentityKind::GitRepo
                            }
                            WorkspaceIdentityKindRelay::P4Workspace => {
                                agentdash_domain::workspace::WorkspaceIdentityKind::P4Workspace
                            }
                            WorkspaceIdentityKindRelay::LocalDir => {
                                agentdash_domain::workspace::WorkspaceIdentityKind::LocalDir
                            }
                        });
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
                    let context = DiagnosticErrorContext::new(
                        "local_runtime.prompt.handle",
                        "workspace_prepare",
                    );
                    diag_error!(
                        Error,
                        Subsystem::AgentRun,
                        context = &context,
                        error = &error,
                        command_id = %id,
                        session_id = %session_id,
                        follow_up = follow_up.is_some(),
                        workspace_identity_kind = ?payload.workspace_identity_kind,
                        "command.prompt workspace prepare 失败"
                    );
                    return RelayMessage::ResponsePrompt {
                        id,
                        payload: None,
                        error: Some(RelayError::runtime_error(format!(
                            "workspace prepare 失败: {error}"
                        ))),
                    };
                }
                Err(error) => {
                    let context = DiagnosticErrorContext::new(
                        "local_runtime.prompt.handle",
                        "workspace_prepare_join",
                    );
                    diag_error!(
                        Error,
                        Subsystem::AgentRun,
                        context = &context,
                        error = &error,
                        command_id = %id,
                        session_id = %session_id,
                        follow_up = follow_up.is_some(),
                        workspace_identity_kind = ?payload.workspace_identity_kind,
                        "command.prompt workspace prepare 任务失败"
                    );
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

        let mcp_server_count = mcp_servers.len();
        let command = LaunchCommand::local_relay_prompt_input(
            typed_relay_prompt_user_input(payload.input, payload.env, executor_config),
            mcp_servers,
            workspace_root,
        )
        .with_follow_up(follow_up.clone());

        diag!(Info, Subsystem::AgentRun,

            session_id = %session_id,
            mount_root_ref = mount_root_ref,
            "收到 command.prompt，启动 Agent 执行"
        );

        let event_tx = self.event_tx.clone();

        match session_runtime
            .launch
            .launch_command(&session_id, command, LaunchPlanningInput::default())
            .await
        {
            Ok(turn_id) => {
                let session_runtime = session_runtime.clone();
                let sid = session_id.clone();
                let tid = turn_id.clone();
                let session_forwarders = self.session_forwarders.clone();

                if claim_session_forwarder(&session_forwarders, &sid).await {
                    tokio::spawn(async move {
                        forward_session_notifications(session_runtime, &sid, &tid, event_tx).await;
                        release_session_forwarder(&session_forwarders, &sid).await;
                    });
                } else {
                    diag!(Debug, Subsystem::AgentRun,

                        session_id = %session_id,
                        "relay session notification forwarder 已存在，复用现有转发任务"
                    );
                }

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
                let context =
                    DiagnosticErrorContext::new("local_runtime.prompt.handle", "launch_command");
                diag_error!(
                    Error,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &e,
                    command_id = %id,
                    session_id = %session_id,
                    follow_up = follow_up.is_some(),
                    mcp_server_count = mcp_server_count,
                    "Agent 启动失败"
                );
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
        let session_runtime = match &self.session_runtime {
            Some(session_runtime) => session_runtime,
            None => {
                return RelayMessage::ResponseCancel {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("Session runtime 未初始化")),
                };
            }
        };

        diag!(Info, Subsystem::AgentRun,
        session_id = %payload.session_id, "收到 command.cancel");
        match session_runtime.runtime.cancel(&payload.session_id).await {
            Ok(()) => RelayMessage::ResponseCancel {
                id,
                payload: Some(ResponseCancelPayload {
                    status: "cancelled".to_string(),
                }),
                error: None,
            },
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("local_runtime.prompt.cancel", "cancel_session");
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    command_id = %id,
                    session_id = %payload.session_id,
                    "command.cancel 失败"
                );
                RelayMessage::ResponseCancel {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(error.to_string())),
                }
            }
        }
    }

    pub(super) async fn handle_steer(
        &self,
        id: String,
        payload: CommandSteerPayload,
    ) -> RelayMessage {
        let session_runtime = match &self.session_runtime {
            Some(session_runtime) => session_runtime,
            None => {
                return RelayMessage::ResponseSteer {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error("Session runtime 未初始化")),
                };
            }
        };

        diag!(Info, Subsystem::AgentRun,
        session_id = %payload.session_id, "收到 command.steer");
        match session_runtime
            .control
            .steer_session(SessionTurnSteerCommand {
                session_id: payload.session_id.clone(),
                expected_turn_id: payload.expected_turn_id.clone(),
                input: payload.input,
            })
            .await
        {
            Ok(()) => RelayMessage::ResponseSteer {
                id,
                payload: Some(ResponseSteerPayload {
                    status: "accepted".to_string(),
                }),
                error: None,
            },
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("local_runtime.prompt.steer", "steer_session");
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    command_id = %id,
                    session_id = %payload.session_id,
                    expected_turn_id = ?payload.expected_turn_id,
                    "command.steer 失败"
                );
                RelayMessage::ResponseSteer {
                    id,
                    payload: None,
                    error: Some(RelayError::runtime_error(error.to_string())),
                }
            }
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
        diag!(Debug, Subsystem::AgentRun,

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

fn typed_relay_prompt_user_input(
    input: Vec<UserInputBlock>,
    env: HashMap<String, String>,
    executor_config: Option<AgentConfig>,
) -> LaunchPromptInput {
    LaunchPromptInput {
        input: Some(input),
        environment_variables: env,
        executor_config,
    }
}

async fn claim_session_forwarder(
    session_forwarders: &Arc<Mutex<HashSet<String>>>,
    session_id: &str,
) -> bool {
    session_forwarders
        .lock()
        .await
        .insert(session_id.to_string())
}

async fn release_session_forwarder(
    session_forwarders: &Arc<Mutex<HashSet<String>>>,
    session_id: &str,
) {
    session_forwarders.lock().await.remove(session_id);
}

/// 订阅 session 通知流并通过事件通道转发到 WebSocket
async fn forward_session_notifications(
    session_runtime: SessionRuntimeServices,
    session_id: &str,
    turn_id: &str,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
) {
    let mut rx = session_runtime.eventing.ensure_session(session_id).await;

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
                    diag!(Warn, Subsystem::AgentRun,

                        operation = "local_runtime.session_notifications",
                        stage = "event_channel_send",
                        session_id = %session_id,
                        turn_id = %turn_id,
                        "事件通道已关闭，停止通知转发"
                    );
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                diag!(Warn, Subsystem::AgentRun,

                    operation = "local_runtime.session_notifications",
                    stage = "broadcast_lagged",
                    session_id = %session_id,
                    skipped = n,
                    "通知流落后，跳过部分消息"
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                diag!(Debug, Subsystem::AgentRun,
        session_id = %session_id, "通知流关闭");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Arc;

    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use tokio::sync::Mutex;

    use super::{
        claim_session_forwarder, release_session_forwarder, typed_relay_prompt_user_input,
    };

    #[test]
    fn typed_relay_prompt_user_input_preserves_blocks() {
        let input = vec![
            codex::UserInput::Text {
                text: "hello".to_string(),
                text_elements: Vec::new(),
            },
            codex::UserInput::Image {
                detail: None,
                url: "data:image/png;base64,AAAA".to_string(),
            },
            codex::UserInput::LocalImage {
                detail: None,
                path: PathBuf::from("assets/local.png"),
            },
            codex::UserInput::Skill {
                name: "reviewer".to_string(),
                path: PathBuf::from("skills/reviewer/SKILL.md"),
            },
            codex::UserInput::Mention {
                name: "main.rs".to_string(),
                path: "file://src/main.rs".to_string(),
            },
        ];

        let user_input = typed_relay_prompt_user_input(input.clone(), HashMap::new(), None);

        assert_eq!(user_input.input, Some(input));
        assert!(user_input.environment_variables.is_empty());
    }

    #[tokio::test]
    async fn session_forwarder_claim_is_unique_until_released() {
        let active = Arc::new(Mutex::new(HashSet::new()));

        assert!(claim_session_forwarder(&active, "session-1").await);
        assert!(
            !claim_session_forwarder(&active, "session-1").await,
            "同一 session 已有 forwarder 时不应重复启动"
        );

        release_session_forwarder(&active, "session-1").await;
        assert!(claim_session_forwarder(&active, "session-1").await);
    }
}
