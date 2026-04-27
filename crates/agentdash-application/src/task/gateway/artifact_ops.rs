//! Task artifact 持久化 helpers。
//!
//! 职责：把 tool call / turn failure 等执行产物以 Artifact 形式写回 Story aggregate，
//! 并同步追加一条 StateChange 投影。仅负责 artifact 相关的数据落盘，不做状态机决策。

use serde_json::{Map, Value, json};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::story::ChangeKind;
use agentdash_domain::task::{Artifact, ArtifactType};

use crate::repository_set::RepositorySet;
use crate::task::artifact::upsert_tool_execution_artifact;

use super::repo_ops::append_task_change;

pub struct ToolCallArtifactInput<'a> {
    pub task_id: Uuid,
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub tool_call_id: &'a str,
    pub patch: Map<String, Value>,
    pub backend_id: &'a str,
    pub reason: &'a str,
}

pub async fn persist_tool_call_artifact(
    repos: &RepositorySet,
    input: ToolCallArtifactInput<'_>,
) -> Result<(), DomainError> {
    let Some(mut story) = repos.story_repo.find_by_task_id(input.task_id).await? else {
        return Ok(());
    };
    let Some(task_snapshot) = story.find_task(input.task_id).cloned() else {
        return Ok(());
    };

    let mut updated_task = task_snapshot.clone();
    let changed = upsert_tool_execution_artifact(
        &mut updated_task,
        input.session_id,
        input.turn_id,
        input.tool_call_id,
        input.patch,
    )?;
    if !changed {
        return Ok(());
    }

    // artifact 已经在 updated_task 上加好了，直接替换 story 内的 task。
    story.mutate_task_artifacts(input.task_id, |artifacts| {
        *artifacts = updated_task.artifacts().to_vec();
    });
    // 同步 spec 字段（title 等可能也被间接修改，虽然本路径主要改 artifacts）。
    story.update_task(input.task_id, |view| {
        *view.executor_session_id = updated_task.executor_session_id.clone();
    });
    repos.story_repo.update(&story).await?;
    append_task_change(
        repos,
        updated_task.id,
        input.backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": input.reason,
            "task_id": updated_task.id,
            "story_id": updated_task.story_id,
            "session_id": input.session_id,
            "turn_id": input.turn_id,
            "tool_call_id": input.tool_call_id,
            "artifact_type": "tool_execution",
        }),
    )
    .await?;

    Ok(())
}

pub async fn persist_turn_failure_artifact(
    repos: &RepositorySet,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
    error_message: &str,
) -> Result<(), DomainError> {
    let Some(mut story) = repos.story_repo.find_by_task_id(task_id).await? else {
        return Ok(());
    };
    if story.find_task(task_id).is_none() {
        return Ok(());
    }

    let new_artifact = Artifact {
        id: Uuid::new_v4(),
        artifact_type: ArtifactType::LogOutput,
        content: json!({
            "kind": "turn_error",
            "session_id": session_id,
            "turn_id": turn_id,
            "message": error_message,
            "created_at": chrono::Utc::now().to_rfc3339(),
        }),
        created_at: chrono::Utc::now(),
    };
    story.push_task_artifact(task_id, new_artifact.clone());
    let (story_id, project_id) = (story.id, story.project_id);
    let _ = (story_id, project_id);
    repos.story_repo.update(&story).await?;
    append_task_change(
        repos,
        task_id,
        backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": "turn_failed_error_summary",
            "task_id": task_id,
            "story_id": story.id,
            "session_id": session_id,
            "turn_id": turn_id,
            "artifact_type": "log_output",
        }),
    )
    .await?;

    Ok(())
}
