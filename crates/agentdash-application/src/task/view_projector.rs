//! 启动期 Task view 投影器 — 从 LifecycleRun orchestration runtime state 反投影到 `Story.tasks[i].status`。
//!
//! **方向**：LifecycleRun 真相源 → Task view（只读投影），属于 projection 方向。
//! 对应运行期反向（业务终态 → session cancel）的 command 通道见
//! [`crate::reconcile::terminal_cancel`]。
//!
//! 真相源 = LifecycleRun.orchestrations；Task view 仅为只读投影。
//!
//! 投影匹配策略：
//! 从 `SubjectRef(kind=task)` 查找 `LifecycleSubjectAssociation`，再沿
//! association anchor agent → `LifecycleAgent.current_frame` →
//! `RuntimeSessionExecutionAnchor` → runtime node 坐标派生明确 node fact。
//!
//! 没有明确 lifecycle runtime node fact 时不写 Task 终态。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{ChangeKind, StateChangeRepository, StoryRepository};
use agentdash_domain::task::Task;
use agentdash_domain::workflow::{
    LifecycleAgent, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository, RuntimeNodeStatus,
    RuntimeSessionExecutionAnchorRepository, SubjectRef,
};

use super::runtime_coordinate::task_runtime_projection_from_anchor;

#[derive(Debug, thiserror::Error)]
pub enum TaskViewProjectionError {
    #[error(transparent)]
    Domain(#[from] agentdash_domain::DomainError),
    #[error("Task {0} 不存在")]
    TaskNotFound(Uuid),
}

/// 将一个明确的 runtime node 状态投影到 Task view。
///
/// 入口用于 runtime command 已经推进 orchestration node 后的同步投影；Task
/// status 仍然只消费 lifecycle runtime node 状态，不成为 cancel/start/continue 的事实源。
pub async fn project_task_view_from_runtime_node_status(
    repos: &RepositorySet,
    task_id: Uuid,
    node_status: RuntimeNodeStatus,
    reason: &str,
    context: serde_json::Value,
) -> Result<Task, TaskViewProjectionError> {
    let mut story = repos
        .story_repo
        .find_by_task_id(task_id)
        .await?
        .ok_or(TaskViewProjectionError::TaskNotFound(task_id))?;
    let previous_status = story.find_task(task_id).map(|task| task.status().clone());
    let changed = story
        .apply_task_projection(task_id, node_status)
        .ok_or(TaskViewProjectionError::TaskNotFound(task_id))?;
    let task = story
        .find_task(task_id)
        .cloned()
        .ok_or(TaskViewProjectionError::TaskNotFound(task_id))?;

    if changed {
        let project_id = story.project_id;
        let story_id = story.id;
        let next_status = task.status().clone();
        repos.story_repo.update(&story).await?;
        repos
            .state_change_repo
            .append_change(
                project_id,
                task_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": reason,
                    "task_id": task_id,
                    "story_id": story_id,
                    "runtime_node_status": node_status,
                    "from": previous_status,
                    "to": next_status,
                    "context": context,
                }),
                None,
            )
            .await?;
    }

    Ok(task)
}

/// 启动期 Task view 投影入口。
///
/// 方向：LifecycleRun/step state → Story.tasks 只读 view。
///
/// 投影链路：
/// `SubjectRef(kind=task)` → `LifecycleSubjectAssociation` → `LifecycleAgent.current_frame`
/// → `RuntimeSessionExecutionAnchor` → `LifecycleRun.orchestrations[].node_tree`
/// → `Story::apply_task_projection`
pub async fn project_task_views_on_boot(
    project_repo: &Arc<dyn ProjectRepository>,
    state_change_repo: &Arc<dyn StateChangeRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    association_repo: &Arc<dyn LifecycleSubjectAssociationRepository>,
    lifecycle_run_repo: &Arc<dyn LifecycleRunRepository>,
    lifecycle_agent_repo: &Arc<dyn LifecycleAgentRepository>,
    execution_anchor_repo: &Arc<dyn RuntimeSessionExecutionAnchorRepository>,
) -> Result<(), TaskViewProjectionError> {
    let projects = project_repo.list_all().await?;
    let mut projected_count: usize = 0;

    for project in projects {
        let stories = story_repo.list_by_project(project.id).await?;
        for story in stories {
            for task in &story.tasks {
                let task_id = task.id;
                let subject = SubjectRef::new("task", task_id);
                let associations = association_repo.list_by_subject(&subject).await?;
                let Some(projection) = resolve_task_runtime_projection(
                    &associations,
                    lifecycle_run_repo,
                    lifecycle_agent_repo,
                    execution_anchor_repo,
                )
                .await?
                else {
                    continue;
                };

                let Some(mut story) = story_repo.find_by_task_id(task_id).await? else {
                    tracing::warn!(
                        task_id = %task_id,
                        "Task view 投影：task 所属 Story 不存在，跳过"
                    );
                    continue;
                };

                let previous_status = story.find_task(task_id).map(|t| t.status().clone());
                let changed = story
                    .apply_task_projection(task_id, projection.node_status)
                    .unwrap_or(false);

                if !changed {
                    continue;
                }

                let project_id = story.project_id;
                let story_id = story.id;
                let next_status = story.find_task(task_id).map(|t| t.status().clone());
                story_repo.update(&story).await?;

                let payload = json!({
                    "reason": "boot_reconcile_subject_association_projection",
                    "task_id": task_id,
                    "story_id": story_id,
                    "run_id": projection.run_id,
                    "association_id": projection.association_id,
                    "orchestration_id": projection.orchestration_id,
                    "node_path": projection.node_path,
                    "node_attempt": projection.node_attempt,
                    "runtime_node_status": projection.node_status,
                    "from": previous_status,
                    "to": next_status,
                });

                if let Err(err) = state_change_repo
                    .append_change(
                        project_id,
                        task_id,
                        ChangeKind::TaskStatusChanged,
                        payload,
                        None,
                    )
                    .await
                {
                    tracing::warn!(
                        task_id = %task_id,
                        run_id = %projection.run_id,
                        error = %err,
                        "Task view 投影：state_change 追加失败（story 已更新）"
                    );
                }

                projected_count += 1;

                tracing::info!(
                    task_id = %task_id,
                    story_id = %story_id,
                    run_id = %projection.run_id,
                    orchestration_id = %projection.orchestration_id,
                    node_path = %projection.node_path,
                    node_attempt = projection.node_attempt,
                    from = ?previous_status,
                    to = ?next_status,
                    "Task view 投影：已从 SubjectAssociation 投影 Task view"
                );
            }
        }
    }

    tracing::info!(
        projected_count,
        "启动阶段 Task view 投影完成（SubjectAssociation 匹配）"
    );
    Ok(())
}

struct TaskRuntimeProjection {
    association_id: Uuid,
    run_id: Uuid,
    orchestration_id: Uuid,
    node_path: String,
    node_attempt: u32,
    node_status: RuntimeNodeStatus,
    observed_at: DateTime<Utc>,
}

async fn resolve_task_runtime_projection(
    associations: &[LifecycleSubjectAssociation],
    lifecycle_run_repo: &Arc<dyn LifecycleRunRepository>,
    lifecycle_agent_repo: &Arc<dyn LifecycleAgentRepository>,
    execution_anchor_repo: &Arc<dyn RuntimeSessionExecutionAnchorRepository>,
) -> Result<Option<TaskRuntimeProjection>, TaskViewProjectionError> {
    let mut latest: Option<TaskRuntimeProjection> = None;

    for association in associations {
        let Some(agent) = resolve_association_agent(association, lifecycle_agent_repo).await?
        else {
            continue;
        };
        let Some(current_frame_id) = agent.current_frame_id else {
            continue;
        };
        let Some(run) = lifecycle_run_repo
            .get_by_id(association.anchor_run_id)
            .await?
        else {
            continue;
        };
        let anchors = execution_anchor_repo.list_by_agent(agent.id).await?;
        for anchor in anchors {
            if anchor.run_id != run.id || anchor.launch_frame_id != current_frame_id {
                continue;
            }
            let Some(projection) =
                projection_from_anchor(association, &run, &agent, current_frame_id, &anchor)
            else {
                continue;
            };
            if latest
                .as_ref()
                .map(|current| projection.observed_at > current.observed_at)
                .unwrap_or(true)
            {
                latest = Some(projection);
            }
        }
    }

    Ok(latest)
}

async fn resolve_association_agent(
    association: &LifecycleSubjectAssociation,
    lifecycle_agent_repo: &Arc<dyn LifecycleAgentRepository>,
) -> Result<Option<LifecycleAgent>, TaskViewProjectionError> {
    if let Some(agent_id) = association.anchor_agent_id {
        let agent = lifecycle_agent_repo.get(agent_id).await?;
        return Ok(agent.filter(|agent| agent.run_id == association.anchor_run_id));
    }
    Ok(lifecycle_agent_repo
        .list_by_run(association.anchor_run_id)
        .await?
        .into_iter()
        .filter(|agent| agent.status == "active")
        .max_by_key(|agent| agent.updated_at))
}

fn projection_from_anchor(
    association: &LifecycleSubjectAssociation,
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    frame_id: Uuid,
    anchor: &agentdash_domain::workflow::RuntimeSessionExecutionAnchor,
) -> Option<TaskRuntimeProjection> {
    let runtime_projection = task_runtime_projection_from_anchor(run, agent, frame_id, anchor)?;
    let node_status = task_projection_status_from_node(runtime_projection.node_status)?;

    Some(TaskRuntimeProjection {
        association_id: association.id,
        run_id: runtime_projection.coordinate.run_id,
        orchestration_id: runtime_projection.coordinate.orchestration_id,
        node_path: runtime_projection.coordinate.node_path,
        node_attempt: runtime_projection.coordinate.attempt,
        node_status,
        observed_at: runtime_projection.observed_at,
    })
}

fn task_projection_status_from_node(status: RuntimeNodeStatus) -> Option<RuntimeNodeStatus> {
    match status {
        RuntimeNodeStatus::Pending => None,
        RuntimeNodeStatus::Ready
        | RuntimeNodeStatus::Claiming
        | RuntimeNodeStatus::Running
        | RuntimeNodeStatus::Blocked
        | RuntimeNodeStatus::Completed
        | RuntimeNodeStatus::Failed
        | RuntimeNodeStatus::Cancelled
        | RuntimeNodeStatus::Skipped => Some(status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use uuid::Uuid;

    use agentdash_domain::DomainError;
    use agentdash_domain::project::{
        Project, ProjectRepository, ProjectSubjectGrant, ProjectSubjectType,
    };
    use agentdash_domain::story::{StateChange, Story};
    use agentdash_domain::task::{Task, TaskStatus};
    use agentdash_domain::workflow::{
        LifecycleAgent, LifecycleRunStatus, LifecycleSubjectAssociation, OrchestrationInstance,
        OrchestrationPlanSnapshot, OrchestrationSourceRef, PlanNode, PlanNodeKind,
        RuntimeNodeState, RuntimeSessionExecutionAnchor, SubjectRef,
    };
    use chrono::Utc;

    // ── In-memory test doubles ──────────────────────────────────

    struct InMemoryProjectRepo {
        projects: Mutex<Vec<Project>>,
    }

    #[async_trait]
    impl ProjectRepository for InMemoryProjectRepo {
        async fn create(&self, project: &Project) -> Result<(), DomainError> {
            self.projects.lock().unwrap().push(project.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError> {
            Ok(self
                .projects
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.id == id)
                .cloned())
        }
        async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
            Ok(self.projects.lock().unwrap().clone())
        }
        async fn update(&self, project: &Project) -> Result<(), DomainError> {
            let mut guard = self.projects.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|p| p.id == project.id) {
                *existing = project.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.projects.lock().unwrap().retain(|p| p.id != id);
            Ok(())
        }
        async fn list_subject_grants(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(vec![])
        }
        async fn upsert_subject_grant(
            &self,
            _grant: &ProjectSubjectGrant,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn delete_subject_grant(
            &self,
            _project_id: Uuid,
            _subject_type: ProjectSubjectType,
            _subject_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct InMemoryStoryRepo {
        stories: Mutex<Vec<Story>>,
    }

    #[async_trait]
    impl StoryRepository for InMemoryStoryRepo {
        async fn create(&self, story: &Story) -> Result<(), DomainError> {
            self.stories.lock().unwrap().push(story.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<Story>, DomainError> {
            Ok(self
                .stories
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.id == id)
                .cloned())
        }
        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError> {
            Ok(self
                .stories
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn update(&self, story: &Story) -> Result<(), DomainError> {
            let mut guard = self.stories.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|s| s.id == story.id) {
                *existing = story.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.stories.lock().unwrap().retain(|s| s.id != id);
            Ok(())
        }
        async fn find_by_task_id(&self, task_id: Uuid) -> Result<Option<Story>, DomainError> {
            Ok(self
                .stories
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.tasks.iter().any(|t| t.id == task_id))
                .cloned())
        }
    }

    struct InMemoryStateChangeRepo {
        changes: Mutex<Vec<(Uuid, Uuid, ChangeKind)>>,
    }

    #[async_trait]
    impl StateChangeRepository for InMemoryStateChangeRepo {
        async fn get_changes_since(
            &self,
            _since_id: i64,
            _limit: i64,
        ) -> Result<Vec<StateChange>, DomainError> {
            Ok(vec![])
        }
        async fn get_changes_since_by_project(
            &self,
            _project_id: Uuid,
            _since_id: i64,
            _limit: i64,
        ) -> Result<Vec<StateChange>, DomainError> {
            Ok(vec![])
        }
        async fn latest_event_id(&self) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn latest_event_id_by_project(&self, _project_id: Uuid) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn append_change(
            &self,
            project_id: Uuid,
            entity_id: Uuid,
            kind: ChangeKind,
            _payload: serde_json::Value,
            _backend_id: Option<&str>,
        ) -> Result<(), DomainError> {
            self.changes
                .lock()
                .unwrap()
                .push((project_id, entity_id, kind));
            Ok(())
        }
        async fn delete_by_project(&self, project_id: Uuid) -> Result<u64, DomainError> {
            let mut changes = self.changes.lock().unwrap();
            let before = changes.len();
            changes.retain(|(change_project_id, _, _)| *change_project_id != project_id);
            Ok((before - changes.len()) as u64)
        }
    }

    struct InMemoryLifecycleRunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for InMemoryLifecycleRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().push(run.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == id)
                .cloned())
        }
        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| ids.contains(&r.id))
                .cloned()
                .collect())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut guard = self.runs.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|r| r.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().retain(|r| r.id != id);
            Ok(())
        }
    }

    struct InMemorySubjectAssociationRepo {
        associations: Mutex<Vec<LifecycleSubjectAssociation>>,
    }

    #[async_trait]
    impl LifecycleSubjectAssociationRepository for InMemorySubjectAssociationRepo {
        async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
            self.associations.lock().unwrap().push(assoc.clone());
            Ok(())
        }
        async fn list_by_subject(
            &self,
            subject: &SubjectRef,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .associations
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.subject_kind == subject.kind && a.subject_id == subject.id)
                .cloned()
                .collect())
        }
        async fn list_by_anchor(
            &self,
            run_id: Uuid,
            agent_id: Option<Uuid>,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .associations
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.anchor_run_id == run_id && a.anchor_agent_id == agent_id)
                .cloned()
                .collect())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.associations.lock().unwrap().retain(|a| a.id != id);
            Ok(())
        }
    }

    struct InMemoryLifecycleAgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for InMemoryLifecycleAgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().unwrap().push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut agents = self.agents.lock().unwrap();
            if let Some(existing) = agents.iter_mut().find(|existing| existing.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    struct InMemoryExecutionAnchorRepo {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for InMemoryExecutionAnchorRepo {
        async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            let mut anchors = self.anchors.lock().unwrap();
            if let Some(existing) = anchors
                .iter_mut()
                .find(|existing| existing.runtime_session_id == anchor.runtime_session_id)
            {
                *existing = anchor.clone();
            } else {
                anchors.push(anchor.clone());
            }
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors
                .lock()
                .unwrap()
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }

        async fn latest_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .max_by_key(|anchor| anchor.updated_at)
                .cloned())
        }
    }

    // ── Fixtures ─────────────────────────────────────────────────

    fn make_run_with_runtime_node_status(
        project_id: Uuid,
        graph_id: Uuid,
        _session_id: &str,
        activity_key: &str,
        target: RuntimeNodeStatus,
    ) -> LifecycleRun {
        let mut run = LifecycleRun::new_control(project_id);
        run.status = LifecycleRunStatus::Running;

        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id,
            graph_version: Some(1),
        };
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_digest: format!("sha256:test-{activity_key}"),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: activity_key.to_string(),
                node_path: activity_key.to_string(),
                parent_node_id: None,
                kind: PlanNodeKind::AgentCall,
                label: None,
                executor: None,
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec![activity_key.to_string()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        let mut orchestration = OrchestrationInstance::new("root", source_ref, plan_snapshot);
        orchestration.status = agentdash_domain::workflow::OrchestrationStatus::Running;
        orchestration.node_tree = vec![RuntimeNodeState {
            node_id: activity_key.to_string(),
            node_path: activity_key.to_string(),
            kind: PlanNodeKind::AgentCall,
            status: target,
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: None,
            children: Vec::new(),
            phase_path: Vec::new(),
            started_at: None,
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        }];
        run.add_orchestration(orchestration);
        run
    }

    fn agent_for_run(run: &LifecycleRun) -> LifecycleAgent {
        let mut agent = LifecycleAgent::new_root(run.id, run.project_id, "task_agent");
        agent.set_current_frame(Uuid::new_v4());
        agent
    }

    fn association_for_task(
        run_id: Uuid,
        agent_id: Uuid,
        task_id: Uuid,
    ) -> LifecycleSubjectAssociation {
        LifecycleSubjectAssociation::new_agent_scoped(
            run_id,
            agent_id,
            &SubjectRef::new("task", task_id),
            "user_initiated",
            None,
        )
    }

    fn anchor_for_runtime_node(
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        orchestration_id: Uuid,
        node_path: &str,
    ) -> RuntimeSessionExecutionAnchor {
        RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            format!("sess-{node_path}"),
            run.id,
            agent.current_frame_id.expect("agent current frame"),
            agent.id,
            orchestration_id,
            node_path,
            1,
        )
    }

    // ── Tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn projects_task_from_active_run_via_subject_association() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);

        let lifecycle_id = Uuid::new_v4();
        let run = make_run_with_runtime_node_status(
            project_id,
            lifecycle_id,
            "sess-boot-running",
            "only",
            RuntimeNodeStatus::Running,
        );
        let agent = agent_for_run(&run);
        let anchor =
            anchor_for_runtime_node(&run, &agent, run.orchestrations[0].orchestration_id, "only");
        let assoc = association_for_task(run.id, agent.id, task_id);

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> = Arc::new(InMemoryStateChangeRepo {
            changes: Mutex::new(Vec::new()),
        });
        let association_repo: Arc<dyn LifecycleSubjectAssociationRepository> =
            Arc::new(InMemorySubjectAssociationRepo {
                associations: Mutex::new(vec![assoc]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });
        let lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository> =
            Arc::new(InMemoryLifecycleAgentRepo {
                agents: Mutex::new(vec![agent]),
            });
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(InMemoryExecutionAnchorRepo {
                anchors: Mutex::new(vec![anchor]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &lifecycle_agent_repo,
            &execution_anchor_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Running,
            "step=Running → task=Running"
        );
    }

    #[tokio::test]
    async fn projects_task_from_completed_step_to_awaiting_verification() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);
        story.force_set_task_status(task_id, TaskStatus::Running);

        let lifecycle_id = Uuid::new_v4();
        let run = make_run_with_runtime_node_status(
            project_id,
            lifecycle_id,
            "sess-boot-completed",
            "only",
            RuntimeNodeStatus::Completed,
        );
        let agent = agent_for_run(&run);
        let anchor =
            anchor_for_runtime_node(&run, &agent, run.orchestrations[0].orchestration_id, "only");
        let assoc = association_for_task(run.id, agent.id, task_id);

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> = Arc::new(InMemoryStateChangeRepo {
            changes: Mutex::new(Vec::new()),
        });
        let association_repo: Arc<dyn LifecycleSubjectAssociationRepository> =
            Arc::new(InMemorySubjectAssociationRepo {
                associations: Mutex::new(vec![assoc]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });
        let lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository> =
            Arc::new(InMemoryLifecycleAgentRepo {
                agents: Mutex::new(vec![agent]),
            });
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(InMemoryExecutionAnchorRepo {
                anchors: Mutex::new(vec![anchor]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &lifecycle_agent_repo,
            &execution_anchor_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::AwaitingVerification
        );
    }

    #[tokio::test]
    async fn projects_task_from_cancelled_step_to_cancelled() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);
        story.force_set_task_status(task_id, TaskStatus::Running);

        let lifecycle_id = Uuid::new_v4();
        let run = make_run_with_runtime_node_status(
            project_id,
            lifecycle_id,
            "sess-boot-cancelled",
            "only",
            RuntimeNodeStatus::Cancelled,
        );
        let agent = agent_for_run(&run);
        let anchor =
            anchor_for_runtime_node(&run, &agent, run.orchestrations[0].orchestration_id, "only");
        let assoc = association_for_task(run.id, agent.id, task_id);

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> = Arc::new(InMemoryStateChangeRepo {
            changes: Mutex::new(Vec::new()),
        });
        let association_repo: Arc<dyn LifecycleSubjectAssociationRepository> =
            Arc::new(InMemorySubjectAssociationRepo {
                associations: Mutex::new(vec![assoc]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });
        let lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository> =
            Arc::new(InMemoryLifecycleAgentRepo {
                agents: Mutex::new(vec![agent]),
            });
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(InMemoryExecutionAnchorRepo {
                anchors: Mutex::new(vec![anchor]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &lifecycle_agent_repo,
            &execution_anchor_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn running_task_without_lifecycle_fact_stays_unchanged() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);
        story.force_set_task_status(task_id, TaskStatus::Running);

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> = Arc::new(InMemoryStateChangeRepo {
            changes: Mutex::new(Vec::new()),
        });
        let association_repo: Arc<dyn LifecycleSubjectAssociationRepository> =
            Arc::new(InMemorySubjectAssociationRepo {
                associations: Mutex::new(vec![]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![]),
            });
        let lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository> =
            Arc::new(InMemoryLifecycleAgentRepo {
                agents: Mutex::new(vec![]),
            });
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(InMemoryExecutionAnchorRepo {
                anchors: Mutex::new(vec![]),
            });
        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &lifecycle_agent_repo,
            &execution_anchor_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Running,
            "没有 lifecycle runtime fact 时不从缺失关系推断终态"
        );
    }

    #[tokio::test]
    async fn task_association_without_runtime_anchor_stays_unchanged() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);

        let lifecycle_id = Uuid::new_v4();
        let run = make_run_with_runtime_node_status(
            project_id,
            lifecycle_id,
            "sess-boot-no-anchor",
            "only",
            RuntimeNodeStatus::Running,
        );
        let agent = agent_for_run(&run);
        let assoc = association_for_task(run.id, agent.id, task_id);

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> = Arc::new(InMemoryStateChangeRepo {
            changes: Mutex::new(Vec::new()),
        });
        let association_repo: Arc<dyn LifecycleSubjectAssociationRepository> =
            Arc::new(InMemorySubjectAssociationRepo {
                associations: Mutex::new(vec![assoc]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });
        let lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository> =
            Arc::new(InMemoryLifecycleAgentRepo {
                agents: Mutex::new(vec![agent]),
            });
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(InMemoryExecutionAnchorRepo {
                anchors: Mutex::new(vec![]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &lifecycle_agent_repo,
            &execution_anchor_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Pending,
            "缺少 runtime anchor 不应影响 task 状态"
        );
    }

    #[tokio::test]
    async fn task_without_association_stays_unchanged() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);

        let lifecycle_id = Uuid::new_v4();
        let run = make_run_with_runtime_node_status(
            project_id,
            lifecycle_id,
            "sess-boot-no-assoc",
            "only",
            RuntimeNodeStatus::Running,
        );

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> = Arc::new(InMemoryStateChangeRepo {
            changes: Mutex::new(Vec::new()),
        });
        let association_repo: Arc<dyn LifecycleSubjectAssociationRepository> =
            Arc::new(InMemorySubjectAssociationRepo {
                associations: Mutex::new(vec![]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });
        let lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository> =
            Arc::new(InMemoryLifecycleAgentRepo {
                agents: Mutex::new(vec![]),
            });
        let execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository> =
            Arc::new(InMemoryExecutionAnchorRepo {
                anchors: Mutex::new(vec![]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &lifecycle_agent_repo,
            &execution_anchor_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Pending,
            "无 association 的 task 保持原状"
        );
    }
}
