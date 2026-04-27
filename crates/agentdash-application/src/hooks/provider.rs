use std::sync::Arc;

use agentdash_domain::agent::{AgentRepository, ProjectAgentLinkRepository};
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
    build_effective_contract,
};
use agentdash_spi::hooks::PendingExecutionLogEntry;
use agentdash_spi::{
    ActiveWorkflowMeta, HookDiagnosticEntry, HookError, HookEvaluationQuery, HookInjection,
    HookResolution, HookTrigger, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionHookSnapshotQuery, SessionSnapshotMetadata,
};
use async_trait::async_trait;

use agentdash_spi::{ExecutionHookProvider, HookStepAdvanceRequest};

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
    pub(super) inline_file_repo: Arc<dyn InlineFileRepository>,
    pub(super) owner_resolver: SessionOwnerResolver,
    pub(super) workflow_builder: WorkflowSnapshotBuilder,
    pub(super) script_engine: HookScriptEngine,
}

const SESSION_BASELINE_INJECTION_SLOTS: &[&str] = &["companion_agents"];

fn is_session_baseline_injection(injection: &HookInjection) -> bool {
    SESSION_BASELINE_INJECTION_SLOTS
        .iter()
        .any(|slot| injection.slot == *slot)
}

fn filter_user_prompt_injections(snapshot: &SessionHookSnapshot) -> Vec<HookInjection> {
    snapshot
        .injections
        .iter()
        .filter(|injection| !is_session_baseline_injection(injection))
        .cloned()
        .collect()
}

impl AppExecutionHookProvider {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        agent_repo: Arc<dyn AgentRepository>,
        agent_link_repo: Arc<dyn ProjectAgentLinkRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
    ) -> Self {
        let preset_scripts = builtin_preset_scripts();
        let wf_binding = session_binding_repo.clone();
        let wf_inline = inline_file_repo.clone();
        Self {
            session_binding_repo,
            agent_repo,
            agent_link_repo,
            inline_file_repo,
            owner_resolver: SessionOwnerResolver::new(project_repo, story_repo),
            workflow_builder: WorkflowSnapshotBuilder::new(
                wf_binding,
                workflow_definition_repo,
                lifecycle_definition_repo,
                lifecycle_run_repo,
                wf_inline,
            ),
            script_engine: HookScriptEngine::new(&preset_scripts),
        }
    }

    async fn build_companion_agents_injection(
        &self,
        snapshot: &SessionHookSnapshot,
        bindings: &[agentdash_domain::session_binding::SessionBinding],
    ) -> Option<HookInjection> {
        let project_id = snapshot
            .owners
            .iter()
            .find_map(|o| o.project_id.as_deref())
            .and_then(|id| id.parse::<uuid::Uuid>().ok())?;

        let links = self
            .agent_link_repo
            .list_by_project(project_id)
            .await
            .ok()?;
        if links.is_empty() {
            return None;
        }

        // Resolve caller agent's allowed_companions from its link config
        let caller_allowed: Option<Vec<String>> = self
            .resolve_caller_allowed_companions(bindings, &links)
            .await;

        let mut agents_info: Vec<(String, String, String)> = Vec::new(); // (name, agent_type, display)
        for link in &links {
            if let Ok(Some(agent)) = self.agent_repo.get_by_id(link.agent_id).await {
                // If caller has explicit allowed list, skip agents not in it
                if let Some(ref allowed) = caller_allowed {
                    if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&agent.name)) {
                        continue;
                    }
                }
                let display = link
                    .merged_config(&agent.base_config)
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| agent.name.clone());
                agents_info.push((agent.name, agent.agent_type, display));
            }
        }

        if agents_info.is_empty() {
            return None;
        }

        let mut lines = vec!["## Companion Agents\n以下 agent 已关联到当前项目，可通过 `companion_request` 工具的 `agent_key` 参数按名称指定：\n".to_string()];
        for (name, agent_type, display) in &agents_info {
            lines.push(format!(
                "- **{name}** (executor: `{agent_type}`): {display}"
            ));
        }

        Some(HookInjection {
            slot: "companion_agents".to_string(),
            content: lines.join("\n"),
            source: "builtin:companion_agents".to_string(),
        })
    }

    async fn resolve_caller_allowed_companions(
        &self,
        bindings: &[agentdash_domain::session_binding::SessionBinding],
        project_links: &[agentdash_domain::agent::ProjectAgentLink],
    ) -> Option<Vec<String>> {
        // Find the caller agent UUID from binding label (project_agent:<uuid>)
        let caller_agent_id = bindings.iter().find_map(|b| {
            b.label
                .strip_prefix("project_agent:")
                .and_then(|id_str| id_str.parse::<uuid::Uuid>().ok())
        })?;

        let link = project_links
            .iter()
            .find(|l| l.agent_id == caller_agent_id)?;

        let agent = self.agent_repo.get_by_id(caller_agent_id).await.ok()??;
        let merged = link.merged_config(&agent.base_config);

        let allowed = merged
            .get("allowed_companions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty());

        allowed
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
            tags: Vec::new(),
            injections: Vec::new(),
            diagnostics: Vec::new(),
            metadata: Some(SessionSnapshotMetadata {
                turn_id: query.turn_id,
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

                let task_uuid = task_id.parse::<uuid::Uuid>().unwrap_or(uuid::Uuid::nil());
                if let Ok(Some(task)) = crate::task::load_task(
                    self.owner_resolver.story_repo(),
                    task_uuid,
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
                            serde_json::Value::String(format!("{:?}", task.status())),
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
                .resolve_active_workflow(&query.session_id)
                .await?
            {
                let wf_source = workflow_source(&workflow);

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
                    let step_status = workflow
                        .run
                        .step_states
                        .iter()
                        .find(|s| s.step_key == workflow.active_step.key)
                        .map(|s| format!("{:?}", s.status).to_ascii_lowercase());
                    let node_type = Some(match workflow.active_step.node_type {
                        agentdash_domain::workflow::LifecycleNodeType::AgentNode => {
                            "agent_node".to_string()
                        }
                        agentdash_domain::workflow::LifecycleNodeType::PhaseNode => {
                            "phase_node".to_string()
                        }
                    });
                    meta.active_workflow = Some(ActiveWorkflowMeta {
                        lifecycle_id: Some(workflow.lifecycle.id),
                        lifecycle_key: Some(workflow.lifecycle.key.clone()),
                        lifecycle_name: Some(workflow.lifecycle.name.clone()),
                        run_id: Some(workflow.run.id),
                        run_status: Some(workflow.run.status),
                        step_key: Some(workflow.active_step.key.clone()),
                        step_title: Some(step_title),
                        step_status,
                        node_type,
                        workflow_key: workflow.active_step.workflow_key.clone(),
                        transition_policy: Some(transition_policy.to_string()),
                        primary_workflow_id: workflow.primary_workflow.as_ref().map(|w| w.id),
                        primary_workflow_name: workflow
                            .primary_workflow
                            .as_ref()
                            .map(|w| w.name.clone()),
                        effective_contract: Some(build_effective_contract(
                            &workflow.lifecycle.key,
                            &workflow.active_step.key,
                            workflow.primary_workflow.as_ref(),
                        )),
                        output_port_keys: {
                            // port 归属已迁移到 step 级别
                            let port_keys: Vec<String> = workflow
                                .active_step
                                .output_ports
                                .iter()
                                .map(|p| p.key.clone())
                                .collect();
                            if port_keys.is_empty() {
                                None
                            } else {
                                Some(port_keys)
                            }
                        },
                        fulfilled_port_keys: {
                            let map = crate::workflow::load_port_output_map(
                                self.inline_file_repo.as_ref(),
                                workflow.run.id,
                            )
                            .await;
                            if map.is_empty() {
                                None
                            } else {
                                Some(map.into_keys().collect())
                            }
                        },
                        gate_collision_count: workflow
                            .run
                            .step_states
                            .iter()
                            .find(|s| s.step_key == workflow.active_step.key)
                            .map(|s| s.gate_collision_count),
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

            }

            snapshot.owners.push(owner);
        }

        if let Some(injection) = self
            .build_companion_agents_injection(&snapshot, &bindings)
            .await
        {
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
            HookTrigger::SessionStart => {
                resolution.injections = snapshot.injections.clone();
            }
            HookTrigger::UserPromptSubmit => {
                resolution.injections = filter_user_prompt_injections(&snapshot);
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
            }
            HookTrigger::SessionTerminal => {
                // owner_default_hook_rules 会为 task owner 自动注入
                // task_session_terminal preset；port 完成门禁由 port_output_gate
                // preset 在 BeforeStop 阶段驱动。
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                    &self.script_engine,
                );
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
            HookTrigger::BeforeCompact | HookTrigger::AfterCompact => {
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::BeforeProviderRequest => {
                apply_hook_rules(
                    HookEvaluationContext {
                        snapshot: &snapshot,
                        query: &query,
                    },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::CapabilityChanged => {
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
    use agentdash_spi::hooks::{
        HookEvaluationQuery, HookInjection, HookResolution, SessionHookSnapshot,
    };
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
    use super::{filter_user_prompt_injections, is_session_baseline_injection};

    #[test]
    fn session_baseline_slot_marks_companion_agents_only() {
        let companion = HookInjection {
            slot: "companion_agents".to_string(),
            content: "companions".to_string(),
            source: "builtin:companion_agents".to_string(),
        };
        let workflow = HookInjection {
            slot: "workflow".to_string(),
            content: "workflow-step".to_string(),
            source: "workflow:test".to_string(),
        };
        assert!(is_session_baseline_injection(&companion));
        assert!(!is_session_baseline_injection(&workflow));
    }

    #[test]
    fn user_prompt_injection_filter_excludes_companion_agents() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-filter".to_string(),
            injections: vec![
                HookInjection {
                    slot: "companion_agents".to_string(),
                    content: "## Companion Agents\n- agent".to_string(),
                    source: "builtin:companion_agents".to_string(),
                },
                HookInjection {
                    slot: "workflow".to_string(),
                    content: "当前 workflow step: implement".to_string(),
                    source: "workflow:example".to_string(),
                },
            ],
            ..SessionHookSnapshot::default()
        };

        let filtered = filter_user_prompt_injections(&snapshot);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].slot, "workflow");
    }

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
        let delegate = HookRuntimeDelegate::new_with_mount_root(
            hook_session.clone(),
            Some("/tmp/test-workspace".to_string()),
        );

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
