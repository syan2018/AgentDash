use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::LocalExtensionHostError;
use super::host_api::{
    host_api_tool_error, optional_string, require_string, resolve_workspace_root,
};
use super::permission_guard::require_declared_permission;
use super::process::ActiveExtension;

pub(super) async fn resolve_workspace_read_text(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.read".to_string()])?;
    let path = require_string(params, "path")?;
    let workspace_root = resolve_workspace_root(active, params)?;
    let content = active
        .tool_executor
        .file_read(&path, &workspace_root)
        .await
        .map_err(host_api_tool_error)?;
    Ok(json!(content))
}

pub(super) async fn resolve_workspace_write_text(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.write".to_string()])?;
    let path = require_string(params, "path")?;
    let content = require_string(params, "content")?;
    let workspace_root = resolve_workspace_root(active, params)?;
    active
        .tool_executor
        .file_write(&path, &content, &workspace_root)
        .await
        .map_err(host_api_tool_error)?;
    Ok(Value::Null)
}

pub(super) async fn resolve_workspace_list(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.list".to_string()])?;
    let path = optional_string(params, "path").unwrap_or_else(|| ".".to_string());
    let workspace_root = resolve_workspace_root(active, params)?;
    let entries = active
        .tool_executor
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

pub(super) async fn resolve_workspace_stat(
    active: &ActiveExtension,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    require_declared_permission(active, params, &["workspace.vfs.read".to_string()])?;
    let path = require_string(params, "path")?;
    let workspace_root = resolve_workspace_root(active, params)?;
    let full_path = active
        .tool_executor
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
