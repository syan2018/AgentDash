use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::{
    SessionBinding, SessionBindingRepository, SessionOwnerType,
};
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    WorkflowDefinition, WorkflowDefinitionRepository, WorkflowPhaseDefinition, WorkflowRun,
    WorkflowRunRepository, WorkflowRunStatus, WorkflowTargetKind,
};
use agentdash_executor::{
    ExecutionHookProvider, HookConstraint, HookContextFragment, HookDiagnosticEntry, HookError,
    HookEvaluationQuery, HookOwnerSummary, HookResolution, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionHookSnapshotQuery,
};
use async_trait::async_trait;
use uuid::Uuid;

pub struct AppExecutionHookProvider {
    project_repo: Arc<dyn ProjectRepository>,
    story_repo: Arc<dyn StoryRepository>,
    task_repo: Arc<dyn TaskRepository>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    workflow_run_repo: Arc<dyn WorkflowRunRepository>,
}

impl AppExecutionHookProvider {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
        task_repo: Arc<dyn TaskRepository>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        workflow_run_repo: Arc<dyn WorkflowRunRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
            task_repo,
            session_binding_repo,
            workflow_definition_repo,
            workflow_run_repo,
        }
    }

    async fn resolve_owner(
        &self,
        binding: &SessionBinding,
    ) -> Result<ResolvedOwnerSummary, HookError> {
        let mut summary = HookOwnerSummary {
            owner_type: binding.owner_type.to_string(),
            owner_id: binding.owner_id.to_string(),
            label: None,
            project_id: None,
            story_id: None,
            task_id: None,
        };
        let mut diagnostics = vec![HookDiagnosticEntry {
            code: "session_binding_found".to_string(),
            summary: format!(
                "命中会话绑定：{} {}（label={}）",
                binding.owner_type, binding.owner_id, binding.label
            ),
            detail: None,
            source_summary: vec![format!("session_binding:{}", binding.id)],
        }];

        match binding.owner_type {
            SessionOwnerType::Project => {
                let project = self
                    .project_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?;
                if let Some(project) = project {
                    summary.label = Some(project.name);
                    summary.project_id = Some(project.id.to_string());
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        summary: "会话绑定引用的 Project 已不存在".to_string(),
                        detail: Some(binding.owner_id.to_string()),
                        source_summary: vec![format!("session_binding:{}", binding.id)],
                    });
                }
            }
            SessionOwnerType::Story => {
                let story = self
                    .story_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?;
                if let Some(story) = story {
                    summary.label = Some(story.title);
                    summary.project_id = Some(story.project_id.to_string());
                    summary.story_id = Some(story.id.to_string());
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        summary: "会话绑定引用的 Story 已不存在".to_string(),
                        detail: Some(binding.owner_id.to_string()),
                        source_summary: vec![format!("session_binding:{}", binding.id)],
                    });
                }
            }
            SessionOwnerType::Task => {
                let task = self
                    .task_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?;
                if let Some(task) = task {
                    summary.label = Some(task.title);
                    summary.task_id = Some(task.id.to_string());
                    summary.story_id = Some(task.story_id.to_string());

                    let story = self
                        .story_repo
                        .get_by_id(task.story_id)
                        .await
                        .map_err(map_hook_error)?;
                    if let Some(story) = story {
                        summary.project_id = Some(story.project_id.to_string());
                    } else {
                        diagnostics.push(HookDiagnosticEntry {
                            code: "task_story_missing".to_string(),
                            summary: "Task 对应的 Story 已不存在，无法补全 project_id".to_string(),
                            detail: Some(task.story_id.to_string()),
                            source_summary: vec![format!("task:{}", task.id)],
                        });
                    }
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        summary: "会话绑定引用的 Task 已不存在".to_string(),
                        detail: Some(binding.owner_id.to_string()),
                        source_summary: vec![format!("session_binding:{}", binding.id)],
                    });
                }
            }
        }

        Ok(ResolvedOwnerSummary {
            summary,
            diagnostics,
        })
    }

    async fn resolve_active_workflow(
        &self,
        owner: &HookOwnerSummary,
    ) -> Result<Option<ResolvedWorkflowPhase>, HookError> {
        let owner_id = Uuid::parse_str(owner.owner_id.as_str())
            .map_err(|error| HookError::Runtime(format!("owner_id 不是有效 UUID: {error}")))?;
        let target_kind = match owner.owner_type.as_str() {
            "project" => WorkflowTargetKind::Project,
            "story" => WorkflowTargetKind::Story,
            "task" => WorkflowTargetKind::Task,
            other => {
                return Err(HookError::Runtime(format!(
                    "未知 session owner_type，无法映射 workflow target: {other}"
                )));
            }
        };

        let runs = self
            .workflow_run_repo
            .list_by_target(target_kind, owner_id)
            .await
            .map_err(map_hook_error)?;

        let Some(run) = select_active_run(runs) else {
            return Ok(None);
        };
        let Some(current_phase_key) = run.current_phase_key.as_deref() else {
            return Ok(None);
        };

        let definition = self
            .workflow_definition_repo
            .get_by_id(run.workflow_id)
            .await
            .map_err(map_hook_error)?
            .filter(|definition| definition.enabled);
        let Some(definition) = definition else {
            return Ok(None);
        };

        let Some(phase) = definition
            .phases
            .iter()
            .find(|phase| phase.key == current_phase_key)
            .cloned()
        else {
            return Ok(None);
        };

        Ok(Some(ResolvedWorkflowPhase {
            run,
            definition,
            phase,
        }))
    }
}

#[async_trait]
impl ExecutionHookProvider for AppExecutionHookProvider {
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        let bindings = self
            .session_binding_repo
            .list_by_session(&query.session_id)
            .await
            .map_err(map_hook_error)?;

        let mut snapshot = SessionHookSnapshot {
            session_id: query.session_id.clone(),
            owners: Vec::new(),
            tags: query.tags,
            context_fragments: Vec::new(),
            constraints: Vec::new(),
            policies: Vec::new(),
            diagnostics: Vec::new(),
            metadata: Some(serde_json::json!({
                "turn_id": query.turn_id,
                "connector_id": query.connector_id,
                "executor": query.executor,
                "working_directory": query.working_directory,
            })),
        };

        if bindings.is_empty() {
            snapshot.diagnostics.push(HookDiagnosticEntry {
                code: "session_binding_missing".to_string(),
                summary: "当前 session 没有关联的业务 owner，Hook snapshot 为空基线".to_string(),
                detail: None,
                source_summary: vec![format!("session:{}", snapshot.session_id)],
            });
        }

        for binding in bindings.iter() {
            let resolved_owner = self.resolve_owner(binding).await?;
            snapshot
                .diagnostics
                .extend(resolved_owner.diagnostics.into_iter());
            let owner = resolved_owner.summary;
            snapshot.tags.push(format!("owner:{}", owner.owner_type));
            snapshot.tags.push(format!("owner_id:{}", owner.owner_id));
            if let Some(project_id) = owner.project_id.as_deref() {
                snapshot.tags.push(format!("project:{project_id}"));
            }
            if let Some(story_id) = owner.story_id.as_deref() {
                snapshot.tags.push(format!("story:{story_id}"));
            }
            if let Some(task_id) = owner.task_id.as_deref() {
                snapshot.tags.push(format!("task:{task_id}"));
            }

            if let Some(workflow) = self.resolve_active_workflow(&owner).await? {
                let source_summary = vec![
                    format!("workflow:{}", workflow.definition.key),
                    format!("workflow_phase:{}", workflow.phase.key),
                    format!("workflow_run:{}", workflow.run.id),
                ];

                snapshot
                    .tags
                    .push(format!("workflow:{}", workflow.definition.key));
                snapshot
                    .tags
                    .push(format!("workflow_phase:{}", workflow.phase.key));
                snapshot.tags.push(format!(
                    "workflow_status:{}",
                    workflow_run_status_tag(workflow.run.status)
                ));

                snapshot.diagnostics.push(HookDiagnosticEntry {
                    code: "workflow_phase_resolved".to_string(),
                    summary: format!(
                        "命中 active workflow phase：{} / {}",
                        workflow.definition.key, workflow.phase.key
                    ),
                    detail: Some(format!(
                        "workflow={}, phase_title={}, status={}",
                        workflow.definition.name,
                        workflow.phase.title,
                        workflow_run_status_tag(workflow.run.status)
                    )),
                    source_summary: source_summary.clone(),
                });

                snapshot.context_fragments.push(HookContextFragment {
                    slot: "workflow".to_string(),
                    label: "active_workflow_phase".to_string(),
                    content: build_phase_summary_markdown(&workflow),
                    source_summary: source_summary.clone(),
                });

                if !workflow.phase.agent_instructions.is_empty() {
                    snapshot.context_fragments.push(HookContextFragment {
                        slot: "instruction_append".to_string(),
                        label: "workflow_phase_constraints".to_string(),
                        content: build_phase_instruction_markdown(&workflow),
                        source_summary: source_summary.clone(),
                    });
                }

                snapshot.constraints.extend(
                    workflow.phase.agent_instructions.iter().enumerate().map(
                        |(index, instruction)| HookConstraint {
                            key: format!(
                                "workflow:{}:{}:instruction:{}",
                                workflow.definition.key,
                                workflow.phase.key,
                                index + 1
                            ),
                            description: instruction.clone(),
                            source_summary: source_summary.clone(),
                        },
                    ),
                );
            }

            snapshot.owners.push(owner);
        }

        snapshot.tags = dedupe_tags(snapshot.tags);
        Ok(snapshot)
    }

    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        self.load_session_snapshot(SessionHookSnapshotQuery {
            session_id: query.session_id,
            turn_id: query.turn_id,
            connector_id: None,
            executor: None,
            working_directory: None,
            owners: Vec::new(),
            tags: Vec::new(),
        })
        .await
    }

    async fn evaluate_hook(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError> {
        let diagnostics = query
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .diagnostics
                    .iter()
                    .filter(|entry| {
                        matches!(
                            entry.code.as_str(),
                            "workflow_phase_resolved" | "session_binding_found"
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(HookResolution {
            diagnostics,
            ..HookResolution::default()
        })
    }
}

struct ResolvedOwnerSummary {
    summary: HookOwnerSummary,
    diagnostics: Vec<HookDiagnosticEntry>,
}

struct ResolvedWorkflowPhase {
    run: WorkflowRun,
    definition: WorkflowDefinition,
    phase: WorkflowPhaseDefinition,
}

fn select_active_run(runs: Vec<WorkflowRun>) -> Option<WorkflowRun> {
    runs.into_iter()
        .filter(|run| {
            run.current_phase_key.is_some()
                && matches!(
                    run.status,
                    WorkflowRunStatus::Ready
                        | WorkflowRunStatus::Running
                        | WorkflowRunStatus::Blocked
                )
        })
        .max_by_key(|run| (workflow_run_priority(run.status), run.updated_at))
}

fn workflow_run_priority(status: WorkflowRunStatus) -> i32 {
    match status {
        WorkflowRunStatus::Running => 3,
        WorkflowRunStatus::Ready => 2,
        WorkflowRunStatus::Blocked => 1,
        WorkflowRunStatus::Draft
        | WorkflowRunStatus::Completed
        | WorkflowRunStatus::Failed
        | WorkflowRunStatus::Cancelled => 0,
    }
}

fn workflow_run_status_tag(status: WorkflowRunStatus) -> &'static str {
    match status {
        WorkflowRunStatus::Draft => "draft",
        WorkflowRunStatus::Ready => "ready",
        WorkflowRunStatus::Running => "running",
        WorkflowRunStatus::Blocked => "blocked",
        WorkflowRunStatus::Completed => "completed",
        WorkflowRunStatus::Failed => "failed",
        WorkflowRunStatus::Cancelled => "cancelled",
    }
}

fn build_phase_summary_markdown(workflow: &ResolvedWorkflowPhase) -> String {
    format!(
        "## Active Workflow Phase\n- workflow: {} (`{}`)\n- phase: {} (`{}`)\n- status: `{}`\n- requires_session: {}\n\n{}",
        workflow.definition.name,
        workflow.definition.key,
        workflow.phase.title,
        workflow.phase.key,
        workflow_run_status_tag(workflow.run.status),
        if workflow.phase.requires_session {
            "yes"
        } else {
            "no"
        },
        workflow.phase.description
    )
}

fn build_phase_instruction_markdown(workflow: &ResolvedWorkflowPhase) -> String {
    format!(
        "## Workflow Constraints\n- 当前 workflow phase: {} (`{}`)\n{}",
        workflow.phase.title,
        workflow.phase.key,
        workflow
            .phase
            .agent_instructions
            .iter()
            .map(|instruction| format!("- {instruction}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn dedupe_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.into_iter()
        .filter(|tag| seen.insert(tag.clone()))
        .collect()
}

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}
