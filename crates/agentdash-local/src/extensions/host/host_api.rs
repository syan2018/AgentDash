use agentdash_domain::shared_library::EXTENSION_PERMISSION_LOCAL_PROFILE_READ;
use serde_json::{Value, json};

use crate::tool_executor::{ShellResult, ToolError};

use super::LocalExtensionHostError;
use super::permission_guard::require_declared_permission;
use super::process::ActiveExtension;
use super::{http_api, process_api, workspace_api};

pub(super) const DEFAULT_HOST_API_TIMEOUT_MS: u64 = 30_000;
pub(super) const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;

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
        "workspace.read_text" => workspace_api::resolve_workspace_read_text(active, params).await,
        "workspace.write_text" => workspace_api::resolve_workspace_write_text(active, params).await,
        "workspace.list" => workspace_api::resolve_workspace_list(active, params).await,
        "workspace.stat" => workspace_api::resolve_workspace_stat(active, params).await,
        "process.shell" => process_api::resolve_process_shell(active, params).await,
        "process.exec" => process_api::resolve_process_exec(active, params).await,
        "http.fetch" => http_api::resolve_http_fetch(active, params).await,
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

pub(super) fn require_string(
    params: &Value,
    field: &str,
) -> Result<String, LocalExtensionHostError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| LocalExtensionHostError::Host(format!("host api 参数 `{field}` 不能为空")))
}

pub(super) fn optional_string(params: &Value, field: &str) -> Option<String> {
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn optional_u64(params: &Value, field: &str) -> Option<u64> {
    params.get(field).and_then(Value::as_u64)
}

pub(super) fn resolve_workspace_root(
    active: &ActiveExtension,
    params: &Value,
) -> Result<String, LocalExtensionHostError> {
    optional_string(params, "workspace_root").map_or_else(|| default_workspace_root(active), Ok)
}

pub(super) fn resolve_workspace_root_from_options(
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

pub(super) fn process_result_value(
    result: ShellResult,
    timed_out: bool,
    max_output_bytes: usize,
) -> Value {
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

pub(super) fn host_api_tool_error(error: ToolError) -> LocalExtensionHostError {
    LocalExtensionHostError::Host(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionProtocolChannelDefinition, ExtensionProtocolChannelMethodDefinition,
        ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::extensions::host::{LocalExtensionHostProfile, LocalExtensionHostWorkspaceRoot};
    use crate::tool_executor::ToolExecutor;

    #[tokio::test]
    async fn host_api_permission_and_param_errors_are_diagnostic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let active = active_extension(
            temp.path(),
            &["local.profile.read", "env.read:PATH"],
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
        let mut executor_roots = workspace_roots.clone();
        if let Some(default_root) = default_workspace_root
            && !executor_roots.iter().any(|root| root == default_root)
        {
            executor_roots.push(default_root.to_path_buf());
        }
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
            tool_executor: ToolExecutor::new(executor_roots),
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
