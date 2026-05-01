use std::collections::HashMap;

use agentdash_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use codex_app_server_protocol as codex;
use executors::{
    approvals::ToolCallMetadata,
    logs::{ActionType, NormalizedEntry, NormalizedEntryType, ToolStatus},
};

/// NormalizedEntry → BackboneEnvelope 转换器。
///
/// 将 executors crate 的 NormalizedEntry 直接映射到 Codex 对齐的 BackboneEvent。
#[derive(Debug)]
pub struct NormalizedToBackboneConverter {
    session_id: String,
    turn_id: String,
    source: SourceInfo,
    /// 已发出的完整 agent text（去重用）
    emitted_agent: String,
    /// 已发出的完整 thought text（去重用）
    emitted_thought: String,
    /// tool_call_id → 是否已发出 ItemStarted
    tool_started: HashMap<String, bool>,
}

impl NormalizedToBackboneConverter {
    pub fn new(
        session_id: impl Into<String>,
        source: SourceInfo,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            source,
            emitted_agent: String::new(),
            emitted_thought: String::new(),
            tool_started: HashMap::new(),
        }
    }

    fn wrap(&self, event: BackboneEvent, entry_index: usize) -> BackboneEnvelope {
        BackboneEnvelope::new(event, &self.session_id, self.source.clone()).with_trace(TraceInfo {
            turn_id: Some(self.turn_id.clone()),
            entry_index: Some(entry_index as u32),
        })
    }

    fn synth_item_id(&self, entry_index: usize, suffix: &str) -> String {
        format!("{}:{}:{}", self.turn_id, entry_index, suffix)
    }

    pub fn apply(&mut self, entry_index: usize, entry: NormalizedEntry) -> Vec<BackboneEnvelope> {
        match &entry.entry_type {
            NormalizedEntryType::UserMessage => {
                self.emitted_agent.clear();
                self.emitted_thought.clear();
                // 用户消息在 Codex 协议中不作为独立 notification；
                // 通过 Platform meta update 透传（供前端展示历史）。
                if entry.content.is_empty() {
                    return Vec::new();
                }
                vec![self.wrap(
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                        key: "user_message".to_string(),
                        value: serde_json::json!({ "content": entry.content }),
                    }),
                    entry_index,
                )]
            }
            NormalizedEntryType::AssistantMessage => {
                let delta = compute_delta(&mut self.emitted_agent, &entry.content);
                match delta {
                    Some(d) => vec![self.wrap(
                        BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                            thread_id: self.session_id.clone(),
                            turn_id: self.turn_id.clone(),
                            item_id: self.synth_item_id(entry_index, "msg"),
                            delta: d,
                        }),
                        entry_index,
                    )],
                    None => Vec::new(),
                }
            }
            NormalizedEntryType::Thinking => {
                let delta = compute_delta(&mut self.emitted_thought, &entry.content);
                match delta {
                    Some(d) => vec![self.wrap(
                        BackboneEvent::ReasoningTextDelta(
                            codex::ReasoningTextDeltaNotification {
                                thread_id: self.session_id.clone(),
                                turn_id: self.turn_id.clone(),
                                item_id: self.synth_item_id(entry_index, "reason"),
                                delta: d,
                                content_index: 0,
                            },
                        ),
                        entry_index,
                    )],
                    None => Vec::new(),
                }
            }
            NormalizedEntryType::SystemMessage => {
                vec![self.wrap(
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                        key: "system_message".to_string(),
                        value: serde_json::json!({ "content": entry.content }),
                    }),
                    entry_index,
                )]
            }
            NormalizedEntryType::ErrorMessage { .. } => {
                vec![self.wrap(
                    BackboneEvent::Error(codex::ErrorNotification {
                        error: codex::TurnError {
                            message: entry.content.clone(),
                            codex_error_info: None,
                            additional_details: None,
                        },
                        will_retry: false,
                        thread_id: self.session_id.clone(),
                        turn_id: self.turn_id.clone(),
                    }),
                    entry_index,
                )]
            }
            NormalizedEntryType::UserFeedback { .. }
            | NormalizedEntryType::UserAnsweredQuestions { .. } => {
                vec![self.wrap(
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                        key: "user_feedback".to_string(),
                        value: serde_json::json!({ "content": entry.content }),
                    }),
                    entry_index,
                )]
            }
            NormalizedEntryType::Loading | NormalizedEntryType::NextAction { .. } => Vec::new(),
            NormalizedEntryType::TokenUsageInfo(info) => {
                vec![self.wrap(
                    BackboneEvent::TokenUsageUpdated(
                        codex::ThreadTokenUsageUpdatedNotification {
                            thread_id: self.session_id.clone(),
                            turn_id: self.turn_id.clone(),
                            token_usage: codex::ThreadTokenUsage {
                                total: codex::TokenUsageBreakdown {
                                    total_tokens: info.total_tokens as i64,
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    cached_input_tokens: 0,
                                    reasoning_output_tokens: 0,
                                },
                                last: codex::TokenUsageBreakdown {
                                    total_tokens: 0,
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    cached_input_tokens: 0,
                                    reasoning_output_tokens: 0,
                                },
                                model_context_window: Some(info.model_context_window as i64),
                            },
                        },
                    ),
                    entry_index,
                )]
            }
            NormalizedEntryType::ToolUse {
                tool_name,
                action_type,
                status,
            } => self.tool_use_envelopes(entry_index, tool_name, action_type, status, &entry),
        }
    }

    fn tool_use_envelopes(
        &mut self,
        entry_index: usize,
        tool_name: &str,
        action_type: &ActionType,
        status: &ToolStatus,
        entry: &NormalizedEntry,
    ) -> Vec<BackboneEnvelope> {
        // Plan 类特殊处理
        if let ActionType::PlanPresentation { plan } = action_type {
            return vec![self.wrap(
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "plan_presentation".to_string(),
                    value: serde_json::json!({ "plan": plan }),
                }),
                entry_index,
            )];
        }
        if let ActionType::TodoManagement { todos, .. } = action_type {
            return vec![self.wrap(
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "todo_management".to_string(),
                    value: serde_json::to_value(todos).unwrap_or_default(),
                }),
                entry_index,
            )];
        }

        let tool_call_id = tool_call_id_from_entry(&self.turn_id, entry_index, entry);
        let item_id = self.synth_item_id(entry_index, &tool_call_id);

        let arguments = serde_json::to_value(action_type).unwrap_or_default();

        let codex_status = match status {
            ToolStatus::Created | ToolStatus::PendingApproval { .. } => {
                codex::DynamicToolCallStatus::InProgress
            }
            ToolStatus::Success => codex::DynamicToolCallStatus::Completed,
            ToolStatus::Failed | ToolStatus::TimedOut | ToolStatus::Denied { .. } => {
                codex::DynamicToolCallStatus::Failed
            }
        };

        let success = match status {
            ToolStatus::Success => Some(true),
            ToolStatus::Failed | ToolStatus::TimedOut | ToolStatus::Denied { .. } => Some(false),
            _ => None,
        };

        let content_items = extract_content_items(action_type, &entry.content);

        let item = codex::ThreadItem::DynamicToolCall {
            id: item_id,
            tool: tool_name.to_string(),
            arguments,
            status: codex_status,
            content_items,
            success,
            duration_ms: None,
        };

        let is_new = !self.tool_started.contains_key(&tool_call_id);
        if is_new {
            self.tool_started.insert(tool_call_id, true);
            vec![self.wrap(
                BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                    item,
                    thread_id: self.session_id.clone(),
                    turn_id: self.turn_id.clone(),
                }),
                entry_index,
            )]
        } else if matches!(
            status,
            ToolStatus::Success | ToolStatus::Failed | ToolStatus::TimedOut | ToolStatus::Denied { .. }
        ) {
            vec![self.wrap(
                BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                    item,
                    thread_id: self.session_id.clone(),
                    turn_id: self.turn_id.clone(),
                }),
                entry_index,
            )]
        } else {
            // 中间更新 — 作为新的 ItemStarted 覆盖（Codex 协议无 item update 概念）
            vec![self.wrap(
                BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                    item,
                    thread_id: self.session_id.clone(),
                    turn_id: self.turn_id.clone(),
                }),
                entry_index,
            )]
        }
    }
}

/// 去重增量计算：返回 full_content 中相比已发送内容的新增部分。
fn compute_delta(emitted: &mut String, full_content: &str) -> Option<String> {
    if full_content.is_empty() {
        return None;
    }

    if full_content.starts_with(emitted.as_str()) {
        let suffix = &full_content[emitted.len()..];
        if suffix.is_empty() {
            return None;
        }
        emitted.push_str(suffix);
        return Some(suffix.to_string());
    }

    if emitted.is_empty() {
        *emitted = full_content.to_string();
        return Some(full_content.to_string());
    }

    tracing::warn!("normalized chunk not prefixed by emitted text; drop inconsistent chunk");
    None
}

fn tool_call_id_from_entry(turn_prefix: &str, entry_index: usize, entry: &NormalizedEntry) -> String {
    if let Some(meta) = entry.metadata.as_ref()
        && let Ok(parsed) = serde_json::from_value::<ToolCallMetadata>(meta.clone())
        && !parsed.tool_call_id.trim().is_empty()
    {
        return parsed.tool_call_id;
    }
    format!("tool-{}-{}", turn_prefix, entry_index)
}

fn extract_content_items(
    action_type: &ActionType,
    fallback_content: &str,
) -> Option<Vec<codex::DynamicToolCallOutputContentItem>> {
    let text = match action_type {
        ActionType::CommandRun { result, .. } => result
            .as_ref()
            .and_then(|r| r.output.clone())
            .unwrap_or_else(|| fallback_content.to_string()),
        _ => fallback_content.to_string(),
    };
    if text.is_empty() {
        return None;
    }
    Some(vec![codex::DynamicToolCallOutputContentItem::InputText { text }])
}
