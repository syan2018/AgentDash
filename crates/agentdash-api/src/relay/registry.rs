use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{RwLock, mpsc, oneshot};

use agentdash_domain::backend::RuntimeBackendAnchorError;
use agentdash_integration_remote_runtime::{
    RemoteRuntimeTransportError, RuntimeWirePlacementRequest,
};
use agentdash_relay::RuntimeRelayStreamId;
use agentdash_relay::{CapabilitiesPayload, RelayMessage};
use agentdash_spi::RelayMcpCallContext;

pub type BackendSender = mpsc::UnboundedSender<RelayMessage>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegisterBackendError {
    #[error("backend already online: {backend_id}")]
    AlreadyOnline { backend_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendCommandError {
    #[error("Backend 不在线: {backend_id}")]
    Offline { backend_id: String },
    #[error("发送至本机后端失败: {backend_id}")]
    SendFailed { backend_id: String },
    #[error("命令超时: {backend_id}")]
    Timeout { backend_id: String },
    #[error("本机后端响应通道已关闭: {backend_id}")]
    ResponseDropped { backend_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RelayMcpBackendResolutionError {
    #[error("relay MCP runtime context 缺失: server={server}")]
    MissingContext { server: String },
    #[error(transparent)]
    MissingAnchor(#[from] RuntimeBackendAnchorError),
    #[error("relay MCP backend anchor 指向的 backend 不在线: backend_id={backend_id}")]
    BackendOffline { backend_id: String },
}

/// 已连接的本机后端
pub struct ConnectedBackend {
    pub backend_id: String,
    pub name: String,
    pub version: String,
    pub capabilities: CapabilitiesPayload,
    pub sender: BackendSender,
    pub connected_at: DateTime<Utc>,
}

/// 返回给 API 的后端在线信息
#[derive(Debug, Clone, Serialize)]
pub struct OnlineBackendInfo {
    pub backend_id: String,
    pub name: String,
    pub version: String,
    pub capabilities: CapabilitiesPayload,
    pub connected_at: DateTime<Utc>,
}

/// 中继后端注册表 — 跟踪所有通过 WebSocket 连接的本机后端
pub struct BackendRegistry {
    backends: RwLock<HashMap<String, ConnectedBackend>>,
    /// 等待本机响应的挂起请求（msg_id → pending request）
    pending: RwLock<HashMap<String, PendingRequest>>,
    runtime_wire_routes:
        RwLock<HashMap<RuntimeRelayStreamId, Arc<super::runtime_wire::CloudRuntimeWirePlacement>>>,
}

struct PendingRequest {
    backend_id: String,
    tx: oneshot::Sender<RelayMessage>,
}

impl BackendRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            backends: RwLock::new(HashMap::new()),
            pending: RwLock::new(HashMap::new()),
            runtime_wire_routes: RwLock::new(HashMap::new()),
        })
    }

    pub async fn try_register(
        &self,
        backend: ConnectedBackend,
    ) -> Result<(), RegisterBackendError> {
        let id = backend.backend_id.clone();
        let mut backends = self.backends.write().await;
        if backends.contains_key(&id) {
            return Err(RegisterBackendError::AlreadyOnline { backend_id: id });
        }
        backends.insert(id.clone(), backend);
        drop(backends);
        let routes = self
            .runtime_wire_routes
            .read()
            .await
            .values()
            .filter(|route| route.request().host_id == id)
            .cloned()
            .collect::<Vec<_>>();
        for route in routes {
            if let Err(error) = route.reopen().await {
                diag!(Warn, Subsystem::Relay,
                    backend_id = %id,
                    error = %error,
                    "Runtime Wire stream reopen failed after backend registration"
                );
            }
        }
        diag!(Info, Subsystem::Relay,
        backend_id = %id, "本机后端已注册");
        Ok(())
    }

    pub async fn unregister(&self, backend_id: &str) {
        {
            let mut backends = self.backends.write().await;
            backends.remove(backend_id);
        }
        self.pending
            .write()
            .await
            .retain(|_, pending| pending.backend_id != backend_id);
        let routes = self
            .runtime_wire_routes
            .read()
            .await
            .values()
            .filter(|route| route.request().host_id == backend_id)
            .cloned()
            .collect::<Vec<_>>();
        for route in routes {
            route.disconnect().await;
        }
        diag!(Info, Subsystem::Relay,
        backend_id = %backend_id, "本机后端已断开");
    }

    /// 向指定后端发送命令并等待响应
    pub async fn send_command(
        &self,
        backend_id: &str,
        msg: RelayMessage,
    ) -> Result<RelayMessage, BackendCommandError> {
        self.send_command_with_timeout(backend_id, msg, std::time::Duration::from_secs(30))
            .await
    }

    /// 匹配并分发一条响应消息到等待方
    pub async fn resolve_response(&self, msg: &RelayMessage) -> bool {
        let id = msg.id().to_string();
        if let Some(pending) = self.pending.write().await.remove(&id) {
            let _ = pending.tx.send(msg.clone());
            true
        } else {
            false
        }
    }

    pub(crate) async fn send_runtime_wire_message(
        &self,
        backend_id: &str,
        message: RelayMessage,
    ) -> Result<(), BackendCommandError> {
        let sender = self
            .backends
            .read()
            .await
            .get(backend_id)
            .map(|backend| backend.sender.clone())
            .ok_or_else(|| BackendCommandError::Offline {
                backend_id: backend_id.to_string(),
            })?;
        sender
            .send(message)
            .map_err(|_| BackendCommandError::SendFailed {
                backend_id: backend_id.to_string(),
            })
    }

    pub(crate) async fn resolve_runtime_wire_placement(
        self: &Arc<Self>,
        request: RuntimeWirePlacementRequest,
        max_in_flight_frames: usize,
    ) -> Result<Arc<super::runtime_wire::CloudRuntimeWirePlacement>, RemoteRuntimeTransportError>
    {
        let candidate = super::runtime_wire::CloudRuntimeWirePlacement::new(
            request.clone(),
            max_in_flight_frames,
            self.clone(),
        );
        let placement = {
            let mut routes = self.runtime_wire_routes.write().await;
            if let Some(existing) = routes.get(candidate.stream_id()) {
                if existing.request() != &request {
                    return Err(RemoteRuntimeTransportError::Protocol {
                        reason: "Runtime Wire stream identity was reused with different provenance"
                            .to_string(),
                        critical: true,
                    });
                }
                let existing = existing.clone();
                drop(routes);
                existing.wait_until_open().await?;
                return Ok(existing);
            }
            routes.insert(candidate.stream_id().clone(), candidate.clone());
            candidate
        };
        if let Err(error) = placement.open().await {
            self.runtime_wire_routes
                .write()
                .await
                .remove(placement.stream_id());
            return Err(error);
        }
        Ok(placement)
    }

    pub(crate) async fn handle_runtime_wire_message(
        &self,
        backend_id: &str,
        message: &RelayMessage,
    ) -> bool {
        if let RelayMessage::Error { id, error } = message
            && let Some(stream_id) = id
                .strip_prefix("runtime-wire-open:")
                .or_else(|| id.strip_prefix("runtime-wire-reopen:"))
        {
            let route = self
                .runtime_wire_routes
                .read()
                .await
                .get(&RuntimeRelayStreamId(stream_id.to_string()))
                .cloned();
            if let Some(route) = route
                && route.request().host_id == backend_id
            {
                route.reject_open(error.message.clone()).await;
                return true;
            }
        }
        let (stream_id, action) = match message {
            RelayMessage::RuntimeWireOpenAck { payload, .. } => (payload.stream_id.clone(), 0_u8),
            RelayMessage::RuntimeWireFrame { payload, .. } => (payload.stream_id.clone(), 1_u8),
            RelayMessage::RuntimeWireAck { payload, .. } => (payload.stream_id.clone(), 2_u8),
            _ => return false,
        };
        let route = self
            .runtime_wire_routes
            .read()
            .await
            .get(&stream_id)
            .cloned();
        let Some(route) = route else {
            return false;
        };
        if route.request().host_id != backend_id {
            return true;
        }
        let result = match (action, message) {
            (0, RelayMessage::RuntimeWireOpenAck { payload, .. }) => {
                match route.accept_open(payload.clone()).await {
                    Ok(replay) => {
                        for frame in replay {
                            if let Err(error) = self
                                .send_runtime_wire_message(
                                    backend_id,
                                    RelayMessage::RuntimeWireFrame {
                                        id: format!(
                                            "runtime-wire-replay:{}:{}",
                                            frame.stream_id.0, frame.sequence
                                        ),
                                        payload: Box::new(frame),
                                    },
                                )
                                .await
                            {
                                return {
                                    diag!(Warn, Subsystem::Relay, error = %error,
                                        "Runtime Wire replay send failed");
                                    true
                                };
                            }
                        }
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            (1, RelayMessage::RuntimeWireFrame { id, payload }) => {
                match route.accept_frame((**payload).clone()).await {
                    Ok(ack) => self
                        .send_runtime_wire_message(
                            backend_id,
                            RelayMessage::RuntimeWireAck {
                                id: id.clone(),
                                payload: ack,
                            },
                        )
                        .await
                        .map_err(|error| RemoteRuntimeTransportError::Unavailable {
                            reason: error.to_string(),
                            retryable: true,
                        }),
                    Err(error) => Err(error),
                }
            }
            (2, RelayMessage::RuntimeWireAck { payload, .. }) => {
                route.accept_ack(payload.clone()).await
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            diag!(Warn, Subsystem::Relay,
                backend_id = %backend_id,
                stream_id = %stream_id.0,
                error = %error,
                "Runtime Wire persistent route rejected a message"
            );
        }
        true
    }

    pub async fn list_online(&self) -> Vec<OnlineBackendInfo> {
        let backends = self.backends.read().await;
        backends
            .values()
            .map(|b| OnlineBackendInfo {
                backend_id: b.backend_id.clone(),
                name: b.name.clone(),
                version: b.version.clone(),
                capabilities: b.capabilities.clone(),
                connected_at: b.connected_at,
            })
            .collect()
    }

    pub async fn is_online(&self, backend_id: &str) -> bool {
        self.backends.read().await.contains_key(backend_id)
    }

    pub async fn list_online_ids(&self) -> Vec<String> {
        self.backends.read().await.keys().cloned().collect()
    }

    /// 获取任意一个在线 backend ID。
    ///
    /// 仅保留给手工诊断/测试辅助。产品路径不得用它兜底选择 backend；
    /// runtime relay MCP discovery/call 必须消费 `RelayMcpCallContext.backend_anchor`，
    /// setup probe 必须先解析明确的用户本机 backend target。
    pub async fn find_any_online_backend_for_setup_probe(&self) -> Option<String> {
        self.backends.read().await.keys().next().cloned()
    }

    // ── MCP Relay 支持 ──

    /// 更新指定 backend 的能力信息（含 MCP server 列表）
    pub async fn update_capabilities(&self, backend_id: &str, capabilities: CapabilitiesPayload) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.get_mut(backend_id) {
            backend.capabilities = capabilities;
            diag!(Info, Subsystem::Relay,
        backend_id = %backend_id, "后端能力已更新");
        }
    }

    /// 查找上报了指定 MCP server catalog 的在线 backend。
    ///
    /// 仅用于配置/诊断展示。runtime relay MCP discovery/call 中 `server_name`
    /// 只选择 MCP server，不能用它反向选择 backend。
    pub async fn find_backend_for_mcp_server_catalog_diagnostic(
        &self,
        server_name: &str,
    ) -> Option<String> {
        let backends = self.backends.read().await;
        backends
            .values()
            .find(|b| {
                b.capabilities
                    .mcp_servers
                    .iter()
                    .any(|s| s.name == server_name)
            })
            .map(|b| b.backend_id.clone())
    }

    /// 解析 runtime relay MCP 应投递到的本机 backend。
    ///
    /// 运行中的 relay MCP discovery/call 必须使用 Lifecycle/AgentRun 写入
    /// `RelayMcpCallContext.backend_anchor` 的 backend。`server_name` 只选择 MCP server，
    /// 不参与 backend routing；缺 context、缺 anchor 或 anchor backend 离线时都不能
    /// fallback 到 session route、backend MCP catalog 或其它在线 backend。
    pub async fn resolve_backend_for_relay_mcp(
        &self,
        server_name: &str,
        context: Option<&RelayMcpCallContext>,
    ) -> Result<String, RelayMcpBackendResolutionError> {
        let Some(context) = context else {
            diag!(Warn, Subsystem::Relay,

                operation = "relay.mcp.resolve_backend",
                stage = "missing_context",
                server = %server_name,
                "relay MCP runtime context 缺失，跳过 backend fallback"
            );
            return Err(RelayMcpBackendResolutionError::MissingContext {
                server: server_name.to_string(),
            });
        };

        let anchor = context
            .require_backend_anchor("relay_mcp")
            .inspect_err(|error| {
                let diagnostic_context =
                    DiagnosticErrorContext::new("relay.mcp.resolve_backend", "require_anchor")
                        .with_field("session_id", &context.session_id)
                        .with_field("server", server_name);
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &diagnostic_context,
                    error = error,

                    session_id = %context.session_id,
                    turn_id = ?context.turn_id,
                    server = %server_name,
                    "relay MCP runtime context 缺少 backend anchor，跳过 backend fallback"
                );
            })?;
        let backend_id = anchor.backend_id();

        if self.is_online(backend_id).await {
            return Ok(backend_id.to_string());
        }

        diag!(Warn, Subsystem::Relay,

            operation = "relay.mcp.resolve_backend",
            stage = "backend_offline",
            session_id = %context.session_id,
            turn_id = ?context.turn_id,
            backend_id = %backend_id,
            anchor_source = %anchor.source.as_str(),
            server = %server_name,
            "relay MCP backend anchor 指向的 backend 已离线，跳过 backend fallback"
        );
        Err(RelayMcpBackendResolutionError::BackendOffline {
            backend_id: backend_id.to_string(),
        })
    }

    /// 列出所有在线 backend 上报的 MCP server 信息
    pub async fn list_all_mcp_servers(&self) -> Vec<(String, agentdash_relay::McpServerInfoRelay)> {
        let backends = self.backends.read().await;
        let mut result = Vec::new();
        for backend in backends.values() {
            for server in &backend.capabilities.mcp_servers {
                result.push((backend.backend_id.clone(), server.clone()));
            }
        }
        result
    }

    /// 向指定后端发送命令并等待响应（可指定超时）
    pub async fn send_command_with_timeout(
        &self,
        backend_id: &str,
        msg: RelayMessage,
        timeout: std::time::Duration,
    ) -> Result<RelayMessage, BackendCommandError> {
        let msg_id = msg.id().to_string();
        let message_kind = relay_message_kind(&msg);

        let sender = {
            let backends = self.backends.read().await;
            let backend = match backends.get(backend_id) {
                Some(backend) => backend,
                None => {
                    let error = BackendCommandError::Offline {
                        backend_id: backend_id.to_string(),
                    };
                    let context = DiagnosticErrorContext::new(
                        "relay.registry.send_command",
                        "resolve_backend",
                    )
                    .with_field("backend_id", backend_id)
                    .with_field("request_id", &msg_id)
                    .with_field("message_kind", message_kind);
                    diag_error!(
                        Warn,
                        Subsystem::Relay,
                        context = &context,
                        error = &error,
                        backend_id = %backend_id,
                        request_id = %msg_id,
                        message_kind = %message_kind,
                        "relay command cannot be sent because backend is offline"
                    );
                    return Err(error);
                }
            };
            backend.sender.clone()
        };

        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(
            msg_id.clone(),
            PendingRequest {
                backend_id: backend_id.to_string(),
                tx,
            },
        );

        if sender.send(msg).is_err() {
            self.pending.write().await.remove(&msg_id);
            let error = BackendCommandError::SendFailed {
                backend_id: backend_id.to_string(),
            };
            let context =
                DiagnosticErrorContext::new("relay.registry.send_command", "backend_channel_send")
                    .with_field("backend_id", backend_id)
                    .with_field("request_id", &msg_id)
                    .with_field("message_kind", message_kind);
            diag_error!(
                Warn,
                Subsystem::Relay,
                context = &context,
                error = &error,
                backend_id = %backend_id,
                request_id = %msg_id,
                message_kind = %message_kind,
                "relay command channel send failed"
            );
            return Err(error);
        }

        let resp = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.pending.write().await.remove(&msg_id);
                let error = BackendCommandError::ResponseDropped {
                    backend_id: backend_id.to_string(),
                };
                let context =
                    DiagnosticErrorContext::new("relay.registry.send_command", "response_dropped")
                        .with_field("backend_id", backend_id)
                        .with_field("request_id", &msg_id)
                        .with_field("message_kind", message_kind);
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &context,
                    error = &error,
                    backend_id = %backend_id,
                    request_id = %msg_id,
                    message_kind = %message_kind,
                    "relay command response channel dropped"
                );
                return Err(error);
            }
            Err(_) => {
                self.pending.write().await.remove(&msg_id);
                let error = BackendCommandError::Timeout {
                    backend_id: backend_id.to_string(),
                };
                let context =
                    DiagnosticErrorContext::new("relay.registry.send_command", "response_timeout")
                        .with_field("backend_id", backend_id)
                        .with_field("request_id", &msg_id)
                        .with_field("message_kind", message_kind)
                        .with_field("timeout_ms", timeout.as_millis());
                diag_error!(
                    Warn,
                    Subsystem::Relay,
                    context = &context,
                    error = &error,
                    backend_id = %backend_id,
                    request_id = %msg_id,
                    message_kind = %message_kind,
                    timeout_ms = timeout.as_millis(),
                    "relay command response timed out"
                );
                return Err(error);
            }
        };

        Ok(resp)
    }

    #[cfg(test)]
    async fn drop_pending_for_test(&self, msg_id: &str) {
        self.pending.write().await.remove(msg_id);
    }
}

pub(crate) fn relay_message_kind(msg: &RelayMessage) -> &'static str {
    match msg {
        RelayMessage::Register { .. } => "register",
        RelayMessage::RegisterAck { .. } => "register_ack",
        RelayMessage::Ping { .. } => "ping",
        RelayMessage::Pong { .. } => "pong",
        RelayMessage::RuntimeWireOpen { .. } => "runtime_wire.open",
        RelayMessage::RuntimeWireOpenAck { .. } => "runtime_wire.open_ack",
        RelayMessage::RuntimeWireFrame { .. } => "runtime_wire.frame",
        RelayMessage::RuntimeWireAck { .. } => "runtime_wire.ack",
        RelayMessage::CommandPrompt { .. } => "command.prompt",
        RelayMessage::CommandCancel { .. } => "command.cancel",
        RelayMessage::CommandSteer { .. } => "command.steer",
        RelayMessage::CommandDiscover { .. } => "command.discover",
        RelayMessage::CommandDiscoverOptions { .. } => "command.discover_options",
        RelayMessage::CommandWorkspaceDetect { .. } => "command.workspace_detect",
        RelayMessage::CommandWorkspaceDetectGit { .. } => "command.workspace_detect_git",
        RelayMessage::CommandWorkspaceDiscoverByIdentity { .. } => {
            "command.workspace_discover_by_identity"
        }
        RelayMessage::CommandBrowseDirectory { .. } => "command.browse_directory",
        RelayMessage::CommandToolFileRead { .. } => "command.tool.file_read",
        RelayMessage::CommandToolFileReadBinary { .. } => "command.tool.file_read_binary",
        RelayMessage::CommandToolFileWrite { .. } => "command.tool.file_write",
        RelayMessage::CommandToolFileDelete { .. } => "command.tool.file_delete",
        RelayMessage::CommandToolFileRename { .. } => "command.tool.file_rename",
        RelayMessage::CommandToolApplyPatch { .. } => "command.tool.apply_patch",
        RelayMessage::CommandToolShellExec { .. } => "command.tool.shell_exec",
        RelayMessage::CommandToolShellRead { .. } => "command.tool.shell_read",
        RelayMessage::CommandToolShellInput { .. } => "command.tool.shell_input",
        RelayMessage::CommandToolShellTerminate { .. } => "command.tool.shell_terminate",
        RelayMessage::CommandVfsMaterialize { .. } => "command.vfs.materialize",
        RelayMessage::CommandToolFileList { .. } => "command.tool.file_list",
        RelayMessage::CommandToolSearch { .. } => "command.tool.search",
        RelayMessage::ResponsePrompt { .. } => "response.prompt",
        RelayMessage::ResponseCancel { .. } => "response.cancel",
        RelayMessage::ResponseSteer { .. } => "response.steer",
        RelayMessage::ResponseDiscover { .. } => "response.discover",
        RelayMessage::ResponseWorkspaceDetect { .. } => "response.workspace_detect",
        RelayMessage::ResponseWorkspaceDetectGit { .. } => "response.workspace_detect_git",
        RelayMessage::ResponseWorkspaceDiscoverByIdentity { .. } => {
            "response.workspace_discover_by_identity"
        }
        RelayMessage::ResponseBrowseDirectory { .. } => "response.browse_directory",
        RelayMessage::ResponseToolFileRead { .. } => "response.tool.file_read",
        RelayMessage::ResponseToolFileReadBinary { .. } => "response.tool.file_read_binary",
        RelayMessage::ResponseToolFileWrite { .. } => "response.tool.file_write",
        RelayMessage::ResponseToolFileDelete { .. } => "response.tool.file_delete",
        RelayMessage::ResponseToolFileRename { .. } => "response.tool.file_rename",
        RelayMessage::ResponseToolApplyPatch { .. } => "response.tool.apply_patch",
        RelayMessage::ResponseToolShellExec { .. } => "response.tool.shell_exec",
        RelayMessage::ResponseToolShellRead { .. } => "response.tool.shell_read",
        RelayMessage::ResponseToolShellInput { .. } => "response.tool.shell_input",
        RelayMessage::ResponseToolShellTerminate { .. } => "response.tool.shell_terminate",
        RelayMessage::ResponseVfsMaterialize { .. } => "response.vfs.materialize",
        RelayMessage::ResponseToolFileList { .. } => "response.tool.file_list",
        RelayMessage::ResponseToolSearch { .. } => "response.tool.search",
        RelayMessage::EventCapabilitiesChanged { .. } => "event.capabilities_changed",
        RelayMessage::EventSessionNotification { .. } => "event.session_notification",
        RelayMessage::EventRuntimeSessionStateChanged { .. } => {
            "event.runtime_session_state_changed"
        }
        RelayMessage::EventDiscoverOptionsPatch { .. } => "event.discover_options_patch",
        RelayMessage::CommandMcpProbeTransport { .. } => "command.mcp_probe_transport",
        RelayMessage::CommandMcpListTools { .. } => "command.mcp_list_tools",
        RelayMessage::CommandMcpCallTool { .. } => "command.mcp_call_tool",
        RelayMessage::CommandMcpClose { .. } => "command.mcp_close",
        RelayMessage::CommandExtensionActionInvoke { .. } => "command.extension_action_invoke",
        RelayMessage::CommandExtensionChannelInvoke { .. } => "command.extension_channel_invoke",
        RelayMessage::CommandExtensionBackendServiceInvoke { .. } => {
            "command.extension_backend_service_invoke"
        }
        RelayMessage::ResponseMcpProbeTransport { .. } => "response.mcp_probe_transport",
        RelayMessage::ResponseMcpListTools { .. } => "response.mcp_list_tools",
        RelayMessage::ResponseMcpCallTool { .. } => "response.mcp_call_tool",
        RelayMessage::ResponseMcpClose { .. } => "response.mcp_close",
        RelayMessage::ResponseExtensionActionInvoke { .. } => "response.extension_action_invoke",
        RelayMessage::ResponseExtensionChannelInvoke { .. } => "response.extension_channel_invoke",
        RelayMessage::ResponseExtensionBackendServiceInvoke { .. } => {
            "response.extension_backend_service_invoke"
        }
        RelayMessage::EventToolShellOutput { .. } => "event.tool.shell_output",
        RelayMessage::CommandTerminalSpawn { .. } => "command.terminal.spawn",
        RelayMessage::CommandTerminalInput { .. } => "command.terminal.input",
        RelayMessage::CommandTerminalResize { .. } => "command.terminal.resize",
        RelayMessage::CommandTerminalKill { .. } => "command.terminal.kill",
        RelayMessage::ResponseTerminalSpawn { .. } => "response.terminal.spawn",
        RelayMessage::ResponseTerminalInput { .. } => "response.terminal.input",
        RelayMessage::ResponseTerminalResize { .. } => "response.terminal.resize",
        RelayMessage::ResponseTerminalKill { .. } => "response.terminal.kill",
        RelayMessage::EventTerminalOutput { .. } => "event.terminal.output",
        RelayMessage::EventPtyTerminalStateChanged { .. } => "event.pty_terminal.state_changed",
        RelayMessage::Error { .. } => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::CloudRuntimeWirePlacementResolver;
    use agentdash_agent_runtime_contract::{RuntimeDriverGeneration, RuntimeServiceInstanceId};
    use agentdash_agent_runtime_wire::{
        RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireAck as DriverWireAck, RuntimeWireEnvelope,
        RuntimeWireFrame as DriverWireFrame, RuntimeWireFrameId,
    };
    use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorSource};
    use agentdash_integration_api::{AgentRuntimePlacementId, AgentServiceDefinitionId};
    use agentdash_integration_remote_runtime::{
        RuntimeWirePlacementEvent, RuntimeWirePlacementRequest, RuntimeWirePlacementResolver,
    };
    use agentdash_relay::{CommandBrowseDirectoryPayload, McpServerInfoRelay};

    fn connected_backend(backend_id: &str) -> ConnectedBackend {
        let (sender, _rx) = mpsc::unbounded_channel();
        connected_backend_with_sender(backend_id, sender)
    }

    fn connected_backend_with_sender(backend_id: &str, sender: BackendSender) -> ConnectedBackend {
        ConnectedBackend {
            backend_id: backend_id.to_string(),
            name: "测试后端".to_string(),
            version: "0.1.0".to_string(),
            capabilities: CapabilitiesPayload {
                executors: Vec::new(),
                supports_cancel: true,
                supports_discover_options: true,
                ..Default::default()
            },
            sender,
            connected_at: Utc::now(),
        }
    }
    #[tokio::test]
    async fn runtime_wire_route_reports_connection_epochs_and_does_not_replay_lost_work() {
        let registry = BackendRegistry::new();
        let (first_sender, mut first_outbound) = mpsc::unbounded_channel();
        registry
            .try_register(connected_backend_with_sender("local-a", first_sender))
            .await
            .expect("register first backend connection");
        let request = RuntimeWirePlacementRequest {
            host_id: "local-a".to_string(),
            transport_id: AgentRuntimePlacementId::new("desktop-runtime-wire")
                .expect("transport id"),
            definition_id: AgentServiceDefinitionId::new("enterprise.agent")
                .expect("definition id"),
            service_instance_id: RuntimeServiceInstanceId::new("enterprise-instance")
                .expect("instance id"),
            generation: RuntimeDriverGeneration(3),
        };
        let resolver = CloudRuntimeWirePlacementResolver::new(registry.clone(), 8);
        let resolve = tokio::spawn(async move { resolver.resolve(request).await });
        let open = first_outbound.recv().await.expect("Runtime Wire open");
        let RelayMessage::RuntimeWireOpen { id, payload: open } = open else {
            panic!("expected Runtime Wire open")
        };
        let profile = agentdash_integration_native_agent::native_runtime_profile();
        let profile_digest = agentdash_agent_runtime_host::profile_digest(&profile)
            .expect("transport profile digest");
        assert!(
            registry
                .handle_runtime_wire_message(
                    "local-a",
                    &RelayMessage::RuntimeWireOpenAck {
                        id,
                        payload: agentdash_relay::RuntimeRelayOpenAck {
                            stream_id: open.stream_id.clone(),
                            selected_protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                            accepted_after_sequence: 0,
                            transport_profile: profile.clone(),
                            transport_profile_digest: profile_digest.clone(),
                            max_in_flight_frames: 8,
                        },
                    },
                )
                .await
        );
        let placement = resolve
            .await
            .expect("resolver task")
            .expect("resolved placement");
        let envelope = RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: RuntimeWireFrameId(10),
            critical: true,
            frame: DriverWireFrame::Ack(DriverWireAck {
                through_frame_id: RuntimeWireFrameId(9),
            }),
        };
        placement
            .send(envelope.clone())
            .await
            .expect("send Runtime Wire envelope");
        let first_frame = first_outbound
            .recv()
            .await
            .expect("first Runtime Wire frame");
        let RelayMessage::RuntimeWireFrame {
            payload: first_frame,
            ..
        } = first_frame
        else {
            panic!("expected Runtime Wire frame")
        };

        registry.unregister("local-a").await;
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(1), placement.receive())
                .await
                .expect("disconnect receive must not hang")
                .expect("disconnect event"),
            RuntimeWirePlacementEvent::Disconnected { .. }
        ));
        let (second_sender, mut second_outbound) = mpsc::unbounded_channel();
        registry
            .try_register(connected_backend_with_sender("local-a", second_sender))
            .await
            .expect("register replacement backend connection");
        let reopen = second_outbound.recv().await.expect("Runtime Wire reopen");
        let RelayMessage::RuntimeWireOpen {
            id: reopen_id,
            payload: reopen,
        } = reopen
        else {
            panic!("expected Runtime Wire reopen")
        };
        assert_eq!(reopen.stream_id, open.stream_id);
        assert_eq!(reopen.provenance, open.provenance);
        assert!(
            registry
                .handle_runtime_wire_message(
                    "local-a",
                    &RelayMessage::RuntimeWireOpenAck {
                        id: reopen_id,
                        payload: agentdash_relay::RuntimeRelayOpenAck {
                            stream_id: reopen.stream_id,
                            selected_protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                            accepted_after_sequence: 0,
                            transport_profile: profile,
                            transport_profile_digest: profile_digest,
                            max_in_flight_frames: 8,
                        },
                    },
                )
                .await
        );
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(1), placement.receive())
                .await
                .expect("reconnect receive must not hang")
                .expect("reconnect event"),
            RuntimeWirePlacementEvent::Reconnected
        ));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), second_outbound.recv(),)
                .await
                .is_err(),
            "request abandoned as Lost must not replay after reconnect"
        );
        assert_eq!(first_frame.envelope, envelope);
    }

    fn capabilities_with_mcp_server(server_name: &str) -> CapabilitiesPayload {
        CapabilitiesPayload {
            executors: Vec::new(),
            supports_cancel: true,
            supports_discover_options: true,
            mcp_servers: vec![McpServerInfoRelay {
                name: server_name.to_string(),
                transport: "http".to_string(),
            }],
            ..Default::default()
        }
    }

    fn runtime_anchor(backend_id: &str) -> RuntimeBackendAnchor {
        RuntimeBackendAnchor::new(backend_id, RuntimeBackendAnchorSource::System)
            .expect("runtime backend anchor")
    }

    fn relay_mcp_context(session_id: &str, backend_id: &str) -> RelayMcpCallContext {
        RelayMcpCallContext {
            session_id: session_id.to_string(),
            turn_id: None,
            tool_call_id: None,
            backend_anchor: Some(runtime_anchor(backend_id)),
            vfs: None,
            vfs_access_policy: None,
            identity: None,
        }
    }

    fn relay_mcp_context_without_anchor(session_id: &str) -> RelayMcpCallContext {
        RelayMcpCallContext {
            session_id: session_id.to_string(),
            turn_id: None,
            tool_call_id: None,
            backend_anchor: None,
            vfs: None,
            vfs_access_policy: None,
            identity: None,
        }
    }

    fn browse_command(prefix: &str) -> RelayMessage {
        RelayMessage::CommandBrowseDirectory {
            id: RelayMessage::new_id(prefix),
            payload: CommandBrowseDirectoryPayload { path: None },
        }
    }

    #[tokio::test]
    async fn try_register_rejects_duplicate_backend_id() {
        let registry = BackendRegistry::new();
        registry
            .try_register(connected_backend("local-a"))
            .await
            .expect("首次注册应成功");

        let err = registry
            .try_register(connected_backend("local-a"))
            .await
            .expect_err("重复 backend_id 应被拒绝");

        assert_eq!(
            err,
            RegisterBackendError::AlreadyOnline {
                backend_id: "local-a".to_string()
            }
        );
    }

    #[tokio::test]
    async fn send_command_reports_offline_backend() {
        let registry = BackendRegistry::new();

        let err = registry
            .send_command("missing", browse_command("offline"))
            .await
            .expect_err("offline backend should be classified");

        assert_eq!(
            err,
            BackendCommandError::Offline {
                backend_id: "missing".to_string()
            }
        );
    }

    #[tokio::test]
    async fn send_command_reports_send_failed_when_receiver_is_gone() {
        let registry = BackendRegistry::new();
        registry
            .try_register(connected_backend("local-a"))
            .await
            .expect("backend should register");

        let err = registry
            .send_command("local-a", browse_command("send-failed"))
            .await
            .expect_err("dropped receiver should fail send");

        assert_eq!(
            err,
            BackendCommandError::SendFailed {
                backend_id: "local-a".to_string()
            }
        );
    }

    #[tokio::test]
    async fn send_command_with_timeout_reports_timeout() {
        let registry = BackendRegistry::new();
        let (sender, _rx) = mpsc::unbounded_channel();
        registry
            .try_register(connected_backend_with_sender("local-a", sender))
            .await
            .expect("backend should register");

        let err = registry
            .send_command_with_timeout(
                "local-a",
                browse_command("timeout"),
                std::time::Duration::from_millis(1),
            )
            .await
            .expect_err("missing response should timeout");

        assert_eq!(
            err,
            BackendCommandError::Timeout {
                backend_id: "local-a".to_string()
            }
        );
    }

    #[tokio::test]
    async fn send_command_reports_response_dropped() {
        let registry = BackendRegistry::new();
        let (sender, mut rx) = mpsc::unbounded_channel();
        registry
            .try_register(connected_backend_with_sender("local-a", sender))
            .await
            .expect("backend should register");

        let command = browse_command("response-dropped");
        let msg_id = command.id().to_string();
        let pending = {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { registry.send_command("local-a", command).await })
        };

        rx.recv().await.expect("command should be sent");
        registry.drop_pending_for_test(&msg_id).await;

        let err = pending
            .await
            .expect("join should succeed")
            .expect_err("dropped pending sender should be classified");
        assert_eq!(
            err,
            BackendCommandError::ResponseDropped {
                backend_id: "local-a".to_string()
            }
        );
    }

    #[tokio::test]
    async fn unregister_drops_pending_requests_for_that_backend() {
        let registry = BackendRegistry::new();
        let (sender, mut rx) = mpsc::unbounded_channel();
        registry
            .try_register(connected_backend_with_sender("local-a", sender))
            .await
            .expect("backend should register");

        let command = browse_command("disconnect-drops-pending");
        let pending = {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { registry.send_command("local-a", command).await })
        };

        rx.recv().await.expect("command should be sent");
        registry.unregister("local-a").await;

        let err = tokio::time::timeout(std::time::Duration::from_millis(100), pending)
            .await
            .expect("pending command should not wait for command timeout")
            .expect("join should succeed")
            .expect_err("dropped pending sender should be classified");
        assert_eq!(
            err,
            BackendCommandError::ResponseDropped {
                backend_id: "local-a".to_string()
            }
        );
    }
    #[tokio::test]
    async fn relay_mcp_backend_resolution_prefers_anchor_over_catalog() {
        let registry = BackendRegistry::new();
        registry
            .try_register(connected_backend("local-a"))
            .await
            .expect("backend should register");
        let mut backend_b = connected_backend("local-b");
        backend_b.capabilities = capabilities_with_mcp_server("declared-tools");
        registry
            .try_register(backend_b)
            .await
            .expect("backend should register");
        let backend_id = registry
            .resolve_backend_for_relay_mcp(
                "declared-tools",
                Some(&relay_mcp_context("session-a", "local-b")),
            )
            .await
            .expect("anchor backend should resolve");

        assert_eq!(backend_id, "local-b");
    }

    #[tokio::test]
    async fn relay_mcp_backend_resolution_without_anchor_does_not_fallback_to_catalog() {
        let registry = BackendRegistry::new();
        registry
            .try_register(connected_backend("local-a"))
            .await
            .expect("backend should register");
        let mut backend_b = connected_backend("local-b");
        backend_b.capabilities = capabilities_with_mcp_server("declared-tools");
        registry
            .try_register(backend_b)
            .await
            .expect("backend should register");

        let error = registry
            .resolve_backend_for_relay_mcp(
                "declared-tools",
                Some(&relay_mcp_context_without_anchor("session-a")),
            )
            .await
            .expect_err("missing anchor must not fallback");

        assert!(matches!(
            error,
            RelayMcpBackendResolutionError::MissingAnchor(
                RuntimeBackendAnchorError::Missing { .. }
            )
        ));
    }

    #[tokio::test]
    async fn relay_mcp_backend_resolution_without_context_does_not_fallback_to_any_online_backend()
    {
        let registry = BackendRegistry::new();
        registry
            .try_register(connected_backend("local-a"))
            .await
            .expect("backend should register");
        let mut backend_b = connected_backend("local-b");
        backend_b.capabilities = capabilities_with_mcp_server("declared-tools");
        registry
            .try_register(backend_b)
            .await
            .expect("backend should register");

        let error = registry
            .resolve_backend_for_relay_mcp("declared-tools", None)
            .await
            .expect_err("missing context must not fallback");

        assert_eq!(
            error,
            RelayMcpBackendResolutionError::MissingContext {
                server: "declared-tools".to_string()
            }
        );
    }

    #[tokio::test]
    async fn relay_mcp_backend_resolution_rejects_offline_anchor_without_fallback() {
        let registry = BackendRegistry::new();
        let mut backend_b = connected_backend("local-b");
        backend_b.capabilities = capabilities_with_mcp_server("declared-tools");
        registry
            .try_register(backend_b)
            .await
            .expect("backend should register");

        let error = registry
            .resolve_backend_for_relay_mcp(
                "declared-tools",
                Some(&relay_mcp_context("session-a", "local-a")),
            )
            .await
            .expect_err("offline anchor must not fallback");

        assert_eq!(
            error,
            RelayMcpBackendResolutionError::BackendOffline {
                backend_id: "local-a".to_string()
            }
        );
    }
}
