use agentdash_domain::shared_library::EXTENSION_PERMISSION_PROCESS_EXECUTE;
use serde_json::{Value, json};

use crate::process_executor::ProcessEnvOverlay;
use crate::tool_executor::ToolError;

use super::LocalExtensionHostError;
use super::host_api::{
    DEFAULT_HOST_API_TIMEOUT_MS, DEFAULT_OUTPUT_LIMIT_BYTES, host_api_tool_error, optional_string,
    optional_u64, process_result_value, require_string, resolve_workspace_root_from_options,
};
use super::permission_guard::require_declared_permission;
use super::process::ActiveExtension;

pub(super) async fn resolve_process_shell(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(
        active,
        params,
        &[EXTENSION_PERMISSION_PROCESS_EXECUTE.to_string()],
    )?;
    let command = require_string(params, "command")?;
    let options = params.get("options").unwrap_or(&Value::Null);
    let env = parse_env_overlay(options)?;
    require_env_overlay_permissions(active, params, &env)?;
    let workspace_root = resolve_workspace_root_from_options(active, options)?;
    let cwd = optional_string(options, "cwd");
    let timeout_ms = optional_u64(options, "timeout_ms").unwrap_or(DEFAULT_HOST_API_TIMEOUT_MS);
    let max_output_bytes =
        optional_u64(options, "max_output_bytes").unwrap_or(DEFAULT_OUTPUT_LIMIT_BYTES as u64);
    match active
        .tool_executor
        .process_executor()
        .shell_exec(
            &command,
            &workspace_root,
            cwd.as_deref(),
            Some(timeout_ms),
            &env,
        )
        .await
    {
        Ok(result) => Ok(process_result_value(
            result,
            false,
            max_output_bytes as usize,
        )),
        Err(ToolError::Timeout(_)) => Ok(timed_out_process_result()),
        Err(error) => Err(host_api_tool_error(error)),
    }
}

pub(super) async fn resolve_process_exec(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(
        active,
        params,
        &[EXTENSION_PERMISSION_PROCESS_EXECUTE.to_string()],
    )?;
    let command = require_string(params, "command")?;
    let args = parse_args(params)?;
    let options = params.get("options").unwrap_or(&Value::Null);
    let env = parse_env_overlay(options)?;
    require_env_overlay_permissions(active, params, &env)?;
    let workspace_root = resolve_workspace_root_from_options(active, options)?;
    let timeout_ms = optional_u64(options, "timeout_ms").unwrap_or(DEFAULT_HOST_API_TIMEOUT_MS);
    let max_output_bytes =
        optional_u64(options, "max_output_bytes").unwrap_or(DEFAULT_OUTPUT_LIMIT_BYTES as u64);
    match active
        .tool_executor
        .process_executor()
        .exec(
            &command,
            &args,
            &workspace_root,
            optional_string(options, "cwd").as_deref(),
            Some(timeout_ms),
            &env,
        )
        .await
    {
        Ok(result) => Ok(process_result_value(
            result,
            false,
            max_output_bytes as usize,
        )),
        Err(ToolError::Timeout(_)) => Ok(timed_out_process_result()),
        Err(error) => Err(host_api_tool_error(error)),
    }
}

fn parse_args(params: &Value) -> Result<Vec<String>, LocalExtensionHostError> {
    let Some(value) = params.get("args") else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(host_param_error("`args` 必须是字符串数组"));
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| host_param_error(&format!("`args[{index}]` 必须是字符串")))
        })
        .collect()
}

fn parse_env_overlay(options: &Value) -> Result<ProcessEnvOverlay, LocalExtensionHostError> {
    let Some(value) = options.get("env") else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let Some(values) = value.as_object() else {
        return Err(host_param_error("`options.env` 必须是对象"));
    };
    values
        .iter()
        .map(|(key, value)| {
            if key.trim().is_empty() {
                return Err(host_param_error("`options.env` 包含空变量名"));
            }
            value
                .as_str()
                .map(|value| (key.to_string(), value.to_string()))
                .ok_or_else(|| host_param_error(&format!("`options.env.{key}` 的值必须是字符串")))
        })
        .collect()
}

fn require_env_overlay_permissions(
    active: &ActiveExtension,
    params: &Value,
    env: &ProcessEnvOverlay,
) -> Result<(), LocalExtensionHostError> {
    for (key, _) in env {
        let permissions = vec!["env.read".to_string(), format!("env.read:{key}")];
        require_declared_permission(active, params, &permissions)?;
    }
    Ok(())
}

fn host_param_error(message: &str) -> LocalExtensionHostError {
    LocalExtensionHostError::Host(format!("host api 参数 {message}"))
}

fn timed_out_process_result() -> Value {
    json!({
        "exit_code": -1,
        "stdout": "",
        "stderr": "",
        "timed_out": true,
        "truncated": false,
    })
}
