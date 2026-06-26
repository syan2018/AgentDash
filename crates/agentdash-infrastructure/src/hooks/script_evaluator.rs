//! Rhai-backed implementation of the [`HookScriptEvaluator`] SPI port.
//!
//! Registers the Hook-specific Rhai helper surface and preset cache. The
//! shared engine, sandbox, AST cache and JSON bridge live in
//! [`crate::script_runtime::RhaiScriptRuntime`].

use agentdash_diagnostics::{diag, Subsystem};
use std::collections::HashMap;
use std::sync::RwLock;

use agentdash_spi::HookScriptEvaluator;
use rhai::{AST, Dynamic, Engine};

use crate::script_runtime::RhaiScriptRuntime;

/// Rhai-backed Hook script evaluator.
pub struct RhaiHookScriptEvaluator {
    runtime: RhaiScriptRuntime,
    preset_asts: RwLock<HashMap<String, AST>>,
}

impl RhaiHookScriptEvaluator {
    /// Builds the evaluator and pre-compiles the supplied preset scripts.
    pub fn new(preset_scripts: &[(&str, &str)]) -> Self {
        let runtime = RhaiScriptRuntime::new(Self::register_helpers);

        let mut preset_asts = HashMap::new();
        for (key, script) in preset_scripts {
            match runtime.compile_script(script) {
                Ok(ast) => {
                    preset_asts.insert(key.to_string(), ast);
                }
                Err(e) => {
                    diag!(Error, Subsystem::Hooks,
        preset_key = key, error = %e, "preset Rhai 脚本编译失败");
                }
            }
        }

        Self {
            runtime,
            preset_asts: RwLock::new(preset_asts),
        }
    }

    fn register_helpers(engine: &mut Engine) {
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
}

impl HookScriptEvaluator for RhaiHookScriptEvaluator {
    fn register_preset(&self, key: &str, script: &str) -> Result<(), String> {
        let ast = self.runtime.compile_script(script)?;
        self.preset_asts
            .write()
            .map_err(|e| format!("preset lock: {e}"))?
            .insert(key.to_string(), ast);
        Ok(())
    }

    fn remove_preset(&self, key: &str) -> bool {
        self.preset_asts
            .write()
            .ok()
            .map(|mut map| map.remove(key).is_some())
            .unwrap_or(false)
    }

    fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.runtime.validate_script(script)
    }

    fn eval_preset(
        &self,
        preset_key: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let ast = self
            .preset_asts
            .read()
            .map_err(|e| format!("preset lock: {e}"))?
            .get(preset_key)
            .cloned()
            .ok_or_else(|| format!("未知 preset: {preset_key}"))?;

        let start = std::time::Instant::now();
        let result = self.runtime.eval_ast(&ast, ctx);
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => diag!(Debug, Subsystem::Hooks,
        
                preset = preset_key,
                elapsed_us = elapsed.as_micros() as u64,
                "rhai preset 执行完成"
            ),
            Err(e) => diag!(Warn, Subsystem::Hooks,
        
                preset = preset_key,
                elapsed_us = elapsed.as_micros() as u64,
                error = %e,
                "rhai preset 执行失败"
            ),
        }
        result
    }

    fn eval_script(
        &self,
        script: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let hash = RhaiScriptRuntime::script_hash(script);

        let start = std::time::Instant::now();
        let result = self.runtime.eval_script(script, ctx);
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => diag!(Debug, Subsystem::Hooks,
        
                script_hash = hash,
                elapsed_us = elapsed.as_micros() as u64,
                "rhai 自定义脚本执行完成"
            ),
            Err(e) => diag!(Warn, Subsystem::Hooks,
        
                script_hash = hash,
                elapsed_us = elapsed.as_micros() as u64,
                error = %e,
                "rhai 自定义脚本执行失败"
            ),
        }
        result
    }
}
