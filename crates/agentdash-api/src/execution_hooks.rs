use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::{
    SessionBinding, SessionBindingRepository, SessionOwnerType,
};
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::{TaskRepository, TaskStatus};
use agentdash_domain::workflow::{
    WorkflowDefinition, WorkflowDefinitionRepository, WorkflowPhaseDefinition, WorkflowRun,
    WorkflowRunRepository, WorkflowRunStatus, WorkflowTargetKind,
};
use agentdash_executor::{
    ExecutionHookProvider, HookConstraint, HookContextFragment, HookDiagnosticEntry, HookError,
    HookEvaluationQuery, HookOwnerSummary, HookPolicy, HookResolution, HookTrigger,
    SessionHookRefreshQuery, SessionHookSnapshot, SessionHookSnapshotQuery,
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
            task_status: match binding.owner_type {
                SessionOwnerType::Task => self
                    .task_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?
                    .map(|task| task_status_tag(task.status).to_string()),
                SessionOwnerType::Project | SessionOwnerType::Story => None,
            },
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
            let task_status = resolved_owner.task_status.clone();
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
            if let Some(task_status) = task_status.as_deref() {
                snapshot.tags.push(format!("task_status:{task_status}"));
                if let Some(metadata) = snapshot
                    .metadata
                    .as_mut()
                    .and_then(|value| value.as_object_mut())
                {
                    metadata.insert(
                        "active_task".to_string(),
                        serde_json::json!({
                            "task_id": owner.task_id.clone(),
                            "task_title": owner.label.clone(),
                            "status": task_status,
                        }),
                    );
                }
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

                if let Some(metadata) = snapshot
                    .metadata
                    .as_mut()
                    .and_then(|value| value.as_object_mut())
                {
                    metadata.insert(
                        "active_workflow".to_string(),
                        serde_json::json!({
                            "workflow_id": workflow.definition.id,
                            "workflow_key": workflow.definition.key,
                            "workflow_name": workflow.definition.name,
                            "run_id": workflow.run.id,
                            "run_status": workflow_run_status_tag(workflow.run.status),
                            "phase_key": workflow.phase.key,
                            "phase_title": workflow.phase.title,
                            "completion_mode": workflow_phase_completion_mode_tag(workflow.phase.completion_mode),
                            "requires_session": workflow.phase.requires_session,
                        }),
                    );
                }

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
                snapshot.policies.extend(build_workflow_policies(&workflow));
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
        let snapshot = query.snapshot.clone().unwrap_or_else(|| SessionHookSnapshot {
            session_id: query.session_id.clone(),
            ..SessionHookSnapshot::default()
        });
        let mut resolution = HookResolution {
            policies: snapshot.policies.clone(),
            diagnostics: snapshot
                .diagnostics
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.code.as_str(),
                        "workflow_phase_resolved" | "session_binding_found"
                    )
                })
                .cloned()
                .collect(),
            ..HookResolution::default()
        };

        match query.trigger {
            HookTrigger::SessionStart | HookTrigger::UserPromptSubmit => {
                resolution.context_fragments = snapshot.context_fragments.clone();
                resolution.constraints = snapshot.constraints.clone();
            }
            HookTrigger::BeforeTool => {
                evaluate_before_tool(&snapshot, &query, &mut resolution);
            }
            HookTrigger::AfterTool => {
                evaluate_after_tool(&snapshot, &query, &mut resolution);
            }
            HookTrigger::AfterTurn => {
                evaluate_after_turn(&snapshot, &query, &mut resolution);
            }
            HookTrigger::BeforeStop => {
                evaluate_before_stop(&snapshot, &query, &mut resolution);
            }
            HookTrigger::BeforeSubagentDispatch | HookTrigger::AfterSubagentDispatch => {}
        }

        Ok(resolution)
    }
}

struct ResolvedOwnerSummary {
    summary: HookOwnerSummary,
    diagnostics: Vec<HookDiagnosticEntry>,
    task_status: Option<String>,
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

fn workflow_phase_completion_mode_tag(
    mode: agentdash_domain::workflow::WorkflowPhaseCompletionMode,
) -> &'static str {
    match mode {
        agentdash_domain::workflow::WorkflowPhaseCompletionMode::Manual => "manual",
        agentdash_domain::workflow::WorkflowPhaseCompletionMode::SessionEnded => "session_ended",
        agentdash_domain::workflow::WorkflowPhaseCompletionMode::ChecklistPassed => {
            "checklist_passed"
        }
    }
}

fn task_status_tag(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Assigned => "assigned",
        TaskStatus::Running => "running",
        TaskStatus::AwaitingVerification => "awaiting_verification",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
    }
}

fn workflow_completion_mode(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("completion_mode"))
        .and_then(serde_json::Value::as_str)
}

fn workflow_phase_key(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("phase_key"))
        .and_then(serde_json::Value::as_str)
}

fn active_task_status(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_task"))
        .and_then(|value| value.get("status"))
        .and_then(serde_json::Value::as_str)
}

fn active_workflow_source_summary(snapshot: &SessionHookSnapshot) -> Vec<String> {
    let mut summary = Vec::new();
    if let Some(workflow_key) = snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("workflow_key"))
        .and_then(serde_json::Value::as_str)
    {
        summary.push(format!("workflow:{workflow_key}"));
    }
    if let Some(phase_key) = workflow_phase_key(snapshot) {
        summary.push(format!("workflow_phase:{phase_key}"));
    }
    summary
}

fn build_workflow_policies(workflow: &ResolvedWorkflowPhase) -> Vec<HookPolicy> {
    let mut policies = vec![HookPolicy {
        key: format!(
            "workflow:{}:{}:completion_mode",
            workflow.definition.key, workflow.phase.key
        ),
        description: format!(
            "当前 phase 使用 `{}` 作为完成语义。",
            workflow_phase_completion_mode_tag(workflow.phase.completion_mode)
        ),
        payload: Some(serde_json::json!({
            "workflow_key": workflow.definition.key,
            "phase_key": workflow.phase.key,
            "completion_mode": workflow_phase_completion_mode_tag(workflow.phase.completion_mode),
            "requires_session": workflow.phase.requires_session,
        })),
    }];

    if workflow.phase.key == "implement"
        && workflow.definition.target_kind == WorkflowTargetKind::Task
    {
        policies.push(HookPolicy {
            key: format!(
                "workflow:{}:{}:task_status_gate",
                workflow.definition.key, workflow.phase.key
            ),
            description:
                "Implement phase 期间不应直接把 Task 标记为 completed，应先进入 awaiting_verification。"
                    .to_string(),
            payload: Some(serde_json::json!({
                "tool": "update_task_status",
                "deny_statuses": ["completed"],
                "preferred_status": "awaiting_verification",
            })),
        });
        policies.push(HookPolicy {
            key: format!(
                "workflow:{}:{}:record_gate",
                workflow.definition.key, workflow.phase.key
            ),
            description:
                "Implement phase 不应提前产出 session_summary / archive_suggestion 类记录产物。"
                    .to_string(),
            payload: Some(serde_json::json!({
                "tool": "report_artifact",
                "deny_artifact_types": ["session_summary", "archive_suggestion"],
            })),
        });
    }

    if workflow.phase.completion_mode
        == agentdash_domain::workflow::WorkflowPhaseCompletionMode::ChecklistPassed
    {
        policies.push(HookPolicy {
            key: format!(
                "workflow:{}:{}:checklist_gate",
                workflow.definition.key, workflow.phase.key
            ),
            description:
                "Checklist phase 结束前，Task 状态至少应进入 awaiting_verification 或 completed。"
                    .to_string(),
            payload: Some(serde_json::json!({
                "accepted_task_statuses": ["awaiting_verification", "completed"],
                "tool": "update_task_status",
            })),
        });
    }

    policies
}

fn evaluate_before_tool(
    snapshot: &SessionHookSnapshot,
    query: &HookEvaluationQuery,
    resolution: &mut HookResolution,
) {
    let Some(tool_name) = query.tool_name.as_deref() else {
        return;
    };

    if is_update_task_status_tool(tool_name) {
        if let Some(next_status) = extract_tool_arg(query.payload.as_ref(), "status") {
            if workflow_phase_key(snapshot) == Some("implement") && next_status == "completed" {
                resolution.block_reason = Some(
                    "当前处于 Implement phase，请先完成实现说明并把 Task 状态更新为 awaiting_verification，而不是直接 completed。"
                        .to_string(),
                );
                resolution.diagnostics.push(HookDiagnosticEntry {
                    code: "before_tool_task_status_blocked".to_string(),
                    summary: "Hook 阻止了 Implement phase 期间直接 completed 的状态更新"
                        .to_string(),
                    detail: Some("expected_status=awaiting_verification".to_string()),
                    source_summary: active_workflow_source_summary(snapshot),
                });
                return;
            }

            if workflow_completion_mode(snapshot) == Some("checklist_passed")
                && matches!(next_status, "awaiting_verification" | "completed")
            {
                resolution.refresh_snapshot = true;
                resolution.diagnostics.push(HookDiagnosticEntry {
                    code: "before_tool_checklist_completion_signal".to_string(),
                    summary: format!(
                        "捕获到 checklist completion 信号：Task 即将更新为 `{next_status}`"
                    ),
                    detail: None,
                    source_summary: active_workflow_source_summary(snapshot),
                });
            }
        }
    }

    if is_report_artifact_tool(tool_name)
        && workflow_phase_key(snapshot) == Some("implement")
        && matches!(
            extract_tool_arg(query.payload.as_ref(), "artifact_type"),
            Some("session_summary" | "archive_suggestion")
        )
    {
        resolution.block_reason = Some(
            "当前处于 Implement phase，不应提前产出 session_summary / archive_suggestion 类记录产物，请先完成实现阶段。"
                .to_string(),
        );
        resolution.diagnostics.push(HookDiagnosticEntry {
            code: "before_tool_record_artifact_blocked".to_string(),
            summary: "Hook 阻止了 Implement phase 的提前归档型产物上报".to_string(),
            detail: None,
            source_summary: active_workflow_source_summary(snapshot),
        });
    }
}

fn evaluate_after_tool(
    snapshot: &SessionHookSnapshot,
    query: &HookEvaluationQuery,
    resolution: &mut HookResolution,
) {
    let Some(tool_name) = query.tool_name.as_deref() else {
        return;
    };
    if tool_call_failed(query.payload.as_ref()) {
        return;
    }

    if is_update_task_status_tool(tool_name) || is_report_artifact_tool(tool_name) {
        resolution.refresh_snapshot = true;
        resolution.diagnostics.push(HookDiagnosticEntry {
            code: "after_tool_runtime_refresh".to_string(),
            summary: format!(
                "工具 `{tool_name}` 可能改变 workflow/hook 观察面，已请求刷新 snapshot"
            ),
            detail: None,
            source_summary: active_workflow_source_summary(snapshot),
        });
    }
}

fn evaluate_after_turn(
    snapshot: &SessionHookSnapshot,
    _query: &HookEvaluationQuery,
    resolution: &mut HookResolution,
) {
    if workflow_phase_key(snapshot) == Some("check") {
        resolution.constraints.push(HookConstraint {
            key: "after_turn:check_phase_summary".to_string(),
            description:
                "Check phase 的阶段性输出应明确写出发现的问题、已修复项、剩余风险与未覆盖验证。"
                    .to_string(),
            source_summary: active_workflow_source_summary(snapshot),
        });
    }
}

fn evaluate_before_stop(
    snapshot: &SessionHookSnapshot,
    _query: &HookEvaluationQuery,
    resolution: &mut HookResolution,
) {
    match workflow_completion_mode(snapshot) {
        Some("session_ended") => {
            resolution.diagnostics.push(HookDiagnosticEntry {
                code: "before_stop_session_ended".to_string(),
                summary: "当前 workflow phase 允许在 session 结束时自然完成".to_string(),
                detail: None,
                source_summary: vec!["workflow_completion_mode:session_ended".to_string()],
            });
        }
        Some("checklist_passed") => {
            if matches!(
                active_task_status(snapshot),
                Some("awaiting_verification" | "completed")
            ) {
                resolution.diagnostics.push(HookDiagnosticEntry {
                    code: "before_stop_checklist_satisfied".to_string(),
                    summary: "Checklist completion 条件已满足，允许结束当前 session".to_string(),
                    detail: active_task_status(snapshot)
                        .map(|status| format!("task_status={status}")),
                    source_summary: active_workflow_source_summary(snapshot),
                });
                return;
            }

            resolution.context_fragments.push(HookContextFragment {
                slot: "workflow".to_string(),
                label: "before_stop_checklist_gate".to_string(),
                content: [
                    "## Session Stop Gate",
                    "- 当前 workflow phase 使用 `checklist_passed` 完成语义。",
                    "- 结束前请先补齐验证结论、剩余风险，并把 Task 状态至少更新为 `awaiting_verification`。",
                    "- 如果还存在未验证项，不要直接结束本轮 session。",
                ]
                .join("\n"),
                source_summary: active_workflow_source_summary(snapshot),
            });
            resolution.constraints.push(HookConstraint {
                key: "before_stop:checklist_pending".to_string(),
                description:
                    "当前 phase 还未满足 checklist_passed 条件；请先给出验证/风险结论并更新 Task 状态，再结束 session。"
                        .to_string(),
                source_summary: active_workflow_source_summary(snapshot),
            });
            resolution.diagnostics.push(HookDiagnosticEntry {
                code: "before_stop_checklist_pending".to_string(),
                summary:
                    "当前 workflow phase 尚未满足 checklist completion 条件，Hook 要求继续 loop"
                        .to_string(),
                detail: active_task_status(snapshot)
                    .map(|status| format!("current_task_status={status}")),
                source_summary: active_workflow_source_summary(snapshot),
            });
        }
        Some("manual") => {
            resolution.diagnostics.push(HookDiagnosticEntry {
                code: "before_stop_manual_phase".to_string(),
                summary: "当前 workflow phase 使用 manual completion，不会由 Hook 自动推进 phase"
                    .to_string(),
                detail: None,
                source_summary: active_workflow_source_summary(snapshot),
            });
        }
        Some(_) | None => {}
    }
}

fn extract_tool_arg<'a>(payload: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    payload
        .and_then(|value| value.get("args"))
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
}

fn tool_call_failed(payload: Option<&serde_json::Value>) -> bool {
    payload
        .and_then(|value| value.get("is_error"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn is_update_task_status_tool(tool_name: &str) -> bool {
    tool_name.ends_with("update_task_status")
}

fn is_report_artifact_tool(tool_name: &str) -> bool {
    tool_name.ends_with("report_artifact")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_with_workflow(
        phase_key: &str,
        completion_mode: &str,
        task_status: Option<&str>,
    ) -> SessionHookSnapshot {
        let mut snapshot = SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            metadata: Some(serde_json::json!({
                "active_workflow": {
                    "workflow_key": "trellis_dev_task",
                    "phase_key": phase_key,
                    "completion_mode": completion_mode,
                }
            })),
            ..SessionHookSnapshot::default()
        };
        if let Some(task_status) = task_status {
            if let Some(metadata) = snapshot
                .metadata
                .as_mut()
                .and_then(|value| value.as_object_mut())
            {
                metadata.insert(
                    "active_task".to_string(),
                    serde_json::json!({
                        "task_id": "task-1",
                        "status": task_status,
                    }),
                );
            }
        }
        snapshot
    }

    #[test]
    fn before_tool_blocks_completed_during_implement_phase() {
        let snapshot = snapshot_with_workflow("implement", "session_ended", Some("running"));
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeTool,
            turn_id: None,
            tool_name: Some("mcp_agentdash_task_tools_demo_update_task_status".to_string()),
            tool_call_id: Some("call-1".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "args": {
                    "status": "completed"
                }
            })),
        };

        evaluate_before_tool(&snapshot, &query, &mut resolution);

        assert!(resolution.block_reason.is_some());
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_tool_task_status_blocked")
        );
    }

    #[test]
    fn before_stop_requires_checklist_completion_when_task_not_ready() {
        let snapshot = snapshot_with_workflow("check", "checklist_passed", Some("running"));
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
        };

        evaluate_before_stop(&snapshot, &query, &mut resolution);

        assert!(!resolution.context_fragments.is_empty());
        assert!(!resolution.constraints.is_empty());
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_stop_checklist_pending")
        );
    }

    #[test]
    fn before_stop_allows_checklist_completion_when_task_ready() {
        let snapshot =
            snapshot_with_workflow("check", "checklist_passed", Some("awaiting_verification"));
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
        };

        evaluate_before_stop(&snapshot, &query, &mut resolution);

        assert!(resolution.context_fragments.is_empty());
        assert!(resolution.constraints.is_empty());
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_stop_checklist_satisfied")
        );
    }
}
