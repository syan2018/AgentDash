//! Shared Rhai script runtime for infrastructure adapters.
//!
//! The runtime owns only engine setup, sandbox limits, AST caching and
//! `serde_json::Value` bridging. Business-specific helper functions are
//! registered by the adapter that constructs it.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;

use rhai::{AST, Dynamic, Engine, Scope};

/// Sandbox limits applied to every Rhai runtime used by AgentDash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhaiScriptLimits {
    pub max_operations: u64,
    pub max_call_levels: usize,
    pub max_string_size: usize,
    pub max_array_size: usize,
    pub max_map_size: usize,
}

impl Default for RhaiScriptLimits {
    fn default() -> Self {
        Self {
            max_operations: 10_000,
            max_call_levels: 32,
            max_string_size: 1_048_576,
            max_array_size: 1_000,
            max_map_size: 500,
        }
    }
}

/// Rhai engine wrapper that compiles, caches, and evaluates scripts.
pub struct RhaiScriptRuntime {
    engine: Engine,
    ast_cache: RwLock<HashMap<u64, AST>>,
}

impl RhaiScriptRuntime {
    /// Builds a runtime with the default sandbox limits.
    pub fn new(register_helpers: impl FnOnce(&mut Engine)) -> Self {
        Self::with_limits(RhaiScriptLimits::default(), register_helpers)
    }

    /// Builds a runtime with explicit sandbox limits.
    pub fn with_limits(
        limits: RhaiScriptLimits,
        register_helpers: impl FnOnce(&mut Engine),
    ) -> Self {
        let mut engine = Engine::new();
        Self::apply_limits(&mut engine, limits);
        register_helpers(&mut engine);

        Self {
            engine,
            ast_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Compiles a script without using the inline script cache.
    pub fn compile_script(&self, script: &str) -> Result<AST, String> {
        self.engine.compile(script).map_err(|e| e.to_string())
    }

    /// Compiles the script without executing it, for validation.
    pub fn validate_script(&self, script: &str) -> Result<(), Vec<String>> {
        self.compile_script(script).map(|_| ()).map_err(|e| vec![e])
    }

    /// Executes an inline script against the prebuilt context value.
    pub fn eval_script(
        &self,
        script: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let hash = Self::hash_script(script);
        let ast = self.cached_ast(hash, script)?;
        self.eval_ast(&ast, ctx)
    }

    /// Executes a compiled AST against the prebuilt context value.
    pub fn eval_ast(
        &self,
        ast: &AST,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let ctx_dynamic = json_to_dynamic(ctx);

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

    /// Returns the deterministic process-local hash used for inline AST cache keys.
    pub fn script_hash(script: &str) -> u64 {
        Self::hash_script(script)
    }

    fn apply_limits(engine: &mut Engine, limits: RhaiScriptLimits) {
        engine.set_max_operations(limits.max_operations);
        engine.set_max_call_levels(limits.max_call_levels);
        engine.set_max_string_size(limits.max_string_size);
        engine.set_max_array_size(limits.max_array_size);
        engine.set_max_map_size(limits.max_map_size);
    }

    fn cached_ast(&self, hash: u64, script: &str) -> Result<AST, String> {
        let cached = self
            .ast_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(&hash).cloned());

        match cached {
            Some(ast) => Ok(ast),
            None => {
                let ast = self.compile_script(script)?;
                if let Ok(mut cache) = self.ast_cache.write() {
                    cache.insert(hash, ast.clone());
                }
                Ok(ast)
            }
        }
    }

    fn hash_script(script: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        script.hash(&mut hasher);
        hasher.finish()
    }
}

fn json_to_dynamic(value: &serde_json::Value) -> Dynamic {
    match value {
        serde_json::Value::Null => Dynamic::UNIT,
        serde_json::Value::Bool(flag) => Dynamic::from(*flag),
        serde_json::Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Dynamic::from(value as rhai::INT)
            } else if let Some(value) = number.as_u64() {
                if let Ok(value) = rhai::INT::try_from(value) {
                    Dynamic::from(value)
                } else {
                    Dynamic::from(number.as_f64().unwrap_or_default() as rhai::FLOAT)
                }
            } else {
                Dynamic::from(number.as_f64().unwrap_or_default() as rhai::FLOAT)
            }
        }
        serde_json::Value::String(text) => Dynamic::from(text.clone()),
        serde_json::Value::Array(items) => {
            Dynamic::from(items.iter().map(json_to_dynamic).collect::<rhai::Array>())
        }
        serde_json::Value::Object(object) => {
            let mut map = rhai::Map::new();
            for (key, value) in object {
                map.insert(key.clone().into(), json_to_dynamic(value));
            }
            Dynamic::from(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn eval_script_bridges_json_context_and_registered_helpers() {
        let runtime = RhaiScriptRuntime::new(|engine| {
            engine.register_fn("tag", |value: &str| -> rhai::Map {
                let mut map = rhai::Map::new();
                map.insert("tag".into(), Dynamic::from(value.to_string()));
                map
            });
        });

        let result = runtime
            .eval_script("tag(ctx.name)", &json!({ "name": "alpha" }))
            .expect("script should evaluate");

        assert_eq!(result, json!({ "tag": "alpha" }));
    }

    #[test]
    fn eval_script_applies_operation_limit() {
        let runtime = RhaiScriptRuntime::with_limits(
            RhaiScriptLimits {
                max_operations: 16,
                ..RhaiScriptLimits::default()
            },
            |_| {},
        );

        let result = runtime.eval_script("let x = 0; loop { x += 1; }", &json!({}));

        assert!(
            result
                .expect_err("script should exceed operation limit")
                .contains("Rhai 脚本执行错误"),
            "operation limit errors should be surfaced as execution errors"
        );
    }
}
