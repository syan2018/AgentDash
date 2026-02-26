use std::{
    collections::HashMap,
    path::PathBuf,
};

use agent_client_protocol::{
    ContentBlock,
    ContentChunk,
    Diff,
    Plan,
    PlanEntry,
    PlanEntryPriority,
    PlanEntryStatus,
    SessionId,
    SessionNotification,
    SessionUpdate,
    TextContent,
    ToolCall,
    ToolCallContent,
    ToolCallId,
    ToolCallLocation,
    ToolCallStatus,
    ToolCallUpdate,
    ToolCallUpdateFields,
    ToolKind,
};
use executors::{
    approvals::ToolCallMetadata,
    logs::{ActionType, FileChange, NormalizedEntry, NormalizedEntryType, ToolStatus},
};

#[derive(Debug)]
pub struct NormalizedToAcpConverter {
    session_id: SessionId,
    last_by_index: HashMap<usize, NormalizedEntry>,
    tool_call_by_id: HashMap<String, ToolCall>,
}

impl NormalizedToAcpConverter {
    pub fn new(session_id: impl Into<SessionId>) -> Self {
        Self {
            session_id: session_id.into(),
            last_by_index: HashMap::new(),
            tool_call_by_id: HashMap::new(),
        }
    }

    pub fn apply(&mut self, entry_index: usize, entry: NormalizedEntry) -> Vec<SessionNotification> {
        let prev = self.last_by_index.insert(entry_index, entry.clone());

        match &entry.entry_type {
            NormalizedEntryType::UserMessage => {
                let delta = text_delta(prev.as_ref().map(|p| p.content.as_str()), &entry.content);
                chunk_updates(&self.session_id, SessionUpdate::UserMessageChunk, delta)
            }
            NormalizedEntryType::AssistantMessage => {
                let delta = text_delta(prev.as_ref().map(|p| p.content.as_str()), &entry.content);
                chunk_updates(&self.session_id, SessionUpdate::AgentMessageChunk, delta)
            }
            NormalizedEntryType::Thinking => {
                let delta = text_delta(prev.as_ref().map(|p| p.content.as_str()), &entry.content);
                chunk_updates(&self.session_id, SessionUpdate::AgentThoughtChunk, delta)
            }
            NormalizedEntryType::SystemMessage => {
                let text = format!("[系统] {}", entry.content);
                chunk_updates(&self.session_id, SessionUpdate::AgentMessageChunk, Some(text))
            }
            NormalizedEntryType::ErrorMessage { .. } => {
                let text = format!("[错误] {}", entry.content);
                chunk_updates(&self.session_id, SessionUpdate::AgentMessageChunk, Some(text))
            }
            NormalizedEntryType::UserFeedback { denied_tool } => {
                let text = format!("[用户反馈] 已拒绝工具 `{}`：{}", denied_tool, entry.content);
                chunk_updates(&self.session_id, SessionUpdate::UserMessageChunk, Some(text))
            }
            NormalizedEntryType::Loading => Vec::new(),
            NormalizedEntryType::NextAction { failed, .. } => {
                let text = if *failed {
                    format!("[状态] 下一步执行失败：{}", entry.content)
                } else {
                    format!("[状态] 下一步执行：{}", entry.content)
                };
                chunk_updates(&self.session_id, SessionUpdate::AgentThoughtChunk, Some(text))
            }
            NormalizedEntryType::TokenUsageInfo(info) => {
                let text = format!(
                    "[用量] total_tokens={} context_window={}",
                    info.total_tokens, info.model_context_window
                );
                chunk_updates(&self.session_id, SessionUpdate::AgentThoughtChunk, Some(text))
            }
            NormalizedEntryType::UserAnsweredQuestions { answers } => {
                let text = format!("[用户回答] {} 个问题已回答", answers.len());
                chunk_updates(&self.session_id, SessionUpdate::UserMessageChunk, Some(text))
            }
            NormalizedEntryType::ToolUse {
                tool_name,
                action_type,
                status,
            } => self.tool_use_updates(entry_index, tool_name, action_type, status, &entry),
        }
    }

    fn tool_use_updates(
        &mut self,
        entry_index: usize,
        tool_name: &str,
        action_type: &ActionType,
        status: &ToolStatus,
        entry: &NormalizedEntry,
    ) -> Vec<SessionNotification> {
        match action_type {
            ActionType::PlanPresentation { plan } => {
                let plan = parse_plan_string(plan);
                vec![SessionNotification::new(
                    self.session_id.clone(),
                    SessionUpdate::Plan(plan),
                )]
            }
            ActionType::TodoManagement { todos, .. } => {
                let plan = Plan::new(todos_to_plan_entries(todos));
                vec![SessionNotification::new(
                    self.session_id.clone(),
                    SessionUpdate::Plan(plan),
                )]
            }
            _ => {
                let tool_call_id = tool_call_id_from_entry(entry_index, entry);

                let new_call = build_tool_call(tool_call_id.clone(), tool_name, action_type, status, entry);
                let is_new = !self.tool_call_by_id.contains_key(&tool_call_id);

                if is_new {
                    self.tool_call_by_id
                        .insert(tool_call_id.clone(), new_call.clone());
                    vec![SessionNotification::new(
                        self.session_id.clone(),
                        SessionUpdate::ToolCall(new_call),
                    )]
                } else {
                    let prev = self
                        .tool_call_by_id
                        .get(&tool_call_id)
                        .cloned()
                        .unwrap_or_else(|| ToolCall::new(ToolCallId::new(tool_call_id.clone()), ""));

                    let fields = diff_tool_call_fields(&prev, &new_call);
                    self.tool_call_by_id
                        .insert(tool_call_id.clone(), new_call.clone());

                    if fields_is_empty(&fields) {
                        Vec::new()
                    } else {
                        vec![SessionNotification::new(
                            self.session_id.clone(),
                            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                ToolCallId::new(tool_call_id),
                                fields,
                            )),
                        )]
                    }
                }
            }
        }
    }
}

fn chunk_updates(
    session_id: &SessionId,
    ctor: fn(ContentChunk) -> SessionUpdate,
    text: Option<String>,
) -> Vec<SessionNotification> {
    let Some(text) = text else {
        return Vec::new();
    };
    if text.is_empty() {
        return Vec::new();
    }
    let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)));
    vec![SessionNotification::new(session_id.clone(), ctor(chunk))]
}

fn text_delta(prev: Option<&str>, next: &str) -> Option<String> {
    let prev = prev.unwrap_or("");
    if prev.is_empty() {
        return Some(next.to_string());
    }
    if next.starts_with(prev) {
        let delta = &next[prev.len()..];
        if delta.is_empty() {
            None
        } else {
            Some(delta.to_string())
        }
    } else {
        Some(next.to_string())
    }
}

fn tool_call_id_from_entry(entry_index: usize, entry: &NormalizedEntry) -> String {
    if let Some(meta) = entry.metadata.as_ref() {
        if let Ok(parsed) = serde_json::from_value::<ToolCallMetadata>(meta.clone()) {
            if !parsed.tool_call_id.trim().is_empty() {
                return parsed.tool_call_id;
            }
        }
    }
    format!("tool-{}", entry_index)
}

fn build_tool_call(
    tool_call_id: String,
    tool_name: &str,
    action_type: &ActionType,
    status: &ToolStatus,
    entry: &NormalizedEntry,
) -> ToolCall {
    let (kind, title, locations, content, raw_input, raw_output) =
        map_action_to_tool_call_parts(tool_name, action_type, entry.content.clone());

    ToolCall::new(ToolCallId::new(tool_call_id), title)
        .kind(kind)
        .status(map_tool_status(status))
        .locations(locations)
        .content(content)
        .raw_input(raw_input)
        .raw_output(raw_output)
}

fn map_action_to_tool_call_parts(
    tool_name: &str,
    action: &ActionType,
    fallback_content: String,
) -> (
    ToolKind,
    String,
    Vec<ToolCallLocation>,
    Vec<ToolCallContent>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
) {
    let raw_input = serde_json::to_value(action).ok();

    match action {
        ActionType::FileRead { path } => {
            let title = format!("读取 {}", path);
            let locations = vec![ToolCallLocation::new(PathBuf::from(path))];
            let content = vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(
                format!("读取文件: {}", path),
            )))];
            (ToolKind::Read, title, locations, content, raw_input, None)
        }
        ActionType::FileEdit { path, changes } => {
            let title = format!("编辑 {}", path);
            let locations = vec![ToolCallLocation::new(PathBuf::from(path))];
            let content = file_changes_to_tool_content(path, changes);
            (ToolKind::Edit, title, locations, content, raw_input, None)
        }
        ActionType::CommandRun { command, result, .. } => {
            let title = format!("执行: {}", command);
            let mut content = Vec::new();
            if let Some(r) = result {
                if let Some(out) = &r.output {
                    content.push(ToolCallContent::from(ContentBlock::Text(TextContent::new(
                        out.clone(),
                    ))));
                }
            }
            let raw_output = result.as_ref().and_then(|r| serde_json::to_value(r).ok());
            (ToolKind::Execute, title, vec![], content, raw_input, raw_output)
        }
        ActionType::Search { query } => {
            let title = format!("搜索: {}", query);
            (ToolKind::Search, title, vec![], vec![], raw_input, None)
        }
        ActionType::WebFetch { url } => {
            let title = format!("获取: {}", url);
            (ToolKind::Fetch, title, vec![], vec![], raw_input, None)
        }
        ActionType::Tool { tool_name, result, .. } => {
            let title = tool_name.clone();
            let raw_output = result.as_ref().and_then(|r| serde_json::to_value(r).ok());
            let content = vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(
                fallback_content,
            )))];
            (ToolKind::Other, title, vec![], content, raw_input, raw_output)
        }
        ActionType::TaskCreate { description, .. } => {
            let title = format!("创建任务: {}", description);
            (
                ToolKind::Other,
                title,
                vec![],
                vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(
                    fallback_content,
                )))],
                raw_input,
                None,
            )
        }
        ActionType::AskUserQuestion { .. } => {
            let title = "向用户提问".to_string();
            (
                ToolKind::Other,
                title,
                vec![],
                vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(
                    fallback_content,
                )))],
                raw_input,
                None,
            )
        }
        ActionType::Other { description } => {
            let title = description.clone();
            (
                ToolKind::Other,
                title,
                vec![],
                vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(
                    fallback_content,
                )))],
                raw_input,
                None,
            )
        }
        ActionType::PlanPresentation { .. } | ActionType::TodoManagement { .. } => {
            let title = tool_name.to_string();
            (
                ToolKind::Other,
                title,
                vec![],
                vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(
                    fallback_content,
                )))],
                raw_input,
                None,
            )
        }
    }
}

fn file_changes_to_tool_content(path: &str, changes: &[FileChange]) -> Vec<ToolCallContent> {
    let mut out = Vec::new();
    for c in changes {
        match c {
            FileChange::Write { content } => {
                out.push(ToolCallContent::Diff(Diff::new(PathBuf::from(path), content.clone())));
            }
            FileChange::Delete => {
                out.push(ToolCallContent::from(ContentBlock::Text(TextContent::new(
                    "删除文件",
                ))));
            }
            FileChange::Rename { new_path } => {
                out.push(ToolCallContent::from(ContentBlock::Text(TextContent::new(
                    format!("重命名为 {}", new_path),
                ))));
            }
            FileChange::Edit {
                unified_diff,
                has_line_numbers: _,
            } => {
                out.push(ToolCallContent::from(ContentBlock::Text(TextContent::new(
                    unified_diff.clone(),
                ))));
            }
        }
    }
    if out.is_empty() {
        out.push(ToolCallContent::from(ContentBlock::Text(TextContent::new(
            "文件编辑",
        ))));
    }
    out
}

fn map_tool_status(status: &ToolStatus) -> ToolCallStatus {
    match status {
        ToolStatus::Created => ToolCallStatus::InProgress,
        ToolStatus::Success => ToolCallStatus::Completed,
        ToolStatus::Failed | ToolStatus::TimedOut => ToolCallStatus::Failed,
        ToolStatus::Denied { .. } => ToolCallStatus::Failed,
        ToolStatus::PendingApproval { .. } => ToolCallStatus::Pending,
    }
}

fn diff_tool_call_fields(prev: &ToolCall, next: &ToolCall) -> ToolCallUpdateFields {
    let mut fields = ToolCallUpdateFields::default();

    if prev.title != next.title {
        fields.title = Some(next.title.clone());
    }
    if prev.kind != next.kind {
        fields.kind = Some(next.kind);
    }
    if prev.status != next.status {
        fields.status = Some(next.status);
    }
    if prev.content != next.content {
        fields.content = Some(next.content.clone());
    }
    if prev.locations != next.locations {
        fields.locations = Some(next.locations.clone());
    }
    if prev.raw_input != next.raw_input {
        if let Some(v) = next.raw_input.clone() {
            fields.raw_input = Some(v);
        }
    }
    if prev.raw_output != next.raw_output {
        if let Some(v) = next.raw_output.clone() {
            fields.raw_output = Some(v);
        }
    }

    fields
}

fn fields_is_empty(fields: &ToolCallUpdateFields) -> bool {
    fields.kind.is_none()
        && fields.status.is_none()
        && fields.title.is_none()
        && fields.content.is_none()
        && fields.locations.is_none()
        && fields.raw_input.is_none()
        && fields.raw_output.is_none()
}

fn parse_plan_string(plan: &str) -> Plan {
    let mut entries = Vec::new();
    for line in plan.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        let (status, content) = if let Some(rest) = t.strip_prefix("- [x]") {
            (PlanEntryStatus::Completed, rest.trim())
        } else if let Some(rest) = t.strip_prefix("- [ ]") {
            (PlanEntryStatus::Pending, rest.trim())
        } else if let Some(rest) = t.strip_prefix("* [x]") {
            (PlanEntryStatus::Completed, rest.trim())
        } else if let Some(rest) = t.strip_prefix("* [ ]") {
            (PlanEntryStatus::Pending, rest.trim())
        } else {
            (PlanEntryStatus::Pending, t)
        };

        if content.is_empty() {
            continue;
        }

        entries.push(PlanEntry::new(
            content.to_string(),
            PlanEntryPriority::Medium,
            status,
        ));
    }

    if entries.is_empty() {
        entries.push(PlanEntry::new(
            plan.to_string(),
            PlanEntryPriority::Medium,
            PlanEntryStatus::Pending,
        ));
    }

    Plan::new(entries)
}

fn todos_to_plan_entries(todos: &[executors::logs::TodoItem]) -> Vec<PlanEntry> {
    todos
        .iter()
        .map(|t| {
            let status = match t.status.as_str() {
                "completed" => PlanEntryStatus::Completed,
                "in_progress" => PlanEntryStatus::InProgress,
                _ => PlanEntryStatus::Pending,
            };

            let priority = match t.priority.as_deref() {
                Some("high") => PlanEntryPriority::High,
                Some("low") => PlanEntryPriority::Low,
                _ => PlanEntryPriority::Medium,
            };

            PlanEntry::new(t.content.clone(), priority, status)
        })
        .collect()
}

