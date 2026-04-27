//! 启动期 Task view 投影器 — 从 LifecycleRun/step state 反投影到 `Story.tasks[i].status`。
//!
//! **方向**：session/lifecycle 真相源 → Task view（只读投影），属于 projection 方向。
//! 对应运行期反向（业务终态 → session cancel）的 command 通道见
//! [`crate::reconcile::terminal_cancel`]。
//!
//! M2-c（Model C）：真相源 = LifecycleRun 的 step state；Task view 仅为只读投影。
//! 启动流程在此处按 Scheme A（见 research/m2-decision-points.md D-M2-5）实现：
//!
//! 1. 遍历所有 project 的 active LifecycleRun（`Ready | Running | Blocked`）
//! 2. 扁平化 `run.step_states`，对每个持有 `task_id` 绑定的 step，调
//!    `Story::apply_task_projection` 将 step state 反投影到 task view
//! 3. 同步 append 一条 `state_changes` 全局投影索引（`kind = TaskStatusChanged`）
//! 4. 对于仍处于 `Running` 但没有任何活跃 run 覆盖的孤儿 task，作为 fallback
//!    置为 `Failed`
//!
//! 注：M2-c 放弃 `TaskSessionStateReader` 的 session turn state 查询路径；
//! session turn state 只是派生结果，已不再是真相源。

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{ChangeKind, StateChangeRepository, StoryRepository};
use agentdash_domain::task::TaskStatus;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, LifecycleRunStatus,
};

#[derive(Debug, thiserror::Error)]
pub enum TaskViewProjectionError {
    #[error(transparent)]
    Domain(#[from] agentdash_domain::DomainError),
}

/// 启动期 Task view 投影入口（Scheme A）。
///
/// 方向：LifecycleRun/step state → Story.tasks 只读 view。
///
/// 参数：
/// - `project_repo` / `story_repo` / `state_change_repo`：基础领域仓储
/// - `lifecycle_def_repo`：lifecycle definition 仓储（反查 step.task_id 绑定）
/// - `lifecycle_run_repo`：lifecycle run 仓储（本次投影的事实源）
pub async fn project_task_views_on_boot(
    project_repo: &Arc<dyn ProjectRepository>,
    state_change_repo: &Arc<dyn StateChangeRepository>,
    story_repo: &Arc<dyn StoryRepository>,
    lifecycle_def_repo: &Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: &Arc<dyn LifecycleRunRepository>,
) -> Result<(), TaskViewProjectionError> {
    let projects = project_repo.list_all().await?;
    let mut projected_from_step: usize = 0;
    let mut orphan_fallback: usize = 0;

    // 记录本次已投影到的 task_id，用于识别"孤儿 Running task"。
    let mut covered_tasks: HashSet<uuid::Uuid> = HashSet::new();

    for project in projects {
        // Phase 1 — Scheme A：按 project 拉取所有 run，过滤活跃者并投影 step states。
        let runs = lifecycle_run_repo.list_by_project(project.id).await?;
        for run in runs {
            if !is_run_active(run.status) {
                continue;
            }

            // 加载 lifecycle definition，一次性拿到 step.task_id 映射
            let lifecycle = match lifecycle_def_repo.get_by_id(run.lifecycle_id).await? {
                Some(lc) => lc,
                None => {
                    tracing::warn!(
                        run_id = %run.id,
                        lifecycle_id = %run.lifecycle_id,
                        "Task view 投影：lifecycle definition 不存在，跳过 run"
                    );
                    continue;
                }
            };

            for state in &run.step_states {
                let Some(task_id) = lifecycle
                    .steps
                    .iter()
                    .find(|s| s.key == state.step_key)
                    .and_then(|s| s.task_id)
                else {
                    continue; // 非 task-bound step
                };

                let Some(mut story) = story_repo.find_by_task_id(task_id).await? else {
                    tracing::warn!(
                        task_id = %task_id,
                        run_id = %run.id,
                        step_key = %state.step_key,
                        "Task view 投影：step 绑定的 task 对应 story 不存在，跳过"
                    );
                    continue;
                };

                let previous_status = story.find_task(task_id).map(|t| t.status().clone());
                let changed = story.apply_task_projection(task_id, state).unwrap_or(false);
                covered_tasks.insert(task_id);

                if !changed {
                    continue;
                }

                let project_id = story.project_id;
                let story_id = story.id;
                let next_status = story.find_task(task_id).map(|t| t.status().clone());
                story_repo.update(&story).await?;

                let payload = json!({
                    "reason": "boot_reconcile_step_projection",
                    "task_id": task_id,
                    "story_id": story_id,
                    "run_id": run.id,
                    "lifecycle_id": run.lifecycle_id,
                    "step_key": state.step_key,
                    "step_status": state.status,
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
                    // 不阻塞主流程，与 projector tx-b 语义一致
                    tracing::warn!(
                        task_id = %task_id,
                        run_id = %run.id,
                        error = %err,
                        "Task view 投影：state_change 追加失败（story 已更新）"
                    );
                }

                projected_from_step += 1;

                tracing::info!(
                    task_id = %task_id,
                    story_id = %story_id,
                    run_id = %run.id,
                    step_key = %state.step_key,
                    from = ?previous_status,
                    to = ?next_status,
                    "Task view 投影：已从 step state 投影 Task view"
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

                // 孤儿 Running task：没有活跃 lifecycle run 覆盖；判定为 Interrupted 语义。
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
        projected_from_step,
        orphan_fallback,
        "启动阶段 Task view 投影完成（Scheme A · step state → task view）"
    );
    Ok(())
}

fn is_run_active(status: LifecycleRunStatus) -> bool {
    matches!(
        status,
        LifecycleRunStatus::Ready | LifecycleRunStatus::Running | LifecycleRunStatus::Blocked
    )
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
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, LifecycleStepExecutionStatus,
        WorkflowBindingKind, WorkflowDefinitionSource,
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
        async fn latest_event_id_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<i64, DomainError> {
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

    struct InMemoryLifecycleDefRepo {
        definitions: Mutex<Vec<LifecycleDefinition>>,
    }

    #[async_trait]
    impl LifecycleDefinitionRepository for InMemoryLifecycleDefRepo {
        async fn create(&self, def: &LifecycleDefinition) -> Result<(), DomainError> {
            self.definitions.lock().unwrap().push(def.clone());
            Ok(())
        }
        async fn get_by_id(
            &self,
            id: Uuid,
        ) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == id)
                .cloned())
        }
        async fn get_by_key(
            &self,
            key: &str,
        ) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.key == key)
                .cloned())
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.project_id == project_id && d.key == key)
                .cloned())
        }
        async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self.definitions.lock().unwrap().clone())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .filter(|d| d.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_by_binding_kind(
            &self,
            binding_kind: WorkflowBindingKind,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .filter(|d| d.binding_kind == binding_kind)
                .cloned()
                .collect())
        }
        async fn update(&self, def: &LifecycleDefinition) -> Result<(), DomainError> {
            let mut guard = self.definitions.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|d| d.id == def.id) {
                *existing = def.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.definitions.lock().unwrap().retain(|d| d.id != id);
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
        async fn list_by_lifecycle(
            &self,
            lifecycle_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.lifecycle_id == lifecycle_id)
                .cloned()
                .collect())
        }
        async fn list_by_session(
            &self,
            session_id: &str,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.session_id == session_id)
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

    // ── Fixtures ─────────────────────────────────────────────────

    fn make_step(key: &str, task_id: Option<Uuid>) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: key.to_string(),
            description: String::new(),
            workflow_key: None,
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
            task_id,
        }
    }

    /// 构造一个 run 并把 step_states 手动覆盖为测试目标状态。
    fn make_run_with_step_status(
        project_id: Uuid,
        lifecycle: &LifecycleDefinition,
        session_id: &str,
        step_key: &str,
        target: LifecycleStepExecutionStatus,
    ) -> LifecycleRun {
        let mut run = LifecycleRun::new(
            project_id,
            lifecycle.id,
            session_id,
            &lifecycle.steps,
            &lifecycle.entry_step_key,
            &lifecycle.edges,
        )
        .expect("run");
        if let Some(state) = run.step_states.iter_mut().find(|s| s.step_key == step_key) {
            state.status = target;
        }
        run.status = LifecycleRunStatus::Running;
        run
    }

    // ── Tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn projects_task_from_active_run_running_step() {
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);

        let steps = vec![make_step("only", Some(task_id))];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");

        let run = make_run_with_step_status(
            project_id,
            &lifecycle,
            "sess-boot-running",
            "only",
            LifecycleStepExecutionStatus::Running,
        );

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo {
                changes: Mutex::new(Vec::new()),
            });
        let lifecycle_def_repo: Arc<dyn LifecycleDefinitionRepository> =
            Arc::new(InMemoryLifecycleDefRepo {
                definitions: Mutex::new(vec![lifecycle.clone()]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &lifecycle_def_repo,
            &lifecycle_run_repo,
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
        // 模拟旧状态为 Running，实际 step 已 Completed，投影应把 task 推进到 AwaitingVerification
        story.force_set_task_status(task_id, TaskStatus::Running);

        let steps = vec![make_step("only", Some(task_id))];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");

        let run = make_run_with_step_status(
            project_id,
            &lifecycle,
            "sess-boot-completed",
            "only",
            LifecycleStepExecutionStatus::Completed,
        );

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo {
                changes: Mutex::new(Vec::new()),
            });
        let lifecycle_def_repo: Arc<dyn LifecycleDefinitionRepository> =
            Arc::new(InMemoryLifecycleDefRepo {
                definitions: Mutex::new(vec![lifecycle]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &lifecycle_def_repo,
            &lifecycle_run_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::AwaitingVerification,
            "step=Completed → task=AwaitingVerification（verification 由业务推进）"
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
        // 人为把 task 置为 Running，但没有任何活跃 run
        story.force_set_task_status(task_id, TaskStatus::Running);

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo {
                changes: Mutex::new(Vec::new()),
            });
        let lifecycle_def_repo: Arc<dyn LifecycleDefinitionRepository> =
            Arc::new(InMemoryLifecycleDefRepo {
                definitions: Mutex::new(vec![]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &lifecycle_def_repo,
            &lifecycle_run_repo,
        )
        .await
        .expect("reconcile ok");

        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Failed,
            "孤儿 Running task（无活跃 run）→ Failed"
        );
    }

    #[tokio::test]
    async fn inactive_run_does_not_project() {
        // 非活跃 run（Completed）不应触发投影，task 保留原状
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);

        let steps = vec![make_step("only", Some(task_id))];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");

        let mut run = make_run_with_step_status(
            project_id,
            &lifecycle,
            "sess-boot-inactive",
            "only",
            LifecycleStepExecutionStatus::Running,
        );
        run.status = LifecycleRunStatus::Completed; // 非活跃

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo {
                changes: Mutex::new(Vec::new()),
            });
        let lifecycle_def_repo: Arc<dyn LifecycleDefinitionRepository> =
            Arc::new(InMemoryLifecycleDefRepo {
                definitions: Mutex::new(vec![lifecycle]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &lifecycle_def_repo,
            &lifecycle_run_repo,
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
    async fn step_without_task_binding_is_skipped_and_task_survives() {
        // active run + step 无 task_id 绑定；task=Running 且不在此 run 内 → 走 orphan fallback
        let project = Project::new("P".into(), "".into());
        let project_id = project.id;

        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);

        let steps = vec![make_step("only", None)];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");

        let run = make_run_with_step_status(
            project_id,
            &lifecycle,
            "sess-boot-nobinding",
            "only",
            LifecycleStepExecutionStatus::Running,
        );

        let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepo {
            projects: Mutex::new(vec![project]),
        });
        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo {
            stories: Mutex::new(vec![story]),
        });
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo {
                changes: Mutex::new(Vec::new()),
            });
        let lifecycle_def_repo: Arc<dyn LifecycleDefinitionRepository> =
            Arc::new(InMemoryLifecycleDefRepo {
                definitions: Mutex::new(vec![lifecycle]),
            });
        let lifecycle_run_repo: Arc<dyn LifecycleRunRepository> =
            Arc::new(InMemoryLifecycleRunRepo {
                runs: Mutex::new(vec![run]),
            });

        project_task_views_on_boot(
            &project_repo,
            &state_change_repo,
            &story_repo,
            &lifecycle_def_repo,
            &lifecycle_run_repo,
        )
        .await
        .expect("reconcile ok");

        // task 状态未因 Running 进入 fallback（初始 Pending）
        let after = story_repo.find_by_task_id(task_id).await.unwrap().unwrap();
        assert_eq!(
            *after.find_task(task_id).unwrap().status(),
            TaskStatus::Pending,
            "task 初始 Pending 且未被 step 绑定 → 保持不变"
        );
    }
}
