use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;

use rhai::{AST, Dynamic, Engine, Scope};

use agentdash_domain::workflow::WorkflowConstraintKind;
use agentdash_spi::{
    HookApprovalRequest, HookCompletionStatus, HookDiagnosticEntry, HookEffect, HookInjection,
    HookTrigger,
};

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
    }
}

// ── 脚本引擎 ──

pub(crate) struct HookScriptEngine {
    engine: Engine,
    ast_cache: RwLock<HashMap<u64, AST>>,
    preset_asts: RwLock<HashMap<String, AST>>,
}

impl HookScriptEngine {
    pub fn new(preset_scripts: &[(&str, &str)]) -> Self {
        let mut engine = Engine::new();

        // 安全沙箱
        engine.set_max_operations(10_000);
        engine.set_max_call_levels(32);
        engine.set_max_string_size(1_048_576);
        engine.set_max_array_size(1_000);
        engine.set_max_map_size(500);

        Self::register_helpers(&mut engine);

        let mut preset_asts = HashMap::new();
        for (key, script) in preset_scripts {
            match engine.compile(script) {
                Ok(ast) => {
                    preset_asts.insert(key.to_string(), ast);
                }
                Err(e) => {
                    tracing::error!(preset_key = key, error = %e, "preset Rhai 脚本编译失败");
                }
            }
        }

        Self {
            engine,
            ast_cache: RwLock::new(HashMap::new()),
            preset_asts: RwLock::new(preset_asts),
        }
    }

    /// 运行时注册/更新自定义 preset 脚本。
    pub fn register_preset(&self, key: &str, script: &str) -> Result<(), String> {
        let ast = self.engine.compile(script).map_err(|e| e.to_string())?;
        self.preset_asts
            .write()
            .map_err(|e| format!("preset lock: {e}"))?
            .insert(key.to_string(), ast);
        Ok(())
    }

    /// 移除一个 preset（仅 UserDefined 类型应调用此接口）。
    pub fn remove_preset(&self, key: &str) -> bool {
        self.preset_asts
            .write()
            .ok()
            .map(|mut map| map.remove(key).is_some())
            .unwrap_or(false)
    }

    /// 执行 preset 脚本
    pub fn eval_preset(
        &self,
        preset_key: &str,
        ctx: &HookEvaluationContext<'_>,
        params: Option<&serde_json::Value>,
    ) -> Result<ScriptDecision, String> {
        let ast = self
            .preset_asts
            .read()
            .map_err(|e| format!("preset lock: {e}"))?
            .get(preset_key)
            .cloned()
            .ok_or_else(|| format!("未知 preset: {preset_key}"))?;

        let start = std::time::Instant::now();
        let result = self.eval_ast(&ast, ctx, params);
        let elapsed = start.elapsed();

        match &result {
            Ok(decision) => {
                tracing::debug!(
                    preset = preset_key,
                    trigger = ?ctx.query.trigger,
                    elapsed_us = elapsed.as_micros() as u64,
                    has_block = decision.block.is_some(),
                    injections = decision.inject.len(),
                    diagnostics = decision.diagnostics.len(),
                    "rhai preset 执行完成"
                );
            }
            Err(e) => {
                tracing::warn!(
                    preset = preset_key,
                    trigger = ?ctx.query.trigger,
                    elapsed_us = elapsed.as_micros() as u64,
                    error = %e,
                    "rhai preset 执行失败"
                );
            }
        }
        result
    }

    /// 执行用户自定义脚本
    pub fn eval_script(
        &self,
        script: &str,
        ctx: &HookEvaluationContext<'_>,
        params: Option<&serde_json::Value>,
    ) -> Result<ScriptDecision, String> {
        let hash = Self::hash_script(script);

        let cached = self
            .ast_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(&hash).cloned());

        let ast = match cached {
            Some(ast) => ast,
            None => {
                let ast = self.engine.compile(script).map_err(|e| e.to_string())?;
                if let Ok(mut cache) = self.ast_cache.write() {
                    cache.insert(hash, ast.clone());
                }
                ast
            }
        };

        let start = std::time::Instant::now();
        let result = self.eval_ast(&ast, ctx, params);
        let elapsed = start.elapsed();

        match &result {
            Ok(decision) => {
                tracing::debug!(
                    script_hash = hash,
                    trigger = ?ctx.query.trigger,
                    elapsed_us = elapsed.as_micros() as u64,
                    has_block = decision.block.is_some(),
                    injections = decision.inject.len(),
                    diagnostics = decision.diagnostics.len(),
                    "rhai 自定义脚本执行完成"
                );
            }
            Err(e) => {
                tracing::warn!(
                    script_hash = hash,
                    trigger = ?ctx.query.trigger,
                    elapsed_us = elapsed.as_micros() as u64,
                    error = %e,
                    "rhai 自定义脚本执行失败"
                );
            }
        }
        result
    }

    /// 仅编译，不执行——用于验证 API (R11)
    pub fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.engine
            .compile(script)
            .map(|_| ())
            .map_err(|e| vec![e.to_string()])
    }

    // ── 内部方法 ──

    fn eval_ast(
        &self,
        ast: &AST,
        ctx: &HookEvaluationContext<'_>,
        params: Option<&serde_json::Value>,
    ) -> Result<ScriptDecision, String> {
        let ctx_json = Self::build_ctx_value(ctx, params);
        let ctx_dynamic =
            rhai::serde::to_dynamic(&ctx_json).map_err(|e| format!("ctx 序列化失败: {e}"))?;

        let mut scope = Scope::new();
        scope.push("ctx", ctx_dynamic);

        let result: Dynamic = self
            .engine
            .eval_ast_with_scope(&mut scope, ast)
            .map_err(|e| format!("Rhai 脚本执行错误: {e}"))?;

        Self::parse_decision(&result)
    }

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

        let auto_completion = workflow_auto_completion_snapshot(ctx.snapshot);
        let evidence_present = checklist_evidence_present(ctx.snapshot);
        let has_block_stop = contract
            .map(|c| {
                c.constraints
                    .iter()
                    .any(|cs| cs.kind == WorkflowConstraintKind::BlockStopUntilChecksPass)
            })
            .unwrap_or(false);

        let completion_satisfied = if auto_completion && has_block_stop {
            contract
                .map(|c| {
                    crate::workflow::evaluate_step_completion(
                        Some(&c.completion),
                        &crate::workflow::WorkflowCompletionSignalSet {
                            checklist_evidence_present: evidence_present,
                            ..Default::default()
                        },
                    )
                    .satisfied
                })
                .unwrap_or(false)
        } else {
            false
        };

        let tool_name = ctx.query.tool_name.as_deref().unwrap_or("");
        let tool_failed = super::helpers::tool_call_failed(ctx.query.payload.as_ref());
        let is_artifact_tool = super::helpers::is_report_workflow_artifact_tool(tool_name);
        let denied_types = active_workflow_denied_record_artifact_types(ctx.snapshot);

        let trigger_str = match ctx.query.trigger {
            HookTrigger::SessionStart => "session_start",
            HookTrigger::UserPromptSubmit => "user_prompt_submit",
            HookTrigger::BeforeTool => "before_tool",
            HookTrigger::AfterTool => "after_tool",
            HookTrigger::AfterTurn => "after_turn",
            HookTrigger::BeforeStop => "before_stop",
            HookTrigger::SessionTerminal => "session_terminal",
            HookTrigger::BeforeSubagentDispatch => "before_subagent_dispatch",
            HookTrigger::AfterSubagentDispatch => "after_subagent_dispatch",
            HookTrigger::SubagentResult => "subagent_result",
        };

        let wf_source = active_workflow_source_from_snapshot(ctx.snapshot);

        serde_json::json!({
            "trigger": trigger_str,
            "tool_name": ctx.query.tool_name,
            "tool_call_id": ctx.query.tool_call_id,
            "subagent_type": ctx.query.subagent_type,
            "turn_id": ctx.query.turn_id,
            "session_id": ctx.query.session_id,
            "payload": ctx.query.payload,

            "snapshot": {
                "owners": ctx.snapshot.owners,
                "tags": ctx.snapshot.tags,
                "injections": ctx.snapshot.injections,
            },

            "workflow": {
                "lifecycle_key": aw.and_then(|a| a.lifecycle_key.as_deref()),
                "step_key": aw.and_then(|a| a.step_key.as_deref()),
                "workflow_key": aw.and_then(|a| a.workflow_key.as_deref()),
                "transition_policy": aw.and_then(|a| a.transition_policy.as_deref()),
                "run_status": aw.and_then(|a| a.run_status.as_ref().map(|s| format!("{s:?}").to_ascii_lowercase())),
                "run_id": aw.and_then(|a| a.run_id.map(|id| id.to_string())),
                "checklist_evidence_present": evidence_present,
                "auto_completion": auto_completion,
                "source": wf_source,
            },

            "contract": {
                "hook_rules": contract.map(|c| &c.hook_rules),
                "constraints": contract.map(|c| &c.constraints),
                "checks": contract.map(|c| &c.completion.checks),
            },

            "meta": {
                "permission_policy": ctx.snapshot.metadata.as_ref().and_then(|m| m.permission_policy.as_deref()),
                "working_directory": ctx.snapshot.metadata.as_ref().and_then(|m| m.working_directory.as_deref()),
                "connector_id": ctx.snapshot.metadata.as_ref().and_then(|m| m.connector_id.as_deref()),
                "executor": ctx.snapshot.metadata.as_ref().and_then(|m| m.executor.as_deref()),
                "task_execution_mode": ctx.snapshot.metadata.as_ref().and_then(|m| m.extra.get("task_execution_mode")),
                "task_status": ctx.snapshot.metadata.as_ref().and_then(|m| m.extra.get("task_status")),
                "task_id": ctx.snapshot.metadata.as_ref().and_then(|m| m.extra.get("task_id")),
            },

            "params": params.unwrap_or(&serde_json::Value::Null),

            "signals": {
                "auto_completion": auto_completion,
                "completion_satisfied": completion_satisfied,
                "has_block_stop_constraint": has_block_stop,
                "checklist_evidence_present": evidence_present,
                "tool_call_failed": tool_failed,
                "is_artifact_report_tool": is_artifact_tool,
                "denied_artifact_types": denied_types,
            },
        })
    }

    fn parse_decision(result: &Dynamic) -> Result<ScriptDecision, String> {
        if result.is_unit() {
            return Ok(empty_decision());
        }

        let result_json: serde_json::Value =
            rhai::serde::from_dynamic(result).map_err(|e| format!("返回值解析失败: {e}"))?;

        let obj = match result_json.as_object() {
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

        let effects = obj
            .get("effects")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let o = item.as_object()?;
                        Some(HookEffect {
                            kind: o.get("kind")?.as_str()?.to_string(),
                            payload: o.get("payload").cloned().unwrap_or(serde_json::Value::Null),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ScriptDecision {
            block,
            inject,
            approval,
            completion,
            refresh,
            rewrite_input,
            diagnostics,
            effects,
        })
    }

    fn register_helpers(engine: &mut Engine) {
        engine.register_fn("is_workflow_artifact_tool", |name: &str| -> bool {
            name.ends_with("report_workflow_artifact")
        });

        engine.register_fn("requires_supervised_approval", |name: &str| -> bool {
            let normalized = name.to_ascii_lowercase();
            normalized.ends_with("shell_exec")
                || normalized.ends_with("shell")
                || normalized.ends_with("write_file")
                || normalized.ends_with("fs_apply_patch")
                || normalized.contains("delete")
                || normalized.contains("remove")
                || normalized.contains("move")
                || normalized.contains("rename")
        });

        engine.register_fn(
            "make_injection",
            |slot: &str, content: &str, source: &str| -> rhai::Map {
                let mut m = rhai::Map::new();
                m.insert("slot".into(), Dynamic::from(slot.to_string()));
                m.insert("content".into(), Dynamic::from(content.to_string()));
                m.insert("source".into(), Dynamic::from(source.to_string()));
                m
            },
        );

        engine.register_fn(
            "make_diagnostic",
            |code: &str, message: &str| -> rhai::Map {
                let mut m = rhai::Map::new();
                m.insert("code".into(), Dynamic::from(code.to_string()));
                m.insert("message".into(), Dynamic::from(message.to_string()));
                m
            },
        );

        engine.register_fn("block", |reason: &str| -> rhai::Map {
            let mut m = rhai::Map::new();
            m.insert("block".into(), Dynamic::from(reason.to_string()));
            m
        });

        engine.register_fn(
            "inject",
            |slot: &str, content: &str, source: &str| -> rhai::Map {
                let mut m = rhai::Map::new();
                m.insert(
                    "inject".into(),
                    Dynamic::from(rhai::Array::from(vec![{
                        let mut inj = rhai::Map::new();
                        inj.insert("slot".into(), Dynamic::from(slot.to_string()));
                        inj.insert("content".into(), Dynamic::from(content.to_string()));
                        inj.insert("source".into(), Dynamic::from(source.to_string()));
                        Dynamic::from(inj)
                    }])),
                );
                m
            },
        );

        engine.register_fn("approve", |reason: &str| -> rhai::Map {
            let mut m = rhai::Map::new();
            let mut approval = rhai::Map::new();
            approval.insert("reason".into(), Dynamic::from(reason.to_string()));
            m.insert("approval".into(), Dynamic::from(approval));
            m
        });

        engine.register_fn(
            "complete",
            |mode: &str, satisfied: bool, reason: &str| -> rhai::Map {
                let mut m = rhai::Map::new();
                let mut comp = rhai::Map::new();
                comp.insert("mode".into(), Dynamic::from(mode.to_string()));
                comp.insert("satisfied".into(), Dynamic::from(satisfied));
                comp.insert("reason".into(), Dynamic::from(reason.to_string()));
                m.insert("completion".into(), Dynamic::from(comp));
                m
            },
        );

        engine.register_fn("log", |message: &str| -> rhai::Map {
            let mut m = rhai::Map::new();
            m.insert(
                "diagnostics".into(),
                Dynamic::from(rhai::Array::from(vec![{
                    let mut d = rhai::Map::new();
                    d.insert("code".into(), Dynamic::from("script_log".to_string()));
                    d.insert("message".into(), Dynamic::from(message.to_string()));
                    Dynamic::from(d)
                }])),
            );
            m
        });
    }

    fn hash_script(script: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        script.hash(&mut hasher);
        hasher.finish()
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::{HookEvaluationQuery, HookTrigger, SessionHookSnapshot};

    fn test_engine() -> HookScriptEngine {
        HookScriptEngine::new(&[])
    }

    fn base_ctx() -> (SessionHookSnapshot, HookEvaluationQuery) {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            ..SessionHookSnapshot::default()
        };
        let query = HookEvaluationQuery {
            session_id: "sess-test".to_string(),
            trigger: HookTrigger::BeforeTool,
            turn_id: None,
            tool_name: Some("shell_exec".to_string()),
            tool_call_id: Some("call-1".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: None,
        };
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
        let engine = HookScriptEngine::new(&[("test_preset", r#"#{ block: "from preset" }"#)]);
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let result = engine.eval_preset("test_preset", &ctx, None).unwrap();
        assert_eq!(result.block.as_deref(), Some("from preset"));
    }

    #[test]
    fn helper_is_workflow_artifact_tool() {
        let engine = test_engine();
        let (snapshot, query) = base_ctx();
        let ctx = HookEvaluationContext {
            snapshot: &snapshot,
            query: &query,
        };
        let script = r#"
            if is_workflow_artifact_tool("mcp_agentdash_workflow_tools_report_workflow_artifact") {
                #{ block: "yes" }
            } else {
                #{}
            }
        "#;
        let result = engine.eval_script(script, &ctx, None).unwrap();
        assert_eq!(result.block.as_deref(), Some("yes"));
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
