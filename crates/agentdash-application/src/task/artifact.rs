use agent_client_protocol::{ToolCall, ToolCallStatus, ToolCallUpdate};
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
) -> bool {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    patch.insert("session_id".to_string(), json!(session_id));
    patch.insert("turn_id".to_string(), json!(turn_id));
    patch.insert("tool_call_id".to_string(), json!(tool_call_id));
    patch.insert("updated_at".to_string(), json!(now_str));

    if let Some(index) = task
        .artifacts
        .iter()
        .position(|item| is_same_tool_execution_artifact(item, turn_id, tool_call_id))
    {
        let artifact = &mut task.artifacts[index];
        let before = artifact.content.clone();
        let mut content = artifact.content.as_object().cloned().unwrap_or_default();
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
            return false;
        }
        artifact.content = next;
        return true;
    }

    if !patch.contains_key("started_at") {
        patch.insert("started_at".to_string(), json!(now_str));
    }

    task.artifacts.push(Artifact {
        id: Uuid::new_v4(),
        artifact_type: ArtifactType::ToolExecution,
        content: Value::Object(patch),
        created_at: now,
    });
    true
}

fn is_same_tool_execution_artifact(
    artifact: &Artifact,
    turn_id: &str,
    tool_call_id: &str,
) -> bool {
    artifact.artifact_type == ArtifactType::ToolExecution
        && artifact.content.get("turn_id").and_then(Value::as_str) == Some(turn_id)
        && artifact
            .content
            .get("tool_call_id")
            .and_then(Value::as_str)
            == Some(tool_call_id)
}

pub fn build_tool_call_patch(tool_call: &ToolCall) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert("title".to_string(), json!(tool_call.title));
    patch.insert("kind".to_string(), json!(enum_to_string(&tool_call.kind)));
    patch.insert(
        "status".to_string(),
        json!(tool_status_to_string(tool_call.status)),
    );

    if !tool_call.content.is_empty() {
        let content = serde_json::to_value(&tool_call.content).unwrap_or_else(|_| json!([]));
        patch.insert("content".to_string(), content.clone());
        patch.insert("output_preview".to_string(), json!(preview_value(&content)));
    }
    if !tool_call.locations.is_empty() {
        patch.insert(
            "locations".to_string(),
            serde_json::to_value(&tool_call.locations).unwrap_or_else(|_| json!([])),
        );
    }
    if let Some(raw_input) = tool_call.raw_input.clone() {
        patch.insert("raw_input".to_string(), raw_input.clone());
        patch.insert(
            "input_preview".to_string(),
            json!(preview_value(&raw_input)),
        );
    }
    if let Some(raw_output) = tool_call.raw_output.clone() {
        patch.insert("raw_output".to_string(), raw_output.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&raw_output)),
        );
    }

    patch
}

pub fn build_tool_call_update_patch(update: &ToolCallUpdate) -> Map<String, Value> {
    let mut patch = Map::new();
    if let Some(title) = update.fields.title.clone() {
        patch.insert("title".to_string(), json!(title));
    }
    if let Some(kind) = update.fields.kind {
        patch.insert("kind".to_string(), json!(enum_to_string(&kind)));
    }
    if let Some(status) = update.fields.status {
        patch.insert("status".to_string(), json!(tool_status_to_string(status)));
    }
    if let Some(content) = update.fields.content.clone() {
        let content_value = serde_json::to_value(content).unwrap_or_else(|_| json!([]));
        patch.insert("content".to_string(), content_value.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&content_value)),
        );
    }
    if let Some(locations) = update.fields.locations.clone() {
        patch.insert(
            "locations".to_string(),
            serde_json::to_value(locations).unwrap_or_else(|_| json!([])),
        );
    }
    if let Some(raw_input) = update.fields.raw_input.clone() {
        patch.insert("raw_input".to_string(), raw_input.clone());
        patch.insert(
            "input_preview".to_string(),
            json!(preview_value(&raw_input)),
        );
    }
    if let Some(raw_output) = update.fields.raw_output.clone() {
        patch.insert("raw_output".to_string(), raw_output.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&raw_output)),
        );
    }
    patch
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

pub fn tool_status_to_string(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::InProgress => "in_progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        _ => "pending",
    }
}

pub fn enum_to_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|raw| raw.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "other".to_string())
}
