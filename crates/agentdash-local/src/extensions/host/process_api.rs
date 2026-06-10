use std::process::Stdio;
use std::time::Duration;

use agentdash_domain::shared_library::EXTENSION_PERMISSION_PROCESS_EXECUTE;
use serde_json::{Value, json};
use tokio::process::Command;

use crate::tool_executor::{ShellResult, ToolError};

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
    let workspace_root = resolve_workspace_root_from_options(active, options)?;
    let cwd = optional_string(options, "cwd");
    let timeout_ms = optional_u64(options, "timeout_ms").unwrap_or(DEFAULT_HOST_API_TIMEOUT_MS);
    let max_output_bytes =
        optional_u64(options, "max_output_bytes").unwrap_or(DEFAULT_OUTPUT_LIMIT_BYTES as u64);
    match active
        .tool_executor
        .shell_exec(&command, &workspace_root, cwd.as_deref(), Some(timeout_ms))
        .await
    {
        Ok(result) => Ok(process_result_value(
            result,
            false,
            max_output_bytes as usize,
        )),
        Err(ToolError::Timeout(_)) => Ok(json!({
            "exit_code": -1,
            "stdout": "",
            "stderr": "",
            "timed_out": true,
            "truncated": false,
        })),
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
    let args = params
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let options = params.get("options").unwrap_or(&Value::Null);
    let workspace_root = resolve_workspace_root_from_options(active, options)?;
    let timeout_ms = optional_u64(options, "timeout_ms").unwrap_or(DEFAULT_HOST_API_TIMEOUT_MS);
    let max_output_bytes =
        optional_u64(options, "max_output_bytes").unwrap_or(DEFAULT_OUTPUT_LIMIT_BYTES as u64);
    let cwd = active
        .tool_executor
        .resolve_shell_cwd(&workspace_root, optional_string(options, "cwd").as_deref())
        .map_err(host_api_tool_error)?;
    let mut child = Command::new(&command);
    child
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(env) = options.get("env").and_then(Value::as_object) {
        for (key, value) in env {
            if let Some(value) = value.as_str() {
                child.env(key, value);
            }
        }
    }
    match tokio::time::timeout(Duration::from_millis(timeout_ms), child.output()).await {
        Ok(Ok(output)) => Ok(process_result_value(
            ShellResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            },
            false,
            max_output_bytes as usize,
        )),
        Ok(Err(error)) => Err(LocalExtensionHostError::Io(error)),
        Err(_) => Ok(json!({
            "exit_code": -1,
            "stdout": "",
            "stderr": "",
            "timed_out": true,
            "truncated": false,
        })),
    }
}
