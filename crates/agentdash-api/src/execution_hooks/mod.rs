use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_application::workflow::{
    ActiveWorkflowProjection, CompleteLifecycleStepCommand, LifecycleRunService,
    WorkflowCompletionDecision, WorkflowCompletionSignalSet, WorkflowRecordArtifactDraft,
    build_step_completion_artifact_drafts, resolve_active_workflow_projection,
};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::{
    SessionBinding, SessionBindingRepository, SessionOwnerType,
};
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
    WorkflowRecordArtifactType, WorkflowTargetKind,
};

use agentdash_executor::{
    ExecutionHookProvider, HookCompletionStatus, HookConstraint, HookContextFragment,
    HookContributionSet, HookDiagnosticEntry, HookError, HookEvaluationQuery, HookOwnerSummary,
    HookPolicyView, HookResolution, HookSourceLayer, HookSourceRef, HookStepAdvanceRequest,
    HookTrigger, PendingExecutionLogEntry, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionHookSnapshotQuery,
};
use async_trait::async_trait;
use uuid::Uuid;


mod rules;
mod snapshot_helpers;

use agentdash_application::workflow::execution_log as workflow_recording;
use rules::*;
use snapshot_helpers::*;

fn workflow_scope_key(workflow: &ActiveWorkflowProjection) -> String {
    workflow
        .primary_workflow
        .as_ref()
        .map(|w| w.key.clone())
        .unwrap_or_else(|| workflow.lifecycle.key.clone())
}

fn lifecycle_step_advance_label(
    step: &agentdash_domain::workflow::LifecycleStepDefinition,
) -> &'static str {
    match step
        .workflow_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(_) => "auto",
        None => "manual",
    }
}

pub struct AppExecutionHookProvider {
    project_repo: Arc<dyn ProjectRepository>,
    story_repo: Arc<dyn StoryRepository>,
    task_repo: Arc<dyn TaskRepository>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

impl AppExecutionHookProvider {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
        task_repo: Arc<dyn TaskRepository>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
            task_repo,
            session_binding_repo,
            workflow_definition_repo,
            lifecycle_definition_repo,
            lifecycle_run_repo,
        }
    }

    async fn resolve_owner(
        &self,
        binding: &SessionBinding,
    ) -> Result<ResolvedOwnerSummary, HookError> {
        let binding_source_refs = vec![session_binding_source_ref(binding)];
        let binding_source_summary = source_summary_from_refs(&binding_source_refs);
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
            source_summary: binding_source_summary.clone(),
            source_refs: binding_source_refs.clone(),
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
                        source_summary: binding_source_summary.clone(),
                        source_refs: binding_source_refs.clone(),
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
                        source_summary: binding_source_summary.clone(),
                        source_refs: binding_source_refs.clone(),
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
                            source_summary: source_summary_from_refs(&[task_source_ref(task.id)]),
                            source_refs: vec![task_source_ref(task.id)],
                        });
                    }
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        summary: "会话绑定引用的 Task 已不存在".to_string(),
                        detail: Some(binding.owner_id.to_string()),
                        source_summary: binding_source_summary,
                        source_refs: binding_source_refs,
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
    ) -> Result<Option<ActiveWorkflowProjection>, HookError> {
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

        resolve_active_workflow_projection(
            target_kind,
            owner_id,
            owner.label.clone(),
            self.workflow_definition_repo.as_ref(),
            self.lifecycle_definition_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
        )
        .await
        .map_err(|e| HookError::Runtime(e))
    }

    async fn apply_completion_decision(
        &self,
        snapshot: &SessionHookSnapshot,
        decision: WorkflowCompletionDecision,
        resolution: &mut HookResolution,
    ) -> Result<(), HookError> {
        let source_summary = active_workflow_source_summary(snapshot);
        let source_refs = active_workflow_source_refs(snapshot);
        resolution
            .diagnostics
            .extend(
                decision
                    .evidence
                    .iter()
                    .map(|evidence| HookDiagnosticEntry {
                        code: evidence.code.clone(),
                        summary: evidence.summary.clone(),
                        detail: evidence.detail.clone(),
                        source_summary: source_summary.clone(),
                        source_refs: source_refs.clone(),
                    }),
            );

        let Some(locator) = active_workflow_locator(snapshot) else {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy.clone(),
                satisfied: decision.satisfied,
                advanced: false,
                reason: decision
                    .blocking_reason
                    .or(decision.summary)
                    .unwrap_or_else(|| "当前没有可推进的 active workflow".to_string()),
            });
            return Ok(());
        };

        if !decision.should_complete_step {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy,
                satisfied: decision.satisfied,
                advanced: false,
                reason: decision
                    .blocking_reason
                    .or(decision.summary)
                    .unwrap_or_else(|| "completion 条件尚未满足".to_string()),
            });
            return Ok(());
        }

        let run = self
            .lifecycle_run_repo
            .get_by_id(locator.run_id)
            .await
            .map_err(map_hook_error)?;
        let Some(run) = run else {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy,
                satisfied: true,
                advanced: false,
                reason: format!("workflow run {} 已不存在，无法推进", locator.run_id),
            });
            resolution.diagnostics.push(HookDiagnosticEntry {
                code: "workflow_run_missing_for_completion".to_string(),
                summary: "Hook 发现 workflow run 已不存在，无法写回 completion".to_string(),
                detail: Some(locator.run_id.to_string()),
                source_summary,
                source_refs,
            });
            return Ok(());
        };

        if run.current_step_key.as_deref() != Some(locator.step_key.as_str()) {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy,
                satisfied: true,
                advanced: false,
                reason: format!(
                    "workflow 已离开当前 step（当前为 {:?}），无需重复推进",
                    run.current_step_key
                ),
            });
            return Ok(());
        }

        let record_artifacts = build_completion_record_artifacts_from_snapshot(snapshot, &decision);
        let completion_summary = decision.summary.clone();

        resolution.completion = Some(HookCompletionStatus {
            mode: decision.transition_policy.clone(),
            satisfied: true,
            advanced: false,
            reason: completion_summary
                .clone()
                .unwrap_or_else(|| "completion 条件满足，等待 post-evaluate 推进".to_string()),
        });
        let run_id_str = locator.run_id.to_string();
        let step_key_str = locator.step_key.clone();

        resolution.pending_advance = Some(HookStepAdvanceRequest {
            run_id: run_id_str.clone(),
            step_key: step_key_str.clone(),
            completion_mode: decision.transition_policy,
            summary: completion_summary.clone(),
            record_artifacts: record_artifacts
                .into_iter()
                .map(|a| {
                    serde_json::json!({
                        "title": a.title,
                        "artifact_type": a.artifact_type,
                        "content": a.content,
                    })
                })
                .collect(),
        });

        resolution.pending_execution_log.push(
            workflow_recording::completion_evaluated_entry(
                &run_id_str,
                &step_key_str,
                true,
                completion_summary
                    .as_deref()
                    .unwrap_or("completion satisfied"),
            ),
        );
        resolution.pending_execution_log.push(
            workflow_recording::step_completed_entry(
                &run_id_str,
                &step_key_str,
                completion_summary
                    .as_deref()
                    .unwrap_or("step completed by hook"),
            ),
        );

        resolution.diagnostics.push(HookDiagnosticEntry {
            code: "workflow_step_advance_requested".to_string(),
            summary: format!(
                "Hook 产出 step 推进信号：run={}, step=`{}`",
                locator.run_id, locator.step_key
            ),
            detail: None,
            source_summary,
            source_refs,
        });

        Ok(())
    }
}

pub(super) struct ActiveWorkflowLocator {
    run_id: Uuid,
    step_key: String,
}

pub(super) struct ActiveWorkflowChecklistEvidenceSummary {
    artifact_type: agentdash_domain::workflow::WorkflowRecordArtifactType,
    count: usize,
    artifact_ids: Vec<Uuid>,
    titles: Vec<String>,
}

pub(super) fn global_builtin_sources() -> Vec<HookSourceRef> {
    vec![
        HookSourceRef {
            layer: HookSourceLayer::GlobalBuiltin,
            key: "runtime_trace_observability".to_string(),
            label: "Global Builtin / Runtime Trace".to_string(),
            priority: 100,
        },
        HookSourceRef {
            layer: HookSourceLayer::GlobalBuiltin,
            key: "workspace_path_safety".to_string(),
            label: "Global Builtin / Workspace Path Safety".to_string(),
            priority: 100,
        },
    ]
}

fn session_source_ref(session_id: &str) -> HookSourceRef {
    HookSourceRef {
        layer: HookSourceLayer::Session,
        key: session_id.to_string(),
        label: format!("Session / {session_id}"),
        priority: 500,
    }
}

fn session_binding_source_ref(binding: &SessionBinding) -> HookSourceRef {
    HookSourceRef {
        layer: HookSourceLayer::Session,
        key: format!("binding:{}", binding.id),
        label: format!("Session Binding / {}", binding.label),
        priority: 500,
    }
}

fn task_source_ref(task_id: Uuid) -> HookSourceRef {
    HookSourceRef {
        layer: HookSourceLayer::Task,
        key: task_id.to_string(),
        label: format!("Task / {task_id}"),
        priority: 400,
    }
}

fn workflow_source_refs(workflow: &ActiveWorkflowProjection) -> Vec<HookSourceRef> {
    let scope = workflow_scope_key(workflow);
    let label_name = workflow
        .primary_workflow
        .as_ref()
        .map(|w| w.name.as_str())
        .unwrap_or(workflow.lifecycle.name.as_str());
    vec![HookSourceRef {
        layer: HookSourceLayer::Workflow,
        key: format!("{}:{}", scope, workflow.active_step.key),
        label: format!("Workflow / {} / {}", label_name, workflow.active_step.key),
        priority: 300,
    }]
}

fn source_layer_tag(layer: HookSourceLayer) -> &'static str {
    match layer {
        HookSourceLayer::GlobalBuiltin => "global_builtin",
        HookSourceLayer::Workflow => "workflow",
        HookSourceLayer::Project => "project",
        HookSourceLayer::Story => "story",
        HookSourceLayer::Task => "task",
        HookSourceLayer::Session => "session",
    }
}

fn source_summary_from_refs(source_refs: &[HookSourceRef]) -> Vec<String> {
    source_refs
        .iter()
        .map(|source| format!("{}:{}", source_layer_tag(source.layer), source.key))
        .collect()
}

fn merge_hook_contribution(snapshot: &mut SessionHookSnapshot, contribution: HookContributionSet) {
    snapshot.sources.extend(contribution.sources);
    snapshot.tags.extend(contribution.tags);
    snapshot
        .context_fragments
        .extend(contribution.context_fragments);
    snapshot.constraints.extend(contribution.constraints);
    snapshot.policies.extend(contribution.policies);
    snapshot.diagnostics.extend(contribution.diagnostics);
    snapshot.sources = dedupe_source_refs(snapshot.sources.clone());
}

fn dedupe_source_refs(sources: Vec<HookSourceRef>) -> Vec<HookSourceRef> {
    let mut seen = BTreeSet::new();
    sources
        .into_iter()
        .filter(|source| {
            seen.insert((
                source_layer_tag(source.layer).to_string(),
                source.key.clone(),
            ))
        })
        .collect()
}

fn global_builtin_hook_contribution() -> HookContributionSet {
    let source_refs = global_builtin_sources();
    let source_summary = source_summary_from_refs(&source_refs);
    HookContributionSet {
        sources: source_refs.clone(),
        tags: vec![
            "hook_source:global_builtin".to_string(),
            "hook_builtin:runtime_trace".to_string(),
            "hook_builtin:workspace_path_safety".to_string(),
            "hook_builtin:supervised_tool_approval".to_string(),
        ],
        policies: vec![
            HookPolicyView {
                key: "global_builtin:runtime_trace_observable".to_string(),
                description:
                    "当前 session 的 hook 决策会被记录进 runtime trace / diagnostics 调试面。"
                        .to_string(),
                source_summary: source_summary.clone(),
                source_refs: source_refs.clone(),
                payload: None,
            },
            HookPolicyView {
                key: "global_builtin:workspace_path_safety".to_string(),
                description:
                    "shell_exec 在命中工作区内绝对 cwd 时，可由全局 builtin hook 自动 rewrite 为相对路径。"
                        .to_string(),
                source_summary,
                source_refs: source_refs.clone(),
                payload: None,
            },
            HookPolicyView {
                key: "global_builtin:supervised_tool_approval".to_string(),
                description:
                    "当当前会话 permission_policy=SUPERVISED 时，编辑/执行类工具会在运行前进入人工审批。"
                        .to_string(),
                source_summary: source_summary_from_refs(&source_refs),
                source_refs,
                payload: Some(serde_json::json!({
                    "permission_policy": "SUPERVISED",
                    "approval_tool_classes": ["execute", "edit", "delete", "move"],
                })),
            },
        ],
        ..HookContributionSet::default()
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
            sources: Vec::new(),
            tags: query.tags,
            context_fragments: Vec::new(),
            constraints: Vec::new(),
            policies: Vec::new(),
            diagnostics: Vec::new(),
            metadata: Some(serde_json::json!({
                "turn_id": query.turn_id,
                "connector_id": query.connector_id,
                "executor": query.executor,
                "permission_policy": query.permission_policy,
                "working_directory": query.working_directory,
                "workspace_root": query.workspace_root,
            })),
        };
        merge_hook_contribution(&mut snapshot, global_builtin_hook_contribution());

        if bindings.is_empty() {
            let source_refs = vec![session_source_ref(&snapshot.session_id)];
            snapshot.diagnostics.push(HookDiagnosticEntry {
                code: "session_binding_missing".to_string(),
                summary: "当前 session 没有关联的业务 owner，Hook snapshot 为空基线".to_string(),
                detail: None,
                source_summary: source_summary_from_refs(&source_refs),
                source_refs,
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
                let source_refs = workflow_source_refs(&workflow);
                let mut source_summary = source_summary_from_refs(&source_refs);
                source_summary.push(format!("workflow_run:{}", workflow.run.id));
                let checklist_evidence = active_workflow_checklist_evidence_summary(&workflow);

                snapshot.diagnostics.push(HookDiagnosticEntry {
                    code: "active_workflow_resolved".to_string(),
                    summary: format!(
                        "命中 active lifecycle step：{} / {}",
                        workflow.lifecycle.key, workflow.active_step.key
                    ),
                    detail: Some(format!(
                        "workflow_key={:?}, step={}, status={}",
                        workflow.active_step.workflow_key,
                        workflow.active_step.key,
                        workflow_run_status_tag(workflow.run.status)
                    )),
                    source_summary: source_summary.clone(),
                    source_refs: source_refs.clone(),
                });

                if let Some(metadata) = snapshot
                    .metadata
                    .as_mut()
                    .and_then(|value| value.as_object_mut())
                {
                    let step_advance = lifecycle_step_advance_label(&workflow.active_step);
                    let step_title = if workflow.active_step.description.trim().is_empty() {
                        workflow.active_step.key.clone()
                    } else {
                        workflow.active_step.description.clone()
                    };
                    metadata.insert(
                        "active_workflow".to_string(),
                        serde_json::json!({
                            "lifecycle_id": workflow.lifecycle.id,
                            "lifecycle_key": workflow.lifecycle.key,
                            "lifecycle_name": workflow.lifecycle.name,
                            "run_id": workflow.run.id,
                            "run_status": workflow_run_status_tag(workflow.run.status),
                            "step_key": workflow.active_step.key,
                            "step_title": step_title,
                            "workflow_key": workflow.active_step.workflow_key,
                            "step_advance": step_advance,
                            "transition_policy": step_advance,
                            "primary_workflow_id": workflow.primary_workflow.as_ref().map(|w| w.id),
                            "primary_workflow_key": workflow.primary_workflow.as_ref().map(|w| &w.key),
                            "primary_workflow_name": workflow.primary_workflow.as_ref().map(|w| &w.name),
                            "default_artifact_type": workflow
                                .effective_contract
                                .completion
                                .default_artifact_type
                                .map(workflow_record_artifact_type_tag),
                            "default_artifact_title": workflow.effective_contract.completion.default_artifact_title.clone(),
                            "effective_contract": workflow.effective_contract,
                            "checklist_evidence_artifact_type": workflow_record_artifact_type_tag(checklist_evidence.artifact_type),
                            "checklist_evidence_present": checklist_evidence.count > 0,
                            "checklist_evidence_count": checklist_evidence.count,
                            "checklist_evidence_artifact_ids": checklist_evidence.artifact_ids,
                            "checklist_evidence_titles": checklist_evidence.titles,
                        }),
                    );
                }

                merge_hook_contribution(
                    &mut snapshot,
                    HookContributionSet {
                        sources: source_refs.clone(),
                        tags: vec![
                            format!("workflow:{}", workflow_scope_key(&workflow)),
                            format!("workflow_step:{}", workflow.active_step.key),
                            format!(
                                "workflow_status:{}",
                                workflow_run_status_tag(workflow.run.status)
                            ),
                        ],
                        context_fragments: build_workflow_step_fragments(
                            &workflow,
                            &source_summary,
                            &source_refs,
                        ),
                        constraints: workflow
                            .effective_contract
                            .constraints
                            .iter()
                            .map(|constraint| HookConstraint {
                                key: format!(
                                    "workflow:{}:{}:constraint:{}",
                                    workflow_scope_key(&workflow),
                                    workflow.active_step.key,
                                    constraint.key
                                ),
                                description: constraint.description.clone(),
                                source_summary: source_summary.clone(),
                                source_refs: source_refs.clone(),
                            })
                            .collect(),
                        policies: build_workflow_policies(&workflow, &source_summary, &source_refs),
                        diagnostics: Vec::new(),
                    },
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
            permission_policy: None,
            working_directory: None,
            workspace_root: None,
            owners: Vec::new(),
            tags: Vec::new(),
        })
        .await
    }

    async fn evaluate_hook(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError> {
        let snapshot = query
            .snapshot
            .clone()
            .unwrap_or_else(|| SessionHookSnapshot {
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
                        "active_workflow_resolved" | "session_binding_found"
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
            HookTrigger::BeforeTool | HookTrigger::AfterTool | HookTrigger::AfterTurn => {
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                );
            }
            HookTrigger::BeforeStop => {
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                );
                if let Some(decision) = completion_decision_for_active_workflow_snapshot(
                    &snapshot,
                    &WorkflowCompletionSignalSet {
                        task_status: active_task_status(&snapshot).map(ToString::to_string),
                        checklist_evidence_present: checklist_evidence_present(&snapshot),
                        ..WorkflowCompletionSignalSet::default()
                    },
                ) {
                    resolution.matched_rule_keys.push(format!(
                        "completion:{}:{}",
                        workflow_step_key(&snapshot).unwrap_or("unknown"),
                        decision.transition_policy
                    ));
                    self.apply_completion_decision(&snapshot, decision, &mut resolution)
                        .await?;
                }
            }
            HookTrigger::SessionTerminal => {
                if let Some(decision) = completion_decision_for_active_workflow_snapshot(
                    &snapshot,
                    &WorkflowCompletionSignalSet {
                        session_terminal_state: parse_session_terminal_state(
                            query.payload.as_ref(),
                        ),
                        session_terminal_message: query
                            .payload
                            .as_ref()
                            .and_then(|value| value.get("message"))
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string),
                        ..WorkflowCompletionSignalSet::default()
                    },
                ) {
                    resolution.matched_rule_keys.push(format!(
                        "completion:{}:{}",
                        workflow_step_key(&snapshot).unwrap_or("unknown"),
                        decision.transition_policy
                    ));
                    self.apply_completion_decision(&snapshot, decision, &mut resolution)
                        .await?;
                }
            }
            HookTrigger::BeforeSubagentDispatch
            | HookTrigger::AfterSubagentDispatch
            | HookTrigger::SubagentResult => {
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                );
            }
        }

        Ok(resolution)
    }

    async fn advance_workflow_step(
        &self,
        request: HookStepAdvanceRequest,
    ) -> Result<(), HookError> {
        let run_id = Uuid::parse_str(&request.run_id)
            .map_err(|e| HookError::Runtime(format!("advance: invalid run_id: {e}")))?;

        let record_artifacts: Vec<WorkflowRecordArtifactDraft> = request
            .record_artifacts
            .into_iter()
            .filter_map(|value| {
                let title = value.get("title")?.as_str()?.to_string();
                let content = value.get("content")?.as_str()?.to_string();
                let artifact_type_str = value.get("artifact_type")?.as_str()?;
                let artifact_type: WorkflowRecordArtifactType =
                    serde_json::from_value(serde_json::json!(artifact_type_str)).ok()?;
                Some(WorkflowRecordArtifactDraft {
                    artifact_type,
                    title,
                    content,
                })
            })
            .collect();

        let service = LifecycleRunService::new(
            self.workflow_definition_repo.as_ref(),
            self.lifecycle_definition_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
        );
        service
            .complete_step(CompleteLifecycleStepCommand {
                run_id,
                step_key: request.step_key,
                summary: request.summary,
                record_artifacts,
            })
            .await
            .map_err(|e| HookError::Runtime(format!("advance_workflow_step: {e}")))?;

        Ok(())
    }

    async fn append_execution_log(
        &self,
        entries: Vec<PendingExecutionLogEntry>,
    ) -> Result<(), HookError> {
        workflow_recording::flush_execution_log_entries(
            self.lifecycle_run_repo.as_ref(),
            entries,
        )
        .await
        .map_err(|e| HookError::Runtime(format!("flush execution log: {e}")))
    }
}

fn build_workflow_policies(
    workflow: &ActiveWorkflowProjection,
    source_summary: &[String],
    source_refs: &[HookSourceRef],
) -> Vec<HookPolicyView> {
    let scope = workflow_scope_key(workflow);
    let step_advance = lifecycle_step_advance_label(&workflow.active_step);
    let mut policies = vec![HookPolicyView {
        key: format!(
            "workflow:{}:{}:step_advance",
            scope, workflow.active_step.key
        ),
        description: format!(
            "当前 step 推进模式为 `{step_advance}`（有 workflow_key 时为 auto，否则为 manual）。",
        ),
        source_summary: source_summary.to_vec(),
        source_refs: source_refs.to_vec(),
        payload: Some(serde_json::json!({
            "lifecycle_key": workflow.lifecycle.key,
            "step_key": workflow.active_step.key,
            "step_advance": step_advance,
            "workflow_key": workflow.active_step.workflow_key,
        })),
    }];

    for constraint in &workflow.effective_contract.constraints {
        policies.push(HookPolicyView {
            key: format!(
                "workflow:{}:{}:constraint:{}",
                scope, workflow.active_step.key, constraint.key
            ),
            description: constraint.description.clone(),
            source_summary: source_summary.to_vec(),
            source_refs: source_refs.to_vec(),
            payload: Some(serde_json::json!({
                "kind": constraint.kind,
                "payload": constraint.payload.clone(),
            })),
        });
    }

    if step_advance == "auto" && !workflow.effective_contract.completion.checks.is_empty() {
        policies.push(HookPolicyView {
            key: format!(
                "workflow:{}:{}:check_gate",
                scope, workflow.active_step.key
            ),
            description:
                "当前 step 会基于 contract checks 自动推进；在满足所有检查前，不应提前结束当前 loop。"
                    .to_string(),
            source_summary: source_summary.to_vec(),
            source_refs: source_refs.to_vec(),
            payload: Some(serde_json::json!({
                "check_count": workflow.effective_contract.completion.checks.len(),
                "step_key": workflow.active_step.key,
            })),
        });
    }

    policies
}


fn extract_tool_arg<'a>(payload: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    payload
        .and_then(|value| value.get("args"))
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
}

fn extract_payload_str<'a>(payload: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    payload
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
}

pub(super) fn extract_payload_string_list(payload: Option<&serde_json::Value>, key: &str) -> Vec<String> {
    payload
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(super) fn build_subagent_result_context(
    subagent_type: &str,
    summary: &str,
    status: &str,
    dispatch_id: &str,
    companion_session_id: &str,
    findings: &[String],
    follow_ups: &[String],
    artifact_refs: &[String],
    is_blocking: bool,
) -> String {
    let mut sections = vec![if is_blocking {
        "## Companion Blocking Review".to_string()
    } else {
        "## Companion Follow-up".to_string()
    }];
    sections.push(format!("- 类型: {subagent_type}"));
    sections.push(format!("- status: {status}"));
    sections.push(format!("- dispatch_id: {dispatch_id}"));
    sections.push(format!("- companion_session_id: {companion_session_id}"));
    sections.push(format!("- 摘要: {summary}"));

    if !findings.is_empty() {
        sections.push("\n### 关键发现".to_string());
        sections.extend(findings.iter().map(|item| format!("- {item}")));
    }
    if !follow_ups.is_empty() {
        sections.push("\n### 建议后续动作".to_string());
        sections.extend(follow_ups.iter().map(|item| format!("- {item}")));
    }
    if !artifact_refs.is_empty() {
        sections.push("\n### 相关产物".to_string());
        sections.extend(artifact_refs.iter().map(|item| format!("- {item}")));
    }

    sections.push(if is_blocking {
        "\n请先明确这份 companion 结果如何被主 session 采纳、拒绝或拆解，不要直接忽略后继续结束本轮。"
            .to_string()
    } else {
        "\n请把这份 companion 结果吸收进主 session 的下一步行动中，再继续推进。".to_string()
    });
    sections.join("\n")
}

pub(super) fn shell_exec_rewritten_args(
    snapshot: &SessionHookSnapshot,
    payload: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    let workspace_root = snapshot_workspace_root(snapshot)?;
    let cwd = extract_tool_arg(payload, "cwd")?;
    if !std::path::Path::new(cwd).is_absolute() {
        return None;
    }

    let rewritten_cwd = absolutize_cwd_to_workspace_relative(workspace_root, cwd)?;
    let mut args = payload?.get("args")?.clone();
    args.as_object_mut()?
        .insert("cwd".to_string(), serde_json::Value::String(rewritten_cwd));
    Some(args)
}

fn absolutize_cwd_to_workspace_relative(workspace_root: &str, cwd: &str) -> Option<String> {
    let display_root = normalize_path_display_for_hook(workspace_root);
    let display_cwd = normalize_path_display_for_hook(cwd);
    let normalized_root = display_root.to_ascii_lowercase();
    let normalized_cwd = display_cwd.to_ascii_lowercase();
    if normalized_root.is_empty() || normalized_cwd.is_empty() {
        return None;
    }
    if normalized_cwd == normalized_root {
        return Some(".".to_string());
    }

    let prefix = format!("{normalized_root}/");
    normalized_cwd.strip_prefix(&prefix).and_then(|_| {
        display_cwd
            .get(prefix.len()..)
            .map(|suffix| suffix.trim_matches('/').to_string())
            .filter(|value| !value.is_empty())
    })
}

fn normalize_path_display_for_hook(path: &str) -> String {
    path.replace('\\', "/")
        .trim()
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn tool_call_failed(payload: Option<&serde_json::Value>) -> bool {
    payload
        .and_then(|value| value.get("is_error"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(super) fn is_update_task_status_tool(tool_name: &str) -> bool {
    tool_name.ends_with("update_task_status")
}

pub(super) fn is_report_workflow_artifact_tool(tool_name: &str) -> bool {
    tool_name.ends_with("report_workflow_artifact")
}

fn workflow_record_artifact_type_tag(
    artifact_type: agentdash_domain::workflow::WorkflowRecordArtifactType,
) -> &'static str {
    match artifact_type {
        agentdash_domain::workflow::WorkflowRecordArtifactType::SessionSummary => "session_summary",
        agentdash_domain::workflow::WorkflowRecordArtifactType::JournalUpdate => "journal_update",
        agentdash_domain::workflow::WorkflowRecordArtifactType::ArchiveSuggestion => {
            "archive_suggestion"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::PhaseNote => "phase_note",
        agentdash_domain::workflow::WorkflowRecordArtifactType::ChecklistEvidence => {
            "checklist_evidence"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::ExecutionTrace => {
            "execution_trace"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::DecisionRecord => {
            "decision_record"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::ContextSnapshot => {
            "context_snapshot"
        }
    }
}

fn active_workflow_checklist_evidence_summary(
    workflow: &ActiveWorkflowProjection,
) -> ActiveWorkflowChecklistEvidenceSummary {
    let artifact_type = workflow
        .effective_contract
        .completion
        .default_artifact_type
        .unwrap_or(agentdash_domain::workflow::WorkflowRecordArtifactType::PhaseNote);
    let matching = workflow
        .run
        .record_artifacts
        .iter()
        .filter(|artifact| {
            artifact.step_key == workflow.active_step.key
                && artifact.artifact_type == artifact_type
                && !artifact.content.trim().is_empty()
        })
        .collect::<Vec<_>>();

    ActiveWorkflowChecklistEvidenceSummary {
        artifact_type,
        count: matching.len(),
        artifact_ids: matching.iter().map(|artifact| artifact.id).collect(),
        titles: matching
            .iter()
            .map(|artifact| artifact.title.trim())
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect(),
    }
}

fn build_completion_record_artifacts_from_snapshot(
    snapshot: &SessionHookSnapshot,
    decision: &WorkflowCompletionDecision,
) -> Vec<agentdash_application::workflow::WorkflowRecordArtifactDraft> {
    build_step_completion_artifact_drafts(
        workflow_step_key(snapshot).unwrap_or("workflow_step"),
        active_workflow_default_artifact_type(snapshot),
        active_workflow_default_artifact_title(snapshot),
        decision,
    )
}

fn build_step_summary_markdown(workflow: &ActiveWorkflowProjection) -> String {
    let wf_line = match workflow.primary_workflow.as_ref() {
        Some(w) => format!("- workflow: {} (`{}`)", w.name, w.key),
        None => "- workflow: (none)".to_string(),
    };
    format!(
        "## Active Workflow Step\n- lifecycle: {} (`{}`)\n- step: `{}`\n{}\n- advance: `{}`\n- status: `{}`\n\n{}",
        workflow.lifecycle.name,
        workflow.lifecycle.key,
        workflow.active_step.key,
        wf_line,
        lifecycle_step_advance_label(&workflow.active_step),
        workflow_run_status_tag(workflow.run.status),
        workflow.active_step.description
    )
}

fn build_workflow_step_fragments(
    workflow: &ActiveWorkflowProjection,
    source_summary: &[String],
    source_refs: &[HookSourceRef],
) -> Vec<HookContextFragment> {
    let mut fragments = vec![HookContextFragment {
        slot: "workflow".to_string(),
        label: "active_workflow_step".to_string(),
        content: build_step_summary_markdown(workflow),
        source_summary: source_summary.to_vec(),
        source_refs: source_refs.to_vec(),
    }];

    if !workflow
        .effective_contract
        .injection
        .instructions
        .is_empty()
    {
        fragments.push(HookContextFragment {
            slot: "workflow".to_string(),
            label: "active_workflow_instructions".to_string(),
            content: build_instruction_injection_markdown(
                &workflow.effective_contract.injection.instructions,
            ),
            source_summary: source_summary.to_vec(),
            source_refs: source_refs.to_vec(),
        });
    }

    fragments
}

fn build_instruction_injection_markdown(instructions: &[String]) -> String {
    let body = instructions
        .iter()
        .map(|instruction| format!("- {instruction}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("## Workflow Instructions\n{body}")
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
    use std::sync::Arc;

    use agentdash_agent::{
        AgentContext, AgentMessage, BeforeToolCallInput, ToolCallDecision, ToolCallInfo,
    };
    use agentdash_application::workflow::{evaluate_step_completion, WorkflowTargetSummary};
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowCheckKind,
        WorkflowCheckSpec, WorkflowCompletionSpec, WorkflowConstraintKind, WorkflowContract,
        WorkflowDefinition, WorkflowDefinitionSource, WorkflowInjectionSpec, build_effective_contract,
    };
    use agentdash_executor::{
        ExecutionHookProvider, HookError, HookRuntimeDelegate, HookSessionRuntime,
        HookSessionRuntimeSnapshot, HookTrigger, SessionHookRefreshQuery, SessionHookSnapshotQuery,
    };
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    fn snapshot_with_workflow(
        step_key: &str,
        completion_mode: &str,
        task_status: Option<&str>,
    ) -> SessionHookSnapshot {
        snapshot_with_workflow_and_evidence(step_key, completion_mode, task_status, false)
    }

    fn snapshot_with_workflow_and_evidence(
        step_key: &str,
        completion_mode: &str,
        task_status: Option<&str>,
        checklist_evidence_present: bool,
    ) -> SessionHookSnapshot {
        let (step_advance, workflow_key, mut contract) = match completion_mode {
            "checklist_passed" => (
                "auto",
                Some("trellis_dev_task_check"),
                WorkflowContract {
                    constraints: vec![agentdash_domain::workflow::WorkflowConstraintSpec {
                        key: "block_stop_until_checks_pass".to_string(),
                        kind: WorkflowConstraintKind::BlockStopUntilChecksPass,
                        description: "block stop".to_string(),
                        payload: None,
                    }],
                    completion: WorkflowCompletionSpec {
                        checks: vec![
                            WorkflowCheckSpec {
                                key: "task_ready".to_string(),
                                kind: WorkflowCheckKind::TaskStatusIn,
                                description: "task ready".to_string(),
                                payload: Some(serde_json::json!({
                                    "statuses": ["awaiting_verification", "completed"]
                                })),
                            },
                            WorkflowCheckSpec {
                                key: "checklist_evidence_present".to_string(),
                                kind: WorkflowCheckKind::ChecklistEvidencePresent,
                                description: "checklist evidence".to_string(),
                                payload: None,
                            },
                        ],
                        ..WorkflowCompletionSpec::default()
                    },
                    ..WorkflowContract::default()
                },
            ),
            "session_ended" => (
                "auto",
                Some("trellis_dev_task_implement"),
                WorkflowContract::default(),
            ),
            _ => ("manual", None, WorkflowContract::default()),
        };
        if step_key == "implement" {
            contract.constraints.push(agentdash_domain::workflow::WorkflowConstraintSpec {
                key: "deny_complete_status".to_string(),
                kind: WorkflowConstraintKind::DenyTaskStatusTransition,
                description: "deny completed".to_string(),
                payload: Some(serde_json::json!({
                    "to": ["completed"]
                })),
            });
        }
        let effective_contract = serde_json::json!(contract);
        let workflow_source = HookSourceRef {
            layer: HookSourceLayer::Workflow,
            key: format!("trellis_dev_task:{step_key}"),
            label: format!("Workflow / Trellis Dev Workflow / {step_key}"),
            priority: 300,
        };
        let mut snapshot = SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            sources: vec![workflow_source],
            metadata: Some(serde_json::json!({
                "workspace_root": "F:/Projects/AgentDash",
                "active_workflow": {
                    "lifecycle_key": "trellis_dev_task",
                    "step_key": step_key,
                    "step_advance": step_advance,
                    "step_completion_mode": step_advance,
                    "workflow_key": workflow_key,
                    "run_id": "00000000-0000-0000-0000-0000000000aa",
                    "effective_contract": effective_contract,
                    "checklist_evidence_present": checklist_evidence_present,
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

    fn snapshot_with_supervised_policy() -> SessionHookSnapshot {
        SessionHookSnapshot {
            session_id: "sess-supervised".to_string(),
            metadata: Some(serde_json::json!({
                "workspace_root": "F:/Projects/AgentDash",
                "permission_policy": "SUPERVISED",
            })),
            ..SessionHookSnapshot::default()
        }
    }

    fn workflow_projection_with_instructions(
        instructions: Vec<String>,
    ) -> ActiveWorkflowProjection {
        let contract = WorkflowContract {
            injection: WorkflowInjectionSpec {
                instructions,
                ..WorkflowInjectionSpec::default()
            },
            ..WorkflowContract::default()
        };
        let definition = WorkflowDefinition::new(
            "trellis_dev_task_implement",
            "Trellis Dev Workflow / Implement",
            "workflow desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow definition should build");
        let active_step = LifecycleStepDefinition {
            key: "implement".to_string(),
            description: "实现并记录结果".to_string(),
            workflow_key: Some(definition.key.clone()),
        };
        let lifecycle = LifecycleDefinition::new(
            "trellis_dev_task",
            "Trellis Dev Lifecycle",
            "lifecycle desc",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            "implement",
            vec![active_step.clone()],
        )
        .expect("lifecycle definition should build");
        let project_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let mut run = LifecycleRun::new(
            project_id,
            lifecycle.id,
            WorkflowTargetKind::Task,
            target_id,
            &lifecycle.steps,
            &lifecycle.entry_step_key,
        )
        .expect("workflow run should build");
        run.activate_step("implement")
            .expect("implement step should activate");
        let effective_contract =
            build_effective_contract(&lifecycle.key, &active_step.key, Some(&definition));
        ActiveWorkflowProjection {
            run,
            lifecycle,
            active_step,
            primary_workflow: Some(definition),
            effective_contract,
            target: WorkflowTargetSummary {
                target_kind: WorkflowTargetKind::Task,
                target_id,
                target_label: Some("Task A".to_string()),
            },
        }
    }

    #[test]
    fn workflow_step_fragments_do_not_duplicate_constraints_fragment() {
        let workflow = workflow_projection_with_instructions(vec![
            "先更新 task 状态，再结束 session".to_string(),
        ]);
        let source_refs = vec![HookSourceRef {
            layer: HookSourceLayer::Workflow,
            key: "trellis_dev_task:implement".to_string(),
            label: "Workflow / Trellis Dev Workflow / implement".to_string(),
            priority: 300,
        }];
        let source_summary = source_summary_from_refs(&source_refs);

        let fragments = build_workflow_step_fragments(&workflow, &source_summary, &source_refs);

        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0].label, "active_workflow_step");
        assert_eq!(fragments[1].label, "active_workflow_instructions");
        assert!(
            !fragments
                .iter()
                .any(|fragment| fragment.label == "workflow_step_constraints")
        );
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

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(resolution.block_reason.is_some());
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"workflow_step:implement:block_completed_status".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_tool_task_status_blocked")
        );
    }

    #[test]
    fn before_tool_rewrites_shell_exec_absolute_cwd_to_workspace_relative() {
        let snapshot = snapshot_with_workflow("implement", "session_ended", Some("running"));
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeTool,
            turn_id: None,
            tool_name: Some("shell_exec".to_string()),
            tool_call_id: Some("call-shell-1".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "args": {
                    "cwd": "F:\\Projects\\AgentDash\\crates\\agentdash-agent",
                    "command": "cargo test"
                }
            })),
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert_eq!(
            resolution
                .rewritten_tool_input
                .as_ref()
                .and_then(|value| value.get("cwd"))
                .and_then(serde_json::Value::as_str),
            Some("crates/agentdash-agent")
        );
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"tool:shell_exec:rewrite_absolute_cwd".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_tool_shell_exec_cwd_rewritten")
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

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(!resolution.context_fragments.is_empty());
        assert!(!resolution.constraints.is_empty());
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"workflow_completion:checklist_pending:stop_gate".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_stop_workflow_checks_pending")
        );
    }

    #[test]
    fn before_stop_requires_checklist_evidence_even_when_task_ready() {
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

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(!resolution.context_fragments.is_empty());
        assert!(!resolution.constraints.is_empty());
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"workflow_completion:checklist_pending:stop_gate".to_string())
        );
    }

    #[test]
    fn before_stop_allows_ready_task_with_checklist_evidence() {
        let snapshot = snapshot_with_workflow_and_evidence(
            "check",
            "checklist_passed",
            Some("awaiting_verification"),
            true,
        );
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

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(resolution.context_fragments.is_empty());
        assert!(resolution.constraints.is_empty());
        assert!(resolution.matched_rule_keys.is_empty());
    }

    #[test]
    fn after_turn_does_not_inject_perpetual_check_phase_steering() {
        let snapshot = snapshot_with_workflow("check", "checklist_passed", Some("running"));
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::AfterTurn,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "assistant_message": {
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "检查完成，准备结束。" }]
                },
                "tool_results": []
            })),
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(resolution.context_fragments.is_empty());
        assert!(resolution.constraints.is_empty());
        assert!(resolution.matched_rule_keys.is_empty());
    }

    #[test]
    fn before_stop_allows_checklist_completion_when_task_ready() {
        let snapshot = snapshot_with_workflow("check", "checklist_passed", Some("completed"));
        let contract = active_workflow_contract(&snapshot).expect("contract");
        let decision = evaluate_step_completion(
            Some(&contract.completion),
            &WorkflowCompletionSignalSet {
                task_status: active_task_status(&snapshot).map(ToString::to_string),
                checklist_evidence_present: true,
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(decision.satisfied);
        assert!(decision.should_complete_step);
        assert_eq!(
            decision.summary.as_deref(),
            Some("All completion checks passed")
        );
    }

    #[test]
    fn before_stop_blocks_when_task_still_running_without_active_workflow() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-task-running".to_string(),
            metadata: Some(serde_json::json!({
                "active_task": {
                    "task_id": "task-1",
                    "status": "running",
                }
            })),
            ..SessionHookSnapshot::default()
        };
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

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(
            resolution
                .matched_rule_keys
                .contains(&"task_runtime:running_status:stop_gate".to_string())
        );
        assert!(
            resolution
                .constraints
                .iter()
                .any(|constraint| constraint.key == "before_stop:task_status_running")
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_stop_task_status_running")
        );
    }

    #[derive(Clone)]
    struct RuleEngineTestProvider {
        snapshot: SessionHookSnapshot,
    }

    #[async_trait]
    impl ExecutionHookProvider for RuleEngineTestProvider {
        async fn load_session_snapshot(
            &self,
            _query: SessionHookSnapshotQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Ok(self.snapshot.clone())
        }

        async fn refresh_session_snapshot(
            &self,
            _query: SessionHookRefreshQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Ok(self.snapshot.clone())
        }

        async fn evaluate_hook(
            &self,
            query: HookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            let snapshot = query
                .snapshot
                .clone()
                .unwrap_or_else(|| self.snapshot.clone());
            let mut resolution = HookResolution::default();
            apply_hook_rules(
                HookEvaluationContext {
                    snapshot: &snapshot,
                    query: &query,
                },
                &mut resolution,
            );
            Ok(resolution)
        }
    }

    #[tokio::test]
    async fn runtime_delegate_before_tool_rewrite_records_trace() {
        let snapshot = snapshot_with_workflow("implement", "session_ended", Some("running"));
        let hook_session = Arc::new(HookSessionRuntime::new(
            snapshot.session_id.clone(),
            Arc::new(RuleEngineTestProvider {
                snapshot: snapshot.clone(),
            }),
            snapshot,
        ));
        let delegate = HookRuntimeDelegate::new(hook_session.clone());

        let decision = delegate
            .before_tool_call(
                BeforeToolCallInput {
                    assistant_message: AgentMessage::assistant("准备执行 shell"),
                    tool_call: ToolCallInfo {
                        id: "call-shell-1".to_string(),
                        call_id: None,
                        name: "shell_exec".to_string(),
                        arguments: serde_json::json!({
                            "cwd": "F:\\Projects\\AgentDash\\crates\\agentdash-agent",
                            "command": "cargo test"
                        }),
                    },
                    args: serde_json::json!({
                        "cwd": "F:\\Projects\\AgentDash\\crates\\agentdash-agent",
                        "command": "cargo test"
                    }),
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![],
                        tools: vec![],
                    },
                },
                CancellationToken::new(),
            )
            .await
            .expect("before_tool_call 应返回 rewrite");

        match decision {
            ToolCallDecision::Rewrite { args, note } => {
                assert!(note.is_none());
                assert_eq!(
                    args.get("cwd").and_then(serde_json::Value::as_str),
                    Some("crates/agentdash-agent")
                );
            }
            other => panic!("期望 Rewrite，实际得到 {other:?}"),
        }

        let runtime: HookSessionRuntimeSnapshot = hook_session.runtime_snapshot();
        assert_eq!(runtime.trace.len(), 1);
        assert_eq!(runtime.trace[0].trigger, HookTrigger::BeforeTool);
        assert_eq!(runtime.trace[0].decision, "rewrite");
        assert!(
            runtime.trace[0]
                .matched_rule_keys
                .contains(&"tool:shell_exec:rewrite_absolute_cwd".to_string())
        );
    }

    #[test]
    fn before_tool_supervised_policy_requests_approval() {
        let snapshot = snapshot_with_supervised_policy();
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeTool,
            turn_id: Some("turn-approval-1".to_string()),
            tool_name: Some("shell_exec".to_string()),
            tool_call_id: Some("call-shell-approval".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "args": {
                    "cwd": ".",
                    "command": "cargo test"
                }
            })),
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert_eq!(
            resolution
                .approval_request
                .as_ref()
                .map(|request| request.reason.as_str()),
            Some("当前会话使用 SUPERVISED 权限策略，执行 `shell_exec` 前需要用户审批。")
        );
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"global_builtin:supervised:ask_tool_approval".to_string())
        );
    }

    #[test]
    fn before_subagent_dispatch_inherits_runtime_context_and_constraints() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            sources: vec![HookSourceRef {
                layer: HookSourceLayer::Workflow,
                key: "trellis_dev_task:check".to_string(),
                label: "Workflow / Trellis Dev Workflow / Check".to_string(),
                priority: 300,
            }],
            owners: vec![HookOwnerSummary {
                owner_type: "story".to_string(),
                owner_id: Uuid::new_v4().to_string(),
                label: Some("Story A".to_string()),
                project_id: None,
                story_id: None,
                task_id: None,
            }],
            context_fragments: vec![HookContextFragment {
                slot: "workflow".to_string(),
                label: "active_workflow_step".to_string(),
                content: "step info".to_string(),
                source_summary: vec!["workflow:trellis_dev_task".to_string()],
                source_refs: vec![HookSourceRef {
                    layer: HookSourceLayer::Workflow,
                    key: "trellis_dev_task:check".to_string(),
                    label: "Workflow / Trellis Dev Workflow / Check".to_string(),
                    priority: 300,
                }],
            }],
            constraints: vec![HookConstraint {
                key: "workflow:check".to_string(),
                description: "先验证再结束".to_string(),
                source_summary: vec!["workflow_step:check".to_string()],
                source_refs: vec![HookSourceRef {
                    layer: HookSourceLayer::Workflow,
                    key: "trellis_dev_task:check".to_string(),
                    label: "Workflow / Trellis Dev Workflow / Check".to_string(),
                    priority: 300,
                }],
            }],
            ..SessionHookSnapshot::default()
        };
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeSubagentDispatch,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: Some("companion".to_string()),
            snapshot: None,
            payload: Some(serde_json::json!({
                "prompt": "请帮我 review"
            })),
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert_eq!(resolution.context_fragments.len(), 1);
        assert_eq!(resolution.constraints.len(), 1);
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"subagent_dispatch:inherit_runtime_context".to_string())
        );
    }

    #[test]
    fn subagent_result_records_structured_return_channel_diagnostic() {
        let snapshot =
            snapshot_with_workflow("check", "checklist_passed", Some("awaiting_verification"));
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::SubagentResult,
            turn_id: Some("turn-parent-1".to_string()),
            tool_name: None,
            tool_call_id: None,
            subagent_type: Some("companion".to_string()),
            snapshot: None,
            payload: Some(serde_json::json!({
                "dispatch_id": "dispatch-1",
                "companion_session_id": "sess-companion-1",
                "adoption_mode": "blocking_review",
                "status": "completed",
                "summary": "子 agent 已完成 review，并附带后续建议"
            })),
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(
            resolution
                .matched_rule_keys
                .contains(&"subagent_result:return_channel_recorded".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "subagent_result_recorded"
                    && entry.summary.contains("子 agent 已完成 review"))
        );
        assert_eq!(resolution.context_fragments.len(), 1);
        assert_eq!(resolution.constraints.len(), 1);
        assert!(
            resolution.context_fragments[0]
                .label
                .contains("subagent_blocking_review")
        );
        assert!(
            resolution.constraints[0]
                .key
                .contains("subagent_result:blocking_review")
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "subagent_result_blocking_review_enqueued")
        );
    }
}
