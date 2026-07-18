use agentdash_diagnostics::{Subsystem, diag};
use std::collections::HashMap;
use std::{
    iter::Peekable,
    str::Chars,
    sync::{Arc, RwLock},
};

use agentdash_agent_core::{
    AgentEvent, AgentMessage, AgentRunError, AgentRunErrorKind, AgentToolResult, ContentPart,
    ReadableBodyKind, ReadableToolResultRef, TokenUsage, ToolResultAddressProvider,
};
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::generated::codex_v2::server_notification::CodexErrorInfo;
use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, ShellExecExecutionMode,
};
use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, ItemCompletedNotification, ItemStartedNotification,
    ItemUpdatedNotification, PlatformEvent, ProviderAttemptPhase as ProtocolProviderAttemptPhase,
    ProviderAttemptStatus as ProtocolProviderAttemptStatus, RuntimeTerminalDiagnostic, SourceInfo,
    ThreadTokenUsage, ThreadTokenUsageUpdatedNotification, TraceInfo, backbone::thread_item,
};
use agentdash_agent_protocol::{ContextUsageSource, NormalizedContextUsage, TokenUsageBreakdown};
use agentdash_agent_runtime_contract::{ToolPresentationEmitter, ToolProtocolProjection};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum NativePresentationError {
    #[error("native tool `{tool_name}` cannot be projected as {family}: {reason}")]
    InvalidToolProjection {
        tool_name: String,
        family: &'static str,
        reason: String,
    },
}

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
pub(crate) struct ChunkEmitState {
    emitted_text: String,
    seen_delta: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolCallEmitState {
    entry_index: u32,
    tool_name: String,
    raw_input: Option<serde_json::Value>,
    /// The owner route is pinned by the first event for this logical call. Surface hot-replace
    /// may change later calls, but must not change the family or producer halfway through one
    /// started/updated/completed lifecycle.
    presentation_route: NativeToolPresentationRoute,
}

#[derive(Debug, Clone)]
pub(crate) struct StreamMapperRuntimeContext {
    pub model_context_window: Option<u64>,
    pub reserve_tokens: u64,
    pub session_identity: Arc<NativeSessionItemIdentity>,
    pub fixed_event_timestamp_ms: Option<i64>,
    pub tool_presentation_routes: Arc<RwLock<HashMap<String, NativeToolPresentationRoute>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NativeToolPresentationRoute {
    pub projection: ToolProtocolProjection,
    pub emitter: ToolPresentationEmitter,
}

#[derive(Debug, Default)]
pub(crate) struct NativeSessionItemIdentity {
    state: RwLock<NativeSessionItemIdentityState>,
    presentation_routes: RwLock<Option<Arc<RwLock<HashMap<String, NativeToolPresentationRoute>>>>>,
}

#[derive(Debug, Default)]
struct NativeSessionItemIdentityState {
    turn_aliases: HashMap<String, String>,
    body_aliases: HashMap<(ReadableBodyKind, String), String>,
    body_kinds: HashMap<(String, String), ReadableBodyKind>,
    next_turn: usize,
    next_tool: usize,
    next_command: usize,
}

impl NativeSessionItemIdentity {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn bind_presentation_routes(
        &self,
        routes: Arc<RwLock<HashMap<String, NativeToolPresentationRoute>>>,
    ) {
        *self
            .presentation_routes
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(routes);
    }

    fn body_kind_for_tool(&self, tool_name: &str) -> ReadableBodyKind {
        self.presentation_routes
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .and_then(|routes| {
                routes
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(tool_name)
                    .map(|route| route.projection.clone())
            })
            .filter(|projection| matches!(projection, ToolProtocolProjection::Command))
            .map_or(ReadableBodyKind::Tool, |_| ReadableBodyKind::Command)
    }

    pub(crate) fn observe_tool_result_item_id(&self, item_id: &str) {
        let Some((turn, kind, body)) = parse_tool_result_item_id(item_id) else {
            return;
        };
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.next_turn = state.next_turn.max(turn);
        match kind {
            ReadableBodyKind::Tool => state.next_tool = state.next_tool.max(body),
            ReadableBodyKind::Command => state.next_command = state.next_command.max(body),
        }
    }

    /// Hydrate readable lifecycle watermarks from a durable provider transcript before allocating
    /// any new Native vendor item or ToolResultRef address.
    pub(crate) fn observe_messages(&self, messages: &[AgentMessage]) {
        for message in messages {
            match message {
                AgentMessage::Assistant { tool_calls, .. } => {
                    for tool_call in tool_calls {
                        self.observe_tool_result_item_id(&tool_call.id);
                    }
                }
                AgentMessage::ToolResult {
                    tool_call_id,
                    details,
                    ..
                } => {
                    self.observe_tool_result_item_id(tool_call_id);
                    if let Some(item_id) = details
                        .as_ref()
                        .and_then(|details| details.get("readable_ref"))
                        .and_then(|readable_ref| readable_ref.get("item_id"))
                        .and_then(serde_json::Value::as_str)
                    {
                        self.observe_tool_result_item_id(item_id);
                    }
                }
                AgentMessage::User { .. } | AgentMessage::CompactionSummary { .. } => {}
            }
        }
    }

    fn tool_result_ref_with_kind(
        &self,
        raw_turn_id: &str,
        raw_tool_call_id: &str,
        kind: ReadableBodyKind,
    ) -> ReadableToolResultRef {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let turn_alias = if let Some(alias) = state.turn_aliases.get(raw_turn_id) {
            alias.clone()
        } else {
            state.next_turn += 1;
            let alias = format_readable_alias("turn", state.next_turn);
            state
                .turn_aliases
                .insert(raw_turn_id.to_string(), alias.clone());
            alias
        };
        let kind = *state
            .body_kinds
            .entry((raw_turn_id.to_string(), raw_tool_call_id.to_string()))
            .or_insert(kind);
        let key = (kind, raw_tool_call_id.to_string());
        let body_alias = if let Some(alias) = state.body_aliases.get(&key) {
            alias.clone()
        } else {
            let (prefix, index) = match kind {
                ReadableBodyKind::Tool => {
                    state.next_tool += 1;
                    ("tool", state.next_tool)
                }
                ReadableBodyKind::Command => {
                    state.next_command += 1;
                    ("cmd", state.next_command)
                }
            };
            let alias = format_readable_alias(prefix, index);
            state.body_aliases.insert(key, alias.clone());
            alias
        };
        let item_id = format!("{turn_alias}:{body_alias}");
        ReadableToolResultRef {
            raw_turn_id: raw_turn_id.to_string(),
            raw_tool_call_id: raw_tool_call_id.to_string(),
            turn_alias: turn_alias.clone(),
            body_alias: body_alias.clone(),
            body_kind: kind,
            lifecycle_path: format!(
                "lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt"
            ),
            item_id,
        }
    }

    pub(crate) fn tool_presentation_item_id(
        &self,
        raw_turn_id: &str,
        raw_tool_call_id: &str,
        projection: &ToolProtocolProjection,
    ) -> String {
        let kind = if matches!(projection, ToolProtocolProjection::Command) {
            ReadableBodyKind::Command
        } else {
            ReadableBodyKind::Tool
        };
        self.tool_result_ref_with_kind(raw_turn_id, raw_tool_call_id, kind)
            .item_id
    }
}

impl ToolResultAddressProvider for NativeSessionItemIdentity {
    fn tool_result_ref(
        &self,
        raw_turn_id: &str,
        raw_tool_call_id: &str,
        tool_name: &str,
    ) -> ReadableToolResultRef {
        let kind = self.body_kind_for_tool(tool_name);
        self.tool_result_ref_with_kind(raw_turn_id, raw_tool_call_id, kind)
    }
}

fn format_readable_alias(prefix: &str, index: usize) -> String {
    if index < 1000 {
        format!("{prefix}_{index:03}")
    } else {
        format!("{prefix}_{index}")
    }
}

fn parse_tool_result_item_id(item_id: &str) -> Option<(usize, ReadableBodyKind, usize)> {
    let (turn_alias, body_alias) = item_id.split_once(':')?;
    let parse_alias = |alias: &str, prefix: &str| {
        alias
            .strip_prefix(prefix)?
            .strip_prefix('_')?
            .parse::<usize>()
            .ok()
            .filter(|index| *index > 0)
    };
    let turn = parse_alias(turn_alias, "turn")?;
    if let Some(body) = parse_alias(body_alias, "tool") {
        return Some((turn, ReadableBodyKind::Tool, body));
    }
    parse_alias(body_alias, "cmd").map(|body| (turn, ReadableBodyKind::Command, body))
}

pub(crate) struct StreamMapperEventState<'a> {
    pub entry_index: &'a mut u32,
    pub chunk_emit_states: &'a mut HashMap<String, ChunkEmitState>,
    pub tool_call_states: &'a mut HashMap<String, ToolCallEmitState>,
}

fn chunk_stream_key(turn_id: &str, entry_index: u32, chunk_kind: &str) -> String {
    format!("{turn_id}:{entry_index}:{chunk_kind}")
}

fn upsert_tool_call_state(
    runtime_context: &StreamMapperRuntimeContext,
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
    entry_index: &mut u32,
    tool_call_id: &str,
    tool_name: String,
    raw_input: Option<serde_json::Value>,
) -> Result<(ToolCallEmitState, bool), NativePresentationError> {
    if let Some(existing) = tool_call_states.get_mut(tool_call_id) {
        if !tool_name.trim().is_empty() && existing.tool_name != tool_name {
            return Err(NativePresentationError::InvalidToolProjection {
                tool_name,
                family: "presentation_route",
                reason: format!(
                    "logical tool call `{tool_call_id}` changed its owner name from `{}`",
                    existing.tool_name
                ),
            });
        }
        if let Some(raw_input) = raw_input {
            existing.raw_input = Some(raw_input);
        }
        return Ok((existing.clone(), false));
    }

    let presentation_route = runtime_context
        .tool_presentation_routes
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&tool_name)
        .cloned()
        .ok_or_else(|| NativePresentationError::InvalidToolProjection {
            tool_name: tool_name.clone(),
            family: "presentation_route",
            reason: "effective Native tool surface has no owner-declared presentation route"
                .to_string(),
        })?;
    let state = ToolCallEmitState {
        entry_index: *entry_index,
        tool_name,
        raw_input,
        presentation_route,
    };
    tool_call_states.insert(tool_call_id.to_string(), state.clone());
    Ok((state, true))
}

fn upsert_state_from_tool_name(
    runtime_context: &StreamMapperRuntimeContext,
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
    entry_index: &mut u32,
    tool_call_id: &str,
    tool_name: &str,
    raw_input: Option<serde_json::Value>,
) -> Result<(ToolCallEmitState, bool), NativePresentationError> {
    upsert_tool_call_state(
        runtime_context,
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
) -> Option<&'a agentdash_agent_core::ToolCallInfo> {
    match message {
        AgentMessage::Assistant { tool_calls, .. } => tool_calls
            .iter()
            .find(|tool_call| tool_call.id == tool_call_id),
        _ => None,
    }
}

fn upsert_state_from_message(
    runtime_context: &StreamMapperRuntimeContext,
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
    entry_index: &mut u32,
    message: &AgentMessage,
    tool_call_id: &str,
    fallback_name: &str,
) -> Result<(ToolCallEmitState, bool), NativePresentationError> {
    if let Some(tool_call) = message_tool_call_info(message, tool_call_id) {
        return upsert_tool_call_state(
            runtime_context,
            tool_call_states,
            entry_index,
            tool_call_id,
            tool_call.name.clone(),
            Some(tool_call.arguments.clone()),
        );
    }
    upsert_state_from_tool_name(
        runtime_context,
        tool_call_states,
        entry_index,
        tool_call_id,
        fallback_name,
        None,
    )
}

fn tool_result_item_id(
    runtime_context: &StreamMapperRuntimeContext,
    turn_id: &str,
    tool_call_id: &str,
    projection: &ToolProtocolProjection,
    result: Option<&serde_json::Value>,
) -> String {
    if let Some(item_id) = result.and_then(tool_result_item_id_from_details) {
        return item_id;
    }
    let kind = if matches!(projection, ToolProtocolProjection::Command) {
        ReadableBodyKind::Command
    } else {
        ReadableBodyKind::Tool
    };
    runtime_context
        .session_identity
        .tool_result_ref_with_kind(turn_id, tool_call_id, kind)
        .item_id
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
    phase: agentdash_agent_core::ProviderAttemptPhase,
) -> ProtocolProviderAttemptPhase {
    match phase {
        agentdash_agent_core::ProviderAttemptPhase::Connecting => {
            ProtocolProviderAttemptPhase::Connecting
        }
        agentdash_agent_core::ProviderAttemptPhase::ConnectedWaitingFirstDelta => {
            ProtocolProviderAttemptPhase::ConnectedWaitingFirstDelta
        }
        agentdash_agent_core::ProviderAttemptPhase::Streaming => {
            ProtocolProviderAttemptPhase::Streaming
        }
        agentdash_agent_core::ProviderAttemptPhase::RetryScheduled => {
            ProtocolProviderAttemptPhase::RetryScheduled
        }
        agentdash_agent_core::ProviderAttemptPhase::Retrying => {
            ProtocolProviderAttemptPhase::Retrying
        }
        agentdash_agent_core::ProviderAttemptPhase::Failed => ProtocolProviderAttemptPhase::Failed,
        agentdash_agent_core::ProviderAttemptPhase::Succeeded => {
            ProtocolProviderAttemptPhase::Succeeded
        }
    }
}

fn provider_attempt_status_to_protocol(
    status: &agentdash_agent_core::ProviderAttemptStatus,
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

pub(crate) fn run_error_terminal_diagnostic(error: &AgentRunError) -> RuntimeTerminalDiagnostic {
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
    codex_error_info: Option<CodexErrorInfo>,
    additional_details: Option<String>,
) -> codex::ErrorNotification {
    codex::ErrorNotification {
        error: codex::TurnError {
            message: message.to_string(),
            // The generated Codex protocol distinguishes an omitted field from an
            // explicitly present JSON null. Native presentation owns the exported
            // protocol body, so retain field presence even when either value is null.
            codex_error_info: Some(codex_error_info),
            additional_details: Some(additional_details),
        },
        will_retry: false,
        thread_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
    }
}

fn run_error_codex_error_info(error: &AgentRunError) -> CodexErrorInfo {
    if error.aborted {
        return CodexErrorInfo::Other;
    }
    match error.http_status {
        Some(401 | 403) => CodexErrorInfo::Unauthorized,
        Some(400 | 422) => CodexErrorInfo::BadRequest,
        Some(500..=599) => CodexErrorInfo::InternalServerError,
        _ => match (error.kind, error.code.as_deref()) {
            (AgentRunErrorKind::HookBlocked, _) => CodexErrorInfo::BadRequest,
            (
                _,
                Some("auth_error" | "unauthorized" | "invalid_api_key" | "invalid_credentials"),
            ) => CodexErrorInfo::Unauthorized,
            (_, Some("invalid_request" | "invalid_request_error")) => CodexErrorInfo::BadRequest,
            (_, Some("provider_5xx")) => CodexErrorInfo::InternalServerError,
            (_, Some("timeout" | "rate_limited" | "transient_provider_error")) => {
                CodexErrorInfo::ResponseStreamConnectionFailed {
                    http_status_code: Some(error.http_status),
                }
            }
            _ => CodexErrorInfo::Other,
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
) -> Result<(String, Option<String>, ShellExecExecutionMode), NativePresentationError> {
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NativePresentationError::InvalidToolProjection {
            tool_name: "shell_exec".to_string(),
            family: "command",
            reason: "typed command arguments require a string `command` field".to_string(),
        })?
        .to_string();
    let raw_cwd = args
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match raw_cwd {
        None => Ok((
            command,
            Some("platform://".to_string()),
            ShellExecExecutionMode::Platform,
        )),
        Some(cwd) if cwd.starts_with("platform://") => Ok((
            command,
            Some(cwd.to_string()),
            ShellExecExecutionMode::Platform,
        )),
        Some(cwd) => Ok((
            command,
            Some(cwd.to_string()),
            ShellExecExecutionMode::MountExec,
        )),
    }
}

fn partial_result_details_type(partial_result: &serde_json::Value) -> Option<&str> {
    partial_result
        .get("details")
        .and_then(|d| d.get("type"))
        .and_then(|t| t.as_str())
}

fn decode_tool_result(
    value: &serde_json::Value,
    tool_name: &str,
    family: &'static str,
) -> Result<AgentToolResult, NativePresentationError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        NativePresentationError::InvalidToolProjection {
            tool_name: tool_name.to_string(),
            family,
            reason: format!("malformed typed tool result: {error}"),
        }
    })
}

fn decode_tool_result_lossy(
    value: &serde_json::Value,
    tool_name: &str,
    family: &'static str,
    is_error: bool,
) -> AgentToolResult {
    match decode_tool_result(value, tool_name, family) {
        Ok(result) => result,
        Err(error) => {
            diag!(
                Warn,
                Subsystem::AgentRun,
                tool_name = tool_name,
                family = family,
                reason = %error,
                "Native tool result不是完整typed payload，保留lossy presentation"
            );
            let content = decode_dynamic_tool_content(value).unwrap_or_else(|| {
                value
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .map(|text| vec![ContentPart::text(text)])
                    .unwrap_or_else(|| {
                        if value.is_null() {
                            Vec::new()
                        } else {
                            vec![ContentPart::text(value.to_string())]
                        }
                    })
            });
            AgentToolResult {
                content,
                is_error,
                details: value.get("details").cloned(),
            }
        }
    }
}

fn decode_dynamic_tool_content(value: &serde_json::Value) -> Option<Vec<ContentPart>> {
    let items = serde_json::from_value::<Vec<codex::DynamicToolCallOutputContentItem>>(
        value.get("content_items")?.clone(),
    )
    .ok()?;
    Some(
        items
            .into_iter()
            .map(|item| match item {
                codex::DynamicToolCallOutputContentItem::InputText { text } => {
                    ContentPart::text(text)
                }
                codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                    ContentPart::image("image/*", image_url)
                }
            })
            .collect(),
    )
}

fn tool_result_text(
    result: &AgentToolResult,
    tool_name: &str,
    family: &'static str,
) -> Result<Option<String>, NativePresentationError> {
    let mut text = Vec::new();
    for part in &result.content {
        match part {
            ContentPart::Text { text: value } => text.push(value.as_str()),
            ContentPart::Image { .. } | ContentPart::Reasoning { .. } => {
                diag!(
                    Warn,
                    Subsystem::AgentRun,
                    tool_name = tool_name,
                    family = family,
                    "Native text-only tool presentation过滤非文本result part"
                );
            }
        }
    }
    let text = text.join("\n");
    Ok((!text.is_empty()).then_some(text))
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

fn shell_exit_code_from_result(
    value: &serde_json::Value,
    result: &AgentToolResult,
    tool_name: &str,
) -> Result<Option<i32>, NativePresentationError> {
    let output_text = tool_result_text(result, tool_name, "command")?;
    let exit_code = tool_result_details(value)
        .and_then(|details| details.get("exit_code"))
        .and_then(|exit_code| exit_code.as_i64())
        .and_then(|exit_code| i32::try_from(exit_code).ok())
        .or_else(|| {
            output_text.and_then(|text| {
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
        });
    Ok(exit_code)
}

fn shell_command_status_from_result(
    exit_code: Option<i32>,
    is_error: bool,
) -> codex::CommandExecutionStatus {
    if is_error || exit_code.is_some_and(|exit_code| exit_code != 0) {
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
    args: serde_json::Value,
    status: codex::CommandExecutionStatus,
    aggregated_output: Option<String>,
    exit_code: Option<i32>,
) -> Result<AgentDashThreadItem, NativePresentationError> {
    let (command, cwd, execution_mode) = extract_shell_args(&args)?;
    Ok(AgentDashNativeThreadItem::ShellExec {
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
    .into())
}

fn project_tool_item(
    item_id: &str,
    state: &ToolCallEmitState,
    projection: &ToolProtocolProjection,
    status: codex::DynamicToolCallStatus,
    content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
    success: Option<bool>,
    command: Option<(codex::CommandExecutionStatus, Option<String>, Option<i32>)>,
) -> Result<AgentDashThreadItem, NativePresentationError> {
    match project_tool_item_strict(
        item_id,
        state,
        projection,
        status,
        content_items.clone(),
        success,
        command.clone(),
    ) {
        Ok(item) => Ok(item),
        Err(error) => {
            diag!(
                Warn,
                Subsystem::AgentRun,
                tool_name = state.tool_name,
                projection = ?projection,
                reason = %error,
                "Native typed tool presentation尚不完整，按main语义保留dynamic lifecycle"
            );
            Ok(main_dynamic_tool_call(
                item_id,
                state.tool_name.clone(),
                state
                    .raw_input
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({})),
                status,
                content_items,
                success,
                None,
            )
            .into())
        }
    }
}

fn project_tool_item_strict(
    item_id: &str,
    state: &ToolCallEmitState,
    projection: &ToolProtocolProjection,
    status: codex::DynamicToolCallStatus,
    content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
    success: Option<bool>,
    command: Option<(codex::CommandExecutionStatus, Option<String>, Option<i32>)>,
) -> Result<AgentDashThreadItem, NativePresentationError> {
    let arguments = state
        .raw_input
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let item = match projection {
        ToolProtocolProjection::Command => {
            let (status, aggregated_output, exit_code) =
                command.ok_or_else(|| NativePresentationError::InvalidToolProjection {
                    tool_name: state.tool_name.clone(),
                    family: "command",
                    reason: "command projection requires command lifecycle state".to_string(),
                })?;
            make_shell_exec_item(item_id, arguments, status, aggregated_output, exit_code)?
        }
        ToolProtocolProjection::FileChange => make_apply_patch_file_change_item(
            item_id,
            &arguments,
            patch_apply_status_from_dynamic(&status),
            &state.tool_name,
        )?,
        ToolProtocolProjection::FsRead => AgentDashNativeThreadItem::FsRead {
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
        ToolProtocolProjection::FsGrep => AgentDashNativeThreadItem::FsGrep {
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
        ToolProtocolProjection::FsGlob => AgentDashNativeThreadItem::FsGlob {
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
        ToolProtocolProjection::Mcp { server_key } => {
            let (result, error) = match status {
                codex::DynamicToolCallStatus::Completed => {
                    (serde_json::json!(content_items), serde_json::Value::Null)
                }
                codex::DynamicToolCallStatus::Failed => (
                    serde_json::Value::Null,
                    serde_json::json!({ "message": "native MCP tool execution failed" }),
                ),
                codex::DynamicToolCallStatus::InProgress => {
                    (serde_json::Value::Null, serde_json::Value::Null)
                }
            };
            serde_json::from_value(serde_json::json!({
                "type": "mcpToolCall",
                "id": item_id,
                "server": server_key,
                "tool": state.tool_name,
                "arguments": arguments,
                "status": status,
                "result": result,
                "error": error
            }))
            .map_err(|error| NativePresentationError::InvalidToolProjection {
                tool_name: state.tool_name.clone(),
                family: "mcp",
                reason: error.to_string(),
            })?
        }
        ToolProtocolProjection::Dynamic { namespace } => main_dynamic_tool_call(
            item_id,
            state.tool_name.clone(),
            arguments,
            status,
            content_items,
            success,
            namespace.clone(),
        )
        .into(),
    };
    Ok(item)
}

fn main_dynamic_tool_call(
    id: impl Into<String>,
    tool: impl Into<String>,
    arguments: serde_json::Value,
    status: codex::DynamicToolCallStatus,
    content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
    success: Option<bool>,
    namespace: Option<String>,
) -> codex::ThreadItem {
    codex::ThreadItem::DynamicToolCall {
        id: id.into(),
        namespace: Some(namespace),
        tool: tool.into(),
        arguments,
        status,
        content_items: Some(content_items),
        success: Some(success),
        duration_ms: Some(None),
    }
}

fn make_apply_patch_file_change_item(
    item_id: &str,
    arguments: &serde_json::Value,
    status: codex::PatchApplyStatus,
    tool_name: &str,
) -> Result<AgentDashThreadItem, NativePresentationError> {
    let patch = string_arg(arguments, "patch").ok_or_else(|| {
        NativePresentationError::InvalidToolProjection {
            tool_name: tool_name.to_string(),
            family: "file_change",
            reason: "apply-patch arguments require a string `patch` field".to_string(),
        }
    })?;
    let changes = parse_apply_patch_specs(&patch).map_err(|reason| {
        NativePresentationError::InvalidToolProjection {
            tool_name: tool_name.to_string(),
            family: "file_change",
            reason,
        }
    })?;
    if changes.is_empty() {
        return Err(NativePresentationError::InvalidToolProjection {
            tool_name: tool_name.to_string(),
            family: "file_change",
            reason: "apply-patch payload contains no file changes".to_string(),
        });
    }
    thread_item::file_change(item_id, changes, status)
        .map(Into::into)
        .map_err(|error| NativePresentationError::InvalidToolProjection {
            tool_name: tool_name.to_string(),
            family: "file_change",
            reason: error.to_string(),
        })
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
    runtime_context: &StreamMapperRuntimeContext,
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

pub(crate) fn convert_event_to_envelopes_with_runtime_context(
    event: &AgentEvent,
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    state: StreamMapperEventState<'_>,
    runtime_context: StreamMapperRuntimeContext,
) -> Result<Vec<BackboneEnvelope>, NativePresentationError> {
    let StreamMapperEventState {
        entry_index,
        chunk_emit_states,
        tool_call_states,
    } = state;
    let wrap = |mut event: BackboneEvent, idx: u32| {
        if let Some(timestamp_ms) = runtime_context.fixed_event_timestamp_ms {
            match &mut event {
                BackboneEvent::ItemStarted(notification) => {
                    notification.started_at_ms = timestamp_ms;
                }
                BackboneEvent::ItemUpdated(notification) => {
                    notification.updated_at_ms = timestamp_ms;
                }
                BackboneEvent::ItemCompleted(notification) => {
                    notification.completed_at_ms = timestamp_ms;
                }
                _ => {}
            }
        }
        make_envelope(event, session_id, source, turn_id, idx)
    };

    Ok(match event {
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
            agentdash_agent_core::types::AssistantStreamEvent::ToolCallStart {
                tool_call_id,
                name,
                ..
            } => {
                let (state, created) = upsert_state_from_message(
                    &runtime_context,
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                )?;
                if !matches!(
                    state.presentation_route.emitter,
                    ToolPresentationEmitter::VendorStream
                ) {
                    return Ok(Vec::new());
                }
                if !created {
                    return Ok(Vec::new());
                }
                let projection = state.presentation_route.projection.clone();
                let item_id =
                    tool_result_item_id(&runtime_context, turn_id, tool_call_id, &projection, None);
                let command = matches!(projection, ToolProtocolProjection::Command).then_some((
                    codex::CommandExecutionStatus::InProgress,
                    None,
                    None,
                ));
                let item = project_tool_item(
                    &item_id,
                    &state,
                    &projection,
                    codex::DynamicToolCallStatus::InProgress,
                    None,
                    None,
                    command,
                )?;
                vec![wrap(
                    BackboneEvent::ItemStarted(ItemStartedNotification::new(
                        item,
                        session_id.to_string(),
                        turn_id.to_string(),
                    )),
                    state.entry_index,
                )]
            }
            agentdash_agent_core::types::AssistantStreamEvent::ToolCallDelta {
                tool_call_id,
                name,
                draft,
                is_parseable,
                ..
            } => {
                let (state, _) = upsert_state_from_message(
                    &runtime_context,
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                )?;
                if !matches!(
                    state.presentation_route.emitter,
                    ToolPresentationEmitter::VendorStream
                ) {
                    return Ok(Vec::new());
                }
                let projection = state.presentation_route.projection.clone();
                let args = if matches!(projection, ToolProtocolProjection::FileChange) {
                    apply_patch_preview_args_from_draft(draft, *is_parseable)
                } else {
                    parse_tool_call_args_from_draft(draft, *is_parseable)
                };
                let Some(args) = args else {
                    return Ok(Vec::new());
                };
                let (state, _) = upsert_tool_call_state(
                    &runtime_context,
                    tool_call_states,
                    entry_index,
                    tool_call_id,
                    state.tool_name,
                    Some(args),
                )?;
                let item_id =
                    tool_result_item_id(&runtime_context, turn_id, tool_call_id, &projection, None);
                let command = matches!(projection, ToolProtocolProjection::Command).then_some((
                    codex::CommandExecutionStatus::InProgress,
                    None,
                    None,
                ));
                let item = project_tool_item(
                    &item_id,
                    &state,
                    &projection,
                    codex::DynamicToolCallStatus::InProgress,
                    None,
                    None,
                    command,
                )?;
                vec![wrap(
                    BackboneEvent::ItemUpdated(ItemUpdatedNotification::new(
                        item,
                        session_id.to_string(),
                        turn_id.to_string(),
                    )),
                    state.entry_index,
                )]
            }
            agentdash_agent_core::types::AssistantStreamEvent::ToolCallEnd {
                tool_call, ..
            } => {
                let (state, _) = upsert_tool_call_state(
                    &runtime_context,
                    tool_call_states,
                    entry_index,
                    &tool_call.id,
                    tool_call.name.clone(),
                    Some(tool_call.arguments.clone()),
                )?;
                if !matches!(
                    state.presentation_route.emitter,
                    ToolPresentationEmitter::VendorStream
                ) {
                    return Ok(Vec::new());
                }
                Vec::new()
            }
            agentdash_agent_core::types::AssistantStreamEvent::TextDelta { text, .. } => {
                if text.is_empty() {
                    return Ok(Vec::new());
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
            agentdash_agent_core::types::AssistantStreamEvent::ThinkingDelta { text, .. } => {
                if text.is_empty() {
                    return Ok(Vec::new());
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
                if matches!(stop_reason, Some(agentdash_agent_core::StopReason::Aborted)) {
                    return Ok(Vec::new());
                }
                if let Some(error_message) = error_message {
                    let error =
                        AgentRunError::new(AgentRunErrorKind::Unknown, error_message.clone())
                            .with_code(Some("assistant_error_message".to_string()));
                    return Ok(vec![wrap(
                        BackboneEvent::Error(run_error_notification(session_id, turn_id, &error)),
                        *entry_index,
                    )]);
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
                    let (state, created) = upsert_tool_call_state(
                        &runtime_context,
                        tool_call_states,
                        entry_index,
                        &tool_call.id,
                        tool_call.name.clone(),
                        Some(tool_call.arguments.clone()),
                    )?;
                    if !matches!(
                        state.presentation_route.emitter,
                        ToolPresentationEmitter::VendorStream
                    ) {
                        continue;
                    }
                    if created {
                        let projection = state.presentation_route.projection.clone();
                        let item_id = tool_result_item_id(
                            &runtime_context,
                            turn_id,
                            &tool_call.id,
                            &projection,
                            None,
                        );
                        let command = matches!(projection, ToolProtocolProjection::Command)
                            .then_some((codex::CommandExecutionStatus::InProgress, None, None));
                        let item = project_tool_item(
                            &item_id,
                            &state,
                            &projection,
                            codex::DynamicToolCallStatus::InProgress,
                            None,
                            None,
                            command,
                        )?;
                        envelopes.push(wrap(
                            BackboneEvent::ItemStarted(ItemStartedNotification::new(
                                item,
                                session_id.to_string(),
                                turn_id.to_string(),
                            )),
                            state.entry_index,
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
                        phase: Some(None),
                        memory_citation: Some(None),
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
                    Some(
                        agentdash_agent_core::StopReason::Error
                            | agentdash_agent_core::StopReason::Aborted
                    )
                ) && let Some(usage) = usage.as_ref()
                {
                    envelopes.push(wrap(
                        BackboneEvent::TokenUsageUpdated(token_usage_notification_from_usage(
                            session_id,
                            turn_id,
                            usage,
                            &runtime_context,
                        )),
                        message_entry_index,
                    ));
                }
                return Ok(envelopes);
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
                return Ok(Vec::new());
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
            let (state, created) = upsert_state_from_tool_name(
                &runtime_context,
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            )?;
            if !matches!(
                state.presentation_route.emitter,
                ToolPresentationEmitter::VendorStream
            ) {
                return Ok(Vec::new());
            }
            if !created {
                return Ok(Vec::new());
            }
            let projection = state.presentation_route.projection.clone();
            let item_id =
                tool_result_item_id(&runtime_context, turn_id, tool_call_id, &projection, None);
            let command = matches!(projection, ToolProtocolProjection::Command).then_some((
                codex::CommandExecutionStatus::InProgress,
                None,
                None,
            ));
            let item = project_tool_item(
                &item_id,
                &state,
                &projection,
                codex::DynamicToolCallStatus::InProgress,
                None,
                None,
                command,
            )?;
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
            let (state, _) = upsert_state_from_tool_name(
                &runtime_context,
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            )?;
            if !matches!(
                state.presentation_route.emitter,
                ToolPresentationEmitter::VendorStream
            ) {
                return Ok(Vec::new());
            }
            let projection = state.presentation_route.projection.clone();
            let is_vfs_uri_rewrite = matches!(projection, ToolProtocolProjection::Command)
                && details_type == Some("vfs_uri_rewrite");

            if matches!(projection, ToolProtocolProjection::Command)
                && (is_shell_output || is_vfs_uri_rewrite)
            {
                let item_id = tool_result_item_id(
                    &runtime_context,
                    turn_id,
                    tool_call_id,
                    &projection,
                    Some(partial_result),
                );
                let result =
                    decode_tool_result_lossy(partial_result, tool_name, "command_output", false);
                let delta =
                    tool_result_text(&result, tool_name, "command_output")?.ok_or_else(|| {
                        NativePresentationError::InvalidToolProjection {
                            tool_name: tool_name.clone(),
                            family: "command_output",
                            reason: "command output delta requires non-empty text content"
                                .to_string(),
                        }
                    })?;
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
                let result =
                    decode_tool_result_lossy(partial_result, tool_name, "tool_result", false);
                let content_items =
                    decode_tool_result_to_content_items(partial_result, &result, tool_name)?;
                let item_id = tool_result_item_id(
                    &runtime_context,
                    turn_id,
                    tool_call_id,
                    &projection,
                    Some(partial_result),
                );
                let command = matches!(projection, ToolProtocolProjection::Command).then_some((
                    codex::CommandExecutionStatus::InProgress,
                    None,
                    None,
                ));
                let item = project_tool_item(
                    &item_id,
                    &state,
                    &projection,
                    codex::DynamicToolCallStatus::InProgress,
                    content_items,
                    None,
                    command,
                )?;
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
                &runtime_context,
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            )?;
            if !matches!(
                state.presentation_route.emitter,
                ToolPresentationEmitter::VendorStream
            ) {
                return Ok(Vec::new());
            }
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
                &runtime_context,
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            )?;
            if !matches!(
                state.presentation_route.emitter,
                ToolPresentationEmitter::VendorStream
            ) {
                return Ok(Vec::new());
            }
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
                &runtime_context,
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                None,
            )?;
            if !matches!(
                state.presentation_route.emitter,
                ToolPresentationEmitter::VendorStream
            ) {
                return Ok(Vec::new());
            }
            let projection = state.presentation_route.projection.clone();
            let item_id = tool_result_item_id(
                &runtime_context,
                turn_id,
                tool_call_id,
                &projection,
                Some(result),
            );

            let decoded_result =
                decode_tool_result_lossy(result, tool_name, "tool_result", *is_error);
            let command = if matches!(projection, ToolProtocolProjection::Command) {
                let exit_code = shell_exit_code_from_result(result, &decoded_result, tool_name)?;
                let aggregated_output = tool_result_text(&decoded_result, tool_name, "command")?;
                let status = shell_command_status_from_result(exit_code, *is_error);
                Some((status, aggregated_output, exit_code))
            } else {
                None
            };
            let content_items =
                decode_tool_result_to_content_items(result, &decoded_result, tool_name)?;
            let success = Some(!is_error);
            let status = if *is_error {
                codex::DynamicToolCallStatus::Failed
            } else {
                codex::DynamicToolCallStatus::Completed
            };
            let item = project_tool_item(
                &item_id,
                &state,
                &projection,
                status,
                content_items,
                success,
                command,
            )?;

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
    })
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
    result: &AgentToolResult,
    tool_name: &str,
) -> Result<Option<Vec<codex::DynamicToolCallOutputContentItem>>, NativePresentationError> {
    let mut items = Vec::new();
    for part in &result.content {
        let item = match part {
            ContentPart::Text { text } => {
                codex::DynamicToolCallOutputContentItem::InputText { text: text.clone() }
            }
            ContentPart::Image { mime_type, data } => {
                codex::DynamicToolCallOutputContentItem::InputImage {
                    image_url: format!("data:{mime_type};base64,{data}"),
                }
            }
            ContentPart::Reasoning { .. } => {
                diag!(
                    Warn,
                    Subsystem::AgentRun,
                    tool_name = tool_name,
                    "Native tool result reasoning part无公开协议投影，已从presentation过滤"
                );
                continue;
            }
        };
        items.push(item);
    }

    if is_companion_subagent_dispatch_result(value)
        && let Some(details) = tool_result_details(value)
    {
        items.push(codex::DynamicToolCallOutputContentItem::InputText {
            text: serde_json::json!({ "details": details }).to_string(),
        });
    }

    Ok((!items.is_empty()).then_some(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_tool_item_ids_advance_session_identity_watermarks() {
        let identity = NativeSessionItemIdentity::new();
        identity.bind_presentation_routes(Arc::new(RwLock::new(HashMap::from([
            (
                "read".to_string(),
                NativeToolPresentationRoute {
                    projection: ToolProtocolProjection::FsRead,
                    emitter: ToolPresentationEmitter::VendorStream,
                },
            ),
            (
                "owner_command_alias".to_string(),
                NativeToolPresentationRoute {
                    projection: ToolProtocolProjection::Command,
                    emitter: ToolPresentationEmitter::VendorStream,
                },
            ),
        ]))));
        identity.observe_tool_result_item_id("turn_004:tool_007");
        identity.observe_tool_result_item_id("turn_003:cmd_002");

        assert_eq!(
            identity
                .tool_result_ref("raw-turn", "raw-tool", "read")
                .item_id,
            "turn_005:tool_008"
        );
        assert_eq!(
            identity
                .tool_result_ref("raw-turn-2", "raw-command", "owner_command_alias")
                .item_id,
            "turn_006:cmd_003"
        );
    }

    #[test]
    fn tool_result_image_content_uses_data_url_for_codex_protocol() {
        let value = serde_json::json!({
            "content": [
                { "type": "image", "mime_type": "image/png", "data": "AAECAw==" }
            ],
            "is_error": false,
            "details": null
        });

        let result = decode_tool_result(&value, "image_tool", "tool_result").expect("decode");
        let items = decode_tool_result_to_content_items(&value, &result, "image_tool")
            .expect("project")
            .expect("content items");
        assert_eq!(items.len(), 1);
        match &items[0] {
            codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                assert_eq!(image_url, "data:image/png;base64,AAECAw==");
            }
            other => panic!("expected image item, got {other:?}"),
        }
    }

    #[test]
    fn broker_envelope_projects_typed_content_instead_of_single_line_json() {
        let value = serde_json::json!({
            "module_id": "canvas:cvs-canvas",
            "content_items": [{
                "type": "inputText",
                "text": "模块展示请求已提交\n\n- 模块：`canvas:cvs-canvas`\n- 视图：`preview`"
            }]
        });

        let result =
            decode_tool_result_lossy(&value, "workspace_module_present", "tool_result", false);
        assert_eq!(
            result.content,
            vec![ContentPart::text(
                "模块展示请求已提交\n\n- 模块：`canvas:cvs-canvas`\n- 视图：`preview`"
            )]
        );
        let items =
            decode_tool_result_to_content_items(&value, &result, "workspace_module_present")
                .expect("project")
                .expect("content items");
        assert_eq!(
            items,
            vec![codex::DynamicToolCallOutputContentItem::InputText {
                text: "模块展示请求已提交\n\n- 模块：`canvas:cvs-canvas`\n- 视图：`preview`"
                    .to_string(),
            }]
        );
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

        let result = decode_tool_result(&value, "subagent", "tool_result").expect("decode");
        let items = decode_tool_result_to_content_items(&value, &result, "subagent")
            .expect("project")
            .expect("content items");
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

    #[test]
    fn malformed_tool_result_is_a_typed_presentation_error() {
        let error = decode_tool_result(
            &serde_json::json!({"content": "not-an-array"}),
            "read",
            "tool_result",
        )
        .expect_err("malformed result must not disappear");

        assert!(matches!(
            error,
            NativePresentationError::InvalidToolProjection {
                tool_name,
                family: "tool_result",
                ..
            } if tool_name == "read"
        ));
    }

    #[test]
    fn incomplete_typed_tool_arguments_preserve_main_projection_semantics() {
        let missing_arguments = ToolCallEmitState {
            entry_index: 0,
            tool_name: "read".to_string(),
            raw_input: None,
            presentation_route: NativeToolPresentationRoute {
                projection: ToolProtocolProjection::FsRead,
                emitter: ToolPresentationEmitter::VendorStream,
            },
        };
        let missing_arguments = project_tool_item(
            "item-1",
            &missing_arguments,
            &ToolProtocolProjection::FsRead,
            codex::DynamicToolCallStatus::InProgress,
            None,
            None,
            None,
        )
        .expect("partial stream arguments remain displayable");
        assert!(matches!(
            missing_arguments,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::FsRead { path, .. })
                if path.is_empty()
        ));

        let missing_path = ToolCallEmitState {
            entry_index: 0,
            tool_name: "read".to_string(),
            raw_input: Some(serde_json::json!({})),
            presentation_route: NativeToolPresentationRoute {
                projection: ToolProtocolProjection::FsRead,
                emitter: ToolPresentationEmitter::VendorStream,
            },
        };
        let missing_path = project_tool_item(
            "item-2",
            &missing_path,
            &ToolProtocolProjection::FsRead,
            codex::DynamicToolCallStatus::InProgress,
            None,
            None,
            None,
        )
        .expect("partial fs_read arguments remain displayable");
        assert!(matches!(
            missing_path,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::FsRead { path, .. })
                if path.is_empty()
        ));

        let missing_pattern = ToolCallEmitState {
            entry_index: 0,
            tool_name: "fs_glob".to_string(),
            raw_input: Some(serde_json::json!({})),
            presentation_route: NativeToolPresentationRoute {
                projection: ToolProtocolProjection::FsGlob,
                emitter: ToolPresentationEmitter::VendorStream,
            },
        };
        let missing_pattern = project_tool_item(
            "item-3",
            &missing_pattern,
            &ToolProtocolProjection::FsGlob,
            codex::DynamicToolCallStatus::InProgress,
            None,
            None,
            None,
        )
        .expect("main keeps the typed fs_glob lifecycle when pattern is absent");
        assert!(matches!(
            missing_pattern,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::FsGlob { pattern, .. })
                if pattern.is_empty()
        ));
    }

    #[test]
    fn reasoning_tool_result_is_filtered_without_breaking_tool_lifecycle() {
        let value = serde_json::json!({
            "content": [{ "type": "reasoning", "text": "private", "id": null, "signature": null }],
            "is_error": false,
            "details": null
        });
        let result = decode_tool_result(&value, "read", "tool_result").expect("decode");

        assert_eq!(
            decode_tool_result_to_content_items(&value, &result, "read")
                .expect("reasoning-only tool result remains non-critical"),
            None
        );
    }
}
