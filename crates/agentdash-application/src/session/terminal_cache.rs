use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use serde::Serialize;

/// 交互式终端运行时状态缓存（纯内存，不持久化）。
///
/// 跟踪每个 session 名下活跃的终端实例，供 API 查询和前端渲染。
/// 终端生命周期由 ws_handler 接收 relay 事件后驱动更新。
#[derive(Debug, Default)]
pub struct SessionTerminalCache {
    /// session_id → { terminal_id → TerminalState }
    inner: RwLock<HashMap<String, HashMap<String, TerminalState>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalState {
    pub terminal_id: String,
    pub session_id: String,
    pub backend_id: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited_at: Option<i64>,
}

impl SessionTerminalCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn register_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
        backend_id: &str,
        process_id: Option<u32>,
    ) {
        let state = TerminalState {
            terminal_id: terminal_id.to_string(),
            session_id: session_id.to_string(),
            backend_id: backend_id.to_string(),
            state: "starting".to_string(),
            exit_code: None,
            process_id,
            created_at: Utc::now().timestamp_millis(),
            exited_at: None,
        };
        self.inner
            .write()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .insert(terminal_id.to_string(), state);
    }

    pub fn update_state(
        &self,
        terminal_id: &str,
        new_state: &str,
        exit_code: Option<i32>,
    ) {
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

    pub fn list_terminals(&self, session_id: &str) -> Vec<TerminalState> {
        self.inner
            .read()
            .unwrap()
            .get(session_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
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

    pub fn get_terminal(&self, terminal_id: &str) -> Option<TerminalState> {
        let cache = self.inner.read().unwrap();
        for terminals in cache.values() {
            if let Some(entry) = terminals.get(terminal_id) {
                return Some(entry.clone());
            }
        }
        None
    }

    pub fn remove_terminal(&self, terminal_id: &str) {
        let mut cache = self.inner.write().unwrap();
        for terminals in cache.values_mut() {
            if terminals.remove(terminal_id).is_some() {
                return;
            }
        }
    }

    /// 后端断连时标记其所有终端为 Lost
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
}
