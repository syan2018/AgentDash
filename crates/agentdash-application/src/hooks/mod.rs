use std::collections::BTreeSet;
use std::sync::Arc;

use crate::workflow::{
    ActiveWorkflowProjection, WorkflowCompletionDecision, WorkflowCompletionSignalSet,
    build_step_completion_artifact_drafts,
};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
};

use agentdash_connector_contract::{
    ActiveTaskMeta, ActiveWorkflowMeta, ExecutionHookProvider, HookCompletionStatus,
    HookConstraint, HookContextFragment, HookContributionSet, HookDiagnosticEntry, HookError,
    HookEvaluationQuery, HookPolicyView, HookResolution, HookSourceLayer, HookSourceRef,
    HookStepAdvanceRequest, HookTrigger, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionHookSnapshotQuery, SessionSnapshotMetadata,
};
use agentdash_connector_contract::hooks::PendingExecutionLogEntry;
use async_trait::async_trait;
use uuid::Uuid;

mod owner_resolver;
mod rules;
mod snapshot_helpers;
mod workflow_snapshot;

pub use owner_resolver::SessionOwnerResolver;
pub use workflow_snapshot::WorkflowSnapshotBuilder;

use crate::workflow::execution_log as workflow_recording;
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

/// Facade：组合 SessionOwnerResolver + WorkflowSnapshotBuilder，
/// 对外仍实现 ExecutionHookProvider trait。
pub struct AppExecutionHookProvider {
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    owner_resolver: SessionOwnerResolver,
    workflow_builder: WorkflowSnapshotBuilder,
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
            session_binding_repo,
            owner_resolver: SessionOwnerResolver::new(
                project_repo,
                story_repo,
                task_repo,
            ),
            workflow_builder: WorkflowSnapshotBuilder::new(
                workflow_definition_repo,
                lifecycle_definition_repo,
                lifecycle_run_repo,
            ),
        }
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
            .workflow_builder
            .get_lifecycle_run(locator.run_id)
            .await?;
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
            metadata: Some(SessionSnapshotMetadata {
                turn_id: query.turn_id,
                connector_id: query.connector_id,
                executor: query.executor,
                permission_policy: query.permission_policy,
                working_directory: query.working_directory,
                workspace_root: query.workspace_root,
                ..Default::default()
            }),
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
            let resolved_owner = self.owner_resolver.resolve(binding).await?;
            let task_status = resolved_owner.task_status.clone();
            snapshot
                .diagnostics
                .extend(resolved_owner.diagnostics);
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
                if let Some(meta) = snapshot.metadata.as_mut() {
                    meta.active_task = Some(ActiveTaskMeta {
                        task_id: owner.task_id.clone(),
                        task_title: owner.label.clone(),
                        status: Some(task_status.to_string()),
                    });
                }
            }

            if let Some(workflow) = self.workflow_builder.resolve_active_workflow(&owner).await? {
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

                if let Some(meta) = snapshot.metadata.as_mut() {
                    let step_advance = lifecycle_step_advance_label(&workflow.active_step);
                    let step_title = if workflow.active_step.description.trim().is_empty() {
                        workflow.active_step.key.clone()
                    } else {
                        workflow.active_step.description.clone()
                    };
                    meta.active_workflow = Some(ActiveWorkflowMeta {
                        lifecycle_id: Some(workflow.lifecycle.id.to_string()),
                        lifecycle_key: Some(workflow.lifecycle.key.clone()),
                        lifecycle_name: Some(workflow.lifecycle.name.clone()),
                        run_id: Some(workflow.run.id.to_string()),
                        run_status: Some(workflow_run_status_tag(workflow.run.status).to_string()),
                        step_key: Some(workflow.active_step.key.clone()),
                        step_title: Some(step_title),
                        workflow_key: workflow.active_step.workflow_key.clone(),
                        step_advance: Some(step_advance.to_string()),
                        transition_policy: Some(step_advance.to_string()),
                        primary_workflow_id: workflow.primary_workflow.as_ref().map(|w| w.id.to_string()),
                        primary_workflow_key: workflow.primary_workflow.as_ref().map(|w| w.key.clone()),
                        primary_workflow_name: workflow.primary_workflow.as_ref().map(|w| w.name.clone()),
                        default_artifact_type: workflow
                            .effective_contract
                            .completion
                            .default_artifact_type
                            .map(workflow_record_artifact_type_tag)
                            .map(str::to_string),
                        default_artifact_title: workflow.effective_contract.completion.default_artifact_title.clone(),
                        effective_contract: serde_json::to_value(&workflow.effective_contract).ok(),
                        checklist_evidence_artifact_type: Some(workflow_record_artifact_type_tag(checklist_evidence.artifact_type).to_string()),
                        checklist_evidence_present: Some(checklist_evidence.count > 0),
                        checklist_evidence_count: Some(checklist_evidence.count as u32),
                        checklist_evidence_artifact_ids: Some(checklist_evidence.artifact_ids.iter().map(ToString::to_string).collect()),
                        checklist_evidence_titles: Some(checklist_evidence.titles.clone()),
                    });
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
        self.workflow_builder.advance_workflow_step(request).await
    }

    async fn append_execution_log(
        &self,
        entries: Vec<PendingExecutionLogEntry>,
    ) -> Result<(), HookError> {
        self.workflow_builder.append_execution_log(entries).await
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
) -> Vec<crate::workflow::WorkflowRecordArtifactDraft> {
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
    use crate::workflow::{evaluate_step_completion, WorkflowTargetSummary};
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowCheckKind,
        WorkflowCheckSpec, WorkflowCompletionSpec, WorkflowConstraintKind, WorkflowContract,
        WorkflowDefinition, WorkflowDefinitionSource, WorkflowInjectionSpec, WorkflowTargetKind,
        build_effective_contract,
    };
    use agentdash_connector_contract::{
        ExecutionHookProvider, HookError, HookOwnerSummary,
        HookSessionRuntime, HookSessionRuntimeSnapshot, HookTrigger, SessionHookRefreshQuery,
        SessionHookSnapshotQuery,
    };
    use agentdash_executor::HookRuntimeDelegate;
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
            metadata: Some(SessionSnapshotMetadata {
                workspace_root: Some("F:/Projects/AgentDash".to_string()),
                active_workflow: Some(ActiveWorkflowMeta {
                    lifecycle_key: Some("trellis_dev_task".to_string()),
                    step_key: Some(step_key.to_string()),
                    step_advance: Some(step_advance.to_string()),
                    transition_policy: Some(step_advance.to_string()),
                    workflow_key: workflow_key.map(str::to_string),
                    run_id: Some("00000000-0000-0000-0000-0000000000aa".to_string()),
                    effective_contract: Some(effective_contract),
                    checklist_evidence_present: Some(checklist_evidence_present),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..SessionHookSnapshot::default()
        };
        if let Some(task_status) = task_status {
            if let Some(meta) = snapshot.metadata.as_mut() {
                meta.active_task = Some(ActiveTaskMeta {
                    task_id: Some("task-1".to_string()),
                    status: Some(task_status.to_string()),
                    ..Default::default()
                });
            }
        }
        snapshot
    }

    fn snapshot_with_supervised_policy() -> SessionHookSnapshot {
        SessionHookSnapshot {
            session_id: "sess-supervised".to_string(),
            metadata: Some(SessionSnapshotMetadata {
                workspace_root: Some("F:/Projects/AgentDash".to_string()),
                permission_policy: Some("SUPERVISED".to_string()),
                ..Default::default()
            }),
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
            metadata: Some(SessionSnapshotMetadata {
                active_task: Some(ActiveTaskMeta {
                    task_id: Some("task-1".to_string()),
                    status: Some("running".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
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
