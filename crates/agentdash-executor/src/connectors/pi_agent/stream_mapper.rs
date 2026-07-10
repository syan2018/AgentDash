use agentdash_diagnostics::{Subsystem, diag};
use std::collections::HashMap;
use std::{iter::Peekable, str::Chars, sync::Arc};

use agentdash_agent::{
    AgentEvent, AgentMessage, AgentRunError, AgentRunErrorKind, AgentToolResult, ContentPart,
    TokenUsage, ToolResultAddressProvider, stable_tool_result_item_id,
};
use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEnvelope, BackboneEvent,
    ItemCompletedNotification, ItemStartedNotification, ItemUpdatedNotification, PlatformEvent,
    ProviderAttemptPhase as ProtocolProviderAttemptPhase,
    ProviderAttemptStatus as ProtocolProviderAttemptStatus, RuntimeTerminalDiagnostic,
    ShellExecExecutionMode, SourceInfo, ThreadTokenUsage, ThreadTokenUsageUpdatedNotification,
    TraceInfo, backbone::thread_item,
};
use agentdash_agent_protocol::{ContextUsageSource, NormalizedContextUsage, TokenUsageBreakdown};
use codex_app_server_protocol as codex;

use super::session_item_identity::SessionItemIdentity;

fn make_envelope(
    event: BackboneEvent,
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    entry_index: u32,
) -> BackboneEnvelope {
    BackboneEnvelope::new(event, session_id, source.clone()).with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: Some(entry_index),
    })
}

/// 合成非工具 chunk item_id。工具结果优先使用 session scoped readable ref。
fn synth_item_id(turn_id: &str, entry_index: u32, suffix: &str) -> String {
    format!("{turn_id}:{entry_index}:{suffix}")
}

#[derive(Debug, Default, Clone)]
pub(super) struct ChunkEmitState {
    emitted_text: String,
    seen_delta: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ToolCallEmitState {
    entry_index: u32,
    tool_name: String,
    raw_input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct StreamMapperRuntimeContext {
    pub model_context_window: Option<u64>,
    pub reserve_tokens: u64,
    pub session_identity: Option<Arc<SessionItemIdentity>>,
}

pub(super) struct StreamMapperEventState<'a> {
    pub entry_index: &'a mut u32,
    pub chunk_emit_states: &'a mut HashMap<String, ChunkEmitState>,
    pub tool_call_states: &'a mut HashMap<String, ToolCallEmitState>,
}

fn chunk_stream_key(turn_id: &str, entry_index: u32, chunk_kind: &str) -> String {
    format!("{turn_id}:{entry_index}:{chunk_kind}")
}

fn upsert_tool_call_state(
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
    entry_index: &mut u32,
    tool_call_id: &str,
    tool_name: String,
    raw_input: Option<serde_json::Value>,
) -> (ToolCallEmitState, bool) {
    if let Some(existing) = tool_call_states.get_mut(tool_call_id) {
        if !tool_name.trim().is_empty() {
            existing.tool_name = tool_name;
        }
        if let Some(raw_input) = raw_input {
            existing.raw_input = Some(raw_input);
        }
        return (existing.clone(), false);
    }

    let state = ToolCallEmitState {
        entry_index: *entry_index,
        tool_name,
        raw_input,
    };
    tool_call_states.insert(tool_call_id.to_string(), state.clone());
    (state, true)
}

fn upsert_state_from_tool_name(
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
    entry_index: &mut u32,
    tool_call_id: &str,
    tool_name: &str,
    raw_input: Option<serde_json::Value>,
) -> (ToolCallEmitState, bool) {
    upsert_tool_call_state(
        tool_call_states,
        entry_index,
        tool_call_id,
        tool_name.to_string(),
        raw_input,
    )
}

fn message_tool_call_info<'a>(
    message: &'a AgentMessage,
    tool_call_id: &str,
) -> Option<&'a agentdash_agent::ToolCallInfo> {
    match message {
        AgentMessage::Assistant { tool_calls, .. } => tool_calls
            .iter()
            .find(|tool_call| tool_call.id == tool_call_id),
        _ => None,
    }
}

fn upsert_state_from_message(
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
    entry_index: &mut u32,
    message: &AgentMessage,
    tool_call_id: &str,
    fallback_name: &str,
) -> (ToolCallEmitState, bool) {
    if let Some(tool_call) = message_tool_call_info(message, tool_call_id) {
        return upsert_tool_call_state(
            tool_call_states,
            entry_index,
            tool_call_id,
            tool_call.name.clone(),
            Some(tool_call.arguments.clone()),
        );
    }
    upsert_state_from_tool_name(
        tool_call_states,
        entry_index,
        tool_call_id,
        fallback_name,
        None,
    )
}

/// 判断是否为 shell_exec 工具调用（映射为 AgentDash native shellExec）。
fn is_shell_exec(tool_name: &str) -> bool {
    tool_name == "shell_exec"
}

fn tool_result_item_id(
    runtime_context: &StreamMapperRuntimeContext,
    turn_id: &str,
    tool_call_id: &str,
    tool_name: &str,
    result: Option<&serde_json::Value>,
) -> String {
    if let Some(item_id) = result.and_then(tool_result_item_id_from_details) {
        return item_id;
    }

    runtime_context
        .session_identity
        .as_ref()
        .map(|identity| {
            identity
                .tool_result_ref(turn_id, tool_call_id, tool_name)
                .item_id
        })
        .unwrap_or_else(|| stable_tool_result_item_id(turn_id, tool_call_id))
}

fn tool_result_item_id_from_details(result: &serde_json::Value) -> Option<String> {
    result
        .get("details")
        .and_then(|details| details.get("readable_ref"))
        .and_then(|readable_ref| readable_ref.get("item_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn provider_attempt_phase_to_protocol(
    phase: agentdash_agent::ProviderAttemptPhase,
) -> ProtocolProviderAttemptPhase {
    match phase {
        agentdash_agent::ProviderAttemptPhase::Connecting => {
            ProtocolProviderAttemptPhase::Connecting
        }
        agentdash_agent::ProviderAttemptPhase::ConnectedWaitingFirstDelta => {
            ProtocolProviderAttemptPhase::ConnectedWaitingFirstDelta
        }
        agentdash_agent::ProviderAttemptPhase::Streaming => ProtocolProviderAttemptPhase::Streaming,
        agentdash_agent::ProviderAttemptPhase::RetryScheduled => {
            ProtocolProviderAttemptPhase::RetryScheduled
        }
        agentdash_agent::ProviderAttemptPhase::Retrying => ProtocolProviderAttemptPhase::Retrying,
        agentdash_agent::ProviderAttemptPhase::Failed => ProtocolProviderAttemptPhase::Failed,
        agentdash_agent::ProviderAttemptPhase::Succeeded => ProtocolProviderAttemptPhase::Succeeded,
    }
}

fn provider_attempt_status_to_protocol(
    status: &agentdash_agent::ProviderAttemptStatus,
    turn_id: &str,
) -> ProtocolProviderAttemptStatus {
    ProtocolProviderAttemptStatus {
        turn_id: turn_id.to_string(),
        phase: provider_attempt_phase_to_protocol(status.phase),
        attempt: status.attempt,
        max_attempts: status.max_attempts,
        will_retry: status.will_retry,
        delay_ms: status.delay_ms,
        reason_code: status.reason_code.clone(),
        message: status.message.clone(),
        provider: status.provider.clone(),
        model: status.model.clone(),
    }
}

fn run_error_notification(
    session_id: &str,
    turn_id: &str,
    error: &AgentRunError,
) -> codex::ErrorNotification {
    error_notification(
        session_id,
        turn_id,
        &error.message,
        Some(run_error_codex_error_info(error)),
        run_error_details(error),
    )
}

fn run_error_terminal_diagnostic(error: &AgentRunError) -> RuntimeTerminalDiagnostic {
    RuntimeTerminalDiagnostic {
        kind: match error.kind {
            AgentRunErrorKind::Provider => "provider",
            AgentRunErrorKind::HookBlocked => "hook",
            AgentRunErrorKind::Runtime => "runtime",
            AgentRunErrorKind::Tool => "tool",
            AgentRunErrorKind::Unknown => "unknown",
        }
        .to_string(),
        code: error.code.clone(),
        http_status: error.http_status,
        provider: error.provider.clone(),
        model: error.model.clone(),
        message: error.message.clone(),
        retryable: error.retryable,
    }
}

fn error_notification(
    session_id: &str,
    turn_id: &str,
    message: &str,
    codex_error_info: Option<codex::CodexErrorInfo>,
    additional_details: Option<String>,
) -> codex::ErrorNotification {
    codex::ErrorNotification {
        error: codex::TurnError {
            message: message.to_string(),
            codex_error_info,
            additional_details,
        },
        will_retry: false,
        thread_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
    }
}

fn run_error_codex_error_info(error: &AgentRunError) -> codex::CodexErrorInfo {
    if error.aborted {
        return codex::CodexErrorInfo::Other;
    }
    match error.http_status {
        Some(401 | 403) => codex::CodexErrorInfo::Unauthorized,
        Some(400 | 422) => codex::CodexErrorInfo::BadRequest,
        Some(500..=599) => codex::CodexErrorInfo::InternalServerError,
        _ => match (error.kind, error.code.as_deref()) {
            (AgentRunErrorKind::HookBlocked, _) => codex::CodexErrorInfo::BadRequest,
            (
                _,
                Some("auth_error" | "unauthorized" | "invalid_api_key" | "invalid_credentials"),
            ) => codex::CodexErrorInfo::Unauthorized,
            (_, Some("invalid_request" | "invalid_request_error")) => {
                codex::CodexErrorInfo::BadRequest
            }
            (_, Some("provider_5xx")) => codex::CodexErrorInfo::InternalServerError,
            (_, Some("timeout" | "rate_limited" | "transient_provider_error")) => {
                codex::CodexErrorInfo::ResponseStreamConnectionFailed {
                    http_status_code: error.http_status,
                }
            }
            _ => codex::CodexErrorInfo::Other,
        },
    }
}

fn run_error_details(error: &AgentRunError) -> Option<String> {
    let mut parts = Vec::new();
    parts.push(format!("kind={:?}", error.kind));
    if let Some(code) = error.code.as_deref() {
        parts.push(format!("code={code}"));
    }
    if let Some(http_status) = error.http_status {
        parts.push(format!("http_status={http_status}"));
    }
    if error.retryable {
        parts.push("retryable=true".to_string());
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// 从 shell_exec 的 args JSON 中提取 command / cwd。
/// cwd 保持 Agent-facing 语义：缺省/空值表示 platform shell，非空值必须由执行层校验为
/// platform:// 或 mount_id://relative/path。
fn extract_shell_args(
    args: &serde_json::Value,
) -> (String, Option<String>, ShellExecExecutionMode) {
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)")
        .to_string();
    let raw_cwd = args
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match raw_cwd {
        None => (
            command,
            Some("platform://".to_string()),
            ShellExecExecutionMode::Platform,
        ),
        Some(cwd) if cwd.starts_with("platform://") => (
            command,
            Some(cwd.to_string()),
            ShellExecExecutionMode::Platform,
        ),
        Some(cwd) => (
            command,
            Some(cwd.to_string()),
            ShellExecExecutionMode::MountExec,
        ),
    }
}

fn partial_result_details_type(partial_result: &serde_json::Value) -> Option<&str> {
    partial_result
        .get("details")
        .and_then(|d| d.get("type"))
        .and_then(|t| t.as_str())
}

fn partial_result_terminal_id(partial_result: &serde_json::Value) -> Option<&str> {
    tool_result_details(partial_result)
        .and_then(|details| details.get("terminal_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|terminal_id| !terminal_id.is_empty())
}

fn partial_result_text(partial_result: &serde_json::Value) -> String {
    tool_result_text(partial_result).unwrap_or_default()
}

fn decode_tool_result(value: &serde_json::Value) -> Option<AgentToolResult> {
    serde_json::from_value(value.clone()).ok()
}

fn tool_result_text(value: &serde_json::Value) -> Option<String> {
    let result = decode_tool_result(value)?;
    let text = result
        .content
        .iter()
        .filter_map(ContentPart::extract_text)
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() { None } else { Some(text) }
}

fn tool_result_details(value: &serde_json::Value) -> Option<&serde_json::Value> {
    value.get("details").filter(|details| !details.is_null())
}

fn is_companion_subagent_dispatch_result(value: &serde_json::Value) -> bool {
    tool_result_details(value)
        .and_then(|details| details.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("companion_subagent_dispatch")
}

fn shell_exit_code_from_result(value: &serde_json::Value) -> Option<i32> {
    tool_result_details(value)
        .and_then(|details| details.get("exit_code"))
        .and_then(|exit_code| exit_code.as_i64())
        .and_then(|exit_code| i32::try_from(exit_code).ok())
        .or_else(|| {
            tool_result_text(value).and_then(|text| {
                // exit code 可能被保留在 shell result 文本里；bounded preview 裁掉尾部时，
                // 上面的 details 路径仍能保留结构化状态。
                text.lines().rev().find_map(|line| {
                    let trimmed = line.trim();
                    trimmed
                        .strip_prefix("exit_code: ")
                        .or_else(|| trimmed.strip_prefix("Exit code: "))
                        .and_then(|s| s.parse::<i32>().ok())
                })
            })
        })
}

fn shell_command_status_from_result(
    value: &serde_json::Value,
    is_error: bool,
) -> codex::CommandExecutionStatus {
    if is_error || shell_exit_code_from_result(value).is_some_and(|exit_code| exit_code != 0) {
        codex::CommandExecutionStatus::Failed
    } else {
        codex::CommandExecutionStatus::Completed
    }
}

fn command_status_to_dynamic(
    status: &codex::CommandExecutionStatus,
) -> codex::DynamicToolCallStatus {
    match status {
        codex::CommandExecutionStatus::InProgress => codex::DynamicToolCallStatus::InProgress,
        codex::CommandExecutionStatus::Completed => codex::DynamicToolCallStatus::Completed,
        codex::CommandExecutionStatus::Failed | codex::CommandExecutionStatus::Declined => {
            codex::DynamicToolCallStatus::Failed
        }
    }
}

fn make_shell_exec_item(
    item_id: &str,
    state: &ToolCallEmitState,
    status: codex::CommandExecutionStatus,
    aggregated_output: Option<String>,
    exit_code: Option<i32>,
) -> AgentDashThreadItem {
    let args = state
        .raw_input
        .clone()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let (command, cwd, execution_mode) = extract_shell_args(&args);
    AgentDashNativeThreadItem::ShellExec {
        id: item_id.to_string(),
        command,
        cwd,
        execution_mode,
        arguments: args,
        status: command_status_to_dynamic(&status),
        aggregated_output,
        exit_code,
        success: Some(!matches!(
            status,
            codex::CommandExecutionStatus::Failed | codex::CommandExecutionStatus::Declined
        )),
    }
    .into()
}

/// 构造通用工具 ThreadItem。Codex 已有的语义用 Codex variant；AgentDash 自有
/// read/search/list 语义用 native variant。
fn make_dynamic_tool_item(
    item_id: &str,
    state: &ToolCallEmitState,
    status: codex::DynamicToolCallStatus,
    content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
    success: Option<bool>,
) -> AgentDashThreadItem {
    let arguments = state
        .raw_input
        .clone()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    match state.tool_name.as_str() {
        "fs_apply_patch" => make_apply_patch_file_change_item(
            item_id,
            &arguments,
            patch_apply_status_from_dynamic(&status),
        )
        .unwrap_or_else(|| {
            thread_item::dynamic_tool_call(
                item_id,
                state.tool_name.clone(),
                arguments,
                status,
                content_items,
                success,
            )
            .into()
        }),
        "fs_read" => AgentDashNativeThreadItem::FsRead {
            id: item_id.to_string(),
            path: string_arg(&arguments, "path")
                .or_else(|| string_arg(&arguments, "file_path"))
                .unwrap_or_default(),
            offset: usize_arg(&arguments, "offset"),
            limit: usize_arg(&arguments, "limit"),
            arguments,
            status,
            content_items,
            success,
        }
        .into(),
        "fs_grep" => AgentDashNativeThreadItem::FsGrep {
            id: item_id.to_string(),
            pattern: string_arg(&arguments, "pattern").unwrap_or_default(),
            path: string_arg(&arguments, "path"),
            glob: string_arg(&arguments, "glob"),
            file_type: string_arg(&arguments, "type"),
            output_mode: string_arg(&arguments, "output_mode"),
            head_limit: usize_arg(&arguments, "head_limit"),
            offset: usize_arg(&arguments, "offset"),
            arguments,
            status,
            content_items,
            success,
        }
        .into(),
        "fs_glob" => AgentDashNativeThreadItem::FsGlob {
            id: item_id.to_string(),
            pattern: string_arg(&arguments, "pattern").unwrap_or_default(),
            path: string_arg(&arguments, "path"),
            max_results: usize_arg(&arguments, "max_results")
                .or_else(|| usize_arg(&arguments, "maxResults")),
            arguments,
            status,
            content_items,
            success,
        }
        .into(),
        _ => thread_item::dynamic_tool_call(
            item_id,
            state.tool_name.clone(),
            arguments,
            status,
            content_items,
            success,
        )
        .into(),
    }
}

fn make_apply_patch_file_change_item(
    item_id: &str,
    arguments: &serde_json::Value,
    status: codex::PatchApplyStatus,
) -> Option<AgentDashThreadItem> {
    let patch = string_arg(arguments, "patch")?;
    let changes = parse_apply_patch_specs(&patch).ok()?;
    if changes.is_empty() {
        return None;
    }
    match thread_item::file_change(item_id, changes, status) {
        Ok(item) => Some(item.into()),
        Err(error) => {
            diag!(
                Warn,
                Subsystem::AgentRun,
                "Failed to build FileChange from fs_apply_patch: {error}"
            );
            None
        }
    }
}

fn apply_patch_preview_args_from_draft(
    draft: &str,
    is_parseable: bool,
) -> Option<serde_json::Value> {
    let patch = extract_patch_string_from_tool_call_draft(draft, is_parseable)?;
    let specs = parse_apply_patch_specs(&patch).ok()?;
    if specs.is_empty() {
        return None;
    }
    Some(serde_json::json!({ "patch": patch }))
}

fn extract_patch_string_from_tool_call_draft(draft: &str, is_parseable: bool) -> Option<String> {
    if is_parseable {
        let value = serde_json::from_str::<serde_json::Value>(draft).ok()?;
        return string_arg(&value, "patch");
    }
    extract_json_string_field_prefix(draft, "patch")
}

fn parse_tool_call_args_from_draft(draft: &str, is_parseable: bool) -> Option<serde_json::Value> {
    if !is_parseable {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(draft).ok()
}

fn extract_json_string_field_prefix(draft: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let bytes = draft.as_bytes();
    let mut search_from = 0;

    while let Some(relative_index) = draft[search_from..].find(&needle) {
        let mut index = search_from + relative_index + needle.len();
        index = skip_json_whitespace(bytes, index);
        if bytes.get(index) != Some(&b':') {
            search_from += relative_index + needle.len();
            continue;
        }
        index = skip_json_whitespace(bytes, index + 1);
        if bytes.get(index) != Some(&b'"') {
            return None;
        }
        return decode_json_string_prefix(&draft[index + 1..]);
    }

    None
}

fn skip_json_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    index
}

fn decode_json_string_prefix(input: &str) -> Option<String> {
    let mut output = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(output),
            '\\' => match chars.next() {
                Some('"') => output.push('"'),
                Some('\\') => output.push('\\'),
                Some('n') => output.push('\n'),
                Some('r') => output.push('\r'),
                Some('t') => output.push('\t'),
                Some('/') => output.push('/'),
                Some('b') => output.push('\u{0008}'),
                Some('f') => output.push('\u{000c}'),
                Some('u') => match decode_json_unicode_escape(&mut chars) {
                    Some(decoded) => output.push(decoded),
                    None => {
                        return if output.is_empty() {
                            None
                        } else {
                            Some(output)
                        };
                    }
                },
                Some(other) => output.push(other),
                None => {
                    return if output.is_empty() {
                        None
                    } else {
                        Some(output)
                    };
                }
            },
            other => output.push(other),
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn decode_json_unicode_escape(chars: &mut Peekable<Chars<'_>>) -> Option<char> {
    let value = decode_json_hex_code_unit(chars)?;
    if (0xD800..=0xDBFF).contains(&value) {
        if chars.next()? != '\\' || chars.next()? != 'u' {
            return None;
        }
        let low = decode_json_hex_code_unit(chars)?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return None;
        }
        let scalar = 0x10000 + (((value - 0xD800) << 10) | (low - 0xDC00));
        return char::from_u32(scalar);
    }
    if (0xDC00..=0xDFFF).contains(&value) {
        return None;
    }
    char::from_u32(value)
}

fn decode_json_hex_code_unit(chars: &mut Peekable<Chars<'_>>) -> Option<u32> {
    let mut value = 0_u32;
    for _ in 0..4 {
        value = (value << 4) | chars.next()?.to_digit(16)?;
    }
    Some(value)
}

fn patch_apply_status_from_dynamic(
    status: &codex::DynamicToolCallStatus,
) -> codex::PatchApplyStatus {
    match status {
        codex::DynamicToolCallStatus::InProgress => codex::PatchApplyStatus::InProgress,
        codex::DynamicToolCallStatus::Completed => codex::PatchApplyStatus::Completed,
        codex::DynamicToolCallStatus::Failed => codex::PatchApplyStatus::Failed,
    }
}

fn parse_apply_patch_specs(patch: &str) -> Result<Vec<thread_item::FileChangeSpec>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    let mut index = lines
        .iter()
        .position(|line| line.trim_end() == "*** Begin Patch")
        .ok_or_else(|| "missing begin marker".to_string())?
        + 1;
    let mut specs = Vec::new();

    while index < lines.len() {
        let line = lines[index].trim_end();
        if line == "*** End Patch" {
            break;
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            index += 1;
            let mut diff_lines = Vec::new();
            while index < lines.len() && !is_apply_patch_file_op_or_end(lines[index].trim_end()) {
                let next = lines[index].trim_end();
                if next != "*** End of File" {
                    diff_lines.push(next.to_string());
                }
                index += 1;
            }
            specs.push(thread_item::FileChangeSpec::Add {
                path: path.to_string(),
                diff: diff_lines.join("\n"),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            specs.push(thread_item::FileChangeSpec::Delete {
                path: path.to_string(),
            });
            index += 1;
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            index += 1;
            let mut move_path = None;
            let mut diff_lines = Vec::new();
            while index < lines.len() && !is_apply_patch_file_op_or_end(lines[index].trim_end()) {
                let next = lines[index].trim_end();
                if let Some(target) = next.strip_prefix("*** Move to: ") {
                    move_path = Some(target.to_string());
                } else if next != "*** End of File" {
                    diff_lines.push(next.to_string());
                }
                index += 1;
            }
            let diff = diff_lines.join("\n");
            if let Some(new_path) = move_path {
                specs.push(thread_item::FileChangeSpec::Rename {
                    path: path.to_string(),
                    new_path,
                    diff,
                });
            } else {
                specs.push(thread_item::FileChangeSpec::Edit {
                    path: path.to_string(),
                    unified_diff: diff,
                });
            }
            continue;
        }
        index += 1;
    }

    Ok(specs)
}

fn is_apply_patch_file_op_or_end(line: &str) -> bool {
    line == "*** End Patch"
        || line.starts_with("*** Add File: ")
        || line.starts_with("*** Delete File: ")
        || line.starts_with("*** Update File: ")
}

fn make_context_compaction_item(item_id: &str) -> AgentDashThreadItem {
    codex::ThreadItem::ContextCompaction {
        id: item_id.to_string(),
    }
    .into()
}

fn string_arg(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn usize_arg(args: &serde_json::Value, key: &str) -> Option<usize> {
    args.get(key)
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
}

fn usage_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn token_usage_notification_from_usage(
    session_id: &str,
    turn_id: &str,
    usage: &TokenUsage,
    runtime_context: StreamMapperRuntimeContext,
) -> ThreadTokenUsageUpdatedNotification {
    let provider_context_tokens = usage.context_input_tokens();
    let total_tokens = provider_context_tokens.saturating_add(usage.output);
    let cached_input_tokens = usage
        .cache_read_input
        .saturating_add(usage.cache_creation_input);
    let breakdown = TokenUsageBreakdown {
        total_tokens: usage_to_i64(total_tokens),
        input_tokens: usage_to_i64(usage.input),
        cached_input_tokens: usage_to_i64(cached_input_tokens),
        output_tokens: usage_to_i64(usage.output),
        reasoning_output_tokens: 0,
    };
    let model_context_window = runtime_context.model_context_window.map(usage_to_i64);
    let reserve_tokens = usage_to_i64(runtime_context.reserve_tokens);
    let current_context_tokens = usage_to_i64(provider_context_tokens);

    ThreadTokenUsageUpdatedNotification {
        thread_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
        token_usage: ThreadTokenUsage {
            total: breakdown.clone(),
            last: breakdown,
            model_context_window,
            context: NormalizedContextUsage {
                provider_context_tokens: current_context_tokens,
                pending_estimate_tokens: 0,
                current_context_tokens,
                cumulative_total_tokens: usage_to_i64(total_tokens),
                model_context_window,
                effective_context_window: model_context_window,
                reserve_tokens,
                source: ContextUsageSource::Provider,
            },
        },
    }
}

#[cfg(test)]
pub(super) fn convert_event_to_envelopes(
    event: &AgentEvent,
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    entry_index: &mut u32,
    chunk_emit_states: &mut HashMap<String, ChunkEmitState>,
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
) -> Vec<BackboneEnvelope> {
    convert_event_to_envelopes_with_runtime_context(
        event,
        session_id,
        source,
        turn_id,
        StreamMapperEventState {
            entry_index,
            chunk_emit_states,
            tool_call_states,
        },
        StreamMapperRuntimeContext::default(),
    )
}

pub(super) fn convert_event_to_envelopes_with_runtime_context(
    event: &AgentEvent,
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    state: StreamMapperEventState<'_>,
    runtime_context: StreamMapperRuntimeContext,
) -> Vec<BackboneEnvelope> {
    let StreamMapperEventState {
        entry_index,
        chunk_emit_states,
        tool_call_states,
    } = state;
    let wrap =
        |event: BackboneEvent, idx: u32| make_envelope(event, session_id, source, turn_id, idx);

    match event {
        AgentEvent::ProviderAttemptStatus { status } => {
            vec![wrap(
                BackboneEvent::Platform(PlatformEvent::ProviderAttemptStatus(
                    provider_attempt_status_to_protocol(status, turn_id),
                )),
                *entry_index,
            )]
        }

        AgentEvent::RunError { error } => vec![
            wrap(
                BackboneEvent::Platform(PlatformEvent::RuntimeTerminalDiagnostic(
                    run_error_terminal_diagnostic(error),
                )),
                *entry_index,
            ),
            wrap(
                BackboneEvent::Error(run_error_notification(session_id, turn_id, error)),
                *entry_index,
            ),
        ],

        AgentEvent::MessageUpdate {
            message,
            event: stream_event,
        } => match stream_event {
            agentdash_agent::types::AssistantStreamEvent::ToolCallStart {
                tool_call_id,
                name,
                ..
            } => {
                let (state, created) = upsert_state_from_message(
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                );
                if !created {
                    return Vec::new();
                }
                let item_id = tool_result_item_id(
                    &runtime_context,
                    turn_id,
                    tool_call_id,
                    &state.tool_name,
                    None,
                );
                let item = if is_shell_exec(&state.tool_name) {
                    make_shell_exec_item(
                        &item_id,
                        &state,
                        codex::CommandExecutionStatus::InProgress,
                        None,
                        None,
                    )
                } else {
                    make_dynamic_tool_item(
                        &item_id,
                        &state,
                        codex::DynamicToolCallStatus::InProgress,
                        None,
                        None,
                    )
                };
                vec![wrap(
                    BackboneEvent::ItemStarted(ItemStartedNotification::new(
                        item,
                        session_id.to_string(),
                        turn_id.to_string(),
                    )),
                    state.entry_index,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ToolCallDelta {
                tool_call_id,
                name,
                draft,
                is_parseable,
                ..
            } => {
                let (state, _) = upsert_state_from_message(
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                );
                let args = if state.tool_name == "fs_apply_patch" {
                    apply_patch_preview_args_from_draft(draft, *is_parseable)
                } else {
                    parse_tool_call_args_from_draft(draft, *is_parseable)
                };
                let Some(args) = args else {
                    return Vec::new();
                };
                let (state, _) = upsert_tool_call_state(
                    tool_call_states,
                    entry_index,
                    tool_call_id,
                    state.tool_name,
                    Some(args),
                );
                let item_id = tool_result_item_id(
                    &runtime_context,
                    turn_id,
                    tool_call_id,
                    &state.tool_name,
                    None,
                );
                let item = if is_shell_exec(&state.tool_name) {
                    make_shell_exec_item(
                        &item_id,
                        &state,
                        codex::CommandExecutionStatus::InProgress,
                        None,
                        None,
                    )
                } else {
                    make_dynamic_tool_item(
                        &item_id,
                        &state,
                        codex::DynamicToolCallStatus::InProgress,
                        None,
                        None,
                    )
                };
                vec![wrap(
                    BackboneEvent::ItemUpdated(ItemUpdatedNotification::new(
                        item,
                        session_id.to_string(),
                        turn_id.to_string(),
                    )),
                    state.entry_index,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ToolCallEnd { tool_call, .. } => {
                let (_state, _) = upsert_tool_call_state(
                    tool_call_states,
                    entry_index,
                    &tool_call.id,
                    tool_call.name.clone(),
                    Some(tool_call.arguments.clone()),
                );
                Vec::new()
            }
            agentdash_agent::types::AssistantStreamEvent::TextDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let key = chunk_stream_key(turn_id, *entry_index, "agent_message");
                let state = chunk_emit_states.entry(key).or_default();
                state.seen_delta = true;
                state.emitted_text.push_str(text);
                let item_id = synth_item_id(turn_id, *entry_index, "msg");
                vec![wrap(
                    BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                        thread_id: session_id.to_string(),
                        turn_id: turn_id.to_string(),
                        item_id,
                        delta: text.to_string(),
                    }),
                    *entry_index,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ThinkingDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let key = chunk_stream_key(turn_id, *entry_index, "reasoning");
                let state = chunk_emit_states.entry(key).or_default();
                state.seen_delta = true;
                state.emitted_text.push_str(text);
                let item_id = synth_item_id(turn_id, *entry_index, "reason");
                vec![wrap(
                    BackboneEvent::ReasoningTextDelta(codex::ReasoningTextDeltaNotification {
                        thread_id: session_id.to_string(),
                        turn_id: turn_id.to_string(),
                        item_id,
                        delta: text.to_string(),
                        content_index: 0,
                    }),
                    *entry_index,
                )]
            }
            _ => Vec::new(),
        },

        AgentEvent::MessageEnd { message } => {
            if let AgentMessage::Assistant {
                content,
                error_message,
                tool_calls,
                stop_reason,
                usage,
                ..
            } = message
            {
                if matches!(stop_reason, Some(agentdash_agent::StopReason::Aborted)) {
                    return Vec::new();
                }
                if let Some(error_message) = error_message {
                    let error =
                        AgentRunError::new(AgentRunErrorKind::Unknown, error_message.clone())
                            .with_code(Some("assistant_error_message".to_string()));
                    return vec![wrap(
                        BackboneEvent::Error(run_error_notification(session_id, turn_id, &error)),
                        *entry_index,
                    )];
                }

                let reasoning_text = content
                    .iter()
                    .filter_map(ContentPart::extract_reasoning)
                    .collect::<Vec<_>>()
                    .join("");
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("");

                let mut envelopes = Vec::new();

                // 补发 reasoning 残余增量
                if !reasoning_text.is_empty() {
                    let key = chunk_stream_key(turn_id, *entry_index, "reasoning");
                    let state = chunk_emit_states.get(&key).cloned().unwrap_or_default();
                    let to_emit = reconcile_chunk(
                        &state,
                        &reasoning_text,
                        turn_id,
                        *entry_index,
                        "reasoning",
                    );
                    if let Some(delta) = to_emit {
                        let item_id = synth_item_id(turn_id, *entry_index, "reason");
                        envelopes.push(wrap(
                            BackboneEvent::ReasoningTextDelta(
                                codex::ReasoningTextDeltaNotification {
                                    thread_id: session_id.to_string(),
                                    turn_id: turn_id.to_string(),
                                    item_id,
                                    delta,
                                    content_index: 0,
                                },
                            ),
                            *entry_index,
                        ));
                    }
                }

                // 补发 agent text 残余增量
                if !text.is_empty() {
                    let key = chunk_stream_key(turn_id, *entry_index, "agent_message");
                    let state = chunk_emit_states.get(&key).cloned().unwrap_or_default();
                    let to_emit =
                        reconcile_chunk(&state, &text, turn_id, *entry_index, "agent_message");
                    if let Some(delta) = to_emit {
                        let item_id = synth_item_id(turn_id, *entry_index, "msg");
                        envelopes.push(wrap(
                            BackboneEvent::AgentMessageDelta(
                                codex::AgentMessageDeltaNotification {
                                    thread_id: session_id.to_string(),
                                    turn_id: turn_id.to_string(),
                                    item_id,
                                    delta,
                                },
                            ),
                            *entry_index,
                        ));
                    }
                }

                // 对 MessageEnd 里出现的新 tool_call，补发 ItemStarted
                for tool_call in tool_calls {
                    let (_state, created) = upsert_tool_call_state(
                        tool_call_states,
                        entry_index,
                        &tool_call.id,
                        tool_call.name.clone(),
                        Some(tool_call.arguments.clone()),
                    );
                    if created {
                        let item_id = tool_result_item_id(
                            &runtime_context,
                            turn_id,
                            &tool_call.id,
                            &_state.tool_name,
                            None,
                        );
                        let item = if is_shell_exec(&_state.tool_name) {
                            make_shell_exec_item(
                                &item_id,
                                &_state,
                                codex::CommandExecutionStatus::InProgress,
                                None,
                                None,
                            )
                        } else {
                            make_dynamic_tool_item(
                                &item_id,
                                &_state,
                                codex::DynamicToolCallStatus::InProgress,
                                None,
                                None,
                            )
                        };
                        envelopes.push(wrap(
                            BackboneEvent::ItemStarted(ItemStartedNotification::new(
                                item,
                                session_id.to_string(),
                                turn_id.to_string(),
                            )),
                            _state.entry_index,
                        ));
                    }
                }

                let has_streamable_content = content.iter().any(|part| {
                    part.extract_text().is_some() || part.extract_reasoning().is_some()
                });
                let message_entry_index = *entry_index;

                // 终态承载助手正文 / reasoning：turn 收尾落 durable ItemCompleted，
                // 使重放不再依赖逐条 text delta（delta 仍保留作 live UI snapshot）。
                // 复用与 delta 相同的 item_id 与 message_entry_index，让前端能并入同一气泡。
                if !text.is_empty() {
                    let item_id = synth_item_id(turn_id, message_entry_index, "msg");
                    let item: AgentDashThreadItem = codex::ThreadItem::AgentMessage {
                        id: item_id,
                        text: text.clone(),
                        phase: None,
                        memory_citation: None,
                    }
                    .into();
                    envelopes.push(wrap(
                        BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                            item,
                            session_id.to_string(),
                            turn_id.to_string(),
                        )),
                        message_entry_index,
                    ));
                }
                if !reasoning_text.is_empty() {
                    let item_id = synth_item_id(turn_id, message_entry_index, "reason");
                    let item: AgentDashThreadItem = codex::ThreadItem::Reasoning {
                        id: item_id,
                        summary: vec![],
                        content: vec![reasoning_text.clone()],
                    }
                    .into();
                    envelopes.push(wrap(
                        BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                            item,
                            session_id.to_string(),
                            turn_id.to_string(),
                        )),
                        message_entry_index,
                    ));
                }

                if has_streamable_content || !tool_calls.is_empty() {
                    *entry_index += 1;
                }
                if !matches!(
                    stop_reason,
                    Some(agentdash_agent::StopReason::Error | agentdash_agent::StopReason::Aborted)
                ) && let Some(usage) = usage.as_ref()
                {
                    envelopes.push(wrap(
                        BackboneEvent::TokenUsageUpdated(token_usage_notification_from_usage(
                            session_id,
                            turn_id,
                            usage,
                            runtime_context,
                        )),
                        message_entry_index,
                    ));
                }
                return envelopes;
            }
            Vec::new()
        }

        AgentEvent::ContextCompactionStarted { item_id } => {
            vec![wrap(
                BackboneEvent::ItemStarted(ItemStartedNotification::new(
                    make_context_compaction_item(item_id),
                    session_id.to_string(),
                    turn_id.to_string(),
                )),
                *entry_index,
            )]
        }

        AgentEvent::ContextCompactionNoop {
            item_id,
            reason,
            metadata,
        } => vec![wrap(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "context_compaction_noop".to_string(),
                value: serde_json::json!({
                    "lifecycle_item_id": item_id,
                    "status": "noop",
                    "noop_reason": reason,
                    "trigger": &metadata.trigger,
                    "reason": &metadata.reason,
                    "phase": &metadata.phase,
                    "strategy": &metadata.strategy,
                    "implementation": &metadata.implementation,
                    "request_id": &metadata.request_id,
                    "metadata": metadata,
                }),
            }),
            *entry_index,
        )],

        AgentEvent::ContextCompactionFailed {
            item_id,
            error,
            metadata,
        } => {
            let metadata_value = metadata
                .as_ref()
                .map(|metadata| serde_json::to_value(metadata).unwrap_or(serde_json::Value::Null));
            vec![
                wrap(
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                        key: "context_compaction_failed".to_string(),
                        value: serde_json::json!({
                            "lifecycle_item_id": item_id.clone(),
                            "status": "failed",
                            "error": error.clone(),
                            "metadata": metadata_value,
                        }),
                    }),
                    *entry_index,
                ),
                wrap(
                    BackboneEvent::Error(error_notification(
                        session_id,
                        turn_id,
                        error,
                        None,
                        Some(format!("context_compaction_item_id={item_id}")),
                    )),
                    *entry_index,
                ),
            ]
        }

        AgentEvent::ContextCompacted {
            item_id,
            messages,
            compacted_until_ref,
            first_kept_ref,
            metadata,
            newly_compacted_messages,
            ..
        } => {
            let Some(AgentMessage::CompactionSummary {
                summary,
                tokens_before,
                messages_compacted,
                timestamp,
                ..
            }) = messages.first()
            else {
                return Vec::new();
            };

            vec![
                wrap(
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                        key: "context_compacted".to_string(),
                        value: serde_json::json!({
                            "lifecycle_item_id": item_id,
                            "summary": summary,
                            "tokens_before": tokens_before,
                            "messages_compacted": messages_compacted,
                            "newly_compacted_messages": newly_compacted_messages,
                            "compacted_until_ref": compacted_until_ref,
                            "first_kept_ref": first_kept_ref,
                            "trigger": &metadata.trigger,
                            "reason": &metadata.reason,
                            "phase": &metadata.phase,
                            "strategy": &metadata.strategy,
                            "implementation": &metadata.implementation,
                            "request_id": &metadata.request_id,
                            "timestamp_ms": timestamp,
                        }),
                    }),
                    *entry_index,
                ),
                wrap(
                    BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                        make_context_compaction_item(item_id),
                        session_id.to_string(),
                        turn_id.to_string(),
                    )),
                    *entry_index,
                ),
            ]
        }

        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let item_id =
                tool_result_item_id(&runtime_context, turn_id, tool_call_id, tool_name, None);
            let item = if is_shell_exec(tool_name) {
                make_shell_exec_item(
                    &item_id,
                    &state,
                    codex::CommandExecutionStatus::InProgress,
                    None,
                    None,
                )
            } else {
                make_dynamic_tool_item(
                    &item_id,
                    &state,
                    codex::DynamicToolCallStatus::InProgress,
                    None,
                    None,
                )
            };
            vec![wrap(
                BackboneEvent::ItemStarted(ItemStartedNotification::new(
                    item,
                    session_id.to_string(),
                    turn_id.to_string(),
                )),
                state.entry_index,
            )]
        }

        AgentEvent::ToolExecutionUpdate {
            tool_call_id,
            tool_name,
            args,
            partial_result,
            ..
        } => {
            let details_type = partial_result_details_type(partial_result);
            let is_shell_output = details_type == Some("shell_output");
            let is_vfs_uri_rewrite =
                is_shell_exec(tool_name) && details_type == Some("vfs_uri_rewrite");
            if is_shell_output
                && let Some(identity) = runtime_context.session_identity.as_ref()
                && let Some(terminal_id) = partial_result_terminal_id(partial_result)
            {
                let _terminal_ref = identity.terminal_ref(terminal_id);
            }

            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );

            if is_shell_output || is_vfs_uri_rewrite {
                let item_id = tool_result_item_id(
                    &runtime_context,
                    turn_id,
                    tool_call_id,
                    tool_name,
                    Some(partial_result),
                );
                let delta = partial_result_text(partial_result);
                vec![wrap(
                    BackboneEvent::CommandOutputDelta(
                        codex::CommandExecutionOutputDeltaNotification {
                            thread_id: session_id.to_string(),
                            turn_id: turn_id.to_string(),
                            item_id,
                            delta,
                        },
                    ),
                    state.entry_index,
                )]
            } else {
                let content_items = decode_tool_result_to_content_items(partial_result);
                let item_id = tool_result_item_id(
                    &runtime_context,
                    turn_id,
                    tool_call_id,
                    tool_name,
                    Some(partial_result),
                );
                let item = make_dynamic_tool_item(
                    &item_id,
                    &state,
                    codex::DynamicToolCallStatus::InProgress,
                    content_items,
                    None,
                );
                vec![wrap(
                    BackboneEvent::ItemUpdated(ItemUpdatedNotification::new(
                        item,
                        session_id.to_string(),
                        turn_id.to_string(),
                    )),
                    state.entry_index,
                )]
            }
        }

        AgentEvent::ToolExecutionPendingApproval {
            tool_call_id,
            tool_name,
            args,
            reason,
            details,
            ..
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            vec![wrap(
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "approval_requested".to_string(),
                    value: serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "reason": reason,
                        "args": args,
                        "details": details,
                        "entry_index": state.entry_index,
                    }),
                }),
                state.entry_index,
            )]
        }

        AgentEvent::ToolExecutionApprovalResolved {
            tool_call_id,
            tool_name,
            args,
            approved,
            reason,
            ..
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            vec![wrap(
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "approval_resolved".to_string(),
                    value: serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "approved": approved,
                        "reason": reason,
                        "args": args,
                        "entry_index": state.entry_index,
                    }),
                }),
                state.entry_index,
            )]
        }

        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            is_error,
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                None,
            );
            let item_id = tool_result_item_id(
                &runtime_context,
                turn_id,
                tool_call_id,
                tool_name,
                Some(result),
            );

            let item = if is_shell_exec(tool_name) {
                let exit_code = shell_exit_code_from_result(result);
                let aggregated_output = tool_result_text(result);
                let status = shell_command_status_from_result(result, *is_error);
                make_shell_exec_item(&item_id, &state, status, aggregated_output, exit_code)
            } else {
                let content_items = decode_tool_result_to_content_items(result);
                let success = Some(!is_error);
                let status = if *is_error {
                    codex::DynamicToolCallStatus::Failed
                } else {
                    codex::DynamicToolCallStatus::Completed
                };
                make_dynamic_tool_item(&item_id, &state, status, content_items, success)
            };

            vec![wrap(
                BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                    item,
                    session_id.to_string(),
                    turn_id.to_string(),
                )),
                state.entry_index,
            )]
        }

        _ => Vec::new(),
    }
}

/// 调和 MessageEnd 终态文本与已发送增量链路的差异，只产出真正的残余增量。
fn reconcile_chunk(
    state: &ChunkEmitState,
    full_text: &str,
    turn_id: &str,
    entry_index: u32,
    kind: &str,
) -> Option<String> {
    if state.seen_delta {
        if full_text == state.emitted_text {
            None
        } else if full_text.starts_with(state.emitted_text.as_str()) {
            let suffix = &full_text[state.emitted_text.len()..];
            if suffix.is_empty() {
                None
            } else {
                Some(suffix.to_string())
            }
        } else {
            diag!(Warn, Subsystem::AgentRun,

                turn_id = %turn_id,
                entry_index = entry_index,
                kind = kind,
                "MessageEnd text 与已发送增量不一致，已忽略终态快照"
            );
            None
        }
    } else {
        Some(full_text.to_string())
    }
}

fn decode_tool_result_to_content_items(
    value: &serde_json::Value,
) -> Option<Vec<codex::DynamicToolCallOutputContentItem>> {
    let result = decode_tool_result(value)?;
    let mut items: Vec<codex::DynamicToolCallOutputContentItem> = result
        .content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } => {
                Some(codex::DynamicToolCallOutputContentItem::InputText { text: text.clone() })
            }
            ContentPart::Image { mime_type, data } => {
                Some(codex::DynamicToolCallOutputContentItem::InputImage {
                    image_url: format!("data:{mime_type};base64,{data}"),
                })
            }
            ContentPart::Reasoning { .. } => None,
        })
        .collect();

    if is_companion_subagent_dispatch_result(value)
        && let Some(details) = tool_result_details(value)
    {
        items.push(codex::DynamicToolCallOutputContentItem::InputText {
            text: serde_json::json!({ "details": details }).to_string(),
        });
    }

    if items.is_empty() { None } else { Some(items) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_result_image_content_uses_data_url_for_codex_protocol() {
        let value = serde_json::json!({
            "content": [
                { "type": "image", "mime_type": "image/png", "data": "AAECAw==" }
            ],
            "is_error": false,
            "details": null
        });

        let items = decode_tool_result_to_content_items(&value).expect("content items");
        assert_eq!(items.len(), 1);
        match &items[0] {
            codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                assert_eq!(image_url, "data:image/png;base64,AAECAw==");
            }
            other => panic!("expected image item, got {other:?}"),
        }
    }

    #[test]
    fn companion_subagent_dispatch_result_preserves_structured_details_for_ui() {
        let value = serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "Companion agent completed: 已完成检查"
                }
            ],
            "is_error": false,
            "details": {
                "kind": "companion_subagent_dispatch",
                "child": {
                    "agent_id": "agent-child-1"
                },
                "journal": {
                    "uri": "lifecycle://agent-runs/agent-child-1/sessions/messages"
                },
                "status": "completed",
                "summary": "Child agent completed",
                "result_preview": "已完成检查"
            }
        });

        let items = decode_tool_result_to_content_items(&value).expect("content items");
        assert_eq!(items.len(), 2);
        match &items[1] {
            codex::DynamicToolCallOutputContentItem::InputText { text } => {
                let payload: serde_json::Value = serde_json::from_str(text).expect("json payload");
                assert_eq!(
                    payload
                        .get("details")
                        .and_then(|details| details.get("kind"))
                        .and_then(serde_json::Value::as_str),
                    Some("companion_subagent_dispatch")
                );
                assert_eq!(
                    payload
                        .get("details")
                        .and_then(|details| details.get("result_preview"))
                        .and_then(serde_json::Value::as_str),
                    Some("已完成检查")
                );
                assert!(
                    !text.contains("delivery_runtime_session_id"),
                    "UI protocol payload must not expose delivery runtime session ids"
                );
            }
            other => panic!("expected details text item, got {other:?}"),
        }
    }
}
