use uuid::Uuid;

use agentdash_domain::workflow::SubjectRef;

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
                    Some("active".to_string()),
                    Some(refs.agent_id),
                    Some(refs.run_id),
                    refs.frame_id,
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

        let Some(assoc) = associations
            .iter()
            .find(|assoc| assoc.anchor_agent_id.is_some())
            .or_else(|| associations.first())
        else {
            return Ok(None);
        };

        let run_id = assoc.anchor_run_id;
        let agent = if let Some(agent_id) = assoc.anchor_agent_id {
            self.repos
                .lifecycle_agent_repo
                .get(agent_id)
                .await
                .map_err(|e| TaskExecutionError::Internal(e.to_string()))?
        } else {
            self.repos
                .lifecycle_agent_repo
                .list_by_run(run_id)
                .await
                .map_err(|e| TaskExecutionError::Internal(e.to_string()))?
                .into_iter()
                .find(|a| a.status == "active")
        };
        let Some(agent) = agent else {
            return Ok(None);
        };

        let frame_id = agent.current_frame_id;

        Ok(Some(TaskExecutionRefs {
            run_id,
            agent_id: agent.id,
            frame_id,
        }))
    }
}

/// 从 LifecycleSubjectAssociation 解析到的 Task execution 锚点引用。
#[derive(Debug, Clone)]
struct TaskExecutionRefs {
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Option<Uuid>,
}
