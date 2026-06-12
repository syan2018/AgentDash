use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast};

use super::hub_support::{SessionRuntime, build_session_runtime};
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

    pub async fn execution_state_snapshot(&self, session_id: &str) -> (bool, Option<String>) {
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
                )
            })
            .unwrap_or((false, None))
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

    pub async fn increment_auto_resume_if_allowed(&self, session_id: &str, max: u32) -> bool {
        let mut runtimes = self.runtimes.lock().await;
        let Some(runtime) = runtimes.get_mut(session_id) else {
            return false;
        };
        if runtime.hook_auto_resume_count >= max {
            false
        } else {
            runtime.hook_auto_resume_count += 1;
            true
        }
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
