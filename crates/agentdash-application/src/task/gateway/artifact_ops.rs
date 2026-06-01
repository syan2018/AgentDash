//! Task artifact 持久化 helpers。
//!
//! 职责：把 tool call / turn failure 等执行产物以 Artifact 形式写回 Story aggregate，
//! 并同步追加一条 StateChange 投影。仅负责 artifact 相关的数据落盘，不做状态机决策。

use serde_json::{Map, Value, json};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::story::ChangeKind;

use crate::repository_set::RepositorySet;
use crate::task::artifact::upsert_tool_execution_artifact;

use super::repo_ops::append_task_change;

pub(super) struct VerifiedTaskArtifactContext<'a> {
    task_id: Uuid,
    session_id: &'a str,
    turn_id: &'a str,
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Uuid,
    backend_id: &'a str,
}

impl<'a> VerifiedTaskArtifactContext<'a> {
    pub(super) fn new(
        task_id: Uuid,
        session_id: &'a str,
        turn_id: &'a str,
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Uuid,
        backend_id: &'a str,
    ) -> Self {
        Self {
            task_id,
            session_id,
            turn_id,
            run_id,
            agent_id,
            frame_id,
            backend_id,
        }
    }

    pub(super) fn run_id(&self) -> Uuid {
        self.run_id
    }

    pub(super) fn agent_id(&self) -> Uuid {
        self.agent_id
    }

    pub(super) fn frame_id(&self) -> Uuid {
        self.frame_id
    }

    pub(super) fn session_id(&self) -> &str {
        self.session_id
    }

    pub(super) fn turn_id(&self) -> &str {
        self.turn_id
    }
}

pub(super) struct ToolCallArtifactInput<'a> {
    pub context: VerifiedTaskArtifactContext<'a>,
    pub tool_call_id: &'a str,
    pub patch: Map<String, Value>,
    pub reason: &'a str,
}

pub(super) async fn persist_tool_call_artifact(
    repos: &RepositorySet,
    input: ToolCallArtifactInput<'_>,
) -> Result<(), DomainError> {
    let Some(mut story) = repos
        .story_repo
        .find_by_task_id(input.context.task_id)
        .await?
    else {
        return Ok(());
    };
    let Some(task_snapshot) = story.find_task(input.context.task_id).cloned() else {
        return Ok(());
    };

    let mut updated_task = task_snapshot.clone();
    let changed = upsert_tool_execution_artifact(
        &mut updated_task,
        input.context.session_id,
        input.context.turn_id,
        input.tool_call_id,
        input.patch,
    )?;
    if !changed {
        return Ok(());
    }

    // artifact 已经在 updated_task 上加好了，直接替换 story 内的 task。
    story.mutate_task_artifacts(input.context.task_id, |artifacts| {
        *artifacts = updated_task.artifacts().to_vec();
    });
    repos.story_repo.update(&story).await?;
    append_task_change(
        repos,
        updated_task.id,
        input.context.backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": input.reason,
            "task_id": updated_task.id,
            "story_id": updated_task.story_id,
            "run_id": input.context.run_id,
            "agent_id": input.context.agent_id,
            "frame_id": input.context.frame_id,
            "session_id": input.context.session_id,
            "turn_id": input.context.turn_id,
            "tool_call_id": input.tool_call_id,
            "artifact_type": "tool_execution",
        }),
    )
    .await?;

    Ok(())
}
