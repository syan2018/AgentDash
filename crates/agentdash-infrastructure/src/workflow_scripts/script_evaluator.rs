//! Rhai-backed implementation of the [`WorkflowScriptEvaluator`] SPI port.
//!
//! This adapter registers only workflow builder helpers. The helpers construct
//! serializable builder documents and do not execute workflow side effects.

use agentdash_diagnostics::{diag, Subsystem};
use agentdash_spi::WorkflowScriptEvaluator;
use rhai::{Dynamic, Engine};

use crate::script_runtime::RhaiScriptRuntime;

/// Rhai-backed workflow script evaluator.
pub struct RhaiWorkflowScriptEvaluator {
    runtime: RhaiScriptRuntime,
}

impl RhaiWorkflowScriptEvaluator {
    /// Builds the evaluator with the workflow builder helper surface.
    pub fn new() -> Self {
        Self {
            runtime: RhaiScriptRuntime::new(Self::register_helpers),
        }
    }

    fn register_helpers(engine: &mut Engine) {
        engine.register_fn("workflow", |options: rhai::Map| -> rhai::Map {
            merge_options(map_with_kind("workflow"), options, &["kind"])
        });

        engine.register_fn("phase", |name: &str, body: rhai::Array| -> rhai::Map {
            let mut map = map_with_kind("phase");
            insert_string(&mut map, "name", name);
            map.insert("body".into(), Dynamic::from(body));
            map
        });

        engine.register_fn("log", |message: &str| -> rhai::Map {
            let mut map = map_with_kind("log");
            insert_string(&mut map, "message", message);
            map
        });

        engine.register_fn("agent", |name: &str, options: rhai::Map| -> rhai::Map {
            let mut map = map_with_kind("agent");
            insert_string(&mut map, "name", name);
            merge_options(map, options, &["kind", "name"])
        });

        engine.register_fn("parallel", |branches: rhai::Array| -> rhai::Map {
            let mut map = map_with_kind("parallel");
            map.insert("branches".into(), Dynamic::from(branches));
            map
        });

        engine.register_fn("pipeline", |stages: rhai::Array| -> rhai::Map {
            let mut map = map_with_kind("pipeline");
            map.insert("stages".into(), Dynamic::from(stages));
            map
        });

        engine.register_fn("function", |name: &str, request: rhai::Map| -> rhai::Map {
            let mut map = map_with_kind("function");
            insert_string(&mut map, "name", name);
            map.insert("request".into(), Dynamic::from(request));
            map
        });

        engine.register_fn(
            "function",
            |name: &str, request: rhai::Map, options: rhai::Map| -> rhai::Map {
                let mut map = map_with_kind("function");
                insert_string(&mut map, "name", name);
                map.insert("request".into(), Dynamic::from(request));
                merge_options(map, options, &["kind", "name", "request"])
            },
        );

        engine.register_fn(
            "local_effect",
            |name: &str, effect: rhai::Map| -> rhai::Map {
                let mut map = map_with_kind("local_effect");
                insert_string(&mut map, "name", name);
                map.insert("effect".into(), Dynamic::from(effect));
                map
            },
        );

        engine.register_fn(
            "local_effect",
            |name: &str, effect: rhai::Map, options: rhai::Map| -> rhai::Map {
                let mut map = map_with_kind("local_effect");
                insert_string(&mut map, "name", name);
                map.insert("effect".into(), Dynamic::from(effect));
                merge_options(map, options, &["kind", "name", "effect"])
            },
        );

        engine.register_fn(
            "human_gate",
            |name: &str, options: rhai::Map| -> rhai::Map {
                let mut map = map_with_kind("human_gate");
                insert_string(&mut map, "name", name);
                merge_options(map, options, &["kind", "name"])
            },
        );

        engine.register_fn("api_request", |options: rhai::Map| -> rhai::Map {
            merge_options(map_with_kind("api_request"), options, &["kind"])
        });

        engine.register_fn("bash_exec", |options: rhai::Map| -> rhai::Map {
            merge_options(map_with_kind("bash_exec"), options, &["kind"])
        });

        engine.register_fn("capability_effect", |capability_key: &str| -> rhai::Map {
            let mut map = map_with_kind("capability_effect");
            insert_string(&mut map, "capability_key", capability_key);
            map
        });

        engine.register_fn(
            "capability_effect",
            |capability_key: &str, input: Dynamic| -> rhai::Map {
                let mut map = map_with_kind("capability_effect");
                insert_string(&mut map, "capability_key", capability_key);
                if !input.is_unit() {
                    map.insert("input".into(), input);
                }
                map
            },
        );
    }
}

impl Default for RhaiWorkflowScriptEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowScriptEvaluator for RhaiWorkflowScriptEvaluator {
    fn validate_workflow_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.runtime.validate_script(script)
    }

    fn eval_workflow_script(
        &self,
        script: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let hash = RhaiScriptRuntime::script_hash(script);
        let start = std::time::Instant::now();
        let result = self.runtime.eval_script(script, ctx);
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => diag!(Debug, Subsystem::Workflow,
        
                script_hash = hash,
                elapsed_us = elapsed.as_micros() as u64,
                "rhai workflow 脚本执行完成"
            ),
            Err(e) => diag!(Warn, Subsystem::Workflow,
        
                script_hash = hash,
                elapsed_us = elapsed.as_micros() as u64,
                error = %e,
                "rhai workflow 脚本执行失败"
            ),
        }

        result
    }
}

fn map_with_kind(kind: &str) -> rhai::Map {
    let mut map = rhai::Map::new();
    insert_string(&mut map, "kind", kind);
    map
}

fn insert_string(map: &mut rhai::Map, key: &str, value: &str) {
    map.insert(key.into(), Dynamic::from(value.to_string()));
}

fn merge_options(mut base: rhai::Map, options: rhai::Map, reserved_keys: &[&str]) -> rhai::Map {
    for (key, value) in options {
        if reserved_keys
            .iter()
            .any(|reserved| key.as_str() == *reserved)
        {
            continue;
        }
        base.insert(key, value);
    }
    base
}

#[cfg(test)]
mod tests {
    use agentdash_spi::WorkflowScriptEvaluator;
    use serde_json::json;

    use super::*;

    #[test]
    fn workflow_evaluator_eval_returns_builder_document() {
        let evaluator = RhaiWorkflowScriptEvaluator::new();

        let document = evaluator
            .eval_workflow_script(
                r#"
                    workflow(#{
                        name: "research_review",
                        args: #{ topic: "string" },
                        limits: #{ max_agents: 6, max_effects: 4 },
                        body: [
                            phase("collect", [
                                parallel([
                                    agent("scan_docs", #{
                                        procedure: "researcher",
                                        prompt: "Scan docs for " + ctx.topic,
                                        outputs: ["notes"]
                                    }),
                                    function("fetch_index", api_request(#{
                                        method: "GET",
                                        url: "https://example.test/index"
                                    }))
                                ]),
                                pipeline([
                                    local_effect("format_notes", bash_exec(#{
                                        command: "pnpm",
                                        args: ["format"]
                                    })),
                                    human_gate("approve_notes", #{
                                        form_schema: "workflow.approval",
                                        decision_port: "decision"
                                    })
                                ])
                            ])
                        ]
                    })
                "#,
                &json!({ "topic": "orchestration" }),
            )
            .expect("workflow script should evaluate");

        assert_eq!(document["kind"], json!("workflow"));
        assert_eq!(document["name"], json!("research_review"));
        assert_eq!(document["body"][0]["kind"], json!("phase"));
        assert_eq!(document["body"][0]["body"][0]["kind"], json!("parallel"));
        assert_eq!(
            document["body"][0]["body"][0]["branches"][0]["kind"],
            json!("agent")
        );
        assert_eq!(
            document["body"][0]["body"][0]["branches"][1]["request"]["kind"],
            json!("api_request")
        );
        assert_eq!(
            document["body"][0]["body"][1]["stages"][0]["effect"]["kind"],
            json!("bash_exec")
        );
    }

    #[test]
    fn workflow_evaluator_does_not_register_hook_helpers() {
        let evaluator = RhaiWorkflowScriptEvaluator::new();

        let error = evaluator
            .eval_workflow_script(r#"block("forbidden")"#, &json!({}))
            .expect_err("hook helper should not be registered");

        assert!(error.contains("block"), "{error}");
    }

    #[test]
    fn workflow_evaluator_validate_surfaces_syntax_errors() {
        let evaluator = RhaiWorkflowScriptEvaluator::new();

        let diagnostics = evaluator
            .validate_workflow_script("workflow(#{")
            .expect_err("invalid Rhai syntax should fail validation");

        assert!(!diagnostics.is_empty());
    }
}
