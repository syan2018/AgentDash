//! Rhai-backed implementation of the [`HookScriptEvaluator`] SPI port.
//!
//! Owns the embedded Rhai engine, the compiled-AST caches, the sandbox
//! limits, and the script helper functions. The application layer passes a
//! prebuilt context value and receives the raw decision value back.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;

use agentdash_spi::HookScriptEvaluator;
use rhai::{AST, Dynamic, Engine, Scope};

/// Rhai engine wrapper that compiles, caches, and evaluates hook scripts.
pub struct RhaiHookScriptEvaluator {
    engine: Engine,
    ast_cache: RwLock<HashMap<u64, AST>>,
    preset_asts: RwLock<HashMap<String, AST>>,
}

impl RhaiHookScriptEvaluator {
    /// Builds the evaluator and pre-compiles the supplied preset scripts.
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

    fn eval_ast(&self, ast: &AST, ctx: &serde_json::Value) -> Result<serde_json::Value, String> {
        let ctx_dynamic =
            rhai::serde::to_dynamic(ctx).map_err(|e| format!("ctx 序列化失败: {e}"))?;

        let mut scope = Scope::new();
        scope.push("ctx", ctx_dynamic);

        let result: Dynamic = self
            .engine
            .eval_ast_with_scope(&mut scope, ast)
            .map_err(|e| format!("Rhai 脚本执行错误: {e}"))?;

        if result.is_unit() {
            return Ok(serde_json::Value::Null);
        }

        rhai::serde::from_dynamic(&result).map_err(|e| format!("返回值解析失败: {e}"))
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

    fn hash_script(script: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        script.hash(&mut hasher);
        hasher.finish()
    }
}

impl HookScriptEvaluator for RhaiHookScriptEvaluator {
    fn register_preset(&self, key: &str, script: &str) -> Result<(), String> {
        let ast = self.engine.compile(script).map_err(|e| e.to_string())?;
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
        self.engine
            .compile(script)
            .map(|_| ())
            .map_err(|e| vec![e.to_string()])
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
        let result = self.eval_ast(&ast, ctx);
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => tracing::debug!(
                preset = preset_key,
                elapsed_us = elapsed.as_micros() as u64,
                "rhai preset 执行完成"
            ),
            Err(e) => tracing::warn!(
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
        let result = self.eval_ast(&ast, ctx);
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => tracing::debug!(
                script_hash = hash,
                elapsed_us = elapsed.as_micros() as u64,
                "rhai 自定义脚本执行完成"
            ),
            Err(e) => tracing::warn!(
                script_hash = hash,
                elapsed_us = elapsed.as_micros() as u64,
                error = %e,
                "rhai 自定义脚本执行失败"
            ),
        }
        result
    }
}
