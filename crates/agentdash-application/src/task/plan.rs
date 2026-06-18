use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleRun, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    LifecycleTaskPlanItem, LifecycleTaskPlanItemDraft, LifecycleTaskPlanItemPatch, SubjectRef,
    TaskPlanStatus,
};

use crate::ApplicationError;

#[derive(Debug, Clone, Default)]
pub struct RunTaskPlanFilter {
    pub created_by_agent_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
    pub include_archived: bool,
}

#[derive(Debug, Clone)]
pub struct RunTaskPlanView {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub tasks: Vec<LifecycleTaskPlanItem>,
}

#[derive(Debug, Clone)]
pub struct RunTaskCommandResult {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub task: LifecycleTaskPlanItem,
}

#[derive(Debug, Clone)]
pub struct LocatedTaskPlanItem {
    pub run: LifecycleRun,
    pub task: LifecycleTaskPlanItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskPlanPolicyAction {
    Create,
    Update,
    Assign,
    Review,
    Done,
    Archive,
    StatusTransition(TaskPlanStatus),
}

#[derive(Debug, Clone)]
pub struct TaskPlanPolicyHook<'a> {
    pub action: TaskPlanPolicyAction,
    pub run: &'a LifecycleRun,
    pub task_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryTaskProjectionSourceKind {
    OwningRun,
    LinkedRun,
    StoryRef,
}

#[derive(Debug, Clone)]
pub struct StoryTaskProjectionSourceView {
    pub kind: StoryTaskProjectionSourceKind,
    pub run_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub story_ref: Option<SubjectRef>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct StoryTaskProjectionItemView {
    pub project_id: Uuid,
    pub owning_run_id: Uuid,
    pub task: LifecycleTaskPlanItem,
    pub sources: Vec<StoryTaskProjectionSourceView>,
}

#[derive(Debug, Clone)]
pub struct StoryTaskProjectionView {
    pub story_id: Uuid,
    pub tasks: Vec<StoryTaskProjectionItemView>,
}

pub async fn list_run_tasks(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
    filter: RunTaskPlanFilter,
) -> Result<RunTaskPlanView, ApplicationError> {
    let run = load_run(lifecycle_run_repo, run_id).await?;
    let tasks = filter_tasks(run.tasks.clone(), &filter);
    Ok(RunTaskPlanView {
        project_id: run.project_id,
        run_id: run.id,
        tasks,
    })
}

pub async fn create_run_task(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
    draft: LifecycleTaskPlanItemDraft,
) -> Result<RunTaskCommandResult, ApplicationError> {
    let mut run = load_run(lifecycle_run_repo, run_id).await?;
    ensure_task_plan_policy_allowed(TaskPlanPolicyHook {
        action: TaskPlanPolicyAction::Create,
        run: &run,
        task_id: draft.id,
    })?;
    let task = run.create_task(draft).map_err(ApplicationError::from)?;
    lifecycle_run_repo
        .update(&run)
        .await
        .map_err(ApplicationError::from)?;
    Ok(RunTaskCommandResult {
        project_id: run.project_id,
        run_id: run.id,
        task,
    })
}

pub async fn update_run_task(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
    task_id: Uuid,
    patch: LifecycleTaskPlanItemPatch,
) -> Result<RunTaskCommandResult, ApplicationError> {
    let mut run = load_run(lifecycle_run_repo, run_id).await?;
    let action = if patch.assigned_agent_id.is_some() {
        TaskPlanPolicyAction::Assign
    } else {
        TaskPlanPolicyAction::Update
    };
    ensure_task_plan_policy_allowed(TaskPlanPolicyHook {
        action,
        run: &run,
        task_id: Some(task_id),
    })?;
    let task = run
        .update_task(task_id, patch)
        .map_err(ApplicationError::from)?;
    lifecycle_run_repo
        .update(&run)
        .await
        .map_err(ApplicationError::from)?;
    Ok(RunTaskCommandResult {
        project_id: run.project_id,
        run_id: run.id,
        task,
    })
}

pub async fn archive_run_task(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
    task_id: Uuid,
) -> Result<RunTaskCommandResult, ApplicationError> {
    let mut run = load_run(lifecycle_run_repo, run_id).await?;
    ensure_task_plan_policy_allowed(TaskPlanPolicyHook {
        action: TaskPlanPolicyAction::Archive,
        run: &run,
        task_id: Some(task_id),
    })?;
    let task = run.archive_task(task_id).map_err(ApplicationError::from)?;
    lifecycle_run_repo
        .update(&run)
        .await
        .map_err(ApplicationError::from)?;
    Ok(RunTaskCommandResult {
        project_id: run.project_id,
        run_id: run.id,
        task,
    })
}

pub async fn transition_run_task_status(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
    task_id: Uuid,
    status: agentdash_domain::workflow::TaskPlanStatus,
) -> Result<RunTaskCommandResult, ApplicationError> {
    let mut run = load_run(lifecycle_run_repo, run_id).await?;
    let action = match status {
        TaskPlanStatus::Review => TaskPlanPolicyAction::Review,
        TaskPlanStatus::Done => TaskPlanPolicyAction::Done,
        other => TaskPlanPolicyAction::StatusTransition(other),
    };
    ensure_task_plan_policy_allowed(TaskPlanPolicyHook {
        action,
        run: &run,
        task_id: Some(task_id),
    })?;
    let task = run
        .transition_task_status(task_id, status)
        .map_err(ApplicationError::from)?;
    lifecycle_run_repo
        .update(&run)
        .await
        .map_err(ApplicationError::from)?;
    Ok(RunTaskCommandResult {
        project_id: run.project_id,
        run_id: run.id,
        task,
    })
}

pub async fn reorder_run_tasks(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
    ordered_task_ids: Vec<Uuid>,
) -> Result<RunTaskPlanView, ApplicationError> {
    let mut run = load_run(lifecycle_run_repo, run_id).await?;
    for task_id in &ordered_task_ids {
        ensure_task_plan_policy_allowed(TaskPlanPolicyHook {
            action: TaskPlanPolicyAction::Update,
            run: &run,
            task_id: Some(*task_id),
        })?;
    }
    run.reorder_tasks(&ordered_task_ids)
        .map_err(ApplicationError::from)?;
    lifecycle_run_repo
        .update(&run)
        .await
        .map_err(ApplicationError::from)?;
    Ok(RunTaskPlanView {
        project_id: run.project_id,
        run_id: run.id,
        tasks: run.tasks,
    })
}

pub fn ensure_task_plan_policy_allowed(
    hook: TaskPlanPolicyHook<'_>,
) -> Result<(), ApplicationError> {
    tracing::debug!(
        run_id = %hook.run.id,
        project_id = %hook.run.project_id,
        task_id = ?hook.task_id,
        action = ?hook.action,
        "Task plan policy hook allowed by default"
    );
    Ok(())
}

pub async fn find_project_task_plan_item(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    project_id: Uuid,
    task_id: Uuid,
) -> Result<Option<LocatedTaskPlanItem>, ApplicationError> {
    let runs = lifecycle_run_repo
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)?;
    locate_task_in_runs(runs, task_id)
}

pub async fn find_task_plan_item_for_subject(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    association_repo: &dyn LifecycleSubjectAssociationRepository,
    project_id: Uuid,
    task_id: Uuid,
) -> Result<Option<LocatedTaskPlanItem>, ApplicationError> {
    let subject = SubjectRef::new("task", task_id);
    let associations = association_repo
        .list_by_subject(&subject)
        .await
        .map_err(ApplicationError::from)?;
    let mut run_ids = Vec::new();
    for association in associations {
        if !run_ids.contains(&association.anchor_run_id) {
            run_ids.push(association.anchor_run_id);
        }
    }
    let mut runs = lifecycle_run_repo
        .list_by_ids(&run_ids)
        .await
        .map_err(ApplicationError::from)?;
    runs.retain(|run| run.project_id == project_id);
    if let Some(located) = locate_task_in_runs(runs, task_id)? {
        return Ok(Some(located));
    }

    find_project_task_plan_item(lifecycle_run_repo, project_id, task_id).await
}

pub async fn find_task_plan_item_by_subject(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    association_repo: &dyn LifecycleSubjectAssociationRepository,
    task_id: Uuid,
) -> Result<Option<LocatedTaskPlanItem>, ApplicationError> {
    let subject = SubjectRef::new("task", task_id);
    let associations = association_repo
        .list_by_subject(&subject)
        .await
        .map_err(ApplicationError::from)?;
    let mut run_ids = Vec::new();
    for association in associations {
        if !run_ids.contains(&association.anchor_run_id) {
            run_ids.push(association.anchor_run_id);
        }
    }
    let runs = lifecycle_run_repo
        .list_by_ids(&run_ids)
        .await
        .map_err(ApplicationError::from)?;
    locate_task_in_runs(runs, task_id)
}

pub async fn build_story_task_projection(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    association_repo: &dyn LifecycleSubjectAssociationRepository,
    project_id: Uuid,
    story_id: Uuid,
) -> Result<StoryTaskProjectionView, ApplicationError> {
    let story_subject = SubjectRef::new("story", story_id);
    let associations = association_repo
        .list_by_subject(&story_subject)
        .await
        .map_err(ApplicationError::from)?;

    let story_run_ids = associations
        .iter()
        .map(|association| association.anchor_run_id)
        .collect::<HashSet<_>>();
    let story_runs = lifecycle_run_repo
        .list_by_ids(&story_run_ids.iter().copied().collect::<Vec<_>>())
        .await
        .map_err(ApplicationError::from)?;

    let mut items: HashMap<(Uuid, Uuid), StoryTaskProjectionItemView> = HashMap::new();
    for run in story_runs
        .into_iter()
        .filter(|run| run.project_id == project_id)
    {
        let run_sources = associations
            .iter()
            .filter(|association| association.anchor_run_id == run.id)
            .map(|association| {
                let kind = if association.role == "subject" && association.anchor_agent_id.is_none()
                {
                    StoryTaskProjectionSourceKind::OwningRun
                } else {
                    StoryTaskProjectionSourceKind::LinkedRun
                };
                StoryTaskProjectionSourceView {
                    kind,
                    run_id: run.id,
                    agent_id: association.anchor_agent_id,
                    story_ref: None,
                    reason: match kind {
                        StoryTaskProjectionSourceKind::OwningRun => {
                            "Task 位于 Story-bound LifecycleRun 内".to_string()
                        }
                        StoryTaskProjectionSourceKind::LinkedRun => {
                            "Task 位于与 Story 关联的 linked run / agent scope 内".to_string()
                        }
                        StoryTaskProjectionSourceKind::StoryRef => unreachable!(),
                    },
                }
            })
            .collect::<Vec<_>>();

        for task in visible_tasks(&run) {
            upsert_projection_item(&mut items, &run, task, run_sources.clone());
        }
    }

    let project_runs = lifecycle_run_repo
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)?;
    for run in project_runs {
        for task in visible_tasks(&run).into_iter().filter(|task| {
            task.story_ref
                .as_ref()
                .is_some_and(|subject| subject.kind == "story" && subject.id == story_id)
        }) {
            upsert_projection_item(
                &mut items,
                &run,
                task,
                vec![StoryTaskProjectionSourceView {
                    kind: StoryTaskProjectionSourceKind::StoryRef,
                    run_id: run.id,
                    agent_id: None,
                    story_ref: task.story_ref.clone(),
                    reason: "Task 显式携带 story_ref projection hint".to_string(),
                }],
            );
        }
    }

    let mut tasks = items.into_values().collect::<Vec<_>>();
    tasks.sort_by(|a, b| b.task.updated_at.cmp(&a.task.updated_at));
    Ok(StoryTaskProjectionView { story_id, tasks })
}

fn filter_tasks(
    tasks: Vec<LifecycleTaskPlanItem>,
    filter: &RunTaskPlanFilter,
) -> Vec<LifecycleTaskPlanItem> {
    tasks
        .into_iter()
        .filter(|task| filter.include_archived || task.archived_at.is_none())
        .filter(|task| {
            filter
                .created_by_agent_id
                .map(|id| task.created_by_agent_id == Some(id))
                .unwrap_or(true)
        })
        .filter(|task| {
            filter
                .owner_agent_id
                .map(|id| task.owner_agent_id == Some(id))
                .unwrap_or(true)
        })
        .filter(|task| {
            filter
                .assigned_agent_id
                .map(|id| task.assigned_agent_id == Some(id))
                .unwrap_or(true)
        })
        .collect()
}

fn visible_tasks(run: &LifecycleRun) -> Vec<&LifecycleTaskPlanItem> {
    run.tasks
        .iter()
        .filter(|task| task.archived_at.is_none())
        .collect()
}

fn upsert_projection_item(
    items: &mut HashMap<(Uuid, Uuid), StoryTaskProjectionItemView>,
    run: &LifecycleRun,
    task: &LifecycleTaskPlanItem,
    sources: Vec<StoryTaskProjectionSourceView>,
) {
    let entry = items
        .entry((run.id, task.id))
        .or_insert_with(|| StoryTaskProjectionItemView {
            project_id: run.project_id,
            owning_run_id: run.id,
            task: task.clone(),
            sources: Vec::new(),
        });
    for source in sources {
        if !entry.sources.iter().any(|existing| {
            existing.kind == source.kind
                && existing.run_id == source.run_id
                && existing.agent_id == source.agent_id
        }) {
            entry.sources.push(source);
        }
    }
}

fn locate_task_in_runs(
    runs: Vec<LifecycleRun>,
    task_id: Uuid,
) -> Result<Option<LocatedTaskPlanItem>, ApplicationError> {
    let mut found: Option<LocatedTaskPlanItem> = None;
    for run in runs {
        let Some(task) = run.task_by_id(task_id).cloned() else {
            continue;
        };
        if found.is_some() {
            return Err(ApplicationError::Conflict(format!(
                "Task id {task_id} 在多个 LifecycleRun 中出现"
            )));
        }
        found = Some(LocatedTaskPlanItem { run, task });
    }
    Ok(found)
}

async fn load_run(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
) -> Result<LifecycleRun, ApplicationError> {
    lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("LifecycleRun {run_id} 不存在")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        LifecycleRun, LifecycleSubjectAssociation, SubjectRef, TaskPlanStatus,
    };
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemoryLifecycleRunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for InMemoryLifecycleRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().expect("runs").push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .expect("runs")
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .expect("runs")
                .iter()
                .filter(|run| ids.contains(&run.id))
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
                .expect("runs")
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().expect("runs");
            if let Some(existing) = runs.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
                Ok(())
            } else {
                Err(DomainError::NotFound {
                    entity: "LifecycleRun",
                    id: run.id.to_string(),
                })
            }
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().expect("runs").retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAssociationRepo {
        associations: Mutex<Vec<LifecycleSubjectAssociation>>,
    }

    #[async_trait]
    impl LifecycleSubjectAssociationRepository for InMemoryAssociationRepo {
        async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
            self.associations
                .lock()
                .expect("assocs")
                .push(assoc.clone());
            Ok(())
        }

        async fn list_by_subject(
            &self,
            subject: &SubjectRef,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .associations
                .lock()
                .expect("assocs")
                .iter()
                .filter(|assoc| {
                    assoc.subject_kind == subject.kind && assoc.subject_id == subject.id
                })
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
                .expect("assocs")
                .iter()
                .filter(|assoc| assoc.anchor_run_id == run_id && assoc.anchor_agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.associations
                .lock()
                .expect("assocs")
                .retain(|assoc| assoc.id != id);
            Ok(())
        }
    }

    fn run(project_id: Uuid) -> LifecycleRun {
        LifecycleRun::new_plain(project_id)
    }

    #[tokio::test]
    async fn run_scoped_task_command_creates_updates_and_archives_task() {
        let repo = InMemoryLifecycleRunRepo::default();
        let run = run(Uuid::new_v4());
        let run_id = run.id;
        repo.create(&run).await.expect("seed run");

        let created = create_run_task(&repo, run_id, LifecycleTaskPlanItemDraft::new("Plan item"))
            .await
            .expect("create");
        assert_eq!(created.task.status, TaskPlanStatus::Open);

        let updated =
            transition_run_task_status(&repo, run_id, created.task.id, TaskPlanStatus::Active)
                .await
                .expect("activate");
        assert_eq!(updated.task.status, TaskPlanStatus::Active);

        let archived = archive_run_task(&repo, run_id, created.task.id)
            .await
            .expect("archive");
        assert_eq!(archived.task.status, TaskPlanStatus::Dropped);
        assert!(archived.task.archived_at.is_some());

        let view = list_run_tasks(&repo, run_id, RunTaskPlanFilter::default())
            .await
            .expect("list");
        assert!(view.tasks.is_empty());
    }

    #[tokio::test]
    async fn story_projection_includes_story_bound_and_story_ref_but_excludes_unrelated() {
        let lifecycle_repo = InMemoryLifecycleRunRepo::default();
        let association_repo = InMemoryAssociationRepo::default();
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();

        let mut story_bound = run(project_id);
        let story_bound_task = story_bound
            .create_task(LifecycleTaskPlanItemDraft::new("Visible from owning run"))
            .expect("task");
        lifecycle_repo
            .create(&story_bound)
            .await
            .expect("seed story run");
        association_repo
            .create(&LifecycleSubjectAssociation::new_run_scoped(
                story_bound.id,
                &SubjectRef::new("story", story_id),
                "subject",
                None,
            ))
            .await
            .expect("assoc");

        let mut explicit = run(project_id);
        let mut draft = LifecycleTaskPlanItemDraft::new("Visible from story_ref");
        draft.story_ref = Some(SubjectRef::new("story", story_id));
        let explicit_task = explicit.create_task(draft).expect("task");
        lifecycle_repo
            .create(&explicit)
            .await
            .expect("seed explicit");

        let mut unrelated = run(project_id);
        unrelated
            .create_task(LifecycleTaskPlanItemDraft::new("Hidden"))
            .expect("task");
        lifecycle_repo
            .create(&unrelated)
            .await
            .expect("seed unrelated");

        let projection =
            build_story_task_projection(&lifecycle_repo, &association_repo, project_id, story_id)
                .await
                .expect("projection");

        let ids = projection
            .tasks
            .iter()
            .map(|item| item.task.id)
            .collect::<HashSet<_>>();
        assert!(ids.contains(&story_bound_task.id));
        assert!(ids.contains(&explicit_task.id));
        assert_eq!(ids.len(), 2);
    }
}
