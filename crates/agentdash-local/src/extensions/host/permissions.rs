use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use agentdash_domain::shared_library::{
    EXTENSION_PERMISSION_LOCAL_PROFILE_READ, EXTENSION_PERMISSION_PROCESS_EXECUTE,
};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value, json};
use tokio::process::Command;

use crate::tool_executor::{ShellResult, ToolError, ToolExecutor};

use super::LocalExtensionHostError;
use super::process::ActiveExtension;

const DEFAULT_HOST_API_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;

pub(super) async fn resolve_host_api(
    active: Option<&ActiveExtension>,
    method: &str,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    let active =
        active.ok_or_else(|| LocalExtensionHostError::Host("extension 尚未激活".into()))?;
    match method {
        "local.get_profile" => resolve_local_profile(active, params),
        "env.get" => resolve_env_get(active, params),
        "workspace.read_text" => resolve_workspace_read_text(active, params).await,
        "workspace.write_text" => resolve_workspace_write_text(active, params).await,
        "workspace.list" => resolve_workspace_list(active, params).await,
        "workspace.stat" => resolve_workspace_stat(active, params).await,
        "process.shell" => resolve_process_shell(active, params).await,
        "process.exec" => resolve_process_exec(active, params).await,
        "http.fetch" => resolve_http_fetch(active, params).await,
        "extension.channel_invoke" => Err(LocalExtensionHostError::Host(
            "extension channel provider routing 尚未接入 Project registry".to_string(),
        )),
        other => Err(LocalExtensionHostError::Host(format!(
            "未知 host api: {other}"
        ))),
    }
}

fn resolve_local_profile(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(
        active,
        params,
        &[EXTENSION_PERMISSION_LOCAL_PROFILE_READ.to_string()],
    )?;
    serde_json::to_value(&active.profile).map_err(LocalExtensionHostError::from)
}

fn resolve_env_get(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    let name = require_string(params, "name")?;
    let permissions = vec!["env.read".to_string(), format!("env.read:{name}")];
    require_declared_permission(active, params, &permissions)?;
    let value = std::env::var(&name).ok();
    Ok(json!(value))
}

async fn resolve_workspace_read_text(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.read".to_string()])?;
    let path = require_string(params, "path")?;
    let workspace_root = resolve_workspace_root(active, params)?;
    let executor = ToolExecutor::new(active.workspace_roots.clone());
    let content = executor
        .file_read(&path, &workspace_root)
        .await
        .map_err(host_api_tool_error)?;
    Ok(json!(content))
}

async fn resolve_workspace_write_text(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.write".to_string()])?;
    let path = require_string(params, "path")?;
    let content = require_string(params, "content")?;
    let workspace_root = resolve_workspace_root(active, params)?;
    let executor = ToolExecutor::new(active.workspace_roots.clone());
    executor
        .file_write(&path, &content, &workspace_root)
        .await
        .map_err(host_api_tool_error)?;
    Ok(Value::Null)
}

async fn resolve_workspace_list(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.list".to_string()])?;
    let path = optional_string(params, "path").unwrap_or_else(|| ".".to_string());
    let workspace_root = resolve_workspace_root(active, params)?;
    let executor = ToolExecutor::new(active.workspace_roots.clone());
    let entries = executor
        .file_list(&path, &workspace_root, None, false)
        .await
        .map_err(host_api_tool_error)?
        .into_iter()
        .map(|entry| {
            json!({
                "path": entry.path,
                "kind": if entry.is_dir { "directory" } else { "file" },
            })
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(entries))
}

async fn resolve_workspace_stat(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.read".to_string()])?;
    let path = require_string(params, "path")?;
    let workspace_root = resolve_workspace_root(active, params)?;
    let executor = ToolExecutor::new(active.workspace_roots.clone());
    let full_path = executor
        .resolve_existing_path(&path, &workspace_root)
        .map_err(host_api_tool_error)?;
    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(LocalExtensionHostError::from)?;
    let modified_at = metadata
        .modified()
        .ok()
        .map(|value| DateTime::<Utc>::from(value).to_rfc3339());
    Ok(json!({
        "path": path,
        "kind": if metadata.is_dir() { "directory" } else { "file" },
        "size": metadata.len(),
        "modified_at": modified_at,
    }))
}

async fn resolve_process_shell(
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
    let executor = ToolExecutor::new(active.workspace_roots.clone());
    match executor
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

async fn resolve_process_exec(
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
    let executor = ToolExecutor::new(active.workspace_roots.clone());
    let cwd = executor
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

async fn resolve_http_fetch(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    let url = require_string(params, "url")?;
    let parsed = reqwest::Url::parse(&url)
        .map_err(|error| LocalExtensionHostError::Host(format!("http.fetch URL 非法: {error}")))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(LocalExtensionHostError::Host(format!(
                "http.fetch 不支持 URL scheme: {scheme}"
            )));
        }
    }
    let host = parsed.host_str().unwrap_or_default().to_string();
    let permissions = vec!["http.fetch".to_string(), format!("http.fetch:{host}")];
    require_declared_permission(active, params, &permissions)?;

    let options = params.get("options").unwrap_or(&Value::Null);
    let method = optional_string(options, "method").unwrap_or_else(|| "GET".to_string());
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|error| {
        LocalExtensionHostError::Host(format!("http.fetch method 非法: {error}"))
    })?;
    let timeout_ms = optional_u64(options, "timeout_ms").unwrap_or(DEFAULT_HOST_API_TIMEOUT_MS);
    let mut request = reqwest::Client::new().request(method, parsed);
    if let Some(headers) = options.get("headers").and_then(Value::as_object) {
        request = request.headers(parse_headers(headers)?);
    }
    if let Some(body) = options.get("body") {
        request = if let Some(text) = body.as_str() {
            request.body(text.to_string())
        } else {
            request.body(body.to_string())
        };
    }
    let response = tokio::time::timeout(Duration::from_millis(timeout_ms), request.send())
        .await
        .map_err(|_| LocalExtensionHostError::Host("http.fetch timeout".to_string()))?
        .map_err(|error| LocalExtensionHostError::Host(format!("http.fetch 失败: {error}")))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| {
            (
                key.as_str().to_string(),
                Value::String(value.to_str().unwrap_or_default().to_string()),
            )
        })
        .collect::<Map<String, Value>>();
    let body = response.text().await.map_err(|error| {
        LocalExtensionHostError::Host(format!("http.fetch 读取响应失败: {error}"))
    })?;
    Ok(json!({
        "status": status,
        "headers": headers,
        "body": body,
    }))
}

fn require_declared_permission(
    active: &ActiveExtension,
    params: &Value,
    permissions: &[String],
) -> Result<(), LocalExtensionHostError> {
    let requested = permissions
        .first()
        .map(String::as_str)
        .unwrap_or("unknown.permission");
    if let Some(action_key) = optional_string(params, "action_key") {
        let Some(action) = active
            .manifest
            .runtime_actions
            .iter()
            .find(|action| action.action_key == action_key)
        else {
            return Err(LocalExtensionHostError::PermissionDenied(format!(
                "extension action `{action_key}` 不存在"
            )));
        };
        if permissions.iter().any(|permission| {
            action
                .permissions
                .iter()
                .any(|declared| declared == permission)
        }) {
            return Ok(());
        }
        return Err(LocalExtensionHostError::PermissionDenied(format!(
            "extension action `{action_key}` 未声明 {}",
            permissions.join(" 或 ")
        )));
    }

    if let (Some(channel_key), Some(channel_method)) = (
        optional_string(params, "channel_key"),
        optional_string(params, "channel_method"),
    ) {
        let Some(channel) = active
            .manifest
            .protocol_channels
            .iter()
            .find(|channel| channel.channel_key == channel_key)
        else {
            return Err(LocalExtensionHostError::PermissionDenied(format!(
                "extension channel `{channel_key}` 不存在"
            )));
        };
        let Some(method) = channel
            .methods
            .iter()
            .find(|method| method.name == channel_method)
        else {
            return Err(LocalExtensionHostError::PermissionDenied(format!(
                "extension channel method `{channel_key}.{channel_method}` 不存在"
            )));
        };
        if permissions.iter().any(|permission| {
            method
                .permissions
                .iter()
                .any(|declared| declared == permission)
        }) {
            return Ok(());
        }
        return Err(LocalExtensionHostError::PermissionDenied(format!(
            "extension channel method `{channel_key}.{channel_method}` 未声明 {}",
            permissions.join(" 或 ")
        )));
    }

    Err(LocalExtensionHostError::PermissionDenied(format!(
        "{requested} 缺少 action 或 channel invocation context"
    )))
}

fn require_string(params: &Value, field: &str) -> Result<String, LocalExtensionHostError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| LocalExtensionHostError::Host(format!("host api 参数 `{field}` 不能为空")))
}

fn optional_string(params: &Value, field: &str) -> Option<String> {
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_u64(params: &Value, field: &str) -> Option<u64> {
    params.get(field).and_then(Value::as_u64)
}

fn resolve_workspace_root(
    active: &ActiveExtension,
    params: &Value,
) -> Result<String, LocalExtensionHostError> {
    optional_string(params, "workspace_root")
        .map_or_else(|| default_workspace_root(&active.workspace_roots), Ok)
}

fn resolve_workspace_root_from_options(
    active: &ActiveExtension,
    options: &Value,
) -> Result<String, LocalExtensionHostError> {
    optional_string(options, "workspace_root")
        .map_or_else(|| default_workspace_root(&active.workspace_roots), Ok)
}

fn default_workspace_root(roots: &[PathBuf]) -> Result<String, LocalExtensionHostError> {
    roots
        .first()
        .map(|root| root.to_string_lossy().to_string())
        .ok_or_else(|| LocalExtensionHostError::Host("extension host 未绑定 workspace root".into()))
}

fn process_result_value(result: ShellResult, timed_out: bool, max_output_bytes: usize) -> Value {
    let (stdout, stdout_truncated) = truncate_to_limit(result.stdout, max_output_bytes);
    let (stderr, stderr_truncated) = truncate_to_limit(result.stderr, max_output_bytes);
    json!({
        "exit_code": result.exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "timed_out": timed_out,
        "truncated": stdout_truncated || stderr_truncated,
    })
}

fn truncate_to_limit(value: String, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value, false);
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), true)
}

fn host_api_tool_error(error: ToolError) -> LocalExtensionHostError {
    LocalExtensionHostError::Host(error.to_string())
}

fn parse_headers(headers: &Map<String, Value>) -> Result<HeaderMap, LocalExtensionHostError> {
    let mut map = HeaderMap::new();
    for (key, value) in headers {
        let Some(value) = value.as_str() else {
            return Err(LocalExtensionHostError::Host(format!(
                "http.fetch header `{key}` 必须是字符串"
            )));
        };
        let name = HeaderName::from_bytes(key.as_bytes()).map_err(|error| {
            LocalExtensionHostError::Host(format!("http.fetch header name 非法: {error}"))
        })?;
        let value = HeaderValue::from_str(value).map_err(|error| {
            LocalExtensionHostError::Host(format!("http.fetch header value 非法: {error}"))
        })?;
        map.insert(name, value);
    }
    Ok(map)
}
