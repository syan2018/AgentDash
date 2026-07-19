use std::sync::Arc;

use agentdash_platform_spi::hooks::HookContextPresentationFacts;
use agentdash_platform_spi::{
    HookApprovalRequest, HookCompactionDecision, HookCompletionStatus, HookDiagnosticEntry,
    HookEffect, HookInjection, HookScriptEvaluator,
};
use serde::Deserialize;

use crate::HookApplicationError;

use super::snapshot_helpers::*;

use super::rules::HookEvaluationContext;

// ── 脚本返回值（中间态，merge 进 HookResolution 前的结构） ──

pub(crate) struct ScriptDecision {
    pub block: Option<String>,
    pub inject: Vec<HookInjection>,
    pub approval: Option<HookApprovalRequest>,
    pub completion: Option<HookCompletionStatus>,
    pub refresh: bool,
    pub rewrite_input: Option<serde_json::Value>,
    pub diagnostics: Vec<HookDiagnosticEntry>,
    pub effects: Vec<HookEffect>,
    pub compaction: Option<HookCompactionDecision>,
}

/// Script effects stay extensible at the domain payload boundary, while model-visible context
/// must cross the Hook provider boundary as typed semantic facts. Runtime owns frame identity,
/// coordinates, delivery metadata derivation and the atomic journal commit.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ScriptHookEffect {
    kind: String,
    #[serde(default)]
    payload: serde_json::Value,
    #[serde(default)]
    presentation: Option<HookContextPresentationFacts>,
}

impl ScriptDecision {
    pub fn is_empty(&self) -> bool {
        self.block.is_none()
            && self.inject.is_empty()
            && self.approval.is_none()
            && self.completion.is_none()
            && !self.refresh
            && self.rewrite_input.is_none()
            && self.diagnostics.is_empty()
            && self.effects.is_empty()
            && self.compaction.is_none()
    }
}

// ── 脚本引擎 facade ──
//
// 应用层只负责把 `HookEvaluationContext` 折叠为上下文 JSON、调用注入的
// [`HookScriptEvaluator`] port，再把原始决策 JSON 解析回 [`ScriptDecision`]。
// 具体脚本引擎（rhai）实现下沉至 infrastructure。

pub(crate) struct HookScriptEngine {
    evaluator: Arc<dyn HookScriptEvaluator>,
}

impl HookScriptEngine {
    pub fn new(evaluator: Arc<dyn HookScriptEvaluator>) -> Self {
        Self { evaluator }
    }

    /// 运行时注册/更新自定义 preset 脚本。
    pub fn register_preset(&self, key: &str, script: &str) -> Result<(), HookApplicationError> {
        self.evaluator
            .register_preset(key, script)
            .map_err(HookApplicationError::InvalidConfig)
    }

    /// 移除一个 preset（仅 UserDefined 类型应调用此接口）。
    pub fn remove_preset(&self, key: &str) -> bool {
        self.evaluator.remove_preset(key)
    }

    /// 执行 preset 脚本
    pub fn eval_preset(
        &self,
        preset_key: &str,
        ctx: &HookEvaluationContext<'_>,
        params: Option<&serde_json::Value>,
    ) -> Result<ScriptDecision, HookApplicationError> {
        let ctx_json = Self::build_ctx_value(ctx, params);
        let raw = self
            .evaluator
            .eval_preset(preset_key, &ctx_json)
            .map_err(HookApplicationError::Internal)?;
        Self::parse_decision(&raw)
    }

    /// 执行用户自定义脚本
    pub fn eval_script(
        &self,
        script: &str,
        ctx: &HookEvaluationContext<'_>,
        params: Option<&serde_json::Value>,
    ) -> Result<ScriptDecision, HookApplicationError> {
        let ctx_json = Self::build_ctx_value(ctx, params);
        let raw = self
            .evaluator
            .eval_script(script, &ctx_json)
            .map_err(HookApplicationError::InvalidConfig)?;
        Self::parse_decision(&raw)
    }

    /// 仅编译，不执行——用于验证 API (R11)
    pub fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.evaluator.validate_script(script)
    }

    // ── 内部方法 ──

    fn build_ctx_value(
        ctx: &HookEvaluationContext<'_>,
        params: Option<&serde_json::Value>,
    ) -> serde_json::Value {
        let aw = ctx
            .snapshot
            .metadata
            .as_ref()
            .and_then(|m| m.active_workflow.as_ref());
        let contract = aw.and_then(|a| a.effective_contract.as_ref());

        let tool_failed = super::helpers::tool_call_failed(ctx.query.payload.as_ref());

        let trigger_str = ctx.query.trigger.as_key();

        let wf_source = active_workflow_source_from_snapshot(ctx.snapshot);

        serde_json::json!({
            "trigger": trigger_str,
            "tool_name": ctx.query.tool_name,
            "tool_call_id": ctx.query.tool_call_id,
            "subagent_type": ctx.query.subagent_type,
            "turn_id": ctx.query.turn_id(),
            "run_id": ctx.query.target.as_ref().map(|t| t.run_id.to_string()),
            "agent_id": ctx.query.target.as_ref().map(|t| t.agent_id.to_string()),
            "hook_target": ctx.query.target.as_ref(),
            "provenance": ctx.query.provenance,
            "payload": ctx.query.payload,

            "snapshot": {
                "run_context": ctx.snapshot.run_context,
                "tags": ctx.snapshot.tags,
                "injections": ctx.snapshot.injections,
            },

            "workflow": {
                "lifecycle_key": aw.and_then(|a| a.lifecycle_key.as_deref()),
                "activity_key": aw.and_then(|a| a.activity_key.as_deref()),
                "activity_status": aw.and_then(|a| a.activity_status.as_deref()),
                "node_type": aw.and_then(|a| a.node_type.as_deref()),
                "procedure_key": aw.and_then(|a| a.procedure_key.as_deref()),
                "transition_policy": aw.and_then(|a| a.transition_policy.as_deref()),
                "run_status": aw.and_then(|a| a.run_status.as_ref().map(|s| format!("{s:?}").to_ascii_lowercase())),
                "run_id": aw.and_then(|a| a.run_id.map(|id| id.to_string())),
                "source": wf_source,
                "output_port_keys": aw.and_then(|a| a.output_port_keys.as_ref()),
                "fulfilled_port_keys": aw.and_then(|a| a.fulfilled_port_keys.as_ref()),
                "gate_collision_count": aw.and_then(|a| a.gate_collision_count),
            },

            "contract": {
                "hook_rules": contract.map(|c| &c.hook_rules),
            },

            "meta": {
                "working_directory": ctx.snapshot.metadata.as_ref().and_then(|m| m.working_directory.as_deref()),
                "connector_id": ctx.snapshot.metadata.as_ref().and_then(|m| m.connector_id.as_deref()),
                "executor": ctx.snapshot.metadata.as_ref().and_then(|m| m.executor.as_deref()),
                "task_status": ctx.snapshot.metadata.as_ref().and_then(|m| m.extra.get("task_status")),
                "task_id": ctx.snapshot.metadata.as_ref().and_then(|m| m.extra.get("task_id")),
            },

            "token_stats": ctx.query.token_stats.as_ref().map(|ts| serde_json::json!({
                "last_input_tokens": ts.last_input_tokens,
                "current_context_tokens": ts.current_context_tokens,
                "pending_estimate_tokens": ts.pending_estimate_tokens,
                "context_window": ts.context_window,
                "effective_context_window": ts.effective_context_window,
                "reserve_tokens": ts.reserve_tokens,
            })).unwrap_or(serde_json::Value::Null),

            "params": params.unwrap_or(&serde_json::Value::Null),

            "signals": {
                "tool_call_failed": tool_failed,
            },
        })
    }

    fn parse_decision(result: &serde_json::Value) -> Result<ScriptDecision, HookApplicationError> {
        if result.is_null() {
            return Ok(empty_decision());
        }

        let obj = match result.as_object() {
            Some(obj) if obj.is_empty() => return Ok(empty_decision()),
            Some(obj) => obj,
            None => return Ok(empty_decision()),
        };

        let block = obj
            .get("block")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);

        let inject = obj
            .get("inject")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let o = item.as_object()?;
                        Some(HookInjection {
                            slot: o.get("slot")?.as_str()?.to_string(),
                            content: o.get("content")?.as_str()?.to_string(),
                            source: o
                                .get("source")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let approval = obj.get("approval").and_then(|v| {
            let o = v.as_object()?;
            Some(HookApprovalRequest {
                reason: o.get("reason")?.as_str()?.to_string(),
                details: o.get("details").cloned(),
            })
        });

        let completion = obj.get("completion").and_then(|v| {
            let o = v.as_object()?;
            Some(HookCompletionStatus {
                mode: o
                    .get("mode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                satisfied: o
                    .get("satisfied")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                advanced: false,
                reason: o
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        });

        let refresh = obj
            .get("refresh")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let rewrite_input = obj.get("rewrite_input").cloned();

        let diagnostics = obj
            .get("diagnostics")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let o = item.as_object()?;
                        Some(HookDiagnosticEntry {
                            code: o.get("code")?.as_str()?.to_string(),
                            message: o.get("message")?.as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let effects = match obj.get("effects") {
            None | Some(serde_json::Value::Null) => Vec::new(),
            Some(value) => serde_json::from_value::<Vec<ScriptHookEffect>>(value.clone())
                .map_err(|error| {
                    HookApplicationError::InvalidConfig(format!(
                        "effects must use the typed Hook effect schema: {error}"
                    ))
                })?
                .into_iter()
                .map(|effect| HookEffect {
                    kind: effect.kind,
                    payload: effect.payload,
                    presentation: effect.presentation,
                })
                .collect(),
        };

        let compaction = obj
            .get("compaction")
            .and_then(|v| serde_json::from_value::<HookCompactionDecision>(v.clone()).ok());

        Ok(ScriptDecision {
            block,
            inject,
            approval,
            completion,
            refresh,
            rewrite_input,
            diagnostics,
            effects,
            compaction,
        })
    }
}

fn empty_decision() -> ScriptDecision {
    ScriptDecision {
        block: None,
        inject: Vec::new(),
        approval: None,
        completion: None,
        refresh: false,
        rewrite_input: None,
        diagnostics: Vec::new(),
        effects: Vec::new(),
        compaction: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::HookRuleEvaluationQuery;
    use crate::test_script_evaluator::TestHookScriptEvaluator;
    use agentdash_platform_spi::{
        AgentFrameHookSnapshot, HookControlTarget, HookEvaluationQuery, HookTrigger,
        RuntimeAdapterProvenance,
    };
    use uuid::Uuid;

    fn test_engine() -> HookScriptEngine {
        HookScriptEngine::new(Arc::new(TestHookScriptEvaluator::new(&[])))
    }

    fn base_ctx() -> (AgentFrameHookSnapshot, HookRuleEvaluationQuery) {
        let snapshot = AgentFrameHookSnapshot {
            runtime_adapter_runtime_thread_id: "sess-test".to_string(),
            ..AgentFrameHookSnapshot::default()
        };
        let query = HookRuleEvaluationQuery::from_session_query(HookEvaluationQuery {
            session_id: "sess-test".to_string(),
            trigger: HookTrigger::BeforeTool,
            turn_id: None,
            tool_name: Some("shell_exec".to_string()),
            tool_call_id: Some("call-1".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: None,
            token_stats: None,
        });
        (snapshot, query)
    }

    #[test]
    fn empty_script_returns_empty_decision() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine.eval_script("#{}", &ctx, None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn context_carries_frame_target_and_runtime_provenance() {
        let snapshot = AgentFrameHookSnapshot {
            runtime_adapter_runtime_thread_id: "sess-frame".to_string(),
            ..AgentFrameHookSnapshot::default()
        };
        let target = HookControlTarget {
            run_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            agent_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            frame_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
        };
        let query = HookRuleEvaluationQuery {
            target: Some(target),
            provenance: RuntimeAdapterProvenance::runtime_thread(
                "sess-frame",
                Some("turn-frame".to_string()),
                "frame_evaluation_test",
            ),
            trigger: HookTrigger::BeforeTool,
            tool_name: Some("shell_exec".to_string()),
            tool_call_id: None,
            subagent_type: None,
            payload: None,
            token_stats: None,
        };
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };

        let ctx_value = HookScriptEngine::build_ctx_value(&ctx, None);

        assert_eq!(
            ctx_value["hook_target"]["frame_id"],
            "33333333-3333-3333-3333-333333333333"
        );
        assert_eq!(ctx_value["run_id"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(
            ctx_value["agent_id"],
            "22222222-2222-2222-2222-222222222222"
        );
        assert_eq!(ctx_value["turn_id"], "turn-frame");
        assert_eq!(ctx_value["provenance"]["source"], "frame_evaluation_test");
    }

    #[test]
    fn script_can_block() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine
            .eval_script(r#"#{ block: "blocked" }"#, &ctx, None)
            .unwrap();
        assert_eq!(result.block.as_deref(), Some("blocked"));
    }

    #[test]
    fn script_can_inject() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let script = r#"
            #{
                inject: [
                    make_injection("constraint", "请先完成 lint", "test:src")
                ]
            }
        "#;
        let result = engine.eval_script(script, &ctx, None).unwrap();
        assert_eq!(result.inject.len(), 1);
        assert_eq!(result.inject[0].slot, "constraint");
    }

    #[test]
    fn script_effect_preserves_typed_context_presentation_facts() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let decision = engine
            .eval_script(
                "typed_context_presentation",
                &HookEvaluationContext {
                    snapshot: &snapshot,
                    query: &query,
                },
                None,
            )
            .expect("typed effect from script evaluator");

        let effect = decision.effects.first().expect("effect");
        let presentation = effect.presentation.as_ref().expect("presentation facts");
        assert_eq!(
            serde_json::to_value(presentation).expect("presentation"),
            serde_json::json!({
                "kind": "system_notice",
                "title": "Hook Notice",
                "summary": "Hook provider produced typed presentation facts.",
                "body": "继续完成 Hook 请求"
            })
        );
    }

    #[test]
    fn malformed_or_frame_shaped_script_effect_is_rejected() {
        let arbitrary_frame = HookScriptEngine::parse_decision(&serde_json::json!({
            "effects": [{
                "kind": "runtime:context_presentation",
                "presentation": {
                    "id": "caller-owned-frame-id",
                    "kind": "system_notice",
                    "source": "runtime_context_update",
                    "delivery_status": "queued_for_transform_context",
                    "delivery_channel": "turn_start",
                    "message_role": "user",
                    "rendered_text": "notice",
                    "sections": []
                }
            }]
        }));
        assert!(matches!(
            arbitrary_frame,
            Err(HookApplicationError::InvalidConfig(_))
        ));

        let malformed = HookScriptEngine::parse_decision(&serde_json::json!({
            "effects": [{"payload": {}}]
        }));
        assert!(matches!(
            malformed,
            Err(HookApplicationError::InvalidConfig(_))
        ));
    }

    #[test]
    fn script_reads_ctx_trigger() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let script = r#"
            if ctx.trigger == "before_tool" {
                #{ block: "matched trigger" }
            } else {
                #{}
            }
        "#;
        let result = engine.eval_script(script, &ctx, None).unwrap();
        assert_eq!(result.block.as_deref(), Some("matched trigger"));
    }

    #[test]
    fn script_reads_ctx_params() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let params = serde_json::json!({ "max_lines": 100 });
        let script = r#"
            let max = ctx.params.max_lines;
            if max == 100 {
                #{ block: "params work" }
            } else {
                #{}
            }
        "#;
        let result = engine.eval_script(script, &ctx, Some(&params)).unwrap();
        assert_eq!(result.block.as_deref(), Some("params work"));
    }

    #[test]
    fn validate_catches_syntax_error() {
        let engine = test_engine();
        let result = engine.validate_script("if { bad syntax }}}");
        assert!(result.is_err());
    }

    #[test]
    fn validate_accepts_good_script() {
        let engine = test_engine();
        let result = engine.validate_script("let x = 1; #{ block: \"ok\" }");
        assert!(result.is_ok());
    }

    #[test]
    fn preset_registration_and_eval() {
        let engine = HookScriptEngine::new(Arc::new(TestHookScriptEvaluator::new(&[(
            "test_preset",
            r#"#{ block: "from preset" }"#,
        )])));
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine.eval_preset("test_preset", &ctx, None).unwrap();
        assert_eq!(result.block.as_deref(), Some("from preset"));
    }

    #[test]
    fn shortcut_block_returns_decision() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine
            .eval_script(r#"block("forbidden")"#, &ctx, None)
            .unwrap();
        assert_eq!(result.block.as_deref(), Some("forbidden"));
    }

    #[test]
    fn shortcut_inject_returns_decision() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine
            .eval_script(r#"inject("workflow", "content", "src")"#, &ctx, None)
            .unwrap();
        assert_eq!(result.inject.len(), 1);
        assert_eq!(result.inject[0].slot, "workflow");
    }

    #[test]
    fn shortcut_approve_returns_decision() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine
            .eval_script(r#"approve("needs approval")"#, &ctx, None)
            .unwrap();
        assert!(result.approval.is_some());
        assert_eq!(result.approval.unwrap().reason, "needs approval");
    }

    #[test]
    fn shortcut_complete_returns_decision() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine
            .eval_script(r#"complete("auto", true, "all good")"#, &ctx, None)
            .unwrap();
        assert!(result.completion.is_some());
        let comp = result.completion.unwrap();
        assert_eq!(comp.mode, "auto");
        assert!(comp.satisfied);
        assert_eq!(comp.reason, "all good");
    }

    #[test]
    fn shortcut_log_returns_diagnostic() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine
            .eval_script(r#"log("debug info")"#, &ctx, None)
            .unwrap();
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "script_log");
        assert_eq!(result.diagnostics[0].message, "debug info");
    }
}
