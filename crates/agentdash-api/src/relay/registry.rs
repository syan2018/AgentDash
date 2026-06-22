use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{RwLock, mpsc, oneshot};

use agentdash_application_ports::backend_transport::RemoteExecutorInfo;
use agentdash_application_ports::backend_transport::{
    RelaySessionEvent, RelaySessionRoute, RelaySessionRouteInfo, RelayTerminalKind,
};
use agentdash_relay::{CapabilitiesPayload, RelayMessage};
use agentdash_spi::RelayMcpCallContext;

pub type BackendSender = mpsc::UnboundedSender<RelayMessage>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterBackendError {
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
    executor_snapshot: std::sync::RwLock<Vec<RemoteExecutorInfo>>,
    /// 等待本机响应的挂起请求（msg_id → pending request）
    pending: RwLock<HashMap<String, PendingRequest>>,
    /// per-session relay 通知接收端（由 RelayAgentConnector 注册，WebSocket handler 投递）
    session_sinks: std::sync::RwLock<HashMap<String, RelaySessionRoute>>,
}

struct PendingRequest {
    backend_id: String,
    tx: oneshot::Sender<RelayMessage>,
}

impl BackendRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            backends: RwLock::new(HashMap::new()),
            executor_snapshot: std::sync::RwLock::new(Vec::new()),
            pending: RwLock::new(HashMap::new()),
            session_sinks: std::sync::RwLock::new(HashMap::new()),
        })
    }

    /// 向 relay session sink 投递 notification（供 WebSocket handler 调用）。
    /// 返回 true 表示投递成功（有已注册的 sink）。
    pub fn feed_session_event(&self, session_id: &str, event: RelaySessionEvent) -> bool {
        let sinks = self.session_sinks.read().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = sinks.get(session_id) {
            tx.tx.send(event).is_ok()
        } else {
            false
        }
    }

    pub fn feed_backend_terminal(
        &self,
        backend_id: &str,
        kind: RelayTerminalKind,
        message: Option<String>,
    ) -> usize {
        let sinks = self.session_sinks.read().unwrap_or_else(|e| e.into_inner());
        sinks
            .values()
            .filter(|route| route.backend_id == backend_id)
            .filter(|route| {
                route
                    .tx
                    .send(RelaySessionEvent::Terminal {
                        kind,
                        message: message.clone(),
                    })
                    .is_ok()
            })
            .count()
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
        self.rebuild_executor_snapshot(&backends);
        tracing::info!(backend_id = %id, "本机后端已注册");
        Ok(())
    }

    pub async fn unregister(&self, backend_id: &str) {
        {
            let mut backends = self.backends.write().await;
            backends.remove(backend_id);
            self.rebuild_executor_snapshot(&backends);
        }
        self.pending
            .write()
            .await
            .retain(|_, pending| pending.backend_id != backend_id);
        self.session_sinks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, route| route.backend_id != backend_id);
        tracing::info!(backend_id = %backend_id, "本机后端已断开");
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

    /// 获取任意一个在线 backend ID（probe 场景使用，不关心选择哪个）
    pub async fn find_any_online_backend(&self) -> Option<String> {
        self.backends.read().await.keys().next().cloned()
    }

    /// 注册 per-session 通知接收端。
    pub fn register_session_sink(&self, route: RelaySessionRoute) {
        self.session_sinks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(route.session_id.clone(), route);
    }

    /// 注销 per-session 通知接收端。
    pub fn unregister_session_sink(&self, session_id: &str) {
        self.session_sinks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id);
    }

    /// 检查指定 session 是否有已注册的通知接收端。
    pub fn has_session_sink(&self, session_id: &str) -> bool {
        self.session_sinks
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(session_id)
    }

    pub fn session_route(&self, session_id: &str) -> Option<RelaySessionRouteInfo> {
        self.session_sinks
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(session_id)
            .map(|route| RelaySessionRouteInfo {
                session_id: route.session_id.clone(),
                backend_id: route.backend_id.clone(),
                lease_id: route.lease_id,
                turn_id: route.turn_id.clone(),
            })
    }

    // ── MCP Relay 支持 ──

    /// 更新指定 backend 的能力信息（含 MCP server 列表）
    pub async fn update_capabilities(&self, backend_id: &str, capabilities: CapabilitiesPayload) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.get_mut(backend_id) {
            backend.capabilities = capabilities;
            self.rebuild_executor_snapshot(&backends);
            tracing::info!(backend_id = %backend_id, "后端能力已更新");
        }
    }

    pub fn list_online_executors_snapshot(&self) -> Vec<RemoteExecutorInfo> {
        self.executor_snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn rebuild_executor_snapshot(&self, backends: &HashMap<String, ConnectedBackend>) {
        let mut snapshot = Vec::new();
        for backend in backends.values() {
            for executor in &backend.capabilities.executors {
                snapshot.push(RemoteExecutorInfo {
                    backend_id: backend.backend_id.clone(),
                    executor_id: executor.id.clone(),
                    executor_name: executor.name.clone(),
                    variants: executor.variants.clone(),
                    available: executor.available,
                });
            }
        }
        *self
            .executor_snapshot
            .write()
            .unwrap_or_else(|e| e.into_inner()) = snapshot;
    }

    /// 查找提供指定 MCP server 的在线 backend
    pub async fn find_backend_for_mcp_server(&self, server_name: &str) -> Option<String> {
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

    /// 解析 relay MCP 应投递到的本机 backend。
    ///
    /// Project MCP Preset / AgentFrame surface 是 MCP server 的事实源；
    /// backend 上报的 MCP catalog 仅作为 setup/probe 期的本机预配置 catalog。运行中的
    /// session 必须使用已注册的 relay route，route 缺失或 route backend 离线都不能
    /// fallback 到 VFS、catalog 或其它在线 backend。
    pub async fn resolve_backend_for_relay_mcp(
        &self,
        server_name: &str,
        context: Option<&RelayMcpCallContext>,
    ) -> Option<String> {
        if let Some(context) = context {
            let route = match self.session_route(&context.session_id) {
                Some(route) => route,
                None => {
                    tracing::warn!(
                        session_id = %context.session_id,
                        server = %server_name,
                        "relay MCP session context 缺少 session route"
                    );
                    return None;
                }
            };

            if self.is_online(&route.backend_id).await {
                return Some(route.backend_id);
            }

            tracing::warn!(
                    session_id = %context.session_id,
                    backend_id = %route.backend_id,
                    server = %server_name,
                    "relay MCP session route 指向的 backend 已离线"
            );
            return None;
        }

        if let Some(backend_id) = self.find_backend_for_mcp_server(server_name).await {
            return Some(backend_id);
        }

        self.find_any_online_backend().await
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

        let sender = {
            let backends = self.backends.read().await;
            let backend = backends
                .get(backend_id)
                .ok_or_else(|| BackendCommandError::Offline {
                    backend_id: backend_id.to_string(),
                })?;
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
            return Err(BackendCommandError::SendFailed {
                backend_id: backend_id.to_string(),
            });
        }

        let resp = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.pending.write().await.remove(&msg_id);
                return Err(BackendCommandError::ResponseDropped {
                    backend_id: backend_id.to_string(),
                });
            }
            Err(_) => {
                self.pending.write().await.remove(&msg_id);
                return Err(BackendCommandError::Timeout {
                    backend_id: backend_id.to_string(),
                });
            }
        };

        Ok(resp)
    }

    #[cfg(test)]
    async fn drop_pending_for_test(&self, msg_id: &str) {
        self.pending.write().await.remove(msg_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_relay::{AgentInfoRelay, CommandBrowseDirectoryPayload, McpServerInfoRelay};

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
                mcp_servers: Vec::new(),
            },
            sender,
            connected_at: Utc::now(),
        }
    }

    fn capabilities_with_executor(executor_id: &str) -> CapabilitiesPayload {
        CapabilitiesPayload {
            executors: vec![AgentInfoRelay {
                id: executor_id.to_string(),
                name: format!("{executor_id} executor"),
                variants: vec!["default".to_string()],
                available: true,
            }],
            supports_cancel: true,
            supports_discover_options: true,
            mcp_servers: Vec::new(),
        }
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
        }
    }

    fn relay_mcp_context(session_id: &str) -> RelayMcpCallContext {
        RelayMcpCallContext {
            session_id: session_id.to_string(),
            turn_id: None,
            tool_call_id: None,
            vfs: None,
            identity: None,
        }
    }

    fn relay_mcp_context_with_default_backend(
        session_id: &str,
        backend_id: &str,
    ) -> RelayMcpCallContext {
        RelayMcpCallContext {
            session_id: session_id.to_string(),
            turn_id: None,
            tool_call_id: None,
            vfs: Some(agentdash_spi::Vfs {
                mounts: vec![agentdash_spi::Mount {
                    id: "workspace".to_string(),
                    provider: "relay_fs".to_string(),
                    backend_id: backend_id.to_string(),
                    root_ref: "F:/workspace".to_string(),
                    capabilities: Vec::new(),
                    default_write: true,
                    display_name: "Workspace".to_string(),
                    metadata: serde_json::Value::Null,
                }],
                default_mount_id: Some("workspace".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            }),
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
    async fn executor_snapshot_tracks_register_update_and_unregister() {
        let registry = BackendRegistry::new();
        let mut backend = connected_backend("local-a");
        backend.capabilities = capabilities_with_executor("executor-a");
        registry.try_register(backend).await.expect("register");

        let initial = registry.list_online_executors_snapshot();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].backend_id, "local-a");
        assert_eq!(initial[0].executor_id, "executor-a");

        registry
            .update_capabilities("local-a", capabilities_with_executor("executor-b"))
            .await;
        let updated = registry.list_online_executors_snapshot();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].executor_id, "executor-b");

        registry.unregister("local-a").await;
        assert!(registry.list_online_executors_snapshot().is_empty());
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
    async fn unregister_drops_session_routes_for_that_backend_only() {
        let registry = BackendRegistry::new();
        let (tx_a, _rx_a) = mpsc::unbounded_channel();
        let (tx_b, _rx_b) = mpsc::unbounded_channel();
        let lease_a = uuid::Uuid::new_v4();
        let lease_b = uuid::Uuid::new_v4();
        registry.register_session_sink(RelaySessionRoute {
            session_id: "session-a".to_string(),
            backend_id: "local-a".to_string(),
            lease_id: lease_a,
            turn_id: "turn-a".to_string(),
            tx: tx_a,
        });
        registry.register_session_sink(RelaySessionRoute {
            session_id: "session-b".to_string(),
            backend_id: "local-b".to_string(),
            lease_id: lease_b,
            turn_id: "turn-b".to_string(),
            tx: tx_b,
        });

        registry.unregister("local-a").await;

        assert!(registry.session_route("session-a").is_none());
        assert_eq!(
            registry.session_route("session-b"),
            Some(RelaySessionRouteInfo {
                session_id: "session-b".to_string(),
                backend_id: "local-b".to_string(),
                lease_id: lease_b,
                turn_id: "turn-b".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn feed_backend_terminal_notifies_matching_session_routes_without_removing_them() {
        let registry = BackendRegistry::new();
        let (tx_a, mut rx_a) = mpsc::unbounded_channel();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel();
        let lease_a = uuid::Uuid::new_v4();
        let lease_b = uuid::Uuid::new_v4();
        registry.register_session_sink(RelaySessionRoute {
            session_id: "session-a".to_string(),
            backend_id: "local-a".to_string(),
            lease_id: lease_a,
            turn_id: "turn-a".to_string(),
            tx: tx_a,
        });
        registry.register_session_sink(RelaySessionRoute {
            session_id: "session-b".to_string(),
            backend_id: "local-b".to_string(),
            lease_id: lease_b,
            turn_id: "turn-b".to_string(),
            tx: tx_b,
        });

        let count = registry.feed_backend_terminal(
            "local-a",
            RelayTerminalKind::Lost,
            Some("backend disconnected".to_string()),
        );

        assert_eq!(count, 1);
        let event = rx_a
            .recv()
            .await
            .expect("matching route should receive terminal");
        match event {
            RelaySessionEvent::Terminal { kind, message } => {
                assert!(matches!(kind, RelayTerminalKind::Lost));
                assert_eq!(message.as_deref(), Some("backend disconnected"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(rx_b.try_recv().is_err());
        assert!(registry.session_route("session-a").is_some());
        assert!(registry.session_route("session-b").is_some());
    }

    #[tokio::test]
    async fn relay_mcp_backend_resolution_uses_session_route_without_mcp_catalog() {
        let registry = BackendRegistry::new();
        registry
            .try_register(connected_backend("local-a"))
            .await
            .expect("backend should register");
        let (tx, _rx) = mpsc::unbounded_channel();
        let lease_id = uuid::Uuid::new_v4();
        registry.register_session_sink(RelaySessionRoute {
            session_id: "session-a".to_string(),
            backend_id: "local-a".to_string(),
            lease_id,
            turn_id: "turn-a".to_string(),
            tx,
        });

        let backend_id = registry
            .resolve_backend_for_relay_mcp(
                "project-relay-tools",
                Some(&relay_mcp_context("session-a")),
            )
            .await;

        assert_eq!(backend_id.as_deref(), Some("local-a"));
    }

    #[tokio::test]
    async fn relay_mcp_backend_resolution_with_session_context_requires_session_route() {
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
                Some(&relay_mcp_context_with_default_backend(
                    "session-a",
                    "local-a",
                )),
            )
            .await;

        assert_eq!(backend_id, None);
    }

    #[tokio::test]
    async fn relay_mcp_backend_resolution_with_session_context_rejects_offline_route() {
        let registry = BackendRegistry::new();
        let mut backend_b = connected_backend("local-b");
        backend_b.capabilities = capabilities_with_mcp_server("declared-tools");
        registry
            .try_register(backend_b)
            .await
            .expect("backend should register");
        let (tx, _rx) = mpsc::unbounded_channel();
        let lease_id = uuid::Uuid::new_v4();
        registry.register_session_sink(RelaySessionRoute {
            session_id: "session-a".to_string(),
            backend_id: "local-a".to_string(),
            lease_id,
            turn_id: "turn-a".to_string(),
            tx,
        });

        let backend_id = registry
            .resolve_backend_for_relay_mcp("declared-tools", Some(&relay_mcp_context("session-a")))
            .await;

        assert_eq!(backend_id, None);
    }

    #[tokio::test]
    async fn relay_mcp_backend_resolution_falls_back_to_advertised_catalog() {
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
            .resolve_backend_for_relay_mcp("declared-tools", None)
            .await;

        assert_eq!(backend_id.as_deref(), Some("local-b"));
    }
}
