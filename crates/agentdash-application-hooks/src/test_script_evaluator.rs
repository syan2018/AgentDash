use std::collections::BTreeMap;

use agentdash_spi::HookScriptEvaluator;

pub struct TestHookScriptEvaluator {
    scripts: BTreeMap<String, String>,
}

impl TestHookScriptEvaluator {
    pub fn new(scripts: &[(&str, &str)]) -> Self {
        Self {
            scripts: scripts
                .iter()
                .map(|(key, script)| ((*key).to_string(), (*script).to_string()))
                .collect(),
        }
    }
}

impl HookScriptEvaluator for TestHookScriptEvaluator {
    fn register_preset(&self, _key: &str, _script: &str) -> Result<(), String> {
        Ok(())
    }

    fn remove_preset(&self, _key: &str) -> bool {
        false
    }

    fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        if script.contains("bad syntax") {
            Err(vec!["syntax error".to_string()])
        } else {
            Ok(())
        }
    }

    fn eval_preset(
        &self,
        preset_key: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match preset_key {
            "test_preset" => Ok(decision_from_script(
                self.scripts
                    .get(preset_key)
                    .map(String::as_str)
                    .unwrap_or_default(),
                ctx,
            )),
            "port_output_gate" => Ok(port_output_gate(ctx)),
            "subagent_inherit_context" => Ok(serde_json::json!({
                "inject": ctx["snapshot"]["injections"].as_array().cloned().unwrap_or_default(),
                "diagnostics": [{
                    "code": "before_subagent_dispatch_prepared",
                    "message": "test dispatch inheritance"
                }]
            })),
            "companion_result_channel" => Ok(serde_json::json!({
                "inject": [
                    { "slot": "workflow", "content": "companion summary", "source": ctx["workflow"]["source"].as_str().unwrap_or("") },
                    { "slot": "constraint", "content": "companion constraint", "source": ctx["workflow"]["source"].as_str().unwrap_or("") }
                ],
                "diagnostics": [{
                    "code": "companion_result_recorded",
                    "message": "test companion result"
                }]
            })),
            _ => Ok(serde_json::json!({})),
        }
    }

    fn eval_script(
        &self,
        script: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(decision_from_script(script, ctx))
    }
}

fn decision_from_script(script: &str, ctx: &serde_json::Value) -> serde_json::Value {
    if script.trim() == "#{}" {
        return serde_json::json!({});
    }
    if script.contains("from preset") {
        return serde_json::json!({ "block": "from preset" });
    }
    if script.contains("block(\"forbidden\")") {
        return serde_json::json!({ "block": "forbidden" });
    }
    if script.contains("#{ block: \"blocked\" }") {
        return serde_json::json!({ "block": "blocked" });
    }
    if script.contains("make_injection(\"constraint\", \"请先完成 lint\", \"test:src\")") {
        return serde_json::json!({
            "inject": [{ "slot": "constraint", "content": "请先完成 lint", "source": "test:src" }]
        });
    }
    if script.contains("ctx.trigger == \"before_tool\"") {
        return if ctx["trigger"].as_str() == Some("before_tool") {
            serde_json::json!({ "block": "matched trigger" })
        } else {
            serde_json::json!({})
        };
    }
    if script.contains("ctx.params.max_lines") {
        return if ctx["params"]["max_lines"].as_i64() == Some(100) {
            serde_json::json!({ "block": "params work" })
        } else {
            serde_json::json!({})
        };
    }
    if script.contains("inject(\"workflow\", \"content\", \"src\")") {
        return serde_json::json!({
            "inject": [{ "slot": "workflow", "content": "content", "source": "src" }]
        });
    }
    if script.contains("approve(\"needs approval\")") {
        return serde_json::json!({ "approval": { "reason": "needs approval" } });
    }
    if script.contains("complete(\"auto\", true, \"all good\")") {
        return serde_json::json!({
            "completion": { "mode": "auto", "satisfied": true, "reason": "all good" }
        });
    }
    if script.contains("log(\"debug info\")") {
        return serde_json::json!({
            "diagnostics": [{ "code": "script_log", "message": "debug info" }]
        });
    }
    serde_json::json!({})
}

fn port_output_gate(ctx: &serde_json::Value) -> serde_json::Value {
    if ctx["workflow"]["node_type"].as_str() != Some("agent_node") {
        return serde_json::json!({});
    }
    let output_ports = ctx["workflow"]["output_port_keys"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if output_ports.is_empty() {
        return serde_json::json!({});
    }
    let fulfilled = ctx["workflow"]["fulfilled_port_keys"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let missing: Vec<String> = output_ports
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|port| {
            !fulfilled
                .iter()
                .any(|fulfilled| fulfilled.as_str() == Some(*port))
        })
        .map(str::to_string)
        .collect();
    if missing.is_empty() {
        return serde_json::json!({});
    }
    serde_json::json!({
        "completion": {
            "mode": "stop_gate",
            "satisfied": false,
            "reason": format!("Output port 交付未满足: [{}]", missing.join(", "))
        },
        "inject": [{
            "slot": "workflow",
            "content": format!("## Port Output Gate\n{}", missing.join(", ")),
            "source": ctx["workflow"]["source"].as_str().unwrap_or("")
        }],
        "diagnostics": [{
            "code": "port_output_gate_unsatisfied",
            "message": "test port gate"
        }]
    })
}
