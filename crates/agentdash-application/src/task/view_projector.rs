//! 启动期 Task view 投影器 — 从 WorkflowGraphInstance activity state 反投影到 `Story.tasks[i].status`。
//!
//! **方向**：WorkflowGraphInstance 真相源 → Task view（只读投影），属于 projection 方向。
//! 对应运行期反向（业务终态 → session cancel）的 command 通道见
//! [`crate::reconcile::terminal_cancel`]。
//!
//! 真相源 = WorkflowGraphInstance.activity_state；Task view 仅为只读投影。
//!
//! 投影匹配策略（B5a）：
//! 通过 `LifecycleSubjectAssociation(subject_kind=task, subject_id=task_id)`
//! 关联 run → activity state。
//!
//! 1. 遍历所有 project 的 active LifecycleRun
//! 2. 通过 `LifecycleSubjectAssociation` 找到 task_id → run 的映射
//! 3. 对每个关联的 task，从 run 下 graph instances 的 activity_state 取最新 attempt 状态投影到 task view
//! 4. 对于仍处于 `Running` 但没有任何活跃 run 覆盖的孤儿 task，fallback 置为 `Failed`

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{ChangeKind, StateChangeRepository, StoryRepository};
use agentdash_domain::task::{Task, TaskStatus};
use agentdash_domain::workflow::{
    ActivityAttemptState, LifecycleRunRepository, LifecycleRunStatus,
    LifecycleSubjectAssociationRepository, WorkflowGraphInstance, WorkflowGraphInstanceRepository,
};

#[derive(Debug, thiserror::Error)]
pub enum TaskViewProjectionError {
    #[error(transparent)]
    Domain(#[from] agentdash_domain::DomainError),
    #[error("Task {0} 不存在")]
    TaskNotFound(Uuid),
}

/// 将一个明确的 ActivityAttempt 状态投影到 Task view。
///
/// 入口用于 runtime command 已经推进 WorkflowGraphInstance 后的同步投影；Task
/// status 仍然只消费 lifecycle attempt 状态，不成为 cancel/start/continue 的事实源。
pub async fn project_task_view_from_attempt_status(
    repos: &RepositorySet,
    task_id: Uuid,
    attempt_status: agentdash_domain::workflow::ActivityAttemptStatus,
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
        .apply_task_projection(task_id, attempt_status)
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
                    "attempt_status": attempt_status,
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
/// `LifecycleSubjectAssociation(kind=task)` → run → `WorkflowGraphInstance.activity_state` → `Story::apply_task_projection`
pub async fn project_task_views_on_boot(
    project_repo: &Arc<dyn ProjectRepository>,
    state_change_repo: &Arc<dyn StateChangeRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    association_repo: &Arc<dyn LifecycleSubjectAssociationRepository>,
    lifecycle_run_repo: &Arc<dyn LifecycleRunRepository>,
    workflow_graph_instance_repo: &Arc<dyn WorkflowGraphInstanceRepository>,
) -> Result<(), TaskViewProjectionError> {
    let projects = project_repo.list_all().await?;
    let mut projected_count: usize = 0;
    let mut orphan_fallback: usize = 0;

    let mut covered_tasks: HashSet<uuid::Uuid> = HashSet::new();

    for project in projects {
        let runs = lifecycle_run_repo.list_by_project(project.id).await?;

        // Phase 1 — 通过 SubjectAssociation 匹配 task → run 投影
        for run in &runs {
            if !is_run_active(run.status) {
                continue;
            }

            // 查找该 run 下所有 task subject associations
            let run_associations = association_repo.list_by_anchor(run.id, None).await?;

            let task_associations: Vec<_> = run_associations
                .into_iter()
                .filter(|a| a.subject_kind == "task")
                .collect();

            if task_associations.is_empty() {
                continue;
            }

            let graph_instances = workflow_graph_instance_repo.list_by_run(run.id).await?;
            let attempts = lifecycle_task_projection_states(&graph_instances);
            if attempts.is_empty() {
                continue;
            }

            // 取最新 attempt 状态（最后一条 attempt 作为当前投影源）
            let latest_attempt = attempts.last().cloned();

            for assoc in &task_associations {
                let task_id = assoc.subject_id;
                let Some(ref attempt) = latest_attempt else {
                    continue;
                };

                // 通过 story 聚合查找 task 并投影
                let Some(mut story) = story_repo.find_by_task_id(task_id).await? else {
                    tracing::warn!(
                        task_id = %task_id,
                        run_id = %run.id,
                        "Task view 投影：task 所属 Story 不存在，跳过"
                    );
                    continue;
                };

                let previous_status = story.find_task(task_id).map(|t| t.status().clone());
                let changed = story
                    .apply_task_projection(task_id, attempt.status)
                    .unwrap_or(false);
                covered_tasks.insert(task_id);

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
                    "run_id": run.id,
                    "association_id": assoc.id,
                    "attempt_status": attempt.status,
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
                        run_id = %run.id,
                        error = %err,
                        "Task view 投影：state_change 追加失败（story 已更新）"
                    );
                }

                projected_count += 1;

                tracing::info!(
                    task_id = %task_id,
                    story_id = %story_id,
                    run_id = %run.id,
                    from = ?previous_status,
                    to = ?next_status,
                    "Task view 投影：已从 SubjectAssociation 投影 Task view"
                );
            }
        }

        // Phase 2 — fallback：处理孤儿 Running task（没有任何活跃 run 覆盖）
        let stories = story_repo.list_by_project(project.id).await?;
        for story in stories {
            for task in &story.tasks {
                if task.status() != &TaskStatus::Running {
                    continue;
                }
                if covered_tasks.contains(&task.id) {
                    continue;
                }

                let next_status = TaskStatus::Failed;
                let reason = "boot_reconcile_orphan_running";
                let previous_status = task.status().clone();
                let task_id = task.id;
                let mut story_mut = match story_repo.get_by_id(story.id).await? {
                    Some(s) => s,
                    None => continue,
                };
                let applied = story_mut.force_set_task_status(task_id, next_status.clone());
                if !matches!(applied, Some(true)) {
                    continue;
                }
                let project_id = story_mut.project_id;
                let story_id = story_mut.id;
                story_repo.update(&story_mut).await?;

                if let Err(err) = state_change_repo
                    .append_change(
                        project_id,
                        task_id,
                        ChangeKind::TaskStatusChanged,
                        json!({
                            "reason": reason,
                            "task_id": task_id,
                            "story_id": story_id,
                            "from": previous_status,
                            "to": next_status,
                        }),
                        None,
                    )
                    .await
                {
                    tracing::warn!(
                        task_id = %task_id,
                        error = %err,
                        "Task view 投影 fallback：state_change 追加失败"
                    );
                }

                orphan_fallback += 1;

                tracing::info!(
                    task_id = %task_id,
                    story_id = %story_id,
                    reason = reason,
                    from = ?previous_status,
                    to = ?next_status,
                    "Task view 投影 fallback：孤儿 Running task 已回收"
                );
            }
        }
    }

    tracing::info!(
        projected_count,
        orphan_fallback,
        "启动阶段 Task view 投影完成（SubjectAssociation 匹配）"
    );
    Ok(())
}

fn is_run_active(status: LifecycleRunStatus) -> bool {
    matches!(
        status,
        LifecycleRunStatus::Ready | LifecycleRunStatus::Running | LifecycleRunStatus::Blocked
    )
}

fn lifecycle_task_projection_states(
    graph_instances: &[WorkflowGraphInstance],
) -> Vec<ActivityAttemptState> {
    graph_instances
        .iter()
        .filter_map(|instance| instance.activity_state.as_ref())
        .flat_map(|state| state.attempts.iter().cloned())
        .collect()
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
    use agentdash_domain::task::Task;
    use agentdash_domain::workflow::{
        ActivityAttemptState, ActivityAttemptStatus, ActivityLifecycleRunState, ActivityRunStatus,
        LifecycleRun, LifecycleSubjectAssociation, SubjectRef, WorkflowGraphInstance,
        WorkflowGraphInstanceRepository,
    };

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
        async fn list_by_root_graph(
            &self,
            root_graph_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.root_graph_id == Some(root_graph_id))
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

    struct InMemoryWorkflowGraphInstanceRepo {
        instances: Mutex<Vec<WorkflowGraphInstance>>,
    }

    #[async_trait]
    impl WorkflowGraphInstanceRepository for InMemoryWorkflowGraphInstanceRepo {
        async fn create(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            self.instances.lock().unwrap().push(instance.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .instances
                .lock()
                .unwrap()
                .iter()
                .find(|instance| instance.id == id)
                .cloned())
        }

        async fn get_by_run_and_id(
            &self,
            run_id: Uuid,
            id: Uuid,
        ) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .instances
                .lock()
                .unwrap()
                .iter()
                .find(|instance| instance.run_id == run_id && instance.id == id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .instances
                .lock()
                .unwrap()
                .iter()
                .filter(|instance| instance.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            let mut guard = self.instances.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|item| item.id == instance.id) {
                *existing = instance.clone();
            }
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

    // ── Fixtures ─────────────────────────────────────────────────

    fn make_run_with_activity_status(
        project_id: Uuid,
        root_graph_id: Uuid,
        _session_id: &str,
        activity_key: &str,
        target: ActivityAttemptStatus,
    ) -> (LifecycleRun, WorkflowGraphInstance) {
        let mut run = LifecycleRun::new_control(project_id, root_graph_id);
        run.status = LifecycleRunStatus::Running;
        let mut graph_instance = WorkflowGraphInstance::new_root(run.id, root_graph_id);
        let state = ActivityLifecycleRunState {
            graph_instance_id: graph_instance.id,
            status: ActivityRunStatus::Running,
            attempts: vec![ActivityAttemptState {
                activity_key: activity_key.to_string(),
                attempt: 1,
                status: target,
                executor_run: None,
                started_at: None,
                completed_at: None,
                summary: None,
            }],
            outputs: Vec::new(),
            inputs: Vec::new(),
        };
        graph_instance
            .replace_activity_state(state)
            .expect("graph instance state");
        if let Some(state) = graph_instance.activity_state.as_ref() {
            run.sync_graph_instance_activity_projections([(graph_instance.id, state)]);
        }
        (run, graph_instance)
    }

    fn association_for_task(run_id: Uuid, task_id: Uuid) -> LifecycleSubjectAssociation {
        LifecycleSubjectAssociation::new_run_scoped(
            run_id,
            &SubjectRef::new("task", task_id),
            "user_initiated",
            None,
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
        let (run, graph_instance) = make_run_with_activity_status(
            project_id,
            lifecycle_id,
            "sess-boot-running",
            "only",
            ActivityAttemptStatus::Running,
        );
        let assoc = association_for_task(run.id, task_id);

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
        let workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository> =
            Arc::new(InMemoryWorkflowGraphInstanceRepo {
                instances: Mutex::new(vec![graph_instance]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &workflow_graph_instance_repo,
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
        let (run, graph_instance) = make_run_with_activity_status(
            project_id,
            lifecycle_id,
            "sess-boot-completed",
            "only",
            ActivityAttemptStatus::Completed,
        );
        let assoc = association_for_task(run.id, task_id);

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
        let workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository> =
            Arc::new(InMemoryWorkflowGraphInstanceRepo {
                instances: Mutex::new(vec![graph_instance]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &workflow_graph_instance_repo,
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
        let (run, graph_instance) = make_run_with_activity_status(
            project_id,
            lifecycle_id,
            "sess-boot-cancelled",
            "only",
            ActivityAttemptStatus::Cancelled,
        );
        let assoc = association_for_task(run.id, task_id);

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
        let workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository> =
            Arc::new(InMemoryWorkflowGraphInstanceRepo {
                instances: Mutex::new(vec![graph_instance]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &workflow_graph_instance_repo,
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
    async fn orphan_running_task_without_active_run_falls_back_to_failed() {
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
        let workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository> =
            Arc::new(InMemoryWorkflowGraphInstanceRepo {
                instances: Mutex::new(vec![]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &workflow_graph_instance_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Failed,
            "孤儿 Running task → Failed"
        );
    }

    #[tokio::test]
    async fn inactive_run_does_not_project() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);

        let lifecycle_id = Uuid::new_v4();
        let (mut run, graph_instance) = make_run_with_activity_status(
            project_id,
            lifecycle_id,
            "sess-boot-inactive",
            "only",
            ActivityAttemptStatus::Running,
        );
        run.status = LifecycleRunStatus::Completed;
        let assoc = association_for_task(run.id, task_id);

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
        let workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository> =
            Arc::new(InMemoryWorkflowGraphInstanceRepo {
                instances: Mutex::new(vec![graph_instance]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &workflow_graph_instance_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Pending,
            "非活跃 run 不应影响 task 状态"
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
        let (run, graph_instance) = make_run_with_activity_status(
            project_id,
            lifecycle_id,
            "sess-boot-no-assoc",
            "only",
            ActivityAttemptStatus::Running,
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
        let workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository> =
            Arc::new(InMemoryWorkflowGraphInstanceRepo {
                instances: Mutex::new(vec![graph_instance]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &association_repo,
            &lifecycle_run_repo,
            &workflow_graph_instance_repo,
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
