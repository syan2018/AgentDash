//! SPI port for evaluating hook scripts.
//!
//! Hook rules may carry an inline script or reference a registered preset.
//! The concrete scripting engine (currently Rhai) lives in infrastructure;
//! application only depends on this port, passing a prebuilt context value
//! and receiving the raw decision value back for parsing.

/// Evaluates hook scripts against a prebuilt context value.
///
/// The `ctx` argument is a fully-serialized JSON snapshot of the hook
/// evaluation context (including any rule `params` folded under `ctx.params`).
/// The returned value is the raw script decision object; the caller is
/// responsible for parsing it into its domain decision type.
pub trait HookScriptEvaluator: Send + Sync {
    /// Registers or updates a custom preset script by key.
    fn register_preset(&self, key: &str, script: &str) -> Result<(), String>;

    /// Removes a previously registered preset. Returns `true` if one existed.
    fn remove_preset(&self, key: &str) -> bool;

    /// Compiles the script without executing it, for validation.
    fn validate_script(&self, script: &str) -> Result<(), Vec<String>>;

    /// Executes a registered preset against the prebuilt context value.
    fn eval_preset(
        &self,
        preset_key: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Executes an inline script against the prebuilt context value.
    fn eval_script(
        &self,
        script: &str,
        ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}
