use std::sync::Arc;

use agentdash_domain::agent::{AgentRepository, ProjectAgentLinkRepository};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
};
use agentdash_spi::hooks::PendingExecutionLogEntry;
use agentdash_spi::{
    ActiveWorkflowMeta, HookDiagnosticEntry, HookError, HookEvaluationQuery, HookInjection,
    HookResolution, HookTrigger, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionHookSnapshotQuery, SessionSnapshotMetadata,
};
use async_trait::async_trait;

use agentdash_spi::{ExecutionHookProvider, HookStepAdvanceRequest};

use crate::workflow::WorkflowCompletionSignalSet;

use super::completion::active_workflow_checklist_evidence_summary;
use super::owner_resolver::SessionOwnerResolver;
use super::presets::builtin_preset_scripts;
use super::rules::*;
use super::script_engine::HookScriptEngine;
use super::snapshot_helpers::*;
use super::workflow_contribution::build_workflow_step_fragments;
use super::workflow_snapshot::WorkflowSnapshotBuilder;
use super::{
    dedupe_tags, global_builtin_source, lifecycle_step_advance_label, map_hook_error,
    workflow_scope_key, workflow_source,
};

/// Facade：组合 SessionOwnerResolver + WorkflowSnapshotBuilder + HookScriptEngine，
/// 对外仍实现 ExecutionHookProvider trait。
pub struct AppExecutionHookProvider {
    pub(super) session_binding_repo: Arc<dyn SessionBindingRepository>,
    pub(super) agent_repo: Arc<dyn AgentRepository>,
    pub(super) agent_link_repo: Arc<dyn ProjectAgentLinkRepository>,
    pub(super) owner_resolver: SessionOwnerResolver,
    pub(super) workflow_builder: WorkflowSnapshotBuilder,
    pub(super) script_engine: HookScriptEngine,
}

impl AppExecutionHookProvider {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
        task_repo: Arc<dyn TaskRepository>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        agent_repo: Arc<dyn AgentRepository>,
        agent_link_repo: Arc<dyn ProjectAgentLinkRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    ) -> Self {
        let preset_scripts = builtin_preset_scripts();
        Self {
            session_binding_repo,
            agent_repo,
            agent_link_repo,
            owner_resolver: SessionOwnerResolver::new(project_repo, story_repo, task_repo),
            workflow_builder: WorkflowSnapshotBuilder::new(
                workflow_definition_repo,
                lifecycle_definition_repo,
                lifecycle_run_repo,
            ),
            script_engine: HookScriptEngine::new(&preset_scripts),
        }
    }

    async fn build_companion_agents_injection(
        &self,
        snapshot: &SessionHookSnapshot,
    ) -> Option<HookInjection> {
        let project_id = snapshot
            .owners
            .iter()
            .find_map(|o| o.project_id.as_deref())
            .and_then(|id| id.parse::<uuid::Uuid>().ok())?;

        let links = self.agent_link_repo.list_by_project(project_id).await.ok()?;
        if links.is_empty() {
            return None;
        }

        let mut lines = vec!["## Companion Agents\n以下 agent 已关联到当前项目，可通过 `companion_request` 工具的 `agent_key` 参数按名称指定：\n".to_string()];
        for link in &links {
            if let Ok(Some(agent)) = self.agent_repo.get_by_id(link.agent_id).await {
                let display = link
                    .merged_config(&agent.base_config)
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| agent.name.clone());
                lines.push(format!(
                    "- **{}** (executor: `{}`): {}",
                    agent.name,
                    agent.agent_type,
                    display,
                ));
            }
        }

        if lines.len() <= 1 {
            return None;
        }

        Some(HookInjection {
            slot: "companion_agents".to_string(),
            content: lines.join("\n"),
            source: "builtin:companion_agents".to_string(),
        })
    }

    /// 验证 Rhai 脚本语法是否合法，不执行脚本。
    pub fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.script_engine.validate_script(script)
    }

    /// 运行时注册/更新一个自定义 preset。
    pub fn register_preset(&self, key: &str, script: &str) -> Result<(), String> {
        self.script_engine.register_preset(key, script)
    }

    /// 移除一个自定义 preset。
    pub fn remove_preset(&self, key: &str) -> bool {
        self.script_engine.remove_preset(key)
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
            injections: Vec::new(),
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

        // Add global builtin source and tags
        snapshot.sources.push(global_builtin_source().to_string());
        snapshot.tags.extend([
            "hook_source:global_builtin".to_string(),
            "hook_builtin:runtime_trace".to_string(),
            "hook_builtin:workspace_path_safety".to_string(),
            "hook_builtin:supervised_tool_approval".to_string(),
        ]);

        if bindings.is_empty() {
            snapshot.diagnostics.push(HookDiagnosticEntry {
                code: "session_binding_missing".to_string(),
                message: "当前 session 没有关联的业务 owner，Hook snapshot 为空基线".to_string(),
            });
        }

        for binding in bindings.iter() {
            let resolved_owner = self.owner_resolver.resolve(binding).await?;
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

                if let Ok(Some(task)) = self
                    .owner_resolver
                    .task_repo()
                    .get_by_id(
                        task_id
                            .parse::<uuid::Uuid>()
                            .unwrap_or(uuid::Uuid::nil()),
                    )
                    .await
                {
                    if let Some(meta) = snapshot.metadata.as_mut() {
                        meta.extra.insert(
                            "task_execution_mode".to_string(),
                            serde_json::Value::String(format!("{:?}", task.execution_mode)),
                        );
                        meta.extra.insert(
                            "task_status".to_string(),
                            serde_json::Value::String(format!("{:?}", task.status)),
                        );
                        meta.extra.insert(
                            "task_id".to_string(),
                            serde_json::Value::String(task.id.to_string()),
                        );
                    }
                }
            }

            if let Some(workflow) = self
                .workflow_builder
                .resolve_active_workflow(&owner)
                .await?
            {
                let wf_source = workflow_source(&workflow);
                let checklist_evidence = active_workflow_checklist_evidence_summary(&workflow);

                snapshot.diagnostics.push(HookDiagnosticEntry {
                    code: "active_workflow_resolved".to_string(),
                    message: format!(
                        "命中 active lifecycle step：{} / {}",
                        workflow.lifecycle.key, workflow.active_step.key
                    ),
                });

                if let Some(meta) = snapshot.metadata.as_mut() {
                    let transition_policy = lifecycle_step_advance_label(&workflow.active_step);
                    let step_title = if workflow.active_step.description.trim().is_empty() {
                        workflow.active_step.key.clone()
                    } else {
                        workflow.active_step.description.clone()
                    };
                    meta.active_workflow = Some(ActiveWorkflowMeta {
                        lifecycle_id: Some(workflow.lifecycle.id),
                        lifecycle_key: Some(workflow.lifecycle.key.clone()),
                        lifecycle_name: Some(workflow.lifecycle.name.clone()),
                        run_id: Some(workflow.run.id),
                        run_status: Some(workflow.run.status),
                        step_key: Some(workflow.active_step.key.clone()),
                        step_title: Some(step_title),
                        workflow_key: workflow.active_step.workflow_key.clone(),
                        transition_policy: Some(transition_policy.to_string()),
                        primary_workflow_id: workflow.primary_workflow.as_ref().map(|w| w.id),
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
                            .default_artifact_type,
                        default_artifact_title: workflow
                            .effective_contract
                            .completion
                            .default_artifact_title
                            .clone(),
                        effective_contract: Some(workflow.effective_contract.clone()),
                        checklist_evidence_artifact_type: Some(checklist_evidence.artifact_type),
                        checklist_evidence_present: Some(checklist_evidence.count > 0),
                        checklist_evidence_count: Some(checklist_evidence.count as u32),
                        checklist_evidence_artifact_ids: Some(
                            checklist_evidence.artifact_ids.clone(),
                        ),
                        checklist_evidence_titles: Some(checklist_evidence.titles.clone()),
                    });
                }

                // Add workflow source
                snapshot.sources.push(wf_source.clone());

                // Add workflow tags
                snapshot.tags.extend([
                    format!("workflow:{}", workflow_scope_key(&workflow)),
                    format!("workflow_step:{}", workflow.active_step.key),
                    format!(
                        "workflow_status:{}",
                        workflow_run_status_tag(workflow.run.status)
                    ),
                ]);

                // Add workflow step injections
                snapshot
                    .injections
                    .extend(build_workflow_step_fragments(&workflow, &wf_source));

                // Add workflow constraint injections
                snapshot
                    .injections
                    .extend(
                        workflow
                            .effective_contract
                            .constraints
                            .iter()
                            .map(|constraint| HookInjection {
                                slot: "constraint".to_string(),
                                content: constraint.description.clone(),
                                source: wf_source.clone(),
                            }),
                    );
            }

            snapshot.owners.push(owner);
        }

        if let Some(injection) = self.build_companion_agents_injection(&snapshot).await {
            snapshot.injections.push(injection);
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
                resolution.injections = snapshot.injections.clone();
            }
            HookTrigger::BeforeTool | HookTrigger::AfterTool | HookTrigger::AfterTurn => {
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::BeforeStop => {
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                    &self.script_engine,
                );
                if let Some(decision) = completion_decision_for_active_workflow_snapshot(
                    &snapshot,
                    &WorkflowCompletionSignalSet {
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
                // 1) global + workflow contract + owner default hook rules
                //    owner_default_hook_rules 会为 task owner 自动注入
                //    task_session_terminal preset，无需此处硬编码
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                    &self.script_engine,
                );

                // 2) workflow completion decision（step 推进）
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
                    &self.script_engine,
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

    use crate::session::{HookRuntimeDelegate, HookSessionRuntime};
    use agentdash_spi::hooks::HookSessionRuntimeAccess;
    use agentdash_spi::hooks::{HookEvaluationQuery, HookResolution, SessionHookSnapshot};
    use agentdash_spi::{
        AgentContext, AgentMessage, BeforeToolCallInput, ToolCallDecision, ToolCallInfo,
    };
    use agentdash_spi::{
        ExecutionHookProvider, HookError, HookTrigger, SessionHookRefreshQuery,
        SessionHookSnapshotQuery,
    };
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::super::presets::builtin_preset_scripts;
    use super::super::rules::{HookEvaluationContext, apply_hook_rules};
    use super::super::script_engine::HookScriptEngine;
    use super::super::test_fixtures::snapshot_with_workflow;

    struct RuleEngineTestProvider {
        snapshot: SessionHookSnapshot,
        engine: HookScriptEngine,
    }

    impl RuleEngineTestProvider {
        fn new(snapshot: SessionHookSnapshot) -> Self {
            let scripts = builtin_preset_scripts();
            Self {
                snapshot,
                engine: HookScriptEngine::new(&scripts),
            }
        }
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
                &self.engine,
            );
            Ok(resolution)
        }
    }

    #[tokio::test]
    async fn runtime_delegate_before_tool_rewrite_records_trace() {
        let snapshot = snapshot_with_workflow("implement", "session_ended");
        let hook_session = Arc::new(HookSessionRuntime::new(
            snapshot.session_id.clone(),
            Arc::new(RuleEngineTestProvider::new(snapshot.clone())),
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
                            "cwd": "/tmp/test-workspace/crates/agentdash-agent",
                            "command": "cargo test"
                        }),
                    },
                    args: serde_json::json!({
                        "cwd": "/tmp/test-workspace/crates/agentdash-agent",
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
