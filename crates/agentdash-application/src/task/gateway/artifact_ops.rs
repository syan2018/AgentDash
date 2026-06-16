//! Task artifact 持久化 helpers。
//!
//! 职责：把 tool call / turn failure 等执行产物以 Artifact 形式写回 Story aggregate，
//! 并同步追加一条 StateChange 投影。仅负责 artifact 相关的数据落盘，不做状态机决策。

use serde_json::{Map, Value, json};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::story::ChangeKind;

use super::repo_ops::append_task_change;
use crate::repository_set::RepositorySet;

pub(super) struct VerifiedTaskArtifactContext<'a> {
    task_id: Uuid,
    session_id: &'a str,
    turn_id: &'a str,
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Uuid,
    orchestration_id: Uuid,
    node_path: String,
    node_attempt: u32,
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
        orchestration_id: Uuid,
        node_path: &str,
        node_attempt: u32,
        backend_id: &'a str,
    ) -> Self {
        Self {
            task_id,
            session_id,
            turn_id,
            run_id,
            agent_id,
            frame_id,
            orchestration_id,
            node_path: node_path.to_string(),
            node_attempt,
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

    pub(super) fn orchestration_id(&self) -> Uuid {
        self.orchestration_id
    }

    pub(super) fn node_path(&self) -> &str {
        &self.node_path
    }

    pub(super) fn node_attempt(&self) -> u32 {
        self.node_attempt
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
    append_task_change(
        repos,
        input.context.task_id,
        input.context.backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": input.reason,
            "task_id": input.context.task_id,
            "run_id": input.context.run_id,
            "agent_id": input.context.agent_id,
            "frame_id": input.context.frame_id,
            "orchestration_id": input.context.orchestration_id,
            "node_path": input.context.node_path,
            "node_attempt": input.context.node_attempt,
            "session_id": input.context.session_id,
            "turn_id": input.context.turn_id,
            "tool_call_id": input.tool_call_id,
            "patch": Value::Object(input.patch),
            "artifact_type": "tool_execution",
        }),
    )
    .await?;

    Ok(())
}
