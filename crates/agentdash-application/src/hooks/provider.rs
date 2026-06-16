use std::sync::Arc;

use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentProcedureRepository, LifecycleAgentRepository,
    LifecycleRunRepository, LifecycleSubjectAssociationRepository, RuntimeNodeStatus,
    RuntimeSessionExecutionAnchorRepository, build_effective_contract,
    build_effective_contract_from_contract,
};
use agentdash_spi::hooks::PendingExecutionLogEntry;
use agentdash_spi::{
    ActiveWorkflowMeta, AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery,
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, HookDiagnosticEntry, HookError,
    HookResolution, HookScriptEvaluator, HookTrigger, SessionSnapshotMetadata,
};
use async_trait::async_trait;

use agentdash_spi::ExecutionHookProvider;

use super::active_workflow_contribution::build_active_workflow_step_fragments;
use super::active_workflow_snapshot::ActiveWorkflowSnapshotBuilder;
use super::owner_resolver::SessionOwnerResolver;
use super::presets::builtin_preset_scripts;
use super::rules::*;
use super::script_engine::HookScriptEngine;
use super::snapshot_helpers::*;
use super::{dedupe_tags, global_builtin_source, workflow_scope_key, workflow_source};
use crate::ApplicationError;

/// Facade：组合 SessionOwnerResolver + ActiveWorkflowSnapshotBuilder + HookScriptEngine，
/// 对外仍实现 ExecutionHookProvider trait。
pub struct AppExecutionHookProvider {
    pub(super) inline_file_repo: Arc<dyn InlineFileRepository>,
    pub(super) owner_resolver: SessionOwnerResolver,
    pub(super) workflow_builder: ActiveWorkflowSnapshotBuilder,
    pub(super) script_engine: HookScriptEngine,
}

pub struct AppExecutionHookProviderRepos {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    pub agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub inline_file_repo: Arc<dyn InlineFileRepository>,
}

impl AppExecutionHookProvider {
    /// 构造 Facade。
    ///
    /// `script_evaluator_factory` 由 composition root 提供，接收内建 preset
    /// 脚本（key → 源码）并返回具体脚本引擎实现（rhai 实现下沉 infrastructure）。
    pub fn new<F>(repos: AppExecutionHookProviderRepos, script_evaluator_factory: F) -> Self
    where
        F: FnOnce(&[(&str, &str)]) -> Arc<dyn HookScriptEvaluator>,
    {
        let preset_scripts = builtin_preset_scripts();
        let evaluator = script_evaluator_factory(&preset_scripts);
        let lifecycle_run_repo = repos.lifecycle_run_repo.clone();
        Self {
            inline_file_repo: repos.inline_file_repo,
            owner_resolver: SessionOwnerResolver::new(
                repos.project_repo,
                repos.story_repo,
                lifecycle_run_repo,
                repos.lifecycle_subject_association_repo,
            ),
            workflow_builder: ActiveWorkflowSnapshotBuilder::new(
                repos.agent_procedure_repo,
                repos.agent_frame_repo,
                repos.lifecycle_agent_repo,
                repos.lifecycle_run_repo,
                repos.execution_anchor_repo,
            ),
            script_engine: HookScriptEngine::new(evaluator),
        }
    }

    /// 验证 Rhai 脚本语法是否合法，不执行脚本。
    pub fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.script_engine.validate_script(script)
    }

    /// 运行时注册/更新一个自定义 preset。
    pub fn register_preset(&self, key: &str, script: &str) -> Result<(), ApplicationError> {
        self.script_engine.register_preset(key, script)
    }

    /// 移除一个自定义 preset。
    pub fn remove_preset(&self, key: &str) -> bool {
        self.script_engine.remove_preset(key)
    }

    async fn build_snapshot_from_workflow(
        &self,
        runtime_session_id: String,
        turn_id: Option<String>,
        workflow: Option<crate::lifecycle::ActiveWorkflowProjection>,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        let mut snapshot = AgentFrameHookSnapshot {
            runtime_adapter_session_id: runtime_session_id,
            run_context: None,
            sources: Vec::new(),
            tags: Vec::new(),
            injections: Vec::new(),
            diagnostics: Vec::new(),
            metadata: Some(SessionSnapshotMetadata {
                turn_id,
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

        if let Some(workflow) = workflow {
            let wf_source = workflow_source(&workflow);

            snapshot.diagnostics.push(HookDiagnosticEntry {
                code: "active_workflow_resolved".to_string(),
                message: format!(
                    "命中 active lifecycle step：{} / {}",
                    workflow.lifecycle_key, workflow.active_activity.key
                ),
            });

            snapshot.run_context = Some(
                self.owner_resolver
                    .resolve_run_context(&workflow.run)
                    .await
                    .map_err(|err| HookError::Runtime(err.to_string()))?,
            );
            snapshot
                .tags
                .push(format!("project:{}", workflow.run.project_id));

            if let Some(meta) = snapshot.metadata.as_mut() {
                let transition_policy = workflow.advance_label();
                let step_title = if workflow.active_activity.description.trim().is_empty() {
                    workflow.active_activity.key.clone()
                } else {
                    workflow.active_activity.description.clone()
                };
                let activity_status = Some(
                    match workflow.active_attempt.status {
                        RuntimeNodeStatus::Pending => "pending",
                        RuntimeNodeStatus::Ready | RuntimeNodeStatus::Claiming => "ready",
                        RuntimeNodeStatus::Running | RuntimeNodeStatus::Blocked => "running",
                        RuntimeNodeStatus::Completed => "completed",
                        RuntimeNodeStatus::Failed | RuntimeNodeStatus::Cancelled => "failed",
                        RuntimeNodeStatus::Skipped => "skipped",
                    }
                    .to_string(),
                );
                let node_type = Some(match workflow.active_node_type {
                    agentdash_domain::workflow::LifecycleNodeType::AgentNode => {
                        "agent_node".to_string()
                    }
                    agentdash_domain::workflow::LifecycleNodeType::PhaseNode => {
                        "phase_node".to_string()
                    }
                });
                meta.active_workflow = Some(ActiveWorkflowMeta {
                    workflow_graph_id: workflow.lifecycle_graph_id,
                    lifecycle_key: Some(workflow.lifecycle_key.clone()),
                    lifecycle_name: Some(workflow.lifecycle_name.clone()),
                    run_id: Some(workflow.run.id),
                    run_status: Some(workflow.run.status),
                    activity_key: Some(workflow.active_activity.key.clone()),
                    activity_title: Some(step_title),
                    activity_status,
                    node_type,
                    procedure_key: workflow.active_procedure_key.clone(),
                    transition_policy: Some(transition_policy.to_string()),
                    primary_workflow_id: workflow.primary_workflow.as_ref().map(|w| w.id),
                    primary_workflow_name: workflow
                        .primary_workflow
                        .as_ref()
                        .map(|w| w.name.clone()),
                    effective_contract: Some(match workflow.active_contract() {
                        Some(contract) => build_effective_contract_from_contract(
                            &workflow.lifecycle_key,
                            &workflow.active_activity.key,
                            contract,
                        ),
                        None => build_effective_contract(
                            &workflow.lifecycle_key,
                            &workflow.active_activity.key,
                            None,
                        ),
                    }),
                    output_port_keys: {
                        let port_keys: Vec<String> = workflow
                            .active_activity
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
                        let artifact_scope =
                            crate::lifecycle::execution_log::RuntimeNodeArtifactScope {
                                run_id: workflow.run.id,
                                orchestration_id: workflow.orchestration_id,
                                node_path: workflow.node_path.clone(),
                                attempt: workflow.active_attempt.attempt,
                            };
                        let map = crate::lifecycle::load_scoped_port_output_map(
                            self.inline_file_repo.as_ref(),
                            &artifact_scope,
                        )
                        .await;
                        if map.is_empty() {
                            None
                        } else {
                            Some(map.into_keys().collect())
                        }
                    },
                    gate_collision_count: None,
                });
            }

            // Add workflow source
            snapshot.sources.push(wf_source.clone());

            // Add workflow tags
            snapshot.tags.extend([
                format!("workflow:{}", workflow_scope_key(&workflow)),
                format!("workflow_step:{}", workflow.active_activity.key),
                format!(
                    "workflow_status:{}",
                    workflow_run_status_tag(workflow.run.status)
                ),
            ]);

            // Add workflow step injections
            snapshot
                .injections
                .extend(build_active_workflow_step_fragments(&workflow, &wf_source));
        }

        snapshot.tags = dedupe_tags(snapshot.tags);
        Ok(snapshot)
    }
}

#[async_trait]
impl ExecutionHookProvider for AppExecutionHookProvider {
    async fn load_frame_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        let workflow = self
            .workflow_builder
            .resolve_active_workflow_for_target(&query.target)
            .await?;
        self.build_snapshot_from_workflow(
            query.provenance.runtime_session_id.unwrap_or_default(),
            query.provenance.turn_id,
            workflow,
        )
        .await
    }

    async fn refresh_frame_snapshot(
        &self,
        query: AgentFrameHookRefreshQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        self.load_frame_snapshot(AgentFrameHookSnapshotQuery {
            target: query.target,
            provenance: query.provenance,
        })
        .await
    }

    async fn evaluate_frame_hook(
        &self,
        query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, HookError> {
        let snapshot = match query.snapshot.clone() {
            Some(snapshot) => snapshot,
            None => {
                self.load_frame_snapshot(AgentFrameHookSnapshotQuery {
                    target: query.target.clone(),
                    provenance: query.provenance.clone(),
                })
                .await?
            }
        };
        let query = HookRuleEvaluationQuery::from_frame_query(query);
        Ok(self.evaluate_rules(&snapshot, &query))
    }

    async fn append_execution_log(
        &self,
        entries: Vec<PendingExecutionLogEntry>,
    ) -> Result<(), HookError> {
        self.workflow_builder.append_execution_log(entries).await
    }
}

impl AppExecutionHookProvider {
    fn evaluate_rules(
        &self,
        snapshot: &AgentFrameHookSnapshot,
        query: &HookRuleEvaluationQuery,
    ) -> HookResolution {
        let mut resolution = HookResolution {
            diagnostics: snapshot
                .diagnostics
                .iter()
                .filter(|entry| matches!(entry.code.as_str(), "active_workflow_resolved"))
                .cloned()
                .collect(),
            ..HookResolution::default()
        };

        seed_snapshot_injections_for_trigger(&query.trigger, snapshot, &mut resolution);

        match query.trigger {
            HookTrigger::SessionStart => {}
            HookTrigger::UserPromptSubmit => {
                // PR 4（04-30-session-pipeline-architecture-refactor）：静态
                // `snapshot.injections`（companion_agents / workflow / constraint
                // 等"预装"条目）由 prompt_pipeline 在启动阶段合并进 Bundle 的
                // `bootstrap_fragments`，再由 bootstrap_context ContextFrame 投递。
                // UserPromptSubmit 不重新渲染这些静态条目，避免 Agent 可见上下文双写。
                // 此分支只应用动态 hook 规则（如 rhai 规则产出的 per-turn
                // dynamic 注入）。
                apply_hook_rules(
                    HookEvaluationContext { snapshot, query },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::BeforeTool | HookTrigger::AfterTool | HookTrigger::AfterTurn => {
                apply_hook_rules(
                    HookEvaluationContext { snapshot, query },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::BeforeStop => {
                apply_hook_rules(
                    HookEvaluationContext { snapshot, query },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::SessionTerminal => {
                // owner_default_hook_rules 会为 task owner 自动注入
                // task_session_terminal preset；port 完成门禁由 port_output_gate
                // preset 在 BeforeStop 阶段驱动。
                apply_hook_rules(
                    HookEvaluationContext { snapshot, query },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::BeforeSubagentDispatch
            | HookTrigger::AfterSubagentDispatch
            | HookTrigger::CompanionResult => {
                apply_hook_rules(
                    HookEvaluationContext { snapshot, query },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::BeforeCompact | HookTrigger::AfterCompact => {
                apply_hook_rules(
                    HookEvaluationContext { snapshot, query },
                    &mut resolution,
                    &self.script_engine,
                );
            }
            HookTrigger::BeforeProviderRequest => {
                apply_hook_rules(
                    HookEvaluationContext { snapshot, query },
                    &mut resolution,
                    &self.script_engine,
                );
            }
        }

        resolution
    }
}

fn trigger_includes_snapshot_injections(trigger: &HookTrigger) -> bool {
    matches!(trigger, HookTrigger::SessionStart)
}

fn seed_snapshot_injections_for_trigger(
    trigger: &HookTrigger,
    snapshot: &AgentFrameHookSnapshot,
    resolution: &mut HookResolution,
) {
    if trigger_includes_snapshot_injections(trigger) {
        resolution.injections = snapshot.injections.clone();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::agent_run::frame::hook_runtime::AgentFrameHookRuntime;
    use crate::session::HookRuntimeDelegate;
    use agentdash_spi::hooks::HookRuntimeAccess;
    use agentdash_spi::hooks::{
        AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery, AgentFrameHookSnapshot,
        AgentFrameHookSnapshotQuery, HookResolution,
    };
    use agentdash_spi::{
        AgentContext, AgentMessage, BeforeToolCallInput, ToolCallDecision, ToolCallInfo,
    };
    use agentdash_spi::{ExecutionHookProvider, HookError, HookTraceTrigger, HookTrigger};
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use agentdash_infrastructure::RhaiHookScriptEvaluator;

    use super::super::presets::builtin_preset_scripts;
    use super::super::rules::{HookEvaluationContext, HookRuleEvaluationQuery, apply_hook_rules};
    use super::super::script_engine::HookScriptEngine;
    use super::super::test_fixtures::snapshot_with_workflow;

    #[test]
    fn session_start_includes_snapshot_injections() {
        let injection = agentdash_spi::HookInjection {
            slot: "workflow".to_string(),
            content: "## Workflow Guidance\n进入 Apply 阶段".to_string(),
            source: "workflow:builtin_workflow_admin_apply:apply".to_string(),
        };
        let snapshot = AgentFrameHookSnapshot {
            runtime_adapter_session_id: "session-1".to_string(),
            injections: vec![injection.clone()],
            ..AgentFrameHookSnapshot::default()
        };
        let mut resolution = HookResolution::default();

        super::seed_snapshot_injections_for_trigger(
            &HookTrigger::SessionStart,
            &snapshot,
            &mut resolution,
        );

        assert_eq!(resolution.injections, vec![injection]);
        assert!(super::trigger_includes_snapshot_injections(
            &HookTrigger::SessionStart
        ));
        assert!(!super::trigger_includes_snapshot_injections(
            &HookTrigger::UserPromptSubmit
        ));
    }

    struct RuleEngineTestProvider {
        snapshot: AgentFrameHookSnapshot,
        engine: HookScriptEngine,
    }

    impl RuleEngineTestProvider {
        fn new(snapshot: AgentFrameHookSnapshot) -> Self {
            let scripts = builtin_preset_scripts();
            Self {
                snapshot,
                engine: HookScriptEngine::new(Arc::new(RhaiHookScriptEvaluator::new(&scripts))),
            }
        }
    }

    #[async_trait]
    impl ExecutionHookProvider for RuleEngineTestProvider {
        async fn load_frame_snapshot(
            &self,
            _query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(self.snapshot.clone())
        }

        async fn refresh_frame_snapshot(
            &self,
            _query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(self.snapshot.clone())
        }

        async fn evaluate_frame_hook(
            &self,
            query: AgentFrameHookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            let snapshot = query
                .snapshot
                .clone()
                .unwrap_or_else(|| self.snapshot.clone());
            let query = HookRuleEvaluationQuery::from_frame_query(query);
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
        let hook_runtime = Arc::new(AgentFrameHookRuntime::new_test_runtime(
            snapshot.runtime_adapter_session_id.clone(),
            Arc::new(RuleEngineTestProvider::new(snapshot.clone())),
            snapshot,
        ));
        let delegate = HookRuntimeDelegate::new_with_mount_root(
            hook_runtime.clone(),
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
                        message_refs: vec![],
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

        let runtime: agentdash_spi::hooks::AgentFrameRuntimeSnapshot =
            hook_runtime.runtime_snapshot();
        assert_eq!(runtime.trace.len(), 1);
        assert_eq!(runtime.trace[0].trigger, HookTraceTrigger::BeforeTool);
        assert_eq!(runtime.trace[0].decision, "rewrite");
        assert!(
            runtime.trace[0]
                .matched_rule_keys
                .contains(&"tool:shell_exec:rewrite_absolute_cwd".to_string())
        );
    }
}
