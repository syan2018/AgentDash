use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use agentdash_domain::shared_library::{
    EXTENSION_PERMISSION_LOCAL_PROFILE_READ, EXTENSION_PERMISSION_PROCESS_EXECUTE,
    ExtensionPermissionDecision, ExtensionPermissionDecisionReason,
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
        "runtime.invoke" => resolve_runtime_invoke(active, params),
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

fn resolve_runtime_invoke(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    let action_key = require_string(params, "target_action_key")?;
    require_declared_permission(
        active,
        params,
        &[
            "runtime.invoke".to_string(),
            format!("runtime.invoke:{action_key}"),
        ],
    )?;
    Err(LocalExtensionHostError::Host(format!(
        "runtime.invoke 目标 action `{action_key}` 未在当前 Project extension host 预加载"
    )))
}

async fn resolve_workspace_read_text(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.read".to_string()])?;
    let path = require_string(params, "path")?;
    let workspace_root = resolve_workspace_root(active, params)?;
    let executor = ToolExecutor::new(executor_workspace_roots(active));
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
    let executor = ToolExecutor::new(executor_workspace_roots(active));
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
    let executor = ToolExecutor::new(executor_workspace_roots(active));
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
    let executor = ToolExecutor::new(executor_workspace_roots(active));
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
    let executor = ToolExecutor::new(executor_workspace_roots(active));
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
    let executor = ToolExecutor::new(executor_workspace_roots(active));
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
        let decisions = permissions
            .iter()
            .map(|permission| {
                active
                    .manifest
                    .evaluate_action_permission(&action_key, permission)
            })
            .collect::<Vec<_>>();
        if decisions.iter().any(|decision| decision.allowed) {
            return Ok(());
        }
        return Err(LocalExtensionHostError::PermissionDenied(
            action_permission_denial_message(&action_key, permissions, &decisions).to_string(),
        ));
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

fn action_permission_denial_message(
    action_key: &str,
    permissions: &[String],
    decisions: &[ExtensionPermissionDecision],
) -> String {
    if decisions.iter().all(|decision| {
        decision.reason == ExtensionPermissionDecisionReason::MissingActionDeclaration
    }) {
        return format!(
            "extension action `{action_key}` 未声明 {}",
            permissions.join(" 或 ")
        );
    }
    decisions
        .first()
        .map(ExtensionPermissionDecision::denial_message)
        .unwrap_or_else(|| format!("extension action `{action_key}` 未声明权限"))
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
    optional_string(params, "workspace_root").map_or_else(|| default_workspace_root(active), Ok)
}

fn resolve_workspace_root_from_options(
    active: &ActiveExtension,
    options: &Value,
) -> Result<String, LocalExtensionHostError> {
    optional_string(options, "workspace_root").map_or_else(|| default_workspace_root(active), Ok)
}

fn default_workspace_root(active: &ActiveExtension) -> Result<String, LocalExtensionHostError> {
    active
        .default_workspace_root
        .as_ref()
        .map(|root| root.to_string_lossy().to_string())
        .ok_or_else(|| {
            LocalExtensionHostError::Host("extension host 未绑定 session workspace root".into())
        })
}

fn executor_workspace_roots(active: &ActiveExtension) -> Vec<PathBuf> {
    let mut roots = active.workspace_roots.clone();
    if let Some(default_root) = active.default_workspace_root.as_ref()
        && !roots.iter().any(|root| root == default_root)
    {
        roots.push(default_root.clone());
    }
    roots
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionProtocolChannelDefinition, ExtensionProtocolChannelMethodDefinition,
        ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::extensions::host::{LocalExtensionHostProfile, LocalExtensionHostWorkspaceRoot};

    #[tokio::test]
    async fn host_api_permission_and_param_errors_are_diagnostic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let active = active_extension(
            temp.path(),
            &[
                "local.profile.read",
                "env.read:PATH",
                "runtime.invoke:provider.echo",
            ],
            &["env.read:PATH"],
        );

        let profile = resolve_host_api(
            Some(&active),
            "local.get_profile",
            &action_params(json!({})),
        )
        .await
        .expect("profile");
        assert_eq!(profile["backend_id"], "backend-1");

        let env_value = resolve_host_api(
            Some(&active),
            "env.get",
            &action_params(json!({ "name": "PATH" })),
        )
        .await
        .expect("env");
        assert!(env_value.is_string() || env_value.is_null());

        let channel_env = resolve_host_api(
            Some(&active),
            "env.get",
            &channel_params(json!({ "name": "PATH" })),
        )
        .await
        .expect("channel env");
        assert!(channel_env.is_string() || channel_env.is_null());

        let missing_name = resolve_host_api(Some(&active), "env.get", &action_params(json!({})))
            .await
            .expect_err("missing env name");
        assert_contains(&missing_name, "host api 参数 `name` 不能为空");

        let denied = resolve_host_api(
            Some(&active_extension(temp.path(), &[], &[])),
            "env.get",
            &action_params(json!({ "name": "PATH" })),
        )
        .await
        .expect_err("env denied");
        assert_contains(
            &denied,
            "extension action `local-hello.profile` 未声明 env.read 或 env.read:PATH",
        );

        let runtime = resolve_host_api(
            Some(&active),
            "runtime.invoke",
            &action_params(json!({ "target_action_key": "provider.echo" })),
        )
        .await
        .expect_err("runtime not preloaded");
        assert_contains(
            &runtime,
            "runtime.invoke 目标 action `provider.echo` 未在当前 Project extension host 预加载",
        );

        let channel_route = resolve_host_api(
            Some(&active),
            "extension.channel_invoke",
            &action_params(json!({ "channel_key": "provider.api", "method": "echo" })),
        )
        .await
        .expect_err("channel route");
        assert_contains(
            &channel_route,
            "extension channel provider routing 尚未接入 Project registry",
        );

        let unknown = resolve_host_api(Some(&active), "unknown.api", &action_params(json!({})))
            .await
            .expect_err("unknown");
        assert_contains(&unknown, "未知 host api: unknown.api");
    }

    #[tokio::test]
    async fn workspace_host_apis_cover_allowed_denied_and_param_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let active = active_extension(
            temp.path(),
            &[
                "workspace.vfs.write",
                "workspace.vfs.read",
                "workspace.vfs.list",
            ],
            &[],
        );

        resolve_host_api(
            Some(&active),
            "workspace.write_text",
            &action_params(json!({
                "path": "notes/hello.txt",
                "content": "hello",
            })),
        )
        .await
        .expect("write");

        let text = resolve_host_api(
            Some(&active),
            "workspace.read_text",
            &action_params(json!({ "path": "notes/hello.txt" })),
        )
        .await
        .expect("read");
        assert_eq!(text, "hello");

        let entries = resolve_host_api(
            Some(&active),
            "workspace.list",
            &action_params(json!({ "path": "notes" })),
        )
        .await
        .expect("list");
        assert_eq!(entries[0]["path"], "notes/hello.txt");

        let stat = resolve_host_api(
            Some(&active),
            "workspace.stat",
            &action_params(json!({ "path": "notes/hello.txt" })),
        )
        .await
        .expect("stat");
        assert_eq!(stat["kind"], "file");

        let denied = resolve_host_api(
            Some(&active_extension(temp.path(), &[], &[])),
            "workspace.read_text",
            &action_params(json!({ "path": "notes/hello.txt" })),
        )
        .await
        .expect_err("workspace denied");
        assert_contains(
            &denied,
            "extension action `local-hello.profile` 未声明 workspace.vfs.read",
        );

        let missing_path = resolve_host_api(
            Some(&active),
            "workspace.read_text",
            &action_params(json!({})),
        )
        .await
        .expect_err("missing path");
        assert_contains(&missing_path, "host api 参数 `path` 不能为空");
    }

    #[tokio::test]
    async fn process_host_apis_cover_allowed_denied_and_param_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let active = active_extension(temp.path(), &["process.execute"], &[]);

        let shell = resolve_host_api(
            Some(&active),
            "process.shell",
            &action_params(json!({
                "command": "node -e \"console.log('shell-ok')\"",
                "options": { "timeout_ms": 5000, "max_output_bytes": 1024 },
            })),
        )
        .await
        .expect("shell");
        assert_eq!(shell["exit_code"], 0);
        assert!(
            shell["stdout"]
                .as_str()
                .unwrap_or_default()
                .contains("shell-ok")
        );

        let exec = resolve_host_api(
            Some(&active),
            "process.exec",
            &action_params(json!({
                "command": "node",
                "args": ["-e", "console.log('exec-ok')"],
                "options": { "timeout_ms": 5000, "max_output_bytes": 1024 },
            })),
        )
        .await
        .expect("exec");
        assert_eq!(exec["exit_code"], 0);
        assert!(
            exec["stdout"]
                .as_str()
                .unwrap_or_default()
                .contains("exec-ok")
        );

        let denied = resolve_host_api(
            Some(&active_extension(temp.path(), &[], &[])),
            "process.exec",
            &action_params(json!({ "command": "node" })),
        )
        .await
        .expect_err("process denied");
        assert_contains(
            &denied,
            "extension action `local-hello.profile` 未声明 process.execute",
        );

        let missing_command =
            resolve_host_api(Some(&active), "process.exec", &action_params(json!({})))
                .await
                .expect_err("missing command");
        assert_contains(&missing_command, "host api 参数 `command` 不能为空");
    }

    #[tokio::test]
    async fn workspace_host_apis_use_session_root_without_registered_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let active = active_extension_with_roots(
            Some(temp.path()),
            Vec::new(),
            &["workspace.vfs.write", "workspace.vfs.read"],
            &[],
        );

        resolve_host_api(
            Some(&active),
            "workspace.write_text",
            &action_params(json!({
                "path": "notes/session-root.txt",
                "content": "session-root",
            })),
        )
        .await
        .expect("write through session root");

        let text = resolve_host_api(
            Some(&active),
            "workspace.read_text",
            &action_params(json!({ "path": "notes/session-root.txt" })),
        )
        .await
        .expect("read through session root");

        assert_eq!(text, "session-root");
    }

    #[tokio::test]
    async fn workspace_host_apis_do_not_fallback_to_registered_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let active = active_extension_with_roots(
            None,
            vec![temp.path().to_path_buf()],
            &["workspace.vfs.list"],
            &[],
        );

        let error = resolve_host_api(
            Some(&active),
            "workspace.list",
            &action_params(json!({ "path": "." })),
        )
        .await
        .expect_err("missing session root");

        assert_contains(&error, "extension host 未绑定 session workspace root");
    }

    #[tokio::test]
    async fn http_host_api_covers_allowed_denied_and_param_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (url, server) = http_ok_server().await;
        let active = active_extension(temp.path(), &["http.fetch:127.0.0.1"], &[]);

        let response = resolve_host_api(
            Some(&active),
            "http.fetch",
            &action_params(json!({ "url": url })),
        )
        .await
        .expect("http fetch");
        server.await.expect("server task");
        assert_eq!(response["status"], 200);
        assert_eq!(response["body"], "ok");

        let denied = resolve_host_api(
            Some(&active_extension(temp.path(), &[], &[])),
            "http.fetch",
            &action_params(json!({ "url": "https://example.com" })),
        )
        .await
        .expect_err("http denied");
        assert_contains(
            &denied,
            "extension action `local-hello.profile` 未声明 http.fetch 或 http.fetch:example.com",
        );

        let invalid_scheme = resolve_host_api(
            Some(&active),
            "http.fetch",
            &action_params(json!({ "url": "file:///tmp/demo" })),
        )
        .await
        .expect_err("invalid scheme");
        assert_contains(&invalid_scheme, "http.fetch 不支持 URL scheme: file");

        let missing_url = resolve_host_api(Some(&active), "http.fetch", &action_params(json!({})))
            .await
            .expect_err("missing url");
        assert_contains(&missing_url, "host api 参数 `url` 不能为空");
    }

    fn action_params(mut params: Value) -> Value {
        params
            .as_object_mut()
            .expect("params object")
            .insert("action_key".to_string(), json!("local-hello.profile"));
        params
    }

    fn channel_params(mut params: Value) -> Value {
        let object = params.as_object_mut().expect("params object");
        object.insert("channel_key".to_string(), json!("local-hello.api"));
        object.insert("channel_method".to_string(), json!("readEnv"));
        params
    }

    async fn http_ok_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .expect("write response");
        });
        (format!("http://{addr}/demo"), handle)
    }

    fn active_extension(
        workspace_root: &Path,
        action_permissions: &[&str],
        channel_permissions: &[&str],
    ) -> ActiveExtension {
        active_extension_with_roots(
            Some(workspace_root),
            vec![workspace_root.to_path_buf()],
            action_permissions,
            channel_permissions,
        )
    }

    fn active_extension_with_roots(
        default_workspace_root: Option<&Path>,
        workspace_roots: Vec<PathBuf>,
        action_permissions: &[&str],
        channel_permissions: &[&str],
    ) -> ActiveExtension {
        let profile_workspace_roots = workspace_roots
            .iter()
            .enumerate()
            .map(|(index, root)| LocalExtensionHostWorkspaceRoot {
                index,
                name: root
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("workspace-{index}")),
                display_path: root.display().to_string(),
            })
            .collect();
        ActiveExtension {
            extension_key: "local-hello".to_string(),
            manifest: manifest(action_permissions, channel_permissions),
            profile: LocalExtensionHostProfile {
                username: "user".to_string(),
                platform: "windows".to_string(),
                arch: "x64".to_string(),
                backend_id: "backend-1".to_string(),
                project_id: Some("project-1".to_string()),
                session_id: Some("session-1".to_string()),
                workspace_roots: profile_workspace_roots,
            },
            default_workspace_root: default_workspace_root.map(Path::to_path_buf),
            workspace_roots,
        }
    }

    fn manifest(
        action_permissions: &[&str],
        channel_permissions: &[&str],
    ) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "local-hello".to_string(),
            package: ExtensionPackageMetadata {
                name: "@agentdash/local-hello".to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: "local-hello.profile".to_string(),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "Profile".to_string(),
                input_schema: json!(true),
                output_schema: json!(true),
                permissions: action_permissions
                    .iter()
                    .map(|item| item.to_string())
                    .collect(),
            }],
            protocol_channels: vec![ExtensionProtocolChannelDefinition {
                channel_key: "local-hello.api".to_string(),
                version: "1.0.0".to_string(),
                description: "Local API".to_string(),
                methods: vec![ExtensionProtocolChannelMethodDefinition {
                    name: "readEnv".to_string(),
                    description: "Read env".to_string(),
                    input_schema: json!(true),
                    output_schema: json!(true),
                    permissions: channel_permissions
                        .iter()
                        .map(|item| item.to_string())
                        .collect(),
                }],
            }],
            extension_dependencies: vec![],
            workspace_tabs: vec![],
            permissions: vec![],
            bundles: vec![],
        }
    }

    fn assert_contains(error: &LocalExtensionHostError, expected: &str) {
        let message = error.to_string();
        assert!(
            message.contains(expected),
            "expected `{message}` to contain `{expected}`"
        );
    }
}
