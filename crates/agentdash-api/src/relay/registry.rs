use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{RwLock, mpsc, oneshot};

use agentdash_relay::{CapabilitiesPayload, RelayMessage};

pub type BackendSender = mpsc::UnboundedSender<RelayMessage>;

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
}

impl BackendRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            backends: RwLock::new(HashMap::new()),
            pending: RwLock::new(HashMap::new()),
        })
    }

    pub async fn register(&self, backend: ConnectedBackend) {
        let id = backend.backend_id.clone();
        self.backends.write().await.insert(id.clone(), backend);
        tracing::info!(backend_id = %id, "本机后端已注册");
    }

    pub async fn unregister(&self, backend_id: &str) {
        self.backends.write().await.remove(backend_id);
        self.pending
            .write()
            .await
            .retain(|_, _| true); // pending requests will fail naturally when sender drops
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

    /// 查找管理了指定路径的在线后端
    pub async fn find_backend_for_path(&self, path: &str) -> Option<String> {
        let backends = self.backends.read().await;
        for b in backends.values() {
            for root in &b.accessible_roots {
                if path.starts_with(root.as_str()) {
                    return Some(b.backend_id.clone());
                }
            }
        }
        None
    }
}
