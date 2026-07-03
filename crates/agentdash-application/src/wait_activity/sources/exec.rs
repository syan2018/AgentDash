use agentdash_application_runtime_session::session::terminal_cache::TerminalState;
use serde_json::json;

use crate::wait_activity::types::{ResolvedWaitScope, WaitActivityItem};

pub(crate) fn terminal_belongs_to_scope(
    terminal: &TerminalState,
    scope: &ResolvedWaitScope,
) -> bool {
    scope
        .delivery_runtime_session_id
        .as_deref()
        .is_none_or(|session_id| terminal.session_id == session_id)
}

pub(crate) fn exec_item_from_terminal(terminal: &TerminalState) -> WaitActivityItem {
    let status = exec_activity_status(&terminal.state, terminal.exit_code);
    WaitActivityItem {
        activity_ref: terminal.terminal_id.clone(),
        kind: "exec".to_string(),
        status: status.to_string(),
        source_ref: Some(terminal.terminal_id.clone()),
        correlation_ref: None,
        preview: terminal.cwd.clone(),
        result_refs: json!({
            "terminal_id": terminal.terminal_id,
            "mount_id": terminal.mount_id,
            "cwd": terminal.cwd,
            "exit_code": terminal.exit_code,
        }),
        cursor: None,
        next: Some(json!({
            "tool": "shell_exec",
            "operation": "read",
            "terminal_id": terminal.terminal_id,
        })),
        updated_at_ms: terminal.exited_at.unwrap_or(terminal.created_at),
    }
}

fn exec_activity_status(state: &str, exit_code: Option<i32>) -> &'static str {
    match state {
        "exited" => {
            if exit_code.unwrap_or(0) == 0 {
                "completed"
            } else {
                "failed"
            }
        }
        "killed" => "cancelled",
        "lost" => "lost",
        "failed" => "failed",
        "starting" | "running" => "running",
        _ => "running",
    }
}
