use serde_json::Value;

use super::LocalExtensionHostError;
use super::process::ActiveExtension;

pub(super) fn require_declared_permission(
    active: &ActiveExtension,
    params: &Value,
    permissions: &[String],
) -> Result<(), LocalExtensionHostError> {
    let requested = permissions
        .first()
        .map(String::as_str)
        .unwrap_or("unknown.permission");
    if let Some(action_key) = optional_string(params, "action_key") {
        let action = active
            .manifest
            .runtime_actions
            .iter()
            .find(|action| action.action_key == action_key);
        let Some(action) = action else {
            return Err(LocalExtensionHostError::PermissionDenied(format!(
                "extension action `{action_key}` 不存在"
            )));
        };
        let unknown = permissions
            .iter()
            .find(|permission| !is_known_extension_permission_key(permission));
        if let Some(permission) = unknown {
            return Err(LocalExtensionHostError::PermissionDenied(format!(
                "extension action 声明了未知权限: {permission}"
            )));
        }
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

    if let (Some(protocol_key), Some(protocol_method)) = (
        optional_string(params, "protocol_key"),
        optional_string(params, "protocol_method"),
    ) {
        let Some(channel) = active
            .manifest
            .protocols
            .iter()
            .find(|channel| channel.protocol_key == protocol_key)
        else {
            return Err(LocalExtensionHostError::PermissionDenied(format!(
                "extension protocol `{protocol_key}` 不存在"
            )));
        };
        let Some(method) = channel
            .methods
            .iter()
            .find(|method| method.name == protocol_method)
        else {
            return Err(LocalExtensionHostError::PermissionDenied(format!(
                "extension protocol method `{protocol_key}.{protocol_method}` 不存在"
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
            "extension protocol method `{protocol_key}.{protocol_method}` 未声明 {}",
            permissions.join(" 或 ")
        )));
    }

    Err(LocalExtensionHostError::PermissionDenied(format!(
        "{requested} 缺少 action 或 protocol invocation context"
    )))
}

fn optional_string(params: &Value, field: &str) -> Option<String> {
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn is_known_extension_permission_key(permission: &str) -> bool {
    permission == "local.profile.read"
        || permission == "http.fetch"
        || permission.starts_with("http.fetch:")
        || permission.starts_with("workspace.vfs.")
        || permission == "env.read"
        || permission.starts_with("env.read:")
        || permission == "process.exec"
        || permission == "process.shell"
        || permission == "process.env.set"
        || permission.starts_with("process.env.set:")
        || permission == "runtime.invoke"
        || permission.starts_with("runtime.invoke:")
        || permission == "extension.protocol.invoke"
        || permission.starts_with("extension.protocol.invoke:")
}
