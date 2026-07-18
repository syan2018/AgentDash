use serde::{Deserialize, Serialize};

use super::tool::{ToolShellOutputChunk, ToolShellTruncationInfo, truncate_live_output_text};

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
    pub max_output_bytes: usize,
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
    pub terminal_owner_epoch_id: String,
    pub latest_source_sequence: u64,
    pub max_output_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalSourceFence {
    pub terminal_owner_epoch_id: String,
    pub source_sequence: u64,
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
    pub source: TerminalSourceFence,
    pub delta: TerminalOutputDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalOutputDelta {
    Appended {
        stream: TerminalOutputStream,
        data: String,
    },
    Omitted {
        omitted_bytes: usize,
        retained_output: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutputStream {
    Stdout,
    Stderr,
    Pty,
}

impl TerminalOutputPayload {
    pub fn bounded(mut self, max_bytes: usize) -> Self {
        if let TerminalOutputDelta::Appended { data, .. } = &mut self.delta {
            let (bounded, truncation) = truncate_live_output_text(data, max_bytes);
            if truncation.truncated {
                self.delta = TerminalOutputDelta::Omitted {
                    omitted_bytes: truncation.omitted_bytes,
                    retained_output: bounded,
                };
            } else {
                *data = bounded;
            }
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyTerminalStateChangedPayload {
    pub terminal_id: String,
    pub source: TerminalSourceFence,
    pub state: PtyTerminalProcessState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PtyTerminalProcessState {
    Starting,
    Running,
    Exited,
    Lost,
    Killed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalInventoryCursor {
    pub terminal_id: String,
    pub terminal_owner_epoch_id: String,
    pub after_source_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalInventoryRequest {
    pub cursors: Vec<TerminalInventoryCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalSourceSnapshot {
    pub terminal_id: String,
    pub terminal_owner_epoch_id: String,
    pub latest_source_sequence: u64,
    pub max_output_bytes: usize,
    pub state: PtyTerminalProcessState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub chunks: Vec<ToolShellOutputChunk>,
    pub next_output_sequence: u64,
    pub truncation: ToolShellTruncationInfo,
}

/// Complete Local inventory for the current process. Absence is authoritative Unknown; a
/// different owner epoch is authoritative OwnerFenceUnprovable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInventoryResponse {
    pub captured_at_ms: i64,
    pub terminals: Vec<TerminalSourceSnapshot>,
}
