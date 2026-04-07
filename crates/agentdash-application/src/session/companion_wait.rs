use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Debug)]
struct CompanionWaitEntry {
    session_id: String,
    turn_id: String,
    request_type: Option<String>,
    sender: tokio::sync::oneshot::Sender<serde_json::Value>,
}

#[derive(Debug)]
pub struct CompanionWaitResolution {
    pub payload: serde_json::Value,
    pub turn_id: String,
    pub request_type: Option<String>,
}

/// companion_request(wait=true) 的等待 registry。
/// 工具 execute 持有 receiver 不返回，respond 侧找到 sender 发回去。
/// 纯执行层概念，不涉及 hook 通道。
#[derive(Clone, Default)]
pub struct CompanionWaitRegistry {
    inner: Arc<Mutex<HashMap<String, CompanionWaitEntry>>>,
}

impl CompanionWaitRegistry {
    pub async fn register(
        &self,
        session_id: &str,
        request_id: &str,
        turn_id: &str,
        request_type: Option<String>,
    ) -> tokio::sync::oneshot::Receiver<serde_json::Value> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.inner.lock().await.insert(
            request_id.to_string(),
            CompanionWaitEntry {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                request_type,
                sender: tx,
            },
        );
        rx
    }

    pub async fn resolve(
        &self,
        session_id: &str,
        request_id: &str,
        payload: serde_json::Value,
    ) -> Option<CompanionWaitResolution> {
        let entry = self.inner.lock().await.remove(request_id)?;
        if entry.session_id != session_id {
            self.inner
                .lock()
                .await
                .insert(request_id.to_string(), entry);
            return None;
        }
        let _ = entry.sender.send(payload.clone());
        Some(CompanionWaitResolution {
            payload,
            turn_id: entry.turn_id,
            request_type: entry.request_type,
        })
    }

    pub async fn remove(&self, request_id: &str) {
        self.inner.lock().await.remove(request_id);
    }
}
