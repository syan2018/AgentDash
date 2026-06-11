use agentdash_domain::shared_library::{
    ExtensionPermissionDecision, ExtensionPermissionDecisionReason,
};
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

fn optional_string(params: &Value, field: &str) -> Option<String> {
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
