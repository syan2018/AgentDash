use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast};

use super::hub_support::{EPHEMERAL_BUFFER_CAP, SessionRuntime, build_session_runtime};
use super::persistence::PersistedSessionEvent;
use agentdash_spi::hooks::SharedHookRuntime;

#[derive(Clone)]
pub(super) struct SessionRuntimeRegistry {
    runtimes: Arc<Mutex<HashMap<String, SessionRuntime>>>,
}

impl SessionRuntimeRegistry {
    pub fn new(runtimes: Arc<Mutex<HashMap<String, SessionRuntime>>>) -> Self {
        Self { runtimes }
    }

    pub fn shared_runtimes(&self) -> Arc<Mutex<HashMap<String, SessionRuntime>>> {
        self.runtimes.clone()
    }

    pub async fn with_runtime<R>(
        &self,
        session_id: &str,
        f: impl FnOnce(Option<&SessionRuntime>) -> R,
    ) -> R {
        let runtimes = self.runtimes.lock().await;
        f(runtimes.get(session_id))
    }

    pub async fn with_runtime_mut<R>(
        &self,
        session_id: &str,
        f: impl FnOnce(Option<&mut SessionRuntime>) -> R,
    ) -> R {
        let mut runtimes = self.runtimes.lock().await;
        f(runtimes.get_mut(session_id))
    }

    pub async fn subscribe(&self, session_id: &str) -> broadcast::Receiver<PersistedSessionEvent> {
        let mut runtimes = self.runtimes.lock().await;
        let runtime = runtimes.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            build_session_runtime(tx)
        });
        runtime.tx.subscribe()
    }

    pub async fn remove(&self, session_id: &str) {
        let mut runtimes = self.runtimes.lock().await;
        runtimes.remove(session_id);
    }

    pub async fn has_runtime_entry(&self, session_id: &str) -> bool {
        let runtimes = self.runtimes.lock().await;
        runtimes.contains_key(session_id)
    }

    pub async fn has_active_turn(&self, session_id: &str) -> bool {
        let runtimes = self.runtimes.lock().await;
        runtimes
            .get(session_id)
            .is_some_and(|runtime| runtime.turn_state.active_turn().is_some())
    }

    pub async fn running_set(&self, session_ids: &[String]) -> HashSet<String> {
        let runtimes = self.runtimes.lock().await;
        session_ids
            .iter()
            .filter(|id| runtimes.get(id.as_str()).is_some_and(|r| r.is_running()))
            .cloned()
            .collect()
    }

    pub async fn execution_state_snapshot(&self, session_id: &str) -> (bool, Option<String>, bool) {
        let runtimes = self.runtimes.lock().await;
        runtimes
            .get(session_id)
            .map(|runtime| {
                (
                    runtime.is_running(),
                    runtime
                        .turn_state
                        .active_turn()
                        .map(|turn| turn.turn_id.clone()),
                    runtime.turn_state.is_cancelling(),
                )
            })
            .unwrap_or((false, None, false))
    }

    /// Return the cached hook runtime for a delivery RuntimeSession adapter.
    ///
    /// The returned runtime's `control_target()` remains the business owner and
    /// must be validated by target-first service methods before use in business paths.
    pub async fn hook_runtime_target_cache(&self, session_id: &str) -> Option<SharedHookRuntime> {
        let runtimes = self.runtimes.lock().await;
        runtimes
            .get(session_id)
            .and_then(|runtime| runtime.hook_runtime_target_cache.clone())
    }

    pub async fn set_or_replace_hook_runtime_target_cache(
        &self,
        session_id: &str,
        hook_runtime: SharedHookRuntime,
    ) -> SharedHookRuntime {
        let mut runtimes = self.runtimes.lock().await;
        let runtime = runtimes.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            build_session_runtime(tx)
        });
        runtime.hook_runtime_target_cache = Some(hook_runtime.clone());
        hook_runtime
    }

    pub async fn touch_and_sender(
        &self,
        session_id: &str,
    ) -> broadcast::Sender<PersistedSessionEvent> {
        let mut runtimes = self.runtimes.lock().await;
        let runtime = runtimes.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            build_session_runtime(tx)
        });
        runtime.last_activity_at = chrono::Utc::now().timestamp_millis();
        runtime.tx.clone()
    }

    /// 将一条 ephemeral 事件分配单调 `ephemeral_seq`（写入其 `event_seq` 字段）并推入
    /// per-session buffer（超出 cap 则 evict 队首），返回带 seq 的事件供调用方 broadcast。
    /// seq 分配 + 入 buffer 在同一把锁内完成，天然原子、无需额外同步。
    pub async fn push_ephemeral(
        &self,
        session_id: &str,
        mut event: PersistedSessionEvent,
    ) -> PersistedSessionEvent {
        let mut runtimes = self.runtimes.lock().await;
        let runtime = runtimes.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            build_session_runtime(tx)
        });
        runtime.ephemeral_seq += 1;
        event.event_seq = runtime.ephemeral_seq;
        runtime.ephemeral_buffer.push_back(event.clone());
        while runtime.ephemeral_buffer.len() > EPHEMERAL_BUFFER_CAP {
            runtime.ephemeral_buffer.pop_front();
        }
        event
    }

    /// 取 per-session ephemeral buffer 的快照（无 runtime 则空）。
    pub async fn snapshot_ephemeral(&self, session_id: &str) -> Vec<PersistedSessionEvent> {
        let runtimes = self.runtimes.lock().await;
        runtimes
            .get(session_id)
            .map(|runtime| runtime.ephemeral_buffer.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 从 ephemeral buffer 移除指定 `item_id` 的助手文本/ reasoning delta。
    /// 终态助手消息（ItemCompleted AgentMessage/Reasoning）落 durable 后调用：
    /// 防止 reconnect 时 durable backlog 已 SET 全文、随后 ephemeral 快照又补发 in-flight delta，
    /// 导致前端把 delta append 到已 final 的气泡上（脏化）。
    /// 只移除该 item_id 的 text/reasoning delta，保留其余 ephemeral 条目。
    pub async fn prune_ephemeral_by_item_id(&self, session_id: &str, item_id: &str) {
        use agentdash_agent_protocol::BackboneEvent;
        let mut runtimes = self.runtimes.lock().await;
        if let Some(runtime) = runtimes.get_mut(session_id) {
            runtime.ephemeral_buffer.retain(|event| {
                let matches_item = match &event.notification.event {
                    BackboneEvent::AgentMessageDelta(delta) => delta.item_id == item_id,
                    BackboneEvent::ReasoningTextDelta(delta) => delta.item_id == item_id,
                    BackboneEvent::ReasoningSummaryDelta(delta) => delta.item_id == item_id,
                    _ => false,
                };
                !matches_item
            });
        }
    }

    /// 清空 per-session ephemeral buffer（turn 收尾后调用）。
    /// 保留 `ephemeral_seq` 计数器单调，不重置，避免跨 turn 前端误去重。
    pub async fn clear_ephemeral(&self, session_id: &str) {
        let mut runtimes = self.runtimes.lock().await;
        if let Some(runtime) = runtimes.get_mut(session_id) {
            runtime.ephemeral_buffer.clear();
        }
    }

    pub async fn find_stalled_active_turns(&self, stall_timeout_ms: u64) -> Vec<String> {
        let now = chrono::Utc::now().timestamp_millis();
        let threshold = stall_timeout_ms as i64;
        let runtimes = self.runtimes.lock().await;
        runtimes
            .iter()
            .filter(|(_, runtime)| {
                runtime.is_running() && (now - runtime.last_activity_at) > threshold
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::{
        BackboneEnvelope, BackboneEvent, SourceInfo, codex_app_server_protocol as codex,
    };

    fn registry() -> SessionRuntimeRegistry {
        SessionRuntimeRegistry::new(Arc::new(Mutex::new(HashMap::new())))
    }

    fn delta_event(session_id: &str, text: &str) -> PersistedSessionEvent {
        let envelope = BackboneEnvelope::new(
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                delta: text.to_string(),
                thread_id: session_id.to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "turn-1:assistant:0".to_string(),
            }),
            session_id,
            SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "local_executor".to_string(),
                executor_id: None,
            },
        );
        PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq: 0,
            occurred_at_ms: 0,
            committed_at_ms: 0,
            session_update_type: "agent_message_delta".to_string(),
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(1),
            tool_call_id: None,
            ephemeral: true,
            notification: envelope,
        }
    }

    #[tokio::test]
    async fn push_ephemeral_assigns_monotonic_seq_and_buffers() {
        let registry = registry();
        let session_id = "sess-eph";

        let first = registry
            .push_ephemeral(session_id, delta_event(session_id, "a"))
            .await;
        let second = registry
            .push_ephemeral(session_id, delta_event(session_id, "b"))
            .await;
        assert_eq!(first.event_seq, 1);
        assert_eq!(second.event_seq, 2);

        let snapshot = registry.snapshot_ephemeral(session_id).await;
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].event_seq, 1);
        assert_eq!(snapshot[1].event_seq, 2);
    }

    #[tokio::test]
    async fn push_ephemeral_evicts_oldest_over_cap() {
        let registry = registry();
        let session_id = "sess-cap";

        for i in 0..(EPHEMERAL_BUFFER_CAP + 5) {
            registry
                .push_ephemeral(session_id, delta_event(session_id, &i.to_string()))
                .await;
        }

        let snapshot = registry.snapshot_ephemeral(session_id).await;
        assert_eq!(snapshot.len(), EPHEMERAL_BUFFER_CAP);
        // 队首 5 条被 evict；首条 seq = 6（前 5 条 seq 1..=5 被丢）。
        assert_eq!(snapshot.first().expect("non-empty").event_seq, 6);
        assert_eq!(
            snapshot.last().expect("non-empty").event_seq,
            (EPHEMERAL_BUFFER_CAP + 5) as u64
        );
    }

    #[tokio::test]
    async fn clear_ephemeral_empties_buffer_but_keeps_seq_monotonic() {
        let registry = registry();
        let session_id = "sess-clear";

        registry
            .push_ephemeral(session_id, delta_event(session_id, "a"))
            .await;
        registry
            .push_ephemeral(session_id, delta_event(session_id, "b"))
            .await;
        registry.clear_ephemeral(session_id).await;
        assert!(registry.snapshot_ephemeral(session_id).await.is_empty());

        // seq 计数器不重置：clear 后继续递增。
        let next = registry
            .push_ephemeral(session_id, delta_event(session_id, "c"))
            .await;
        assert_eq!(next.event_seq, 3);
    }

    #[tokio::test]
    async fn snapshot_ephemeral_empty_for_unknown_session() {
        let registry = registry();
        assert!(registry.snapshot_ephemeral("nope").await.is_empty());
    }

    fn delta_event_with_item(session_id: &str, item_id: &str, text: &str) -> PersistedSessionEvent {
        let envelope = BackboneEnvelope::new(
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                delta: text.to_string(),
                thread_id: session_id.to_string(),
                turn_id: "turn-1".to_string(),
                item_id: item_id.to_string(),
            }),
            session_id,
            SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "local_executor".to_string(),
                executor_id: None,
            },
        );
        PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq: 0,
            occurred_at_ms: 0,
            committed_at_ms: 0,
            session_update_type: "agent_message_delta".to_string(),
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(1),
            tool_call_id: None,
            ephemeral: true,
            notification: envelope,
        }
    }

    #[tokio::test]
    async fn prune_ephemeral_by_item_id_removes_only_matching_deltas() {
        let registry = registry();
        let session_id = "sess-prune";
        let final_item = "turn-1:0:msg";
        let other_item = "turn-1:1:msg";

        registry
            .push_ephemeral(
                session_id,
                delta_event_with_item(session_id, final_item, "a"),
            )
            .await;
        registry
            .push_ephemeral(
                session_id,
                delta_event_with_item(session_id, final_item, "b"),
            )
            .await;
        registry
            .push_ephemeral(
                session_id,
                delta_event_with_item(session_id, other_item, "c"),
            )
            .await;

        registry
            .prune_ephemeral_by_item_id(session_id, final_item)
            .await;

        let snapshot = registry.snapshot_ephemeral(session_id).await;
        // 仅保留 other_item 的 delta；final_item 的两条被剪除。
        assert_eq!(snapshot.len(), 1);
        match &snapshot[0].notification.event {
            BackboneEvent::AgentMessageDelta(delta) => {
                assert_eq!(delta.item_id, other_item);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
