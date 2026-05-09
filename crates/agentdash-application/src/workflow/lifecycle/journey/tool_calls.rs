use std::collections::BTreeMap;

use agentdash_agent_protocol::BackboneEvent;
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use serde::Serialize;
use serde_json::{Value, json};

use crate::session::PersistedSessionEvent;

use super::{JourneyResult, LifecycleJourneyError};

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallSummary {
    pub tool_call_id: String,
    pub kind: String,
    pub name: String,
    pub provider: Option<String>,
    pub status: Option<String>,
    pub turn_id: Option<String>,
    pub first_event_seq: u64,
    pub last_event_seq: u64,
    pub event_count: usize,
    pub has_request: bool,
    pub has_result: bool,
    pub has_stdout: bool,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct ToolCallProjection {
    pub summary: ToolCallSummary,
    pub request: Option<Value>,
    pub result: Option<Value>,
    pub stdout: String,
    pub raw_events: Vec<PersistedSessionEvent>,
}

#[derive(Debug, Clone)]
struct ToolSnapshot {
    kind: String,
    name: String,
    provider: Option<String>,
    status: Option<String>,
    request: Option<Value>,
    result: Option<Value>,
    stdout: Option<String>,
    is_error: bool,
}

pub fn find_tool_projection<'a>(
    projections: &'a [ToolCallProjection],
    tool_call_id: &str,
) -> JourneyResult<&'a ToolCallProjection> {
    projections
        .iter()
        .find(|projection| projection.summary.tool_call_id == tool_call_id)
        .ok_or_else(|| LifecycleJourneyError::NotFound(format!("tool call 不存在: {tool_call_id}")))
}

pub fn tool_call_projections(events: &[PersistedSessionEvent]) -> Vec<ToolCallProjection> {
    let mut grouped: BTreeMap<String, Vec<PersistedSessionEvent>> = BTreeMap::new();
    for event in events {
        if let Some(id) = event_tool_call_id(event) {
            grouped.entry(id).or_default().push(event.clone());
        }
    }

    grouped
        .into_iter()
        .filter_map(|(tool_call_id, mut raw_events)| {
            raw_events.sort_by_key(|event| event.event_seq);
            build_tool_call_projection(tool_call_id, raw_events)
        })
        .collect()
}

pub fn is_write_projection(projection: &ToolCallProjection) -> bool {
    if projection.summary.kind == "file_change" {
        return true;
    }
    let name = projection.summary.name.to_ascii_lowercase();
    name.contains("write")
        || name.contains("apply_patch")
        || name.contains("patch")
        || name.contains("edit")
}

fn event_tool_call_id(event: &PersistedSessionEvent) -> Option<String> {
    if let Some(id) = event
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(id.to_string());
    }

    match &event.notification.event {
        BackboneEvent::ItemStarted(n) => tool_item_id(&n.item),
        BackboneEvent::ItemCompleted(n) => tool_item_id(&n.item),
        BackboneEvent::CommandOutputDelta(n) => Some(n.item_id.clone()),
        BackboneEvent::FileChangeDelta(n) => Some(n.item_id.clone()),
        BackboneEvent::McpToolCallProgress(n) => Some(n.item_id.clone()),
        _ => None,
    }
}

fn tool_item_id(item: &codex::ThreadItem) -> Option<String> {
    match item {
        codex::ThreadItem::DynamicToolCall { id, .. }
        | codex::ThreadItem::CommandExecution { id, .. }
        | codex::ThreadItem::McpToolCall { id, .. }
        | codex::ThreadItem::FileChange { id, .. }
        | codex::ThreadItem::CollabAgentToolCall { id, .. } => Some(id.clone()),
        _ => None,
    }
}

fn build_tool_call_projection(
    tool_call_id: String,
    raw_events: Vec<PersistedSessionEvent>,
) -> Option<ToolCallProjection> {
    let first = raw_events.first()?;
    let last = raw_events.last()?;
    let mut kind = "tool_call".to_string();
    let mut name = "tool_call".to_string();
    let mut provider = None;
    let mut status = None;
    let mut request = None;
    let mut result = None;
    let mut stdout_parts = Vec::new();
    let mut is_error = false;

    for event in &raw_events {
        match &event.notification.event {
            BackboneEvent::ItemStarted(n) => {
                apply_tool_snapshot(
                    tool_snapshot_from_item(&n.item),
                    &mut kind,
                    &mut name,
                    &mut provider,
                    &mut status,
                    &mut request,
                    &mut result,
                    &mut stdout_parts,
                    &mut is_error,
                );
            }
            BackboneEvent::ItemCompleted(n) => {
                apply_tool_snapshot(
                    tool_snapshot_from_item(&n.item),
                    &mut kind,
                    &mut name,
                    &mut provider,
                    &mut status,
                    &mut request,
                    &mut result,
                    &mut stdout_parts,
                    &mut is_error,
                );
            }
            BackboneEvent::CommandOutputDelta(n) => {
                if !n.delta.is_empty() {
                    stdout_parts.push(n.delta.clone());
                }
            }
            BackboneEvent::FileChangeDelta(n) => {
                if !n.delta.is_empty() {
                    stdout_parts.push(n.delta.clone());
                }
            }
            BackboneEvent::McpToolCallProgress(n) => {
                if !n.message.is_empty() {
                    stdout_parts.push(n.message.clone());
                }
            }
            BackboneEvent::Error(_) => {
                is_error = true;
            }
            _ => {}
        }
    }

    let stdout = stdout_parts.join("");
    if result.is_none() && !stdout.is_empty() {
        result = Some(json!({ "output": stdout }));
    }

    let summary = ToolCallSummary {
        tool_call_id,
        kind,
        name,
        provider,
        status,
        turn_id: raw_events.iter().find_map(|event| event.turn_id.clone()),
        first_event_seq: first.event_seq,
        last_event_seq: last.event_seq,
        event_count: raw_events.len(),
        has_request: request.is_some(),
        has_result: result.is_some(),
        has_stdout: !stdout.is_empty(),
        is_error,
    };

    Some(ToolCallProjection {
        summary,
        request,
        result,
        stdout,
        raw_events,
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_tool_snapshot(
    snapshot: Option<ToolSnapshot>,
    kind: &mut String,
    name: &mut String,
    provider: &mut Option<String>,
    status: &mut Option<String>,
    request: &mut Option<Value>,
    result: &mut Option<Value>,
    stdout_parts: &mut Vec<String>,
    is_error: &mut bool,
) {
    let Some(snapshot) = snapshot else {
        return;
    };
    *kind = snapshot.kind;
    *name = snapshot.name;
    *provider = snapshot.provider.or(provider.take());
    *status = snapshot.status.or(status.take());
    if request.is_none() {
        *request = snapshot.request;
    }
    if snapshot.result.is_some() {
        *result = snapshot.result;
    }
    if let Some(stdout) = snapshot
        .stdout
        .map(|s| s.trim_end_matches('\n').to_string())
        .filter(|s| !s.is_empty())
    {
        stdout_parts.push(stdout);
    }
    *is_error |= snapshot.is_error;
}

fn tool_snapshot_from_item(item: &codex::ThreadItem) -> Option<ToolSnapshot> {
    match item {
        codex::ThreadItem::DynamicToolCall {
            tool,
            arguments,
            status,
            content_items,
            success,
            ..
        } => {
            let result = content_items
                .as_ref()
                .and_then(|items| serde_json::to_value(items).ok());
            Some(ToolSnapshot {
                kind: "dynamic_tool_call".to_string(),
                name: tool.clone(),
                provider: None,
                status: Some(dynamic_tool_call_status_str(status).to_string()),
                request: Some(arguments.clone()),
                stdout: content_items
                    .as_ref()
                    .map(|items| dynamic_content_items_text(items)),
                result,
                is_error: success == &Some(false)
                    || matches!(status, codex::DynamicToolCallStatus::Failed),
            })
        }
        codex::ThreadItem::McpToolCall {
            server,
            tool,
            arguments,
            status,
            result,
            error,
            ..
        } => {
            let result = result
                .as_ref()
                .and_then(|value| serde_json::to_value(value).ok())
                .or_else(|| error.as_ref().map(|e| json!({ "error": e.message })));
            Some(ToolSnapshot {
                kind: "mcp_tool_call".to_string(),
                name: tool.clone(),
                provider: Some(server.clone()),
                status: Some(mcp_tool_call_status_str(status).to_string()),
                request: Some(arguments.clone()),
                stdout: error.as_ref().map(|e| e.message.clone()),
                result,
                is_error: error.is_some() || matches!(status, codex::McpToolCallStatus::Failed),
            })
        }
        codex::ThreadItem::CommandExecution {
            command,
            cwd,
            status,
            aggregated_output,
            exit_code,
            ..
        } => {
            let result = aggregated_output.as_ref().map(|output| {
                json!({
                    "output": output,
                    "exit_code": exit_code,
                })
            });
            Some(ToolSnapshot {
                kind: "command_execution".to_string(),
                name: "command_execution".to_string(),
                provider: None,
                status: Some(command_execution_status_str(status).to_string()),
                request: Some(json!({
                    "command": command,
                    "cwd": cwd,
                })),
                stdout: aggregated_output.clone(),
                result,
                is_error: exit_code.is_some_and(|code| code != 0)
                    || matches!(status, codex::CommandExecutionStatus::Failed),
            })
        }
        codex::ThreadItem::FileChange {
            changes, status, ..
        } => {
            let result = serde_json::to_value(changes).ok();
            Some(ToolSnapshot {
                kind: "file_change".to_string(),
                name: "file_change".to_string(),
                provider: None,
                status: Some(patch_apply_status_str(status).to_string()),
                request: None,
                stdout: None,
                result,
                is_error: matches!(status, codex::PatchApplyStatus::Failed),
            })
        }
        item @ codex::ThreadItem::CollabAgentToolCall { tool, status, .. } => Some(ToolSnapshot {
            kind: "collab_agent_tool_call".to_string(),
            name: serde_json::to_value(tool)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "collab_agent_tool".to_string()),
            provider: None,
            status: serde_json::to_value(status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned)),
            request: serde_json::to_value(item).ok(),
            result: None,
            stdout: None,
            is_error: false,
        }),
        _ => None,
    }
}

fn dynamic_content_items_text(items: &[codex::DynamicToolCallOutputContentItem]) -> String {
    items
        .iter()
        .map(|item| match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => text.as_str(),
            codex::DynamicToolCallOutputContentItem::InputImage { .. } => "[image output]",
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn dynamic_tool_call_status_str(status: &codex::DynamicToolCallStatus) -> &'static str {
    match status {
        codex::DynamicToolCallStatus::InProgress => "in_progress",
        codex::DynamicToolCallStatus::Completed => "completed",
        codex::DynamicToolCallStatus::Failed => "failed",
    }
}

fn mcp_tool_call_status_str(status: &codex::McpToolCallStatus) -> &'static str {
    match status {
        codex::McpToolCallStatus::InProgress => "in_progress",
        codex::McpToolCallStatus::Completed => "completed",
        codex::McpToolCallStatus::Failed => "failed",
    }
}

fn command_execution_status_str(status: &codex::CommandExecutionStatus) -> &'static str {
    match status {
        codex::CommandExecutionStatus::InProgress => "in_progress",
        codex::CommandExecutionStatus::Completed => "completed",
        codex::CommandExecutionStatus::Failed => "failed",
        codex::CommandExecutionStatus::Declined => "declined",
    }
}

fn patch_apply_status_str(status: &codex::PatchApplyStatus) -> &'static str {
    match status {
        codex::PatchApplyStatus::InProgress => "in_progress",
        codex::PatchApplyStatus::Completed => "completed",
        codex::PatchApplyStatus::Failed => "failed",
        codex::PatchApplyStatus::Declined => "declined",
    }
}
