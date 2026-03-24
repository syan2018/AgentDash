use std::env;

use agentdash_plugin_api::{
    AgentDashPlugin, AuthGroup, AuthIdentity, AuthMode, AuthProvider, AuthRequest,
};
use async_trait::async_trait;

const AUTH_MODE_ENV: &str = "AGENTDASH_AUTH_MODE";
const PERSONAL_USER_ID_ENV: &str = "AGENTDASH_PERSONAL_USER_ID";
const PERSONAL_SUBJECT_ENV: &str = "AGENTDASH_PERSONAL_SUBJECT";
const PERSONAL_DISPLAY_NAME_ENV: &str = "AGENTDASH_PERSONAL_DISPLAY_NAME";
const PERSONAL_EMAIL_ENV: &str = "AGENTDASH_PERSONAL_EMAIL";
const PERSONAL_GROUPS_ENV: &str = "AGENTDASH_PERSONAL_GROUPS";
const PERSONAL_IS_ADMIN_ENV: &str = "AGENTDASH_PERSONAL_IS_ADMIN";

/// 开源版默认个人模式认证插件。
///
/// 该插件让个人模式也走统一 `AuthProvider` 契约，避免在宿主里保留“绕过用户系统”的特殊路径。
pub struct PersonalAuthPlugin;

impl AgentDashPlugin for PersonalAuthPlugin {
    fn name(&self) -> &str {
        "builtin.personal_auth"
    }

    fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> {
        if configured_auth_mode() == AuthMode::Enterprise {
            return None;
        }

        Some(Box::new(BuiltinPersonalAuthProvider::from_env()))
    }
}

/// 开源版默认连接器目录插件骨架。
///
/// 当前连接器实际仍由宿主直接构建；该插件用于为 first-party plugin 目录预留位置，
/// 后续可以逐步把内置连接器迁移到真正的插件装配模型。
pub struct ConnectorCatalogPlugin;

impl AgentDashPlugin for ConnectorCatalogPlugin {
    fn name(&self) -> &str {
        "builtin.connector_catalog"
    }
}

pub fn builtin_plugins() -> Vec<Box<dyn AgentDashPlugin>> {
    vec![
        Box::new(PersonalAuthPlugin),
        Box::new(ConnectorCatalogPlugin),
    ]
}

#[derive(Clone)]
struct BuiltinPersonalAuthProvider {
    identity: AuthIdentity,
}

impl BuiltinPersonalAuthProvider {
    fn from_env() -> Self {
        let user_id = env::var(PERSONAL_USER_ID_ENV).unwrap_or_else(|_| "local-user".to_string());
        let subject = env::var(PERSONAL_SUBJECT_ENV).unwrap_or_else(|_| user_id.clone());
        let display_name = normalize_optional_env(PERSONAL_DISPLAY_NAME_ENV)
            .or_else(|| Some("Local User".to_string()));
        let email = normalize_optional_env(PERSONAL_EMAIL_ENV);
        let groups = parse_groups_env(PERSONAL_GROUPS_ENV);
        let is_admin = parse_bool_env(PERSONAL_IS_ADMIN_ENV);

        Self {
            identity: AuthIdentity {
                auth_mode: AuthMode::Personal,
                user_id,
                subject,
                display_name,
                email,
                groups,
                is_admin,
                provider: Some("builtin.personal".to_string()),
                extra: serde_json::Value::Null,
            },
        }
    }
}

#[async_trait]
impl AuthProvider for BuiltinPersonalAuthProvider {
    async fn authenticate(
        &self,
        _req: &AuthRequest,
    ) -> Result<AuthIdentity, agentdash_plugin_api::AuthError> {
        Ok(self.identity.clone())
    }

    async fn authorize(
        &self,
        _identity: &AuthIdentity,
        _resource: &str,
        _action: &str,
    ) -> Result<bool, agentdash_plugin_api::AuthError> {
        Ok(true)
    }
}

fn configured_auth_mode() -> AuthMode {
    env::var(AUTH_MODE_ENV)
        .ok()
        .and_then(|raw| raw.parse::<AuthMode>().ok())
        .unwrap_or(AuthMode::Personal)
}

fn normalize_optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_groups_env(key: &str) -> Vec<AuthGroup> {
    normalize_optional_env(key)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|group_id| AuthGroup {
                    group_id: group_id.to_string(),
                    display_name: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_bool_env(key: &str) -> bool {
    normalize_optional_env(key)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_builtin_plugin_skeletons() {
        let names = builtin_plugins()
            .into_iter()
            .map(|plugin| plugin.name().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "builtin.personal_auth".to_string(),
                "builtin.connector_catalog".to_string()
            ]
        );
    }
}
