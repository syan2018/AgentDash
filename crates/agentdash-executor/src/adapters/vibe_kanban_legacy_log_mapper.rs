use std::collections::HashMap;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, ItemCompletedNotification, ItemStartedNotification,
    PlatformEvent, SourceInfo, TraceInfo,
    backbone::thread_item::{self as builder, FileChangeSpec},
};
use codex_app_server_protocol as codex;
use executors::{
    approvals::ToolCallMetadata,
    logs::{ActionType, FileChange, NormalizedEntry, NormalizedEntryType, ToolStatus},
};

/// vibe-kanban legacy `NormalizedEntry` → BackboneEnvelope 转换器。
///
/// 将 vibe-kanban executors crate 的 legacy normalized log 映射到 AgentDash Backbone。
#[derive(Debug)]
pub struct VibeKanbanLogToBackboneConverter {
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

impl VibeKanbanLogToBackboneConverter {
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
                        BackboneEvent::ReasoningTextDelta(codex::ReasoningTextDeltaNotification {
                            thread_id: self.session_id.clone(),
                            turn_id: self.turn_id.clone(),
                            item_id: self.synth_item_id(entry_index, "reason"),
                            delta: d,
                            content_index: 0,
                        }),
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
                    BackboneEvent::TokenUsageUpdated(codex::ThreadTokenUsageUpdatedNotification {
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
                    }),
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

        let item = legacy_action_type_to_thread_item(
            action_type,
            tool_name,
            status,
            item_id,
            &entry.content,
        );

        let is_new = !self.tool_started.contains_key(&tool_call_id);
        if is_new {
            self.tool_started.insert(tool_call_id, true);
            vec![self.wrap(
                BackboneEvent::ItemStarted(ItemStartedNotification::new(
                    item,
                    self.session_id.clone(),
                    self.turn_id.clone(),
                )),
                entry_index,
            )]
        } else if matches!(
            status,
            ToolStatus::Success
                | ToolStatus::Failed
                | ToolStatus::TimedOut
                | ToolStatus::Denied { .. }
        ) {
            vec![self.wrap(
                BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                    item,
                    self.session_id.clone(),
                    self.turn_id.clone(),
                )),
                entry_index,
            )]
        } else {
            // 中间更新 — 作为新的 ItemStarted 覆盖（Codex 协议无 item update 概念）
            vec![self.wrap(
                BackboneEvent::ItemStarted(ItemStartedNotification::new(
                    item,
                    self.session_id.clone(),
                    self.turn_id.clone(),
                )),
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

fn tool_call_id_from_entry(
    turn_prefix: &str,
    entry_index: usize,
    entry: &NormalizedEntry,
) -> String {
    if let Some(meta) = entry.metadata.as_ref()
        && let Ok(parsed) = serde_json::from_value::<ToolCallMetadata>(meta.clone())
        && !parsed.tool_call_id.trim().is_empty()
    {
        return parsed.tool_call_id;
    }
    format!("tool-{}-{}", turn_prefix, entry_index)
}

/// 把 vibe-kanban legacy `ActionType` 投影到对应的 codex `ThreadItem` variant。
///
/// 设计意图：上游 `executors` 已把工具语义 normalize 出 `FileRead/FileEdit/CommandRun/Search/...`
/// 等分支，本函数据此把语义类型还原到 `ThreadItem` 已有的对应 variant，避免所有
/// 工具退化为 `DynamicToolCall`。前端 SessionEntry 现有的 variant 路由因此能拿到
/// 真实流量。
///
/// 没有专用 ThreadItem variant 的 ActionType（FileRead/WebFetch/TaskCreate/AskUserQuestion/Other）
/// 仍走 DynamicToolCall，但 `tool` 名按工具语义规范化（如 `Read`/`WebFetch`），
/// 让前端二级分发能识别。
fn legacy_action_type_to_thread_item(
    action_type: &ActionType,
    tool_name: &str,
    status: &ToolStatus,
    item_id: String,
    fallback_content: &str,
) -> codex::ThreadItem {
    // 失败时退回 dynamic tool call 兜底（保留完整 ActionType payload 供前端兜底渲染）。
    let fallback = || -> codex::ThreadItem {
        legacy_dynamic_tool_call(
            &item_id,
            tool_name,
            serde_json::to_value(action_type).unwrap_or_default(),
            dynamic_tool_call_status(status),
            fallback_content,
            tool_success(status),
        )
    };

    match action_type {
        ActionType::CommandRun {
            command, result, ..
        } => {
            let exit_code: Option<i32> =
                result
                    .as_ref()
                    .and_then(|r| r.exit_status.as_ref())
                    .map(|es| match es {
                        executors::logs::CommandExitStatus::ExitCode { code } => *code,
                        executors::logs::CommandExitStatus::Success { success } => {
                            if *success {
                                0
                            } else {
                                1
                            }
                        }
                    });
            let aggregated_output = result.as_ref().and_then(|r| r.output.clone()).or_else(|| {
                if fallback_content.is_empty() {
                    None
                } else {
                    Some(fallback_content.to_string())
                }
            });
            builder::command_execution(
                &item_id,
                command.clone(),
                ".",
                command_execution_status(status),
                aggregated_output,
                exit_code,
            )
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to build CommandExecution from ActionType::CommandRun: {e}");
                fallback()
            })
        }

        ActionType::FileEdit { path, changes } => {
            let specs: Vec<FileChangeSpec> = changes
                .iter()
                .map(|c| executors_change_to_spec(path, c))
                .collect();
            builder::file_change(&item_id, specs, patch_apply_status(status)).unwrap_or_else(|e| {
                tracing::warn!("Failed to build FileChange from ActionType::FileEdit: {e}");
                fallback()
            })
        }

        ActionType::Search { query } => builder::web_search(&item_id, query.clone())
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to build WebSearch from ActionType::Search: {e}");
                fallback()
            }),

        // 没有专用 ThreadItem variant 的语义：保持 DynamicToolCall，但工具名规范化以
        // 让前端二级分发能识别。
        ActionType::FileRead { path } => legacy_dynamic_tool_call(
            item_id,
            "Read",
            serde_json::json!({ "path": path }),
            dynamic_tool_call_status(status),
            fallback_content,
            tool_success(status),
        ),
        ActionType::WebFetch { url } => legacy_dynamic_tool_call(
            item_id,
            "WebFetch",
            serde_json::json!({ "url": url }),
            dynamic_tool_call_status(status),
            fallback_content,
            tool_success(status),
        ),
        ActionType::AskUserQuestion { questions } => legacy_dynamic_tool_call(
            item_id,
            "AskUserQuestion",
            serde_json::to_value(questions).unwrap_or_default(),
            dynamic_tool_call_status(status),
            fallback_content,
            tool_success(status),
        ),
        ActionType::TaskCreate {
            description,
            subagent_type,
            ..
        } => legacy_dynamic_tool_call(
            item_id,
            "Task",
            serde_json::json!({
                "description": description,
                "subagent_type": subagent_type,
            }),
            dynamic_tool_call_status(status),
            fallback_content,
            tool_success(status),
        ),
        ActionType::Other { description } => legacy_dynamic_tool_call(
            item_id,
            "Other",
            serde_json::json!({ "description": description }),
            dynamic_tool_call_status(status),
            fallback_content,
            tool_success(status),
        ),

        // 通用 Tool：用 ActionType::Tool 内部的 tool_name 作为工具名。
        ActionType::Tool {
            tool_name: inner_tool,
            ..
        } => builder::dynamic_tool_call(
            item_id,
            inner_tool,
            serde_json::to_value(action_type).unwrap_or_default(),
            dynamic_tool_call_status(status),
            extract_content_items(action_type, fallback_content),
            tool_success(status),
        ),

        // PlanPresentation/TodoManagement 在 tool_use_envelopes 上游已被 SessionMetaUpdate
        // 分流，不会进入此函数；保留 fallback 防御。
        ActionType::PlanPresentation { .. } | ActionType::TodoManagement { .. } => {
            legacy_dynamic_tool_call(
                item_id,
                tool_name,
                serde_json::to_value(action_type).unwrap_or_default(),
                dynamic_tool_call_status(status),
                fallback_content,
                tool_success(status),
            )
        }
    }
}

fn legacy_dynamic_tool_call(
    item_id: impl Into<String>,
    tool_name: impl Into<String>,
    arguments: serde_json::Value,
    status: codex::DynamicToolCallStatus,
    fallback_content: &str,
    success: Option<bool>,
) -> codex::ThreadItem {
    let content_items = if fallback_content.is_empty() {
        None
    } else {
        Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
            text: fallback_content.to_string(),
        }])
    };
    builder::dynamic_tool_call(
        item_id,
        tool_name,
        arguments,
        status,
        content_items,
        success,
    )
}

/// 把 vibe-kanban `executors::FileChange` 子枚举投影成 builder 的 [`FileChangeSpec`]。
fn executors_change_to_spec(path: &str, change: &FileChange) -> FileChangeSpec {
    match change {
        FileChange::Edit { unified_diff, .. } => FileChangeSpec::Edit {
            path: path.to_string(),
            unified_diff: unified_diff.clone(),
        },
        FileChange::Write { .. } => FileChangeSpec::Add {
            path: path.to_string(),
            diff: String::new(),
        },
        FileChange::Delete => FileChangeSpec::Delete {
            path: path.to_string(),
        },
        FileChange::Rename { new_path } => FileChangeSpec::Rename {
            path: path.to_string(),
            new_path: new_path.clone(),
            diff: String::new(),
        },
    }
}

fn dynamic_tool_call_status(status: &ToolStatus) -> codex::DynamicToolCallStatus {
    match status {
        ToolStatus::Created | ToolStatus::PendingApproval { .. } => {
            codex::DynamicToolCallStatus::InProgress
        }
        ToolStatus::Success => codex::DynamicToolCallStatus::Completed,
        ToolStatus::Failed | ToolStatus::TimedOut | ToolStatus::Denied { .. } => {
            codex::DynamicToolCallStatus::Failed
        }
    }
}

fn command_execution_status(status: &ToolStatus) -> codex::CommandExecutionStatus {
    match status {
        ToolStatus::Created | ToolStatus::PendingApproval { .. } => {
            codex::CommandExecutionStatus::InProgress
        }
        ToolStatus::Success => codex::CommandExecutionStatus::Completed,
        ToolStatus::Denied { .. } => codex::CommandExecutionStatus::Declined,
        ToolStatus::Failed | ToolStatus::TimedOut => codex::CommandExecutionStatus::Failed,
    }
}

fn patch_apply_status(status: &ToolStatus) -> codex::PatchApplyStatus {
    match status {
        ToolStatus::Created | ToolStatus::PendingApproval { .. } => {
            codex::PatchApplyStatus::InProgress
        }
        ToolStatus::Success => codex::PatchApplyStatus::Completed,
        ToolStatus::Denied { .. } => codex::PatchApplyStatus::Declined,
        ToolStatus::Failed | ToolStatus::TimedOut => codex::PatchApplyStatus::Failed,
    }
}

fn tool_success(status: &ToolStatus) -> Option<bool> {
    match status {
        ToolStatus::Success => Some(true),
        ToolStatus::Failed | ToolStatus::TimedOut | ToolStatus::Denied { .. } => Some(false),
        _ => None,
    }
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
    Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
        text,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use executors::logs::{CommandExitStatus, CommandRunResult};

    fn run(action_type: ActionType, status: ToolStatus) -> codex::ThreadItem {
        legacy_action_type_to_thread_item(&action_type, "tool", &status, "id-1".to_string(), "")
    }

    #[test]
    fn command_run_maps_to_command_execution() {
        let item = run(
            ActionType::CommandRun {
                command: "ls -la".to_string(),
                result: Some(CommandRunResult {
                    exit_status: Some(CommandExitStatus::ExitCode { code: 0 }),
                    output: Some("output".to_string()),
                }),
                category: Default::default(),
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::CommandExecution {
                command,
                status,
                exit_code,
                aggregated_output,
                ..
            } => {
                assert_eq!(command, "ls -la");
                assert!(matches!(status, codex::CommandExecutionStatus::Completed));
                assert_eq!(exit_code, Some(0));
                assert_eq!(aggregated_output.as_deref(), Some("output"));
            }
            other => panic!("expected CommandExecution, got {other:?}"),
        }
    }

    #[test]
    fn command_run_failed_status_maps_to_failed() {
        let item = run(
            ActionType::CommandRun {
                command: "false".to_string(),
                result: Some(CommandRunResult {
                    exit_status: Some(CommandExitStatus::ExitCode { code: 1 }),
                    output: None,
                }),
                category: Default::default(),
            },
            ToolStatus::Failed,
        );

        match item {
            codex::ThreadItem::CommandExecution {
                status, exit_code, ..
            } => {
                assert!(matches!(status, codex::CommandExecutionStatus::Failed));
                assert_eq!(exit_code, Some(1));
            }
            other => panic!("expected CommandExecution, got {other:?}"),
        }
    }

    #[test]
    fn file_edit_with_unified_diff_maps_to_file_change_update() {
        let item = run(
            ActionType::FileEdit {
                path: "src/foo.rs".to_string(),
                changes: vec![FileChange::Edit {
                    unified_diff: "@@ -1 +1 @@\n-old\n+new".to_string(),
                    has_line_numbers: true,
                }],
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::FileChange {
                changes, status, ..
            } => {
                assert!(matches!(status, codex::PatchApplyStatus::Completed));
                assert_eq!(changes.len(), 1);
                assert!(matches!(
                    changes[0].kind,
                    codex::PatchChangeKind::Update { move_path: None }
                ));
                assert_eq!(changes[0].diff, "@@ -1 +1 @@\n-old\n+new");
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn file_edit_with_write_maps_to_add() {
        let item = run(
            ActionType::FileEdit {
                path: "src/new.rs".to_string(),
                changes: vec![FileChange::Write {
                    content: "fn main() {}".to_string(),
                }],
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::FileChange { changes, .. } => {
                assert!(matches!(changes[0].kind, codex::PatchChangeKind::Add));
                assert!(changes[0].diff.is_empty());
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn file_edit_with_delete_maps_to_delete() {
        let item = run(
            ActionType::FileEdit {
                path: "src/gone.rs".to_string(),
                changes: vec![FileChange::Delete],
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::FileChange { changes, .. } => {
                assert!(matches!(changes[0].kind, codex::PatchChangeKind::Delete));
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn file_edit_with_rename_carries_move_path() {
        let item = run(
            ActionType::FileEdit {
                path: "src/old.rs".to_string(),
                changes: vec![FileChange::Rename {
                    new_path: "src/new.rs".to_string(),
                }],
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::FileChange { changes, .. } => {
                let move_path = match &changes[0].kind {
                    codex::PatchChangeKind::Update { move_path } => move_path.clone(),
                    other => panic!("expected Update kind, got {other:?}"),
                };
                let move_path = move_path.expect("Rename should carry move_path");
                assert_eq!(move_path.to_string_lossy(), "src/new.rs");
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn search_maps_to_web_search() {
        let item = run(
            ActionType::Search {
                query: "rust async".to_string(),
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::WebSearch { query, .. } => {
                assert_eq!(query, "rust async");
            }
            other => panic!("expected WebSearch, got {other:?}"),
        }
    }

    #[test]
    fn file_read_falls_back_to_dynamic_tool_call_with_normalized_name() {
        let item = run(
            ActionType::FileRead {
                path: "src/main.rs".to_string(),
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::DynamicToolCall {
                tool, arguments, ..
            } => {
                assert_eq!(tool, "Read");
                assert_eq!(
                    arguments.get("path").and_then(|v| v.as_str()),
                    Some("src/main.rs")
                );
            }
            other => panic!("expected DynamicToolCall, got {other:?}"),
        }
    }

    #[test]
    fn dynamic_fallback_preserves_fallback_content() {
        let action_type = ActionType::FileRead {
            path: "src/main.rs".to_string(),
        };
        let item = legacy_action_type_to_thread_item(
            &action_type,
            "tool",
            &ToolStatus::Success,
            "id-1".to_string(),
            "file contents",
        );

        match item {
            codex::ThreadItem::DynamicToolCall { content_items, .. } => {
                let content_items = content_items.expect("content items");
                assert_eq!(content_items.len(), 1);
                match &content_items[0] {
                    codex::DynamicToolCallOutputContentItem::InputText { text } => {
                        assert_eq!(text, "file contents");
                    }
                    other => panic!("expected InputText, got {other:?}"),
                }
            }
            other => panic!("expected DynamicToolCall, got {other:?}"),
        }
    }

    #[test]
    fn web_fetch_falls_back_to_dynamic_tool_call() {
        let item = run(
            ActionType::WebFetch {
                url: "https://example.com".to_string(),
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::DynamicToolCall { tool, .. } => {
                assert_eq!(tool, "WebFetch");
            }
            other => panic!("expected DynamicToolCall, got {other:?}"),
        }
    }

    #[test]
    fn task_create_falls_back_to_dynamic_tool_call() {
        let item = run(
            ActionType::TaskCreate {
                description: "do thing".to_string(),
                subagent_type: Some("planner".to_string()),
                result: None,
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::DynamicToolCall {
                tool, arguments, ..
            } => {
                assert_eq!(tool, "Task");
                assert_eq!(
                    arguments.get("subagent_type").and_then(|v| v.as_str()),
                    Some("planner")
                );
            }
            other => panic!("expected DynamicToolCall, got {other:?}"),
        }
    }

    #[test]
    fn other_falls_back_to_dynamic_tool_call_with_other_label() {
        let item = run(
            ActionType::Other {
                description: "obscure thing".to_string(),
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::DynamicToolCall {
                tool, arguments, ..
            } => {
                assert_eq!(tool, "Other");
                assert_eq!(
                    arguments.get("description").and_then(|v| v.as_str()),
                    Some("obscure thing")
                );
            }
            other => panic!("expected DynamicToolCall, got {other:?}"),
        }
    }

    #[test]
    fn generic_tool_keeps_inner_tool_name() {
        let item = run(
            ActionType::Tool {
                tool_name: "MyCustom".to_string(),
                arguments: Some(serde_json::json!({ "k": "v" })),
                result: None,
            },
            ToolStatus::Success,
        );

        match item {
            codex::ThreadItem::DynamicToolCall { tool, .. } => {
                assert_eq!(tool, "MyCustom");
            }
            other => panic!("expected DynamicToolCall, got {other:?}"),
        }
    }
}
