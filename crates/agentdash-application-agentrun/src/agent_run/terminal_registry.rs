use std::collections::HashMap;
use std::sync::RwLock;

use chrono::Utc;
use serde::Serialize;

/// AgentRun scope 终端运行时状态注册表（纯内存，不持久化）。
///
/// 以 `(run_id, agent_id)` 为一级 scope 索引，每个 scope 内以 `terminal_id` 为二级索引。
/// `terminal_id` 全局唯一，支持反查。
///
/// 替代旧的 `SessionTerminalCache`，消除业务模块对 session_id 的一级索引依赖。
#[derive(Debug, Default)]
pub struct AgentRunTerminalRegistry {
    /// (run_id, agent_id) -> { terminal_id -> TerminalState }
    inner: RwLock<HashMap<AgentRunKey, HashMap<String, TerminalState>>>,
    /// session_id -> AgentRunKey reverse lookup, used by sync adapters that only know session_id.
    session_bindings: RwLock<HashMap<String, AgentRunKey>>,
    /// AgentRunKey -> most recently bound session_id (forward lookup for active session resolution).
    active_sessions: RwLock<HashMap<AgentRunKey, String>>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct AgentRunKey {
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalState {
    pub terminal_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub backend_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited_at: Option<i64>,
}

impl AgentRunTerminalRegistry {
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::default())
    }

    pub fn register_terminal(
        &self,
        run_id: &str,
        agent_id: &str,
        terminal_id: &str,
        backend_id: &str,
        process_id: Option<u32>,
    ) {
        self.register_terminal_with_metadata(
            run_id,
            agent_id,
            terminal_id,
            backend_id,
            process_id,
            None,
            None,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_terminal_with_metadata(
        &self,
        run_id: &str,
        agent_id: &str,
        terminal_id: &str,
        backend_id: &str,
        process_id: Option<u32>,
        mount_id: Option<&str>,
        cwd: Option<&str>,
        capability: Option<&str>,
    ) {
        let key = AgentRunKey {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        };
        let state = TerminalState {
            terminal_id: terminal_id.to_string(),
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
            backend_id: backend_id.to_string(),
            mount_id: mount_id.map(str::to_string),
            cwd: cwd.map(str::to_string),
            capability: capability.map(str::to_string),
            state: "starting".to_string(),
            exit_code: None,
            process_id,
            created_at: Utc::now().timestamp_millis(),
            exited_at: None,
        };
        self.inner
            .write()
            .unwrap()
            .entry(key)
            .or_default()
            .insert(terminal_id.to_string(), state);
    }

    /// Global lookup by terminal_id (terminal_id is globally unique).
    pub fn get_terminal(&self, terminal_id: &str) -> Option<TerminalState> {
        let cache = self.inner.read().unwrap();
        for terminals in cache.values() {
            if let Some(entry) = terminals.get(terminal_id) {
                return Some(entry.clone());
            }
        }
        None
    }

    /// List all terminals for a given AgentRun scope.
    pub fn list_terminals(&self, run_id: &str, agent_id: &str) -> Vec<TerminalState> {
        let key = AgentRunKey {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        };
        self.inner
            .read()
            .unwrap()
            .get(&key)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn update_state(&self, terminal_id: &str, new_state: &str, exit_code: Option<i32>) {
        let mut cache = self.inner.write().unwrap();
        for terminals in cache.values_mut() {
            if let Some(entry) = terminals.get_mut(terminal_id) {
                entry.state = new_state.to_string();
                entry.exit_code = exit_code;
                if new_state == "exited" || new_state == "killed" || new_state == "lost" {
                    entry.exited_at = Some(Utc::now().timestamp_millis());
                }
                return;
            }
        }
    }

    pub fn update_process_id(&self, terminal_id: &str, process_id: Option<u32>) {
        let mut cache = self.inner.write().unwrap();
        for terminals in cache.values_mut() {
            if let Some(entry) = terminals.get_mut(terminal_id) {
                entry.process_id = process_id;
                return;
            }
        }
    }

    pub fn remove_terminal(&self, terminal_id: &str) {
        let mut cache = self.inner.write().unwrap();
        for terminals in cache.values_mut() {
            if terminals.remove(terminal_id).is_some() {
                return;
            }
        }
    }

    /// Mark all terminals belonging to the given backend as Lost.
    pub fn handle_backend_disconnect(&self, backend_id: &str) -> Vec<String> {
        let mut lost_ids = Vec::new();
        let mut cache = self.inner.write().unwrap();
        for terminals in cache.values_mut() {
            for entry in terminals.values_mut() {
                if entry.backend_id == backend_id
                    && (entry.state == "running" || entry.state == "starting")
                {
                    entry.state = "lost".to_string();
                    entry.exited_at = Some(Utc::now().timestamp_millis());
                    lost_ids.push(entry.terminal_id.clone());
                }
            }
        }
        lost_ids
    }

    /// Register the binding between a runtime session and an AgentRun scope.
    /// Called when a session is launched/bound for an AgentRun.
    /// Also updates the active delivery session for this AgentRun.
    pub fn bind_session(&self, session_id: &str, run_id: &str, agent_id: &str) {
        let key = AgentRunKey {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        };
        self.session_bindings
            .write()
            .unwrap()
            .insert(session_id.to_string(), key.clone());
        self.active_sessions
            .write()
            .unwrap()
            .insert(key, session_id.to_string());
    }

    /// Resolve the AgentRun scope for a given session_id.
    pub fn resolve_agent_run_for_session(&self, session_id: &str) -> Option<AgentRunKey> {
        self.session_bindings
            .read()
            .unwrap()
            .get(session_id)
            .cloned()
    }

    /// Resolve the most recently bound session_id for an AgentRun scope.
    /// Used by ws_handler to route terminal events to the active delivery session.
    pub fn resolve_active_session(&self, run_id: &str, agent_id: &str) -> Option<String> {
        let key = AgentRunKey {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        };
        self.active_sessions
            .read()
            .unwrap()
            .get(&key)
            .cloned()
    }
}
