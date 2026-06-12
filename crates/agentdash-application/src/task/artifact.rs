use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, codex_app_server_protocol as codex,
};
use agentdash_domain::DomainError;
use agentdash_domain::task::{Artifact, ArtifactType};
use serde::Serialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// 在 Task 的 artifact 列表中 upsert 一条 ToolExecution 记录。
/// 返回 true 表示有变更（新增或更新），false 表示无变更。
pub fn upsert_tool_execution_artifact(
    task: &mut agentdash_domain::task::Task,
    session_id: &str,
    turn_id: &str,
    tool_call_id: &str,
    mut patch: Map<String, Value>,
) -> Result<bool, DomainError> {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    patch.insert("session_id".to_string(), json!(session_id));
    patch.insert("turn_id".to_string(), json!(turn_id));
    patch.insert("tool_call_id".to_string(), json!(tool_call_id));
    patch.insert("updated_at".to_string(), json!(now_str));

    for artifact in task.artifacts() {
        if artifact.artifact_type == ArtifactType::ToolExecution && !artifact.content.is_object() {
            return Err(DomainError::InvalidConfig(format!(
                "tool_execution artifact 内容不是对象: {}",
                artifact.id
            )));
        }
    }

    let artifacts_mut = task.artifacts_mut();
    if let Some(index) = artifacts_mut
        .iter()
        .position(|item| is_same_tool_execution_artifact(item, turn_id, tool_call_id))
    {
        let artifact = &mut artifacts_mut[index];
        let before = artifact.content.clone();
        let mut content = artifact.content.as_object().cloned().ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "tool_execution artifact 内容不是对象: {}",
                artifact.id
            ))
        })?;
        for (key, value) in patch {
            if key == "started_at" && content.contains_key("started_at") {
                continue;
            }
            content.insert(key, value);
        }
        if !content.contains_key("started_at") {
            content.insert("started_at".to_string(), json!(now_str));
        }
        let next = Value::Object(content);
        if before == next {
            return Ok(false);
        }
        artifact.content = next;
        return Ok(true);
    }

    if !patch.contains_key("started_at") {
        patch.insert("started_at".to_string(), json!(now_str));
    }

    task.push_artifact(Artifact {
        id: Uuid::new_v4(),
        artifact_type: ArtifactType::ToolExecution,
        content: Value::Object(patch),
        created_at: now,
    });
    Ok(true)
}

fn is_same_tool_execution_artifact(artifact: &Artifact, turn_id: &str, tool_call_id: &str) -> bool {
    artifact.artifact_type == ArtifactType::ToolExecution
        && artifact.content.get("turn_id").and_then(Value::as_str) == Some(turn_id)
        && artifact.content.get("tool_call_id").and_then(Value::as_str) == Some(tool_call_id)
}

/// 从 BackboneEvent 的 ThreadItem 中提取 tool call 信息，构建 artifact patch。
///
/// 返回 `(tool_call_id, patch)` 或 `None`（如果 ThreadItem 不是 tool call 类型）。
pub fn build_thread_item_patch(item: &AgentDashThreadItem) -> Option<(String, Map<String, Value>)> {
    match item {
        AgentDashThreadItem::Codex(item) => build_codex_thread_item_patch(item),
        AgentDashThreadItem::AgentDash(item) => build_agentdash_thread_item_patch(item),
    }
}

fn build_codex_thread_item_patch(item: &codex::ThreadItem) -> Option<(String, Map<String, Value>)> {
    match item {
        codex::ThreadItem::DynamicToolCall {
            id,
            tool,
            arguments,
            status,
            content_items,
            success,
            duration_ms,
            ..
        } => {
            let mut patch = Map::new();
            patch.insert("title".to_string(), json!(tool));
            patch.insert("kind".to_string(), json!("dynamic_tool_call"));
            patch.insert(
                "status".to_string(),
                json!(dynamic_tool_call_status_str(status)),
            );
            patch.insert("raw_input".to_string(), arguments.clone());
            patch.insert("input_preview".to_string(), json!(preview_value(arguments)));
            if let Some(items) = content_items {
                let content_value = serialize_field(items, "content_items")?;
                patch.insert("content".to_string(), content_value.clone());
                patch.insert(
                    "output_preview".to_string(),
                    json!(preview_value(&content_value)),
                );
            }
            if let Some(s) = success {
                patch.insert("success".to_string(), json!(s));
            }
            if let Some(ms) = duration_ms {
                patch.insert("duration_ms".to_string(), json!(ms));
            }
            Some((id.clone(), patch))
        }
        codex::ThreadItem::McpToolCall {
            id,
            tool,
            arguments,
            status,
            result,
            error,
            duration_ms,
            ..
        } => {
            let mut patch = Map::new();
            patch.insert("title".to_string(), json!(tool));
            patch.insert("kind".to_string(), json!("mcp_tool_call"));
            patch.insert(
                "status".to_string(),
                json!(mcp_tool_call_status_str(status)),
            );
            patch.insert("raw_input".to_string(), arguments.clone());
            patch.insert("input_preview".to_string(), json!(preview_value(arguments)));
            if let Some(r) = result {
                let output = serialize_field(r, "mcp_result")?;
                patch.insert("raw_output".to_string(), output.clone());
                patch.insert("output_preview".to_string(), json!(preview_value(&output)));
            }
            if let Some(e) = error {
                patch.insert("error".to_string(), json!(e.message));
            }
            if let Some(ms) = duration_ms {
                patch.insert("duration_ms".to_string(), json!(ms));
            }
            Some((id.clone(), patch))
        }
        codex::ThreadItem::CommandExecution {
            id,
            command,
            status,
            exit_code,
            aggregated_output,
            duration_ms,
            ..
        } => {
            let mut patch = Map::new();
            patch.insert("title".to_string(), json!(command));
            patch.insert("kind".to_string(), json!("command_execution"));
            patch.insert(
                "status".to_string(),
                json!(command_execution_status_str(status)),
            );
            patch.insert("raw_input".to_string(), json!({ "command": command }));
            if let Some(output) = aggregated_output {
                patch.insert("raw_output".to_string(), json!(output));
                patch.insert(
                    "output_preview".to_string(),
                    json!(preview_value(&json!(output))),
                );
            }
            if let Some(code) = exit_code {
                patch.insert("exit_code".to_string(), json!(code));
            }
            if let Some(ms) = duration_ms {
                patch.insert("duration_ms".to_string(), json!(ms));
            }
            Some((id.clone(), patch))
        }
        codex::ThreadItem::FileChange {
            id,
            changes,
            status,
            ..
        } => {
            let changes_value = serialize_field(changes, "file_changes")?;
            let mut patch = Map::new();
            patch.insert("title".to_string(), json!(file_change_title(changes)));
            patch.insert("kind".to_string(), json!("file_change"));
            patch.insert("status".to_string(), json!(patch_apply_status_str(status)));
            patch.insert("raw_input".to_string(), changes_value.clone());
            patch.insert(
                "input_preview".to_string(),
                json!(preview_value(&changes_value)),
            );
            patch.insert("content".to_string(), changes_value.clone());
            patch.insert(
                "output_preview".to_string(),
                json!(preview_value(&changes_value)),
            );
            match status {
                codex::PatchApplyStatus::Completed => {
                    patch.insert("success".to_string(), json!(true));
                }
                codex::PatchApplyStatus::Failed | codex::PatchApplyStatus::Declined => {
                    patch.insert("success".to_string(), json!(false));
                }
                codex::PatchApplyStatus::InProgress => {}
            }
            Some((id.clone(), patch))
        }
        _ => None,
    }
}

fn build_agentdash_thread_item_patch(
    item: &AgentDashNativeThreadItem,
) -> Option<(String, Map<String, Value>)> {
    let id = item.id().to_string();
    let mut patch = Map::new();
    patch.insert("title".to_string(), json!(agentdash_item_title(item)));
    patch.insert("kind".to_string(), json!(item.tool_name()));
    patch.insert(
        "status".to_string(),
        json!(dynamic_tool_call_status_str(item.status())),
    );
    patch.insert("raw_input".to_string(), item.arguments().clone());
    patch.insert(
        "input_preview".to_string(),
        json!(preview_value(item.arguments())),
    );
    if let Some(items) = item.content_items() {
        let content_value = serialize_field(items, "content_items")?;
        patch.insert("content".to_string(), content_value.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&content_value)),
        );
    }
    if let Some(output) = item.shell_output() {
        patch.insert("raw_output".to_string(), json!(output));
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&json!(output))),
        );
    }
    if let Some(success) = item.success() {
        patch.insert("success".to_string(), json!(success));
    }
    Some((id, patch))
}

fn agentdash_item_title(item: &AgentDashNativeThreadItem) -> String {
    match item {
        AgentDashNativeThreadItem::ShellExec { command, .. } => command.clone(),
        AgentDashNativeThreadItem::FsRead { path, .. } => path.clone(),
        AgentDashNativeThreadItem::FsGrep { pattern, path, .. } => path
            .as_ref()
            .map(|path| format!("{pattern} in {path}"))
            .unwrap_or_else(|| pattern.clone()),
        AgentDashNativeThreadItem::FsGlob { pattern, path, .. } => path
            .as_ref()
            .map(|path| format!("{pattern} in {path}"))
            .unwrap_or_else(|| pattern.clone()),
    }
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

fn file_change_title(changes: &[codex::FileUpdateChange]) -> String {
    match changes {
        [] => "file_change".to_string(),
        [change] => change.path.clone(),
        [first, rest @ ..] => format!("{} (+{} files)", first.path, rest.len()),
    }
}

pub fn preview_value(value: &Value) -> String {
    let raw = value.to_string();
    const MAX_LEN: usize = 240;
    if raw.len() <= MAX_LEN {
        raw
    } else {
        let shortened: String = raw.chars().take(MAX_LEN).collect();
        format!("{shortened}...")
    }
}

pub fn enum_to_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// 序列化 thread item 字段；失败时记录并返回 `None`，调用方据此跳过该 item，
/// 避免在请求路径 panic。
fn serialize_field<T: Serialize>(value: &T, field: &str) -> Option<Value> {
    match serde_json::to_value(value) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::error!(target: "task_artifact", field, error = %error, "序列化 thread item 字段失败，跳过该 item");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::task::{Artifact, ArtifactType, Task};
    use serde::ser::{Error, Serializer};
    use uuid::Uuid;

    #[derive(serde::Serialize)]
    enum SampleEnum {
        Foo,
        Bar,
    }

    struct FailSerialize;

    impl serde::Serialize for FailSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(S::Error::custom("boom"))
        }
    }

    #[test]
    fn serialize_field_returns_value_and_none_on_error() {
        let value = serialize_field(&vec![1, 2, 3], "vec");
        assert_eq!(value, Some(json!([1, 2, 3])));
        assert_eq!(serialize_field(&FailSerialize, "fail"), None);
    }

    #[test]
    fn enum_to_string_returns_serialized_tag() {
        assert_eq!(enum_to_string(&SampleEnum::Foo), "Foo");
        assert_eq!(enum_to_string(&SampleEnum::Bar), "Bar");
    }

    #[test]
    fn dynamic_tool_call_status_str_handles_known_variants() {
        assert_eq!(
            dynamic_tool_call_status_str(&codex::DynamicToolCallStatus::InProgress),
            "in_progress"
        );
        assert_eq!(
            dynamic_tool_call_status_str(&codex::DynamicToolCallStatus::Completed),
            "completed"
        );
        assert_eq!(
            dynamic_tool_call_status_str(&codex::DynamicToolCallStatus::Failed),
            "failed"
        );
    }

    #[test]
    fn file_change_thread_item_builds_tool_execution_patch() {
        let item: codex::ThreadItem = serde_json::from_value(json!({
            "type": "fileChange",
            "id": "patch-1",
            "changes": [{
                "path": "src/lib.rs",
                "kind": { "type": "update", "move_path": null },
                "diff": "@@\n-old\n+new"
            }],
            "status": "completed"
        }))
        .expect("fileChange item should deserialize");

        let Some((tool_call_id, patch)) = build_codex_thread_item_patch(&item) else {
            panic!("fileChange should produce artifact patch");
        };

        assert_eq!(tool_call_id, "patch-1");
        assert_eq!(patch.get("kind"), Some(&json!("file_change")));
        assert_eq!(patch.get("title"), Some(&json!("src/lib.rs")));
        assert_eq!(patch.get("status"), Some(&json!("completed")));
        assert_eq!(patch.get("success"), Some(&json!(true)));
        assert!(patch.get("content").is_some());
    }

    #[test]
    fn upsert_tool_execution_artifact_rejects_non_object_content() {
        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "task".to_string(),
            String::new(),
        );
        task.push_artifact(Artifact {
            id: Uuid::new_v4(),
            artifact_type: ArtifactType::ToolExecution,
            content: json!(["bad"]),
            created_at: chrono::Utc::now(),
        });

        let error =
            upsert_tool_execution_artifact(&mut task, "sess-1", "turn-1", "call-1", Map::new())
                .expect_err("非对象 artifact 应直接报错");

        assert!(error.to_string().contains("内容不是对象"));
    }
}
