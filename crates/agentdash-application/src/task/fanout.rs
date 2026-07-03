use std::collections::HashSet;

use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentPolicy, AgentRuntimeRefs, CapabilityPolicy, ContextPolicy, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, LifecycleTaskPlanItem, LifecycleTaskPlanItemPatch,
    RunPolicy, RuntimePolicy, SubjectExecutionDispatchResult, SubjectExecutionIntent, SubjectRef,
    TaskPlanStatus, WorkflowGraphRef,
};

use crate::ApplicationError;
use crate::lifecycle::{LifecycleDispatchService, WorkflowApplicationError};
use crate::task::plan::{
    RunTaskPlanFilter, StoryTaskProjectionSourceView, build_story_task_projection, list_run_tasks,
};

#[derive(Debug, Clone)]
pub enum TaskFanoutSource {
    RootLifecycleRun { run_id: Uuid },
    StoryProjection { project_id: Uuid, story_id: Uuid },
}

#[derive(Debug, Clone, Default)]
pub struct TaskFanoutSelector {
    pub task_ids: Option<HashSet<Uuid>>,
    pub statuses: Option<Vec<TaskPlanStatus>>,
    pub owner_agent_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
    pub include_archived: bool,
}

#[derive(Debug, Clone)]
pub struct TaskFanoutCandidate {
    pub project_id: Uuid,
    pub owning_run_id: Uuid,
    pub task: LifecycleTaskPlanItem,
    pub story_sources: Vec<StoryTaskProjectionSourceView>,
}

#[derive(Debug, Clone)]
pub struct TaskFanoutCommand {
    pub source: TaskFanoutSource,
    pub selector: TaskFanoutSelector,
    pub parent_agent_id: Option<Uuid>,
    pub workflow_graph_ref: Option<WorkflowGraphRef>,
    pub runtime_policy: RuntimePolicy,
}

impl TaskFanoutCommand {
    pub fn from_root_run(run_id: Uuid) -> Self {
        Self {
            source: TaskFanoutSource::RootLifecycleRun { run_id },
            selector: TaskFanoutSelector::default(),
            parent_agent_id: None,
            workflow_graph_ref: None,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskFanoutDispatch {
    pub project_id: Uuid,
    pub owning_run_id: Uuid,
    pub task_id: Uuid,
    pub assigned_agent_id: Uuid,
    pub runtime_refs: AgentRuntimeRefs,
    pub subject_execution_ref: agentdash_domain::workflow::SubjectExecutionRef,
    pub delivery_runtime_ref: Option<Uuid>,
    pub task: LifecycleTaskPlanItem,
}

#[derive(Debug, Clone)]
pub struct TaskFanoutCommandResult {
    pub selected: Vec<TaskFanoutCandidate>,
    pub dispatches: Vec<TaskFanoutDispatch>,
}

#[async_trait]
pub trait TaskFanoutDispatcher: Send + Sync {
    async fn dispatch_task_subject(
        &self,
        intent: SubjectExecutionIntent,
    ) -> Result<SubjectExecutionDispatchResult, WorkflowApplicationError>;
}

#[async_trait]
impl<'a> TaskFanoutDispatcher for LifecycleDispatchService<'a> {
    async fn dispatch_task_subject(
        &self,
        intent: SubjectExecutionIntent,
    ) -> Result<SubjectExecutionDispatchResult, WorkflowApplicationError> {
        self.execute_subject(&intent).await
    }
}

pub async fn select_task_fanout_candidates(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    association_repo: &dyn LifecycleSubjectAssociationRepository,
    source: TaskFanoutSource,
    selector: &TaskFanoutSelector,
) -> Result<Vec<TaskFanoutCandidate>, ApplicationError> {
    let mut candidates = match source {
        TaskFanoutSource::RootLifecycleRun { run_id } => {
            let view = list_run_tasks(
                lifecycle_run_repo,
                run_id,
                RunTaskPlanFilter {
                    owner_agent_id: selector.owner_agent_id,
                    assigned_agent_id: selector.assigned_agent_id,
                    include_archived: selector.include_archived,
                    ..RunTaskPlanFilter::default()
                },
            )
            .await?;
            view.tasks
                .into_iter()
                .map(|task| TaskFanoutCandidate {
                    project_id: view.project_id,
                    owning_run_id: view.run_id,
                    task,
                    story_sources: Vec::new(),
                })
                .collect::<Vec<_>>()
        }
        TaskFanoutSource::StoryProjection {
            project_id,
            story_id,
        } => {
            build_story_task_projection(lifecycle_run_repo, association_repo, project_id, story_id)
                .await?
                .tasks
                .into_iter()
                .map(|item| TaskFanoutCandidate {
                    project_id: item.project_id,
                    owning_run_id: item.owning_run_id,
                    task: item.task,
                    story_sources: item.sources,
                })
                .collect::<Vec<_>>()
        }
    };

    candidates.retain(|candidate| selector_matches(selector, candidate));
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.task.updated_at));
    Ok(candidates)
}

pub async fn fanout_tasks(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    association_repo: &dyn LifecycleSubjectAssociationRepository,
    dispatcher: &dyn TaskFanoutDispatcher,
    command: TaskFanoutCommand,
) -> Result<TaskFanoutCommandResult, ApplicationError> {
    let selected = select_task_fanout_candidates(
        lifecycle_run_repo,
        association_repo,
        command.source.clone(),
        &command.selector,
    )
    .await?;

    let mut dispatches = Vec::new();
    for candidate in &selected {
        let result = dispatcher
            .dispatch_task_subject(SubjectExecutionIntent {
                project_id: candidate.project_id,
                source: agentdash_domain::workflow::ExecutionSource::ParentAgent,
                created_by_user_id: None,
                subject_ref: SubjectRef::new("task", candidate.task.id),
                parent_run_id: Some(candidate.owning_run_id),
                parent_agent_id: command.parent_agent_id,
                workflow_graph_ref: command.workflow_graph_ref.clone(),
                run_policy: if command.workflow_graph_ref.is_some() {
                    RunPolicy::AppendGraph
                } else {
                    RunPolicy::ReuseExisting
                },
                agent_policy: AgentPolicy::SpawnChild,
                context_policy: ContextPolicy::Inherit,
                capability_policy: CapabilityPolicy::InheritedSlice,
                runtime_policy: command.runtime_policy.clone(),
            })
            .await
            .map_err(application_error_from_workflow)?;

        let assigned_agent_id = result.runtime_refs.agent_ref;
        let task = update_task_assignment(
            lifecycle_run_repo,
            candidate.owning_run_id,
            candidate.task.id,
            assigned_agent_id,
        )
        .await?;

        dispatches.push(TaskFanoutDispatch {
            project_id: candidate.project_id,
            owning_run_id: candidate.owning_run_id,
            task_id: candidate.task.id,
            assigned_agent_id,
            runtime_refs: result.runtime_refs,
            subject_execution_ref: result.subject_execution_ref,
            delivery_runtime_ref: result.delivery_runtime_ref,
            task,
        });
    }

    Ok(TaskFanoutCommandResult {
        selected,
        dispatches,
    })
}

fn selector_matches(selector: &TaskFanoutSelector, candidate: &TaskFanoutCandidate) -> bool {
    if !selector.include_archived && candidate.task.archived_at.is_some() {
        return false;
    }
    if let Some(task_ids) = &selector.task_ids {
        if !task_ids.contains(&candidate.task.id) {
            return false;
        }
    }
    if let Some(statuses) = &selector.statuses {
        if !statuses.contains(&candidate.task.status) {
            return false;
        }
    }
    if let Some(owner_agent_id) = selector.owner_agent_id {
        if candidate.task.owner_agent_id != Some(owner_agent_id) {
            return false;
        }
    }
    if let Some(assigned_agent_id) = selector.assigned_agent_id {
        if candidate.task.assigned_agent_id != Some(assigned_agent_id) {
            return false;
        }
    }
    true
}

async fn update_task_assignment(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    run_id: Uuid,
    task_id: Uuid,
    assigned_agent_id: Uuid,
) -> Result<LifecycleTaskPlanItem, ApplicationError> {
    let mut run = lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("LifecycleRun {run_id} 不存在")))?;
    let task = run
        .update_task(
            task_id,
            LifecycleTaskPlanItemPatch {
                assigned_agent_id: Some(Some(assigned_agent_id)),
                ..LifecycleTaskPlanItemPatch::default()
            },
        )
        .map_err(ApplicationError::from)?;
    lifecycle_run_repo
        .update(&run)
        .await
        .map_err(ApplicationError::from)?;
    Ok(task)
}

fn application_error_from_workflow(error: WorkflowApplicationError) -> ApplicationError {
    match error {
        WorkflowApplicationError::BadRequest(message)
        | WorkflowApplicationError::ModelRequired(message) => ApplicationError::BadRequest(message),
        WorkflowApplicationError::NotFound(message) => ApplicationError::NotFound(message),
        WorkflowApplicationError::Conflict(message) => ApplicationError::Conflict(message),
        WorkflowApplicationError::Internal(message) => ApplicationError::Internal(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentRuntimeRefs, LifecycleRun, LifecycleSubjectAssociation, SubjectExecutionRef,
    };
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

    #[derive(Default)]
    struct RecordingDispatcher {
        intents: Mutex<Vec<SubjectExecutionIntent>>,
        next_agents: Mutex<Vec<Uuid>>,
    }

    #[async_trait]
    impl TaskFanoutDispatcher for RecordingDispatcher {
        async fn dispatch_task_subject(
            &self,
            intent: SubjectExecutionIntent,
        ) -> Result<SubjectExecutionDispatchResult, WorkflowApplicationError> {
            self.intents.lock().expect("intents").push(intent.clone());
            let agent_id = self
                .next_agents
                .lock()
                .expect("next agents")
                .pop()
                .unwrap_or_else(Uuid::new_v4);
            Ok(SubjectExecutionDispatchResult {
                runtime_refs: AgentRuntimeRefs::new(
                    intent.parent_run_id.expect("parent run"),
                    agent_id,
                    Uuid::new_v4(),
                    None,
                ),
                subject_execution_ref: SubjectExecutionRef {
                    subject_ref: intent.subject_ref,
                    association_id: Uuid::new_v4(),
                },
                delivery_runtime_ref: Some(Uuid::new_v4()),
            })
        }
    }

    fn seed_run(repo: &InMemoryLifecycleRunRepo, run: LifecycleRun) {
        repo.runs.lock().expect("runs").push(run);
    }

    #[tokio::test]
    async fn root_selector_filters_plan_tasks_without_runtime_status() {
        let lifecycle_repo = InMemoryLifecycleRunRepo::default();
        let association_repo = InMemoryAssociationRepo::default();
        let mut run = LifecycleRun::new_plain(Uuid::new_v4());
        let first = run
            .create_task(agentdash_domain::workflow::LifecycleTaskPlanItemDraft::new(
                "Fanout candidate",
            ))
            .expect("create task");
        let mut second = agentdash_domain::workflow::LifecycleTaskPlanItemDraft::new("Not active");
        second.status = TaskPlanStatus::Done;
        run.create_task(second).expect("create task");
        let run_id = run.id;
        seed_run(&lifecycle_repo, run);

        let selected = select_task_fanout_candidates(
            &lifecycle_repo,
            &association_repo,
            TaskFanoutSource::RootLifecycleRun { run_id },
            &TaskFanoutSelector {
                statuses: Some(vec![TaskPlanStatus::Open]),
                ..TaskFanoutSelector::default()
            },
        )
        .await
        .expect("select");

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].task.id, first.id);
    }

    #[tokio::test]
    async fn story_projection_selector_keeps_projection_sources() {
        let lifecycle_repo = InMemoryLifecycleRunRepo::default();
        let association_repo = InMemoryAssociationRepo::default();
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_plain(project_id);
        let task = run
            .create_task(agentdash_domain::workflow::LifecycleTaskPlanItemDraft::new(
                "Story visible",
            ))
            .expect("create task");
        association_repo
            .create(&LifecycleSubjectAssociation::new_run_scoped(
                run.id,
                &SubjectRef::new("story", story_id),
                "subject",
                None,
            ))
            .await
            .expect("assoc");
        seed_run(&lifecycle_repo, run);

        let selected = select_task_fanout_candidates(
            &lifecycle_repo,
            &association_repo,
            TaskFanoutSource::StoryProjection {
                project_id,
                story_id,
            },
            &TaskFanoutSelector::default(),
        )
        .await
        .expect("select");

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].task.id, task.id);
        assert_eq!(selected[0].story_sources.len(), 1);
    }

    #[tokio::test]
    async fn fanout_tasks_dispatches_task_subject_and_writes_assignment_only() {
        let lifecycle_repo = InMemoryLifecycleRunRepo::default();
        let association_repo = InMemoryAssociationRepo::default();
        let dispatcher = RecordingDispatcher::default();
        let assigned_agent_id = Uuid::new_v4();
        dispatcher
            .next_agents
            .lock()
            .expect("next agents")
            .push(assigned_agent_id);
        let project_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_plain(project_id);
        let mut draft =
            agentdash_domain::workflow::LifecycleTaskPlanItemDraft::new("Fanout this task");
        draft.status = TaskPlanStatus::Active;
        let task = run.create_task(draft).expect("create task");
        let run_id = run.id;
        seed_run(&lifecycle_repo, run);

        let result = fanout_tasks(
            &lifecycle_repo,
            &association_repo,
            &dispatcher,
            TaskFanoutCommand {
                source: TaskFanoutSource::RootLifecycleRun { run_id },
                selector: TaskFanoutSelector {
                    task_ids: Some(HashSet::from([task.id])),
                    ..TaskFanoutSelector::default()
                },
                parent_agent_id: Some(Uuid::new_v4()),
                workflow_graph_ref: None,
                runtime_policy: RuntimePolicy::CreateRuntimeSession,
            },
        )
        .await
        .expect("fanout");

        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.dispatches.len(), 1);
        assert_eq!(result.dispatches[0].assigned_agent_id, assigned_agent_id);
        assert_eq!(result.dispatches[0].task.status, TaskPlanStatus::Active);
        assert_eq!(
            result.dispatches[0].task.assigned_agent_id,
            Some(assigned_agent_id)
        );

        let updated_run = lifecycle_repo
            .get_by_id(run_id)
            .await
            .expect("load")
            .expect("run");
        let updated_task = updated_run.task_by_id(task.id).expect("task");
        assert_eq!(updated_task.status, TaskPlanStatus::Active);
        assert_eq!(updated_task.assigned_agent_id, Some(assigned_agent_id));

        let intents = dispatcher.intents.lock().expect("intents");
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].subject_ref, SubjectRef::new("task", task.id));
        assert_eq!(intents[0].parent_run_id, Some(run_id));
        assert_eq!(intents[0].run_policy, RunPolicy::ReuseExisting);
        assert_eq!(intents[0].agent_policy, AgentPolicy::SpawnChild);
    }
}
