use serde::{Deserialize, Serialize};

use super::tool::{ToolShellTruncationInfo, truncate_live_output_text};

// ─── 交互式终端 Payload ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalSpawnPayload {
    pub terminal_id: String,
    pub session_id: String,
    pub mount_root_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalSpawnResponse {
    pub terminal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInputPayload {
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInputResponse {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalResizePayload {
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalResizeResponse {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalKillPayload {
    pub terminal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalKillResponse {
    pub terminal_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalOutputPayload {
    pub terminal_id: String,
    pub data: String,
    #[serde(default, skip_serializing_if = "ToolShellTruncationInfo::is_empty")]
    pub truncation: ToolShellTruncationInfo,
}

impl TerminalOutputPayload {
    pub fn bounded(mut self, max_bytes: usize) -> Self {
        let (data, truncation) = truncate_live_output_text(&self.data, max_bytes);
        self.data = data;
        self.truncation = self.truncation.merge(&truncation);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyTerminalStateChangedPayload {
    pub terminal_id: String,
    pub state: PtyTerminalProcessState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PtyTerminalProcessState {
    Running,
    Exited,
    Lost,
    Killed,
}
