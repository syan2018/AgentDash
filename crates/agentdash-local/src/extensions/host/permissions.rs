use agentdash_domain::shared_library::EXTENSION_PERMISSION_LOCAL_PROFILE_READ;
use serde_json::Value;

use super::LocalExtensionHostError;
use super::process::ActiveExtension;

pub(super) fn resolve_host_api(
    active: Option<&ActiveExtension>,
    method: &str,
    params: &Value,
) -> Result<Value, LocalExtensionHostError> {
    let active =
        active.ok_or_else(|| LocalExtensionHostError::Host("extension 尚未激活".into()))?;
    match method {
        "local.get_profile" => {
            let action_key = params
                .get("action_key")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty());
            let Some(action_key) = action_key else {
                return Err(LocalExtensionHostError::PermissionDenied(
                    "local.profile.read 缺少 action context".to_string(),
                ));
            };
            let decision = active
                .manifest
                .evaluate_action_permission(action_key, EXTENSION_PERMISSION_LOCAL_PROFILE_READ);
            if !decision.allowed {
                return Err(LocalExtensionHostError::PermissionDenied(
                    decision.denial_message(),
                ));
            }
            serde_json::to_value(&active.profile).map_err(LocalExtensionHostError::from)
        }
        other => Err(LocalExtensionHostError::PermissionDenied(format!(
            "未知 host api: {other}"
        ))),
    }
}
