use agentdash_application_agentrun::agent_run::terminal_registry::{
    TerminalOutputPreview, TerminalState,
};
use serde_json::{Map, Value, json};

use super::bound_string;
use crate::wait_activity::types::{
    ResolvedWaitScope, WAIT_PREVIEW_CHARS, WaitActivityItem, WaitExecDetails, WaitOutputPreview,
};

const READ_AFTER_SEQ_FROM_START: u64 = 0;

pub(crate) fn terminal_belongs_to_scope(
    terminal: &TerminalState,
    scope: &ResolvedWaitScope,
) -> bool {
    // Scope check: if scope has run_id/agent_id, terminal must match
    match (scope.run_id, scope.agent_id) {
        (Some(run_id), Some(agent_id)) => {
            terminal.run_id == run_id.to_string() && terminal.agent_id == agent_id.to_string()
        }
        _ => true,
    }
}

pub(crate) fn exec_item_from_terminal(terminal: &TerminalState) -> WaitActivityItem {
    let status = exec_activity_status(&terminal.state, terminal.exit_code);
    let result_refs = exec_result_refs(terminal, status);
    let next = exec_next_ref(terminal);
    let exec = exec_details(terminal);
    let diagnostic = exec_diagnostic(status, terminal);
    let updated_at_ms = exec_updated_at_ms(terminal);
    WaitActivityItem {
        activity_ref: terminal.terminal_id.clone(),
        kind: "exec".to_string(),
        status: status.to_string(),
        source_ref: Some(terminal.terminal_id.clone()),
        correlation_ref: None,
        preview: exec_preview(status, terminal),
        diagnostic,
        result_refs,
        exec: Some(exec),
        cursor: Some(updated_at_ms.to_string()),
        next: Some(next),
        updated_at_ms,
    }
}

fn exec_updated_at_ms(terminal: &TerminalState) -> i64 {
    let state_updated_at = terminal.exited_at.unwrap_or(terminal.created_at);
    terminal
        .output_projection
        .as_ref()
        .map(|projection| projection.updated_at.max(state_updated_at))
        .unwrap_or(state_updated_at)
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
        _ => "unknown",
    }
}

fn exec_details(terminal: &TerminalState) -> WaitExecDetails {
    let output = terminal.output_projection.as_ref();
    WaitExecDetails {
        terminal_id: terminal.terminal_id.clone(),
        terminal_state: terminal.state.clone(),
        exit_code: terminal.exit_code,
        stdout_preview: output
            .and_then(|projection| projection.stdout_preview.as_ref())
            .map(output_preview),
        stderr_preview: output
            .and_then(|projection| projection.stderr_preview.as_ref())
            .map(output_preview),
        pty_preview: output
            .and_then(|projection| projection.pty_preview.as_ref())
            .map(output_preview),
    }
}

fn output_preview(preview: &TerminalOutputPreview) -> WaitOutputPreview {
    WaitOutputPreview {
        text: preview.text.clone(),
        bytes: preview.bytes,
        truncated: preview.truncated,
        from: preview.from.clone(),
    }
}

fn exec_result_refs(terminal: &TerminalState, status: &str) -> Value {
    let mut output_ref = Map::new();
    output_ref.insert("kind".to_string(), json!("terminal_output"));
    output_ref.insert("terminal_id".to_string(), json!(terminal.terminal_id));
    output_ref.insert("after_seq".to_string(), json!(READ_AFTER_SEQ_FROM_START));
    if let Some(next_seq) = terminal
        .output_projection
        .as_ref()
        .and_then(|projection| projection.next_seq)
    {
        output_ref.insert("next_seq".to_string(), json!(next_seq));
    }

    json!({
        "terminal_id": terminal.terminal_id,
        "source": {
            "namespace": "terminal",
            "kind": "exec",
            "source_ref": terminal.terminal_id,
            "correlation_ref": Value::Null,
        },
        "output_ref": output_ref,
        "cursor": {
            "after_seq": READ_AFTER_SEQ_FROM_START,
            "next_seq": terminal.output_projection.as_ref().and_then(|projection| projection.next_seq),
        },
        "diagnostic": diagnostic_ref(status, terminal),
        "mount_id": terminal.mount_id,
        "cwd": terminal.cwd,
        "terminal_state": terminal.state,
        "exit_code": terminal.exit_code,
    })
}

fn exec_next_ref(terminal: &TerminalState) -> Value {
    let mut next = Map::new();
    next.insert("tool".to_string(), json!("shell_exec"));
    next.insert("operation".to_string(), json!("read"));
    next.insert("terminal_id".to_string(), json!(terminal.terminal_id));
    next.insert("after_seq".to_string(), json!(READ_AFTER_SEQ_FROM_START));
    if let Some(next_seq) = terminal
        .output_projection
        .as_ref()
        .and_then(|projection| projection.next_seq)
    {
        next.insert("next_seq".to_string(), json!(next_seq));
    }
    Value::Object(next)
}

fn diagnostic_ref(status: &str, terminal: &TerminalState) -> Value {
    match status {
        "completed" | "failed" => json!({
            "kind": "exec_exit",
            "exit_code": terminal.exit_code,
        }),
        "cancelled" => json!({
            "kind": "terminal_killed",
        }),
        "lost" => json!({
            "kind": "terminal_lost",
        }),
        "unknown" => json!({
            "kind": "terminal_state_unknown",
            "terminal_state": terminal.state,
        }),
        "running" => json!({
            "kind": "terminal_running",
        }),
        _ => json!({
            "kind": "terminal_state_unknown",
            "terminal_state": terminal.state,
        }),
    }
}

fn exec_diagnostic(status: &str, terminal: &TerminalState) -> Option<Value> {
    match status {
        "failed" => Some(json!({
            "kind": "exec_exit",
            "code": terminal.exit_code.map(|code| code.to_string()),
            "message": format!(
                "terminal `{}` exited with code {}",
                terminal.terminal_id,
                terminal.exit_code.unwrap_or_default()
            ),
            "retryable": false,
        })),
        "cancelled" => Some(json!({
            "kind": "terminal_killed",
            "message": format!("terminal `{}` was killed", terminal.terminal_id),
            "retryable": false,
        })),
        "lost" => Some(json!({
            "kind": "terminal_lost",
            "message": format!("terminal `{}` was lost", terminal.terminal_id),
            "retryable": true,
        })),
        "unknown" => Some(json!({
            "kind": "terminal_state_unknown",
            "code": terminal.state,
            "message": format!(
                "terminal `{}` has unknown state `{}`",
                terminal.terminal_id, terminal.state
            ),
            "retryable": false,
        })),
        _ => None,
    }
}

fn exec_preview(status: &str, terminal: &TerminalState) -> Option<String> {
    let mut parts = Vec::new();
    match (status, terminal.exit_code) {
        ("completed", Some(code)) | ("failed", Some(code)) => {
            parts.push(format!("exit {code}"));
        }
        ("cancelled", _) => parts.push("cancelled".to_string()),
        ("lost", _) => parts.push("lost".to_string()),
        ("unknown", _) => parts.push(format!("unknown state {}", terminal.state)),
        ("running", _) => parts.push("running".to_string()),
        _ => {}
    }

    if let Some(stderr) = terminal
        .output_projection
        .as_ref()
        .and_then(|projection| projection.stderr_preview.as_ref())
        .filter(|preview| !preview.text.trim().is_empty())
    {
        parts.push(format!(
            "stderr: {}",
            bound_string(stderr.text.trim(), WAIT_PREVIEW_CHARS)
        ));
    } else if let Some(pty) = terminal
        .output_projection
        .as_ref()
        .and_then(|projection| projection.pty_preview.as_ref())
        .filter(|preview| !preview.text.trim().is_empty())
    {
        parts.push(format!(
            "pty: {}",
            bound_string(pty.text.trim(), WAIT_PREVIEW_CHARS)
        ));
    } else if parts.is_empty() {
        return terminal.cwd.clone();
    }

    Some(parts.join("; "))
}
