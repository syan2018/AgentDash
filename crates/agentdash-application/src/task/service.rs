use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleAgent, LifecycleRun, LifecycleSubjectAssociation, RuntimeNodeState, RuntimeNodeStatus,
    RuntimeSessionExecutionAnchor, SubjectRef,
};

use crate::repository_set::RepositorySet;

use super::execution::*;
use super::gateway::get_task as gw_get_task;

/// Story activity activation service — 保留 Task execution 的只读 lifecycle 投影。
pub struct StoryActivityActivationService {
    pub repos: RepositorySet,
}

impl StoryActivityActivationService {
    /// 查询 task 当前执行视图（lifecycle 投影）。
    pub async fn get_task_execution_view(
        &self,
        task_id: Uuid,
    ) -> Result<TaskExecutionView, TaskExecutionError> {
        let task = gw_get_task(&self.repos, task_id).await?;
        let refs = self.resolve_task_execution_refs(task_id).await?;

        let (execution_status, agent_ref, run_ref, frame_ref, delivery_runtime_ref) =
            if let Some(refs) = refs {
                (
                    Some(runtime_node_status_code(refs.node_status).to_string()),
                    Some(refs.agent_id),
                    Some(refs.run_id),
                    Some(refs.frame_id),
                    None,
                )
            } else {
                (None, None, None, None, None)
            };

        Ok(TaskExecutionView {
            task_id: task.id,
            execution_status,
            agent_ref,
            run_ref,
            frame_ref,
            delivery_runtime_ref,
            task_status: task.status().clone(),
        })
    }

    /// 通过 LifecycleSubjectAssociation 查找 task 的活跃 execution refs。
    async fn resolve_task_execution_refs(
        &self,
        task_id: Uuid,
    ) -> Result<Option<TaskExecutionRefs>, TaskExecutionError> {
        let subject = SubjectRef::new("task", task_id);
        let associations = self
            .repos
            .lifecycle_subject_association_repo
            .list_by_subject(&subject)
            .await
            .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;

        let mut latest: Option<TaskExecutionRefs> = None;

        for association in &associations {
            let Some(agent) = self.resolve_association_agent(association).await? else {
                continue;
            };
            let Some(frame_id) = agent.current_frame_id else {
                continue;
            };
            let Some(run) = self
                .repos
                .lifecycle_run_repo
                .get_by_id(association.anchor_run_id)
                .await
                .map_err(|e| TaskExecutionError::Internal(e.to_string()))?
            else {
                continue;
            };
            let anchors = self
                .repos
                .execution_anchor_repo
                .list_by_agent(agent.id)
                .await
                .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;
            for anchor in anchors {
                if anchor.run_id != run.id || anchor.launch_frame_id != frame_id {
                    continue;
                }
                let Some(refs) = task_execution_refs_from_anchor(&run, &agent, frame_id, &anchor)
                else {
                    continue;
                };
                if latest
                    .as_ref()
                    .map(|current| refs.observed_at > current.observed_at)
                    .unwrap_or(true)
                {
                    latest = Some(refs);
                }
            }
        }

        Ok(latest)
    }

    async fn resolve_association_agent(
        &self,
        association: &LifecycleSubjectAssociation,
    ) -> Result<Option<LifecycleAgent>, TaskExecutionError> {
        if let Some(agent_id) = association.anchor_agent_id {
            let agent = self
                .repos
                .lifecycle_agent_repo
                .get(agent_id)
                .await
                .map_err(|e| TaskExecutionError::Internal(e.to_string()))?;
            return Ok(agent.filter(|agent| agent.run_id == association.anchor_run_id));
        }

        Ok(self
            .repos
            .lifecycle_agent_repo
            .list_by_run(association.anchor_run_id)
            .await
            .map_err(|e| TaskExecutionError::Internal(e.to_string()))?
            .into_iter()
            .filter(|agent| agent.status == "active")
            .max_by_key(|agent| agent.updated_at))
    }
}

/// 从 LifecycleSubjectAssociation 解析到的 Task execution 锚点引用。
#[derive(Debug, Clone)]
struct TaskExecutionRefs {
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Uuid,
    node_status: RuntimeNodeStatus,
    observed_at: chrono::DateTime<chrono::Utc>,
}

fn task_execution_refs_from_anchor(
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    frame_id: Uuid,
    anchor: &RuntimeSessionExecutionAnchor,
) -> Option<TaskExecutionRefs> {
    let orchestration_id = anchor.orchestration_id?;
    let node_path = anchor.node_path.as_deref()?;
    let node_attempt = anchor.node_attempt.unwrap_or(1);
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == orchestration_id)?;
    let node = find_runtime_node(&orchestration.node_tree, node_path, node_attempt)?;
    let observed_at = node
        .completed_at
        .or(node.started_at)
        .unwrap_or(anchor.updated_at);

    Some(TaskExecutionRefs {
        run_id: run.id,
        agent_id: agent.id,
        frame_id,
        node_status: node.status,
        observed_at,
    })
}

fn find_runtime_node<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}

fn runtime_node_status_code(status: RuntimeNodeStatus) -> &'static str {
    match status {
        RuntimeNodeStatus::Pending => "pending",
        RuntimeNodeStatus::Ready => "ready",
        RuntimeNodeStatus::Claiming => "claiming",
        RuntimeNodeStatus::Running => "running",
        RuntimeNodeStatus::Blocked => "blocked",
        RuntimeNodeStatus::Completed => "completed",
        RuntimeNodeStatus::Failed => "failed",
        RuntimeNodeStatus::Cancelled => "cancelled",
        RuntimeNodeStatus::Skipped => "skipped",
    }
}
