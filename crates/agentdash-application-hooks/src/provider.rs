use std::sync::Arc;

use agentdash_application_ports::agent_frame_hook_plan::AgentFrameHookRequirement;
use agentdash_application_ports::hook_workflow_projection::{
    HookActiveWorkflowFacts, HookExecutionLogAppendCommand, HookWorkflowProjection,
    HookWorkflowProjectionError, HookWorkflowProjectionPort, HookWorkflowProjectionQuery,
};
use agentdash_domain::workflow::{
    RuntimeNodeStatus, build_effective_contract, build_effective_contract_from_contract,
};
use agentdash_platform_spi::hooks::PendingExecutionLogEntry;
use agentdash_platform_spi::{
    ActiveWorkflowMeta, AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery,
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, HookDiagnosticEntry, HookError,
    HookResolution, HookScriptEvaluator, HookTrigger, SessionSnapshotMetadata,
};
use async_trait::async_trait;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_platform_spi::ExecutionHookProvider;

use super::active_workflow_contribution::build_active_workflow_step_fragments;
use super::presets::builtin_preset_scripts;
use super::rules::*;
use super::script_engine::HookScriptEngine;
use super::snapshot_helpers::*;
use super::{dedupe_tags, global_builtin_source, workflow_scope_key, workflow_source};
use crate::HookApplicationError;

/// Facade：组合 workflow projection port + HookScriptEngine，
/// 对外实现 ExecutionHookProvider trait。
pub struct AppExecutionHookProvider {
    pub(super) workflow_projection: Arc<dyn HookWorkflowProjectionPort>,
    pub(super) script_engine: HookScriptEngine,
}

pub struct AppExecutionHookProviderDeps {
    pub workflow_projection: Arc<dyn HookWorkflowProjectionPort>,
    pub script_evaluator: Arc<dyn HookScriptEvaluator>,
}

impl AppExecutionHookProvider {
    /// 构造 Facade。
    ///
    /// `script_evaluator` 由 composition root 提供，具体 Rhai 实现下沉 infrastructure。
    pub fn new(deps: AppExecutionHookProviderDeps) -> Self {
        Self {
            workflow_projection: deps.workflow_projection,
            script_engine: HookScriptEngine::new(deps.script_evaluator),
        }
    }

    /// 返回内建 preset 脚本，供 composition root 初始化具体 evaluator。
    pub fn builtin_preset_scripts() -> Vec<(&'static str, &'static str)> {
        builtin_preset_scripts()
    }

    /// 验证 Rhai 脚本语法是否合法，不执行脚本。
    pub fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.script_engine.validate_script(script)
    }

    /// 运行时注册/更新一个自定义 preset。
    pub fn register_preset(&self, key: &str, script: &str) -> Result<(), HookApplicationError> {
        self.script_engine.register_preset(key, script)
    }

    /// 移除一个自定义 preset。
    pub fn remove_preset(&self, key: &str) -> bool {
        self.script_engine.remove_preset(key)
    }

    /// Evaluates exactly the Product rule named by one admitted Complete Agent hook route.
    ///
    /// A Complete Agent invokes every bound definition independently. Reusing the aggregate
    /// trigger evaluator here would execute sibling rules once per callback and duplicate Product
    /// effects, so the immutable definition identity is the evaluation boundary.
    pub async fn evaluate_complete_agent_hook(
        &self,
        definition_id: &str,
        query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, HookError> {
        let snapshot = match query.snapshot.clone() {
            Some(snapshot) => snapshot,
            None => {
                self.load_product_hook_snapshot(AgentFrameHookSnapshotQuery {
                    target: query.target.clone(),
                    provenance: query.provenance.clone(),
                })
                .await?
            }
        };
        let query = HookRuleEvaluationQuery::from_frame_query(query);
        let mut resolution = HookResolution::default();
        apply_product_hook_rule(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            definition_id,
            &mut resolution,
            &self.script_engine,
        )
        .map_err(HookError::Runtime)?;
        Ok(resolution)
    }

    /// Evaluates one typed Product-owned event against the immutable requirements selected for
    /// the current AgentFrame.
    ///
    /// Tool Broker and Managed Runtime events are Product boundaries, not Agent callbacks. Their
    /// caller supplies the requirements pinned in the frame HookPlan; this owner resolves and
    /// executes only definitions whose exact Product trigger matches the event.
    pub async fn evaluate_product_hook_event(
        &self,
        requirements: &[AgentFrameHookRequirement],
        query: AgentFrameHookEvaluationQuery,
    ) -> Result<HookResolution, HookError> {
        let snapshot = match query.snapshot.clone() {
            Some(snapshot) => snapshot,
            None => {
                self.load_product_hook_snapshot(AgentFrameHookSnapshotQuery {
                    target: query.target.clone(),
                    provenance: query.provenance.clone(),
                })
                .await?
            }
        };
        let query = HookRuleEvaluationQuery::from_frame_query(query);
        let mut resolution = HookResolution::default();
        apply_product_hook_event_requirements(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            requirements,
            &mut resolution,
            &self.script_engine,
        )
        .map_err(HookError::Runtime)?;
        Ok(resolution)
    }

    pub(crate) async fn load_product_hook_snapshot(
        &self,
        query: AgentFrameHookSnapshotQuery,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        let projection = self
            .workflow_projection
            .load_hook_workflow_projection(HookWorkflowProjectionQuery {
                target: query.target,
                provenance: query.provenance.clone(),
            })
            .await
            .map_err(map_projection_error)?;
        self.build_snapshot_from_workflow(
            query.provenance.runtime_thread_id.unwrap_or_default(),
            query.provenance.turn_id,
            projection,
        )
        .await
    }

    async fn build_snapshot_from_workflow(
        &self,
        runtime_thread_id: String,
        turn_id: Option<String>,
        projection: HookWorkflowProjection,
    ) -> Result<AgentFrameHookSnapshot, HookError> {
        let mut snapshot = AgentFrameHookSnapshot {
            runtime_adapter_runtime_thread_id: runtime_thread_id,
            run_context: projection.run_context,
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
        ]);

        if let Some(HookActiveWorkflowFacts {
            projection: workflow,
            fulfilled_output_ports,
        }) = projection.active_workflow
        {
            let wf_source = workflow_source(&workflow);

            snapshot.diagnostics.push(HookDiagnosticEntry {
                code: "active_workflow_resolved".to_string(),
                message: format!(
                    "命中 active lifecycle step：{} / {}",
                    workflow.lifecycle_key, workflow.active_activity.key
                ),
            });

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
                        if fulfilled_output_ports.is_empty() {
                            None
                        } else {
                            Some(fulfilled_output_ports.into_keys().collect())
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
        self.load_product_hook_snapshot(query).await
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
        diag!(
            Debug,
            Subsystem::Hooks,
            session_id = ?query.provenance.runtime_thread_id,
            trigger = ?query.trigger,
            tool_name = ?query.tool_name,
            "hook: evaluate 触发"
        );
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
        let resolution = self.evaluate_rules(&snapshot, &query);
        if !resolution.matched_rule_keys.is_empty() || resolution.block_reason.is_some() {
            diag!(
                Info,
                Subsystem::Hooks,
                session_id = ?query.runtime_thread_id(),
                trigger = ?query.trigger,
                matched = resolution.matched_rule_keys.len(),
                blocked = resolution.block_reason.is_some(),
                "hook: 命中规则"
            );
        }
        Ok(resolution)
    }

    async fn append_execution_log(
        &self,
        entries: Vec<PendingExecutionLogEntry>,
    ) -> Result<(), HookError> {
        self.workflow_projection
            .append_execution_log(HookExecutionLogAppendCommand { entries })
            .await
            .map_err(map_projection_error)
    }
}

fn map_projection_error(error: HookWorkflowProjectionError) -> HookError {
    diag!(
        Warn,
        Subsystem::Hooks,
        error = %error,
        "hook: workflow projection 加载失败"
    );
    HookError::Runtime(error.to_string())
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
        if !trigger_includes_snapshot_injections(&query.trigger)
            && !has_applicable_hook_work(snapshot, query.trigger)
        {
            return resolution;
        }

        match query.trigger {
            HookTrigger::SessionStart => {}
            HookTrigger::UserPromptSubmit => {
                // PR 4（04-30-session-pipeline-architecture-refactor）：静态
                // `snapshot.injections`（workflow / constraint 等"预装"条目）
                // 由 prompt_pipeline 在启动阶段合并进 assignment fragments，
                // 再由 assignment_context ContextFrame 投递。
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
                // SessionTerminal rules evaluate declared hook effects. Port 完成门禁由
                // port_output_gate preset 在 BeforeStop 阶段驱动。
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
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use agentdash_application_ports::agent_frame_hook_plan::{
        AgentFrameHookRequirement, HookAction, HookDefinitionId, HookExecutionSite,
        HookFailurePolicy, HookPoint, HookRequirement, SemanticStrength,
    };
    use agentdash_application_ports::hook_workflow_projection::{
        HookExecutionLogAppendCommand, HookWorkflowProjection, HookWorkflowProjectionError,
        HookWorkflowProjectionPort, HookWorkflowProjectionQuery,
    };
    use agentdash_domain::workflow::{
        EffectiveSessionContract, WorkflowHookRuleSpec, WorkflowHookTrigger,
    };
    use agentdash_platform_spi::hooks::{
        AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery, AgentFrameHookSnapshot,
        AgentFrameHookSnapshotQuery, HookControlTarget, HookResolution, RuntimeAdapterProvenance,
    };
    use agentdash_platform_spi::{
        ActiveWorkflowMeta, ExecutionHookProvider, HookError, HookTrigger,
    };
    use async_trait::async_trait;

    use super::super::rules::{HookEvaluationContext, HookRuleEvaluationQuery, apply_hook_rules};
    use super::super::script_engine::HookScriptEngine;
    use super::super::test_fixtures::snapshot_with_workflow;
    use super::super::test_script_evaluator::TestHookScriptEvaluator;
    use super::{AppExecutionHookProvider, AppExecutionHookProviderDeps};

    struct UnusedProjection;

    #[async_trait]
    impl HookWorkflowProjectionPort for UnusedProjection {
        async fn load_hook_workflow_projection(
            &self,
            _query: HookWorkflowProjectionQuery,
        ) -> Result<HookWorkflowProjection, HookWorkflowProjectionError> {
            panic!("typed Product hook test supplies its immutable snapshot")
        }

        async fn append_execution_log(
            &self,
            _command: HookExecutionLogAppendCommand,
        ) -> Result<(), HookWorkflowProjectionError> {
            panic!("typed Product hook test does not append aggregate execution logs")
        }
    }

    fn tool_broker_requirement(key: &str) -> AgentFrameHookRequirement {
        AgentFrameHookRequirement {
            definition_id: HookDefinitionId::new(format!("workflow-hook:{key}")).unwrap(),
            requirement: HookRequirement {
                point: HookPoint::AfterTool,
                actions: BTreeSet::from([
                    HookAction::Observe,
                    HookAction::AddContext,
                    HookAction::ContinueTurn,
                    HookAction::EmitEffect,
                ]),
                minimum_strength: SemanticStrength::ExactDurableBoundary,
                failure_policy: HookFailurePolicy::FailOpenWithDiagnostic,
                required: true,
            },
            site: HookExecutionSite::ToolBroker,
        }
    }

    #[test]
    fn session_start_includes_snapshot_injections() {
        let injection = agentdash_platform_spi::HookInjection {
            slot: "workflow".to_string(),
            content: "## Workflow Guidance\n进入 Apply 阶段".to_string(),
            source: "workflow:builtin_workflow_admin_apply:apply".to_string(),
        };
        let snapshot = AgentFrameHookSnapshot {
            runtime_adapter_runtime_thread_id: "session-1".to_string(),
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
            Self {
                snapshot,
                engine: HookScriptEngine::new(Arc::new(TestHookScriptEvaluator::new(&[]))),
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
    async fn before_tool_rewrite_records_resolution() {
        let snapshot = snapshot_with_workflow("implement", "session_ended");
        let provider = RuleEngineTestProvider::new(snapshot.clone());

        let resolution = provider
            .evaluate_frame_hook(AgentFrameHookEvaluationQuery {
                target: HookControlTarget {
                    run_id: uuid::Uuid::new_v4(),
                    agent_id: uuid::Uuid::new_v4(),
                    frame_id: uuid::Uuid::new_v4(),
                },
                provenance: RuntimeAdapterProvenance::runtime_thread(
                    snapshot.runtime_adapter_runtime_thread_id.clone(),
                    None,
                    "provider_test",
                ),
                trigger: HookTrigger::BeforeTool,
                tool_name: Some("shell_exec".to_string()),
                tool_call_id: Some("call-shell-1".to_string()),
                subagent_type: None,
                snapshot: Some(snapshot),
                payload: Some(serde_json::json!({
                    "default_mount_root_ref": "/tmp/test-workspace",
                    "args": {
                        "cwd": "/tmp/test-workspace/crates/agentdash-agent",
                        "command": "cargo test"
                    }
                })),
                token_stats: None,
            })
            .await
            .expect("before_tool 应返回 rewrite resolution");

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
    }

    #[tokio::test]
    async fn typed_product_event_executes_only_the_pinned_exact_trigger() {
        let snapshot = AgentFrameHookSnapshot {
            runtime_adapter_runtime_thread_id: "runtime-thread-parent".to_owned(),
            metadata: Some(agentdash_platform_spi::SessionSnapshotMetadata {
                active_workflow: Some(ActiveWorkflowMeta {
                    effective_contract: Some(EffectiveSessionContract {
                        hook_rules: vec![
                            WorkflowHookRuleSpec {
                                key: "after_dispatch".to_owned(),
                                trigger: WorkflowHookTrigger::AfterSubagentDispatch,
                                description: "after dispatch".to_owned(),
                                preset: None,
                                params: None,
                                script: Some(
                                    "make_injection(\"constraint\", \"请先完成 lint\", \"test:src\")"
                                        .to_owned(),
                                ),
                                enabled: true,
                            },
                            WorkflowHookRuleSpec {
                                key: "companion_result".to_owned(),
                                trigger: WorkflowHookTrigger::CompanionResult,
                                description: "companion result".to_owned(),
                                preset: None,
                                params: None,
                                script: Some(
                                    "inject(\"workflow\", \"content\", \"src\")".to_owned(),
                                ),
                                enabled: true,
                            },
                        ],
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..AgentFrameHookSnapshot::default()
        };
        let provider = AppExecutionHookProvider::new(AppExecutionHookProviderDeps {
            workflow_projection: Arc::new(UnusedProjection),
            script_evaluator: Arc::new(TestHookScriptEvaluator::new(&[])),
        });
        let requirements = vec![
            tool_broker_requirement("after_dispatch"),
            tool_broker_requirement("companion_result"),
        ];

        let resolution = provider
            .evaluate_product_hook_event(
                &requirements,
                AgentFrameHookEvaluationQuery {
                    target: HookControlTarget {
                        run_id: uuid::Uuid::new_v4(),
                        agent_id: uuid::Uuid::new_v4(),
                        frame_id: uuid::Uuid::new_v4(),
                    },
                    provenance: RuntimeAdapterProvenance::runtime_thread(
                        "runtime-thread-parent",
                        Some("turn-parent".to_owned()),
                        "companion_after_dispatch",
                    ),
                    trigger: HookTrigger::AfterSubagentDispatch,
                    tool_name: None,
                    tool_call_id: None,
                    subagent_type: Some("reviewer".to_owned()),
                    snapshot: Some(snapshot),
                    payload: Some(serde_json::json!({"effect_id": "effect-1"})),
                    token_stats: None,
                },
            )
            .await
            .expect("typed Product event");

        assert_eq!(resolution.injections.len(), 1);
        assert_eq!(resolution.injections[0].slot, "constraint");
        assert!(
            resolution
                .matched_rule_keys
                .iter()
                .any(|key| key.contains("after_dispatch"))
        );
        assert!(
            resolution
                .matched_rule_keys
                .iter()
                .all(|key| !key.contains("companion_result"))
        );
    }
}
