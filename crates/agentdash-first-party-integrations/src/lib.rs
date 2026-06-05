use std::{env, sync::Arc};

use agentdash_integration_api::{
    AgentDashIntegration, AuthGroup, AuthIdentity, AuthMode, AuthProvider, AuthRequest,
    IntegrationLibraryAssetSeed, LibraryAssetType, MarketplaceAssetDetail, MarketplaceAssetPage,
    MarketplaceAssetQuery, MarketplaceFetchedAsset, MarketplaceSourceDescriptor,
    MarketplaceSourceError, MarketplaceSourceProvider, MarketplaceSourceProviderKind,
    MarketplaceSourceTrustLevel,
};
use async_trait::async_trait;
use serde_json::json;

const AUTH_MODE_ENV: &str = "AGENTDASH_AUTH_MODE";
const PERSONAL_USER_ID_ENV: &str = "AGENTDASH_PERSONAL_USER_ID";
const PERSONAL_SUBJECT_ENV: &str = "AGENTDASH_PERSONAL_SUBJECT";
const PERSONAL_DISPLAY_NAME_ENV: &str = "AGENTDASH_PERSONAL_DISPLAY_NAME";
const PERSONAL_EMAIL_ENV: &str = "AGENTDASH_PERSONAL_EMAIL";
const PERSONAL_GROUPS_ENV: &str = "AGENTDASH_PERSONAL_GROUPS";
const PERSONAL_IS_ADMIN_ENV: &str = "AGENTDASH_PERSONAL_IS_ADMIN";

/// 开源版默认个人模式认证集成。
///
/// 该集成让个人模式也走统一 `AuthProvider` 契约，避免在宿主里保留“绕过用户系统”的特殊路径。
pub struct PersonalAuthIntegration;

impl AgentDashIntegration for PersonalAuthIntegration {
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

/// 开源版默认连接器目录集成骨架 —— **当前为非功能占位**。
///
/// 不变量/现状声明：本集成**不**装配或暴露任何可用连接器。内置连接器目前仍由宿主直接
/// 构建（见宿主装配代码），与本集成无关。它存在的唯一作用是：
/// 1. 在 first-party integration 目录里占住一个位置，使「连接器最终应走 Host Integration 装配模型」这一意图可见；
/// 2. 通过 [`library_asset_seeds`](AgentDashIntegration::library_asset_seeds) 提供一个示例
///    extension asset seed，给后续真正的连接器集成化提供可参照样例。
///
/// 因此当前**不要**依赖本集成来获得任何连接器能力；后续若要迁移内置连接器到 Host Integration 装配模型，
/// 应在此处补全装配逻辑并同步移除「占位」声明。
pub struct ConnectorCatalogIntegration;

impl AgentDashIntegration for ConnectorCatalogIntegration {
    fn name(&self) -> &str {
        "builtin.connector_catalog"
    }

    fn library_asset_seeds(&self) -> Vec<IntegrationLibraryAssetSeed> {
        vec![IntegrationLibraryAssetSeed {
            asset_type: LibraryAssetType::ExtensionTemplate,
            key: "builtin-session-notes".to_string(),
            display_name: "Session Notes Extension".to_string(),
            description: Some(
                "示例 extension asset：注册一个注入会话备注的 slash command 与运行时 flag。"
                    .to_string(),
            ),
            version: "0.1.1".to_string(),
            payload: json!({
                "manifest_version": "2",
                "extension_id": "builtin-session-notes",
                "package": {
                    "name": "builtin-session-notes",
                    "version": "0.1.1"
                },
                "asset_version": "0.1.1",
                "commands": [{
                    "name": "session-notes:add",
                    "description": "向当前会话注入一条备注提示",
                    "handler": {
                        "kind": "inject_message",
                        "content": "请基于当前上下文整理一条简短会话备注。"
                    }
                }],
                "flags": [{
                    "name": "session-notes.verbose",
                    "type": "bool",
                    "default": false,
                    "description": "备注时输出更详细的上下文说明"
                }],
                "message_renderers": [{
                    "custom_type": "session-notes.note",
                    "renderer": { "kind": "json_card" }
                }],
                "capability_directives": [],
                "asset_refs": []
            }),
        }]
    }

    fn marketplace_source_providers(&self) -> Vec<Arc<dyn MarketplaceSourceProvider>> {
        vec![Arc::new(BuiltinEmptyMarketplaceSource)]
    }
}

struct BuiltinEmptyMarketplaceSource;

#[async_trait]
impl MarketplaceSourceProvider for BuiltinEmptyMarketplaceSource {
    fn descriptor(&self) -> MarketplaceSourceDescriptor {
        MarketplaceSourceDescriptor {
            source_key: "builtin.empty_marketplace".to_string(),
            display_name: "Builtin Empty Marketplace".to_string(),
            description: Some(
                "First-party contract fixture for Skill and MCP marketplace source registration."
                    .to_string(),
            ),
            provider_kind: MarketplaceSourceProviderKind::Builtin,
            supported_asset_types: vec![
                LibraryAssetType::SkillTemplate,
                LibraryAssetType::McpServerTemplate,
            ],
            trust_level: MarketplaceSourceTrustLevel::Curated,
            enabled: true,
        }
    }

    async fn list_assets(
        &self,
        _query: MarketplaceAssetQuery,
    ) -> Result<MarketplaceAssetPage, MarketplaceSourceError> {
        Ok(MarketplaceAssetPage {
            items: vec![],
            next_cursor: None,
        })
    }

    async fn get_asset_detail(
        &self,
        external_id: &str,
    ) -> Result<MarketplaceAssetDetail, MarketplaceSourceError> {
        Err(MarketplaceSourceError::NotFound {
            source_key: self.descriptor().source_key,
            external_id: external_id.to_string(),
        })
    }

    async fn fetch_asset_payload(
        &self,
        external_id: &str,
    ) -> Result<MarketplaceFetchedAsset, MarketplaceSourceError> {
        Err(MarketplaceSourceError::NotFound {
            source_key: self.descriptor().source_key,
            external_id: external_id.to_string(),
        })
    }
}

pub fn builtin_integrations() -> Vec<Box<dyn AgentDashIntegration>> {
    vec![
        Box::new(PersonalAuthIntegration),
        Box::new(ConnectorCatalogIntegration),
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
                avatar_url: None,
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
    ) -> Result<AuthIdentity, agentdash_integration_api::AuthError> {
        Ok(self.identity.clone())
    }

    async fn authorize(
        &self,
        identity: &AuthIdentity,
        _resource: &str,
        _action: &str,
    ) -> Result<bool, agentdash_integration_api::AuthError> {
        // 不变量（Personal 模式）：本集成不做 Provider 级（claim/provider 粗粒度）授权裁决。
        //
        // 个人模式只有一个固定本地用户（见 `BuiltinPersonalAuthProvider::from_env`），不存在
        // 企业 SSO/代理头那种「该身份不属于有效组织」之类的粗粒度入口限制需要在这里拦截。
        // 按 `AuthProvider::authorize` 契约，领域级授权（Project grant、owner/editor/viewer、
        // 共享等）由宿主应用层负责，本 Provider 一律放行 —— 这是有意设计，不是缺失的鉴权。
        //
        // 该 `debug_assert!` 把「只在 Personal 模式下成立」这一前提固化为显式不变量：
        // 一旦未来有人误把本 Provider 接到 Enterprise 身份上，debug 构建会立即报警。
        debug_assert!(
            identity.auth_mode == AuthMode::Personal,
            "BuiltinPersonalAuthProvider 只服务 Personal 模式；收到 {:?} 身份说明装配错误",
            identity.auth_mode
        );
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
    fn exposes_builtin_integration_skeletons() {
        let names = builtin_integrations()
            .into_iter()
            .map(|integration| integration.name().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "builtin.personal_auth".to_string(),
                "builtin.connector_catalog".to_string()
            ]
        );
    }

    #[test]
    fn connector_catalog_declares_extension_asset_seed() {
        let integration = ConnectorCatalogIntegration;
        let seeds = integration.library_asset_seeds();

        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].asset_type, LibraryAssetType::ExtensionTemplate);
        assert_eq!(seeds[0].key, "builtin-session-notes");
    }

    #[test]
    fn connector_catalog_declares_empty_marketplace_source() {
        let integration = ConnectorCatalogIntegration;
        let providers = integration.marketplace_source_providers();

        assert_eq!(providers.len(), 1);
        let descriptor = providers[0].descriptor();
        assert_eq!(descriptor.source_key, "builtin.empty_marketplace");
        assert_eq!(
            descriptor.supported_asset_types,
            vec![
                LibraryAssetType::SkillTemplate,
                LibraryAssetType::McpServerTemplate
            ]
        );
        assert!(descriptor.enabled);
    }

    #[test]
    fn first_party_integration_library_asset_seeds_are_versioned_and_valid() {
        for integration in builtin_integrations() {
            for seed in integration.library_asset_seeds() {
                seed.validate()
                    .expect("first-party integration seed validates");
                assert!(
                    is_semver_core(&seed.version),
                    "first-party integration asset `{}` version must be major.minor.patch",
                    seed.key
                );
            }
        }
    }

    fn is_semver_core(version: &str) -> bool {
        let parts = version.split('.').collect::<Vec<_>>();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    }
}
