use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{RwLock, mpsc, oneshot};

use agentdash_relay::{CapabilitiesPayload, RelayMessage};

pub type BackendSender = mpsc::UnboundedSender<RelayMessage>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterBackendError {
    AlreadyOnline { backend_id: String },
}

/// 已连接的本机后端
pub struct ConnectedBackend {
    pub backend_id: String,
    pub name: String,
    pub version: String,
    pub capabilities: CapabilitiesPayload,
    pub accessible_roots: Vec<String>,
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
    pub accessible_roots: Vec<String>,
    pub connected_at: DateTime<Utc>,
}

/// 中继后端注册表 — 跟踪所有通过 WebSocket 连接的本机后端
pub struct BackendRegistry {
    backends: RwLock<HashMap<String, ConnectedBackend>>,
    /// 等待本机响应的挂起请求（msg_id → oneshot sender）
    pending: RwLock<HashMap<String, oneshot::Sender<RelayMessage>>>,
    /// per-session relay 通知接收端（由 RelayAgentConnector 注册，WebSocket handler 投递）
    session_sinks: std::sync::RwLock<
        HashMap<
            String,
            mpsc::UnboundedSender<agentdash_application::backend_transport::RelaySessionEvent>,
        >,
    >,
}

impl BackendRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            backends: RwLock::new(HashMap::new()),
            pending: RwLock::new(HashMap::new()),
            session_sinks: std::sync::RwLock::new(HashMap::new()),
        })
    }

    /// 向 relay session sink 投递 notification（供 WebSocket handler 调用）。
    /// 返回 true 表示投递成功（有已注册的 sink）。
    pub fn feed_session_event(
        &self,
        session_id: &str,
        event: agentdash_application::backend_transport::RelaySessionEvent,
    ) -> bool {
        let sinks = self.session_sinks.read().unwrap();
        if let Some(tx) = sinks.get(session_id) {
            tx.send(event).is_ok()
        } else {
            false
        }
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
        tracing::info!(backend_id = %id, "本机后端已注册");
        Ok(())
    }

    pub async fn unregister(&self, backend_id: &str) {
        self.backends.write().await.remove(backend_id);
        self.pending.write().await.retain(|_, _| true); // pending requests will fail naturally when sender drops
        tracing::info!(backend_id = %backend_id, "本机后端已断开");
    }

    /// 向指定后端发送命令并等待响应
    pub async fn send_command(
        &self,
        backend_id: &str,
        msg: RelayMessage,
    ) -> Result<RelayMessage, anyhow::Error> {
        let msg_id = msg.id().to_string();

        let sender = {
            let backends = self.backends.read().await;
            let backend = backends
                .get(backend_id)
                .ok_or_else(|| anyhow::anyhow!("Backend 不在线: {backend_id}"))?;
            backend.sender.clone()
        };

        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(msg_id.clone(), tx);

        if sender.send(msg).is_err() {
            self.pending.write().await.remove(&msg_id);
            anyhow::bail!("发送至本机后端失败");
        }

        let resp = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| anyhow::anyhow!("命令超时"))??;

        Ok(resp)
    }

    /// 匹配并分发一条响应消息到等待方
    pub async fn resolve_response(&self, msg: &RelayMessage) -> bool {
        let id = msg.id().to_string();
        if let Some(tx) = self.pending.write().await.remove(&id) {
            let _ = tx.send(msg.clone());
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
                accessible_roots: b.accessible_roots.clone(),
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

    /// 注册 per-session 通知接收端。
    pub fn register_session_sink(
        &self,
        session_id: &str,
        tx: mpsc::UnboundedSender<agentdash_application::backend_transport::RelaySessionEvent>,
    ) {
        self.session_sinks
            .write()
            .unwrap()
            .insert(session_id.to_string(), tx);
    }

    /// 注销 per-session 通知接收端。
    pub fn unregister_session_sink(&self, session_id: &str) {
        self.session_sinks.write().unwrap().remove(session_id);
    }

    /// 检查指定 session 是否有已注册的通知接收端。
    pub fn has_session_sink(&self, session_id: &str) -> bool {
        self.session_sinks.read().unwrap().contains_key(session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected_backend(backend_id: &str) -> ConnectedBackend {
        let (sender, _rx) = mpsc::unbounded_channel();
        ConnectedBackend {
            backend_id: backend_id.to_string(),
            name: "测试后端".to_string(),
            version: "0.1.0".to_string(),
            capabilities: CapabilitiesPayload {
                executors: Vec::new(),
                supports_cancel: true,
                supports_discover_options: true,
            },
            accessible_roots: Vec::new(),
            sender,
            connected_at: Utc::now(),
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
}
