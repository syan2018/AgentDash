use agent_client_protocol::{ToolCall, ToolCallStatus, ToolCallUpdate};
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
        let mut content = artifact
            .content
            .as_object()
            .cloned()
            .expect("tool_execution artifact 内容必须是对象");
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

pub fn build_tool_call_patch(tool_call: &ToolCall) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert("title".to_string(), json!(tool_call.title));
    patch.insert("kind".to_string(), json!(enum_to_string(&tool_call.kind)));
    patch.insert(
        "status".to_string(),
        json!(tool_status_to_string(tool_call.status)),
    );

    if !tool_call.content.is_empty() {
        let content = serialize_or_fail(&tool_call.content, "tool_call.content");
        patch.insert("content".to_string(), content.clone());
        patch.insert("output_preview".to_string(), json!(preview_value(&content)));
    }
    if !tool_call.locations.is_empty() {
        patch.insert(
            "locations".to_string(),
            serialize_or_fail(&tool_call.locations, "tool_call.locations"),
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
        let content_value = serialize_or_fail(&content, "tool_call_update.content");
        patch.insert("content".to_string(), content_value.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&content_value)),
        );
    }
    if let Some(locations) = update.fields.locations.clone() {
        patch.insert(
            "locations".to_string(),
            serialize_or_fail(&locations, "tool_call_update.locations"),
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
        other => panic!("未知 ToolCallStatus: {:?}", other),
    }
}

pub fn enum_to_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("enum 序列化失败")
        .as_str()
        .expect("enum 序列化后不是字符串")
        .to_owned()
}

fn serialize_or_fail<T: Serialize>(value: &T, field: &str) -> Value {
    serde_json::to_value(value).unwrap_or_else(|error| panic!("序列化 {field} 失败: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::ToolCallStatus;
    use agentdash_domain::task::{Artifact, ArtifactType, Task};
    use serde::ser::{Error, Serializer};
    use std::panic;
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
    fn serialize_or_fail_returns_value_and_panics_on_error() {
        let value = serialize_or_fail(&vec![1, 2, 3], "vec");
        assert_eq!(value, json!([1, 2, 3]));
        assert!(panic::catch_unwind(|| serialize_or_fail(&FailSerialize, "fail")).is_err());
    }

    #[test]
    fn enum_to_string_returns_serialized_tag() {
        assert_eq!(enum_to_string(&SampleEnum::Foo), "Foo");
        assert_eq!(enum_to_string(&SampleEnum::Bar), "Bar");
    }

    #[test]
    fn tool_status_to_string_handles_known_variants() {
        assert_eq!(tool_status_to_string(ToolCallStatus::Pending), "pending");
        assert_eq!(
            tool_status_to_string(ToolCallStatus::InProgress),
            "in_progress"
        );
        assert_eq!(
            tool_status_to_string(ToolCallStatus::Completed),
            "completed"
        );
        assert_eq!(tool_status_to_string(ToolCallStatus::Failed), "failed");
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
