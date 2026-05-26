use std::collections::HashMap;

use agentdash_agent::{AgentEvent, AgentMessage, AgentToolResult, ContentPart};
use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use codex_app_server_protocol as codex;

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

/// 合成 item_id — pi_agent 没有原生 item id，用 turn + entry 合成。
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

/// 判断是否为 shell_exec 工具调用（应映射为 CommandExecution 而非 DynamicToolCall）
fn is_shell_exec(tool_name: &str) -> bool {
    tool_name == "shell_exec"
}

/// 从 shell_exec 的 args JSON 中提取 command / cwd（cwd 总是返回绝对路径）
fn extract_shell_args(args: &serde_json::Value) -> (String, String) {
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)")
        .to_string();
    let raw_cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
    let cwd_path = std::path::Path::new(raw_cwd);
    let cwd = if cwd_path.is_absolute() {
        raw_cwd.to_string()
    } else {
        std::env::current_dir()
            .map(|base| base.join(cwd_path).to_string_lossy().to_string())
            .unwrap_or_else(|_| raw_cwd.to_string())
    };
    (command, cwd)
}

fn partial_result_details_type(partial_result: &serde_json::Value) -> Option<&str> {
    partial_result
        .get("details")
        .and_then(|d| d.get("type"))
        .and_then(|t| t.as_str())
}

fn partial_result_text(partial_result: &serde_json::Value) -> String {
    partial_result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string()
}

/// 通过 serde 构造 CommandExecution ThreadItem（绕过 AbsolutePathBuf 未 re-export 的限制）
fn make_command_execution_item(
    item_id: &str,
    state: &ToolCallEmitState,
    status: codex::CommandExecutionStatus,
    aggregated_output: Option<String>,
    exit_code: Option<i32>,
) -> codex::ThreadItem {
    let args = state
        .raw_input
        .clone()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let (command, cwd) = extract_shell_args(&args);
    let json_val = serde_json::json!({
        "type": "commandExecution",
        "id": item_id,
        "command": command,
        "cwd": cwd,
        "processId": null,
        "source": status_to_source_str(),
        "status": status,
        "commandActions": [],
        "aggregatedOutput": aggregated_output,
        "exitCode": exit_code,
        "durationMs": null,
    });
    serde_json::from_value(json_val).unwrap_or_else(|e| {
        tracing::warn!("Failed to construct CommandExecution item via serde: {e}");
        make_dynamic_tool_item(
            item_id,
            state,
            codex::DynamicToolCallStatus::InProgress,
            None,
            None,
        )
    })
}

fn status_to_source_str() -> &'static str {
    "agent"
}

/// 构造 DynamicToolCall ThreadItem 用于 ItemStarted/ItemCompleted。
fn make_dynamic_tool_item(
    item_id: &str,
    state: &ToolCallEmitState,
    status: codex::DynamicToolCallStatus,
    content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
    success: Option<bool>,
) -> codex::ThreadItem {
    let arguments = state
        .raw_input
        .clone()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    codex::ThreadItem::DynamicToolCall {
        id: item_id.to_string(),
        namespace: None,
        tool: state.tool_name.clone(),
        arguments,
        status,
        content_items,
        success,
        duration_ms: None,
    }
}

fn make_context_compaction_item(item_id: &str) -> codex::ThreadItem {
    codex::ThreadItem::ContextCompaction {
        id: item_id.to_string(),
    }
}

pub(super) fn convert_event_to_envelopes(
    event: &AgentEvent,
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    entry_index: &mut u32,
    chunk_emit_states: &mut HashMap<String, ChunkEmitState>,
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
) -> Vec<BackboneEnvelope> {
    let wrap =
        |event: BackboneEvent, idx: u32| make_envelope(event, session_id, source, turn_id, idx);

    match event {
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
                let item_id = synth_item_id(turn_id, state.entry_index, tool_call_id);
                let item = if is_shell_exec(&state.tool_name) {
                    make_command_execution_item(
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
                    BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                        item,
                        thread_id: session_id.to_string(),
                        turn_id: turn_id.to_string(),
                    }),
                    state.entry_index,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ToolCallDelta {
                tool_call_id,
                name,
                ..
            } => {
                let (_state, _) = upsert_state_from_message(
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                );
                // 参数增量在 Codex 协议中没有对应的独立通知，仅影响最终 ItemCompleted。
                Vec::new()
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
                ..
            } = message
            {
                let reasoning_text = content
                    .iter()
                    .filter_map(ContentPart::extract_reasoning)
                    .collect::<Vec<_>>()
                    .join("");
                let text = error_message.clone().unwrap_or_else(|| {
                    content
                        .iter()
                        .filter_map(ContentPart::extract_text)
                        .collect::<Vec<_>>()
                        .join("")
                });

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
                        let item_id = synth_item_id(turn_id, _state.entry_index, &tool_call.id);
                        let item = make_dynamic_tool_item(
                            &item_id,
                            &_state,
                            codex::DynamicToolCallStatus::InProgress,
                            None,
                            None,
                        );
                        envelopes.push(wrap(
                            BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                                item,
                                thread_id: session_id.to_string(),
                                turn_id: turn_id.to_string(),
                            }),
                            _state.entry_index,
                        ));
                    }
                }

                let has_streamable_content = content.iter().any(|part| {
                    part.extract_text().is_some() || part.extract_reasoning().is_some()
                });
                if has_streamable_content || error_message.is_some() || !tool_calls.is_empty() {
                    *entry_index += 1;
                }
                return envelopes;
            }
            Vec::new()
        }

        AgentEvent::ContextCompactionStarted { item_id } => {
            vec![wrap(
                BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                    item: make_context_compaction_item(item_id),
                    thread_id: session_id.to_string(),
                    turn_id: turn_id.to_string(),
                }),
                *entry_index,
            )]
        }

        AgentEvent::ContextCompactionFailed { item_id, error } => {
            vec![
                wrap(
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                        key: "context_compaction_failed".to_string(),
                        value: serde_json::json!({
                            "lifecycle_item_id": item_id.clone(),
                            "status": "failed",
                            "error": error.clone(),
                        }),
                    }),
                    *entry_index,
                ),
                wrap(
                    BackboneEvent::Error(codex::ErrorNotification {
                        error: codex::TurnError {
                            message: error.clone(),
                            codex_error_info: None,
                            additional_details: Some(format!(
                                "context_compaction_item_id={item_id}"
                            )),
                        },
                        will_retry: false,
                        thread_id: session_id.to_string(),
                        turn_id: turn_id.to_string(),
                    }),
                    *entry_index,
                ),
            ]
        }

        AgentEvent::ContextCompacted {
            item_id,
            messages,
            compacted_until_ref,
            first_kept_ref,
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
                            "timestamp_ms": timestamp,
                        }),
                    }),
                    *entry_index,
                ),
                wrap(
                    BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                        item: make_context_compaction_item(item_id),
                        thread_id: session_id.to_string(),
                        turn_id: turn_id.to_string(),
                    }),
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
            let item_id = synth_item_id(turn_id, state.entry_index, tool_call_id);
            let item = if is_shell_exec(tool_name) {
                make_command_execution_item(
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
                BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                    item,
                    thread_id: session_id.to_string(),
                    turn_id: turn_id.to_string(),
                }),
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

            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );

            if is_shell_output || is_vfs_uri_rewrite {
                let item_id = synth_item_id(turn_id, state.entry_index, tool_call_id);
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
                let item_id = synth_item_id(turn_id, state.entry_index, tool_call_id);
                let item = make_dynamic_tool_item(
                    &item_id,
                    &state,
                    codex::DynamicToolCallStatus::InProgress,
                    content_items,
                    None,
                );
                vec![wrap(
                    BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                        item,
                        thread_id: session_id.to_string(),
                        turn_id: turn_id.to_string(),
                    }),
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
            let item_id = synth_item_id(turn_id, state.entry_index, tool_call_id);

            let item = if is_shell_exec(tool_name) {
                let exit_code = result
                    .get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.first())
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                    .and_then(|text| {
                        // exit code 通常作为 "Exit code: N" 出现在输出末尾
                        text.lines().rev().find_map(|line| {
                            line.trim()
                                .strip_prefix("Exit code: ")
                                .and_then(|s| s.parse::<i32>().ok())
                        })
                    });
                let aggregated_output = result
                    .get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.first())
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                let status = if *is_error {
                    codex::CommandExecutionStatus::Failed
                } else {
                    codex::CommandExecutionStatus::Completed
                };
                make_command_execution_item(&item_id, &state, status, aggregated_output, exit_code)
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
                BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                    item,
                    thread_id: session_id.to_string(),
                    turn_id: turn_id.to_string(),
                }),
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
            tracing::warn!(
                turn_id = %turn_id,
                entry_index = entry_index,
                kind = kind,
                "MessageEnd text 与已发送增量不一致，已忽略兜底快照"
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
    let result: AgentToolResult = serde_json::from_value(value.clone()).ok()?;
    let items: Vec<codex::DynamicToolCallOutputContentItem> = result
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
}
