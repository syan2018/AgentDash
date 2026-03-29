use std::sync::Arc;

use agentdash_spi::{
    ActiveTaskMeta, ActiveWorkflowMeta, HookContributionSet, HookConstraint, HookDiagnosticEntry,
    HookError, HookEvaluationQuery, HookResolution, HookTrigger, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionHookSnapshotQuery, SessionSnapshotMetadata,
};
use agentdash_spi::hooks::PendingExecutionLogEntry;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
};
use async_trait::async_trait;

use agentdash_spi::{
    ExecutionHookProvider, HookStepAdvanceRequest,
};

use crate::workflow::WorkflowCompletionSignalSet;

use super::completion::{
    active_workflow_checklist_evidence_summary, workflow_record_artifact_type_tag,
};
use super::owner_resolver::SessionOwnerResolver;
use super::rules::*;
use super::snapshot_helpers::*;
use super::workflow_contribution::{build_workflow_policies, build_workflow_step_fragments};
use super::workflow_snapshot::WorkflowSnapshotBuilder;
use super::{
    dedupe_tags, global_builtin_hook_contribution, lifecycle_step_advance_label, map_hook_error,
    merge_hook_contribution, session_source_ref, source_summary_from_refs, workflow_scope_key,
    workflow_source_refs,
};

/// Facade：组合 SessionOwnerResolver + WorkflowSnapshotBuilder，
/// 对外仍实现 ExecutionHookProvider trait。
pub struct AppExecutionHookProvider {
    pub(super) session_binding_repo: Arc<dyn SessionBindingRepository>,
    pub(super) owner_resolver: SessionOwnerResolver,
    pub(super) workflow_builder: WorkflowSnapshotBuilder,
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
            owner_resolver: SessionOwnerResolver::new(project_repo, story_repo, task_repo),
            workflow_builder: WorkflowSnapshotBuilder::new(
                workflow_definition_repo,
                lifecycle_definition_repo,
                lifecycle_run_repo,
            ),
        }
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
            snapshot.diagnostics.extend(resolved_owner.diagnostics);
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
                        run_status: Some(
                            workflow_run_status_tag(workflow.run.status).to_string(),
                        ),
                        step_key: Some(workflow.active_step.key.clone()),
                        step_title: Some(step_title),
                        workflow_key: workflow.active_step.workflow_key.clone(),
                        step_advance: Some(step_advance.to_string()),
                        transition_policy: Some(step_advance.to_string()),
                        primary_workflow_id: workflow
                            .primary_workflow
                            .as_ref()
                            .map(|w| w.id.to_string()),
                        primary_workflow_key: workflow
                            .primary_workflow
                            .as_ref()
                            .map(|w| w.key.clone()),
                        primary_workflow_name: workflow
                            .primary_workflow
                            .as_ref()
                            .map(|w| w.name.clone()),
                        default_artifact_type: workflow
                            .effective_contract
                            .completion
                            .default_artifact_type
                            .map(workflow_record_artifact_type_tag)
                            .map(str::to_string),
                        default_artifact_title: workflow
                            .effective_contract
                            .completion
                            .default_artifact_title
                            .clone(),
                        effective_contract: serde_json::to_value(&workflow.effective_contract).ok(),
                        checklist_evidence_artifact_type: Some(
                            workflow_record_artifact_type_tag(checklist_evidence.artifact_type)
                                .to_string(),
                        ),
                        checklist_evidence_present: Some(checklist_evidence.count > 0),
                        checklist_evidence_count: Some(checklist_evidence.count as u32),
                        checklist_evidence_artifact_ids: Some(
                            checklist_evidence
                                .artifact_ids
                                .iter()
                                .map(ToString::to_string)
                                .collect(),
                        ),
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
                        policies: build_workflow_policies(
                            &workflow,
                            &source_summary,
                            &source_refs,
                        ),
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_agent::{AgentContext, AgentMessage, BeforeToolCallInput, ToolCallDecision, ToolCallInfo};
    use agentdash_spi::{
        ExecutionHookProvider, HookError, HookTrigger, SessionHookRefreshQuery,
        SessionHookSnapshotQuery,
    };
    use agentdash_spi::hooks::{HookEvaluationQuery, HookResolution, SessionHookSnapshot};
    use crate::session::{HookRuntimeDelegate, HookSessionRuntime};
    use agentdash_spi::hooks::HookSessionRuntimeAccess;
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::super::rules::{HookEvaluationContext, apply_hook_rules};
    use super::super::test_fixtures::snapshot_with_workflow;

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

        let runtime: agentdash_spi::hooks::HookSessionRuntimeSnapshot =
            hook_session.runtime_snapshot();
        assert_eq!(runtime.trace.len(), 1);
        assert_eq!(runtime.trace[0].trigger, HookTrigger::BeforeTool);
        assert_eq!(runtime.trace[0].decision, "rewrite");
        assert!(
            runtime.trace[0]
                .matched_rule_keys
                .contains(&"tool:shell_exec:rewrite_absolute_cwd".to_string())
        );
    }
}
