use std::{env, sync::Arc};

use agentdash_integration_api::{
    AgentDashIntegration, AuthGroup, AuthIdentity, AuthMode, AuthProvider, AuthRequest,
    DiscoveredMemorySource, IntegrationLibraryAssetSeed, LibraryAssetType, MarketplaceAssetDetail,
    MarketplaceAssetListing, MarketplaceAssetPage, MarketplaceAssetQuery, MarketplaceFetchedAsset,
    MarketplaceFetchedAssetPayload, MarketplaceSourceDescriptor, MarketplaceSourceError,
    MarketplaceSourceProvider, MarketplaceSourceProviderKind, MarketplaceSourceTrustLevel,
    MemoryDiscoveryCluster, MemoryDiscoveryContext, MemoryDiscoveryError, MemoryDiscoveryMount,
    MemoryDiscoveryOutput, MemoryDiscoveryProvider, MemoryDiscoveryVfsFile, MemoryDiscoveryVfsRule,
    MemoryIndexStatus, MemorySourceFormat, MemorySourceScope, MemorySourceTrustLevel,
};
use agentdash_integration_codex::CodexRuntimeIntegration;
use async_trait::async_trait;
use serde_json::json;

const AUTH_MODE_ENV: &str = "AGENTDASH_AUTH_MODE";
const PERSONAL_USER_ID_ENV: &str = "AGENTDASH_PERSONAL_USER_ID";
const PERSONAL_SUBJECT_ENV: &str = "AGENTDASH_PERSONAL_SUBJECT";
const PERSONAL_DISPLAY_NAME_ENV: &str = "AGENTDASH_PERSONAL_DISPLAY_NAME";
const PERSONAL_EMAIL_ENV: &str = "AGENTDASH_PERSONAL_EMAIL";
const PERSONAL_GROUPS_ENV: &str = "AGENTDASH_PERSONAL_GROUPS";
const PERSONAL_IS_ADMIN_ENV: &str = "AGENTDASH_PERSONAL_IS_ADMIN";
const PROJECT_AGENT_MEMORY_PROVIDER_KEY: &str = "builtin.project_agent_memory";
const PROJECT_AGENT_MEMORY_MOUNT_ID: &str = "agent";
const PROJECT_AGENT_MEMORY_INDEX_PATH: &str = "MEMORY.md";
const PROJECT_AGENT_MEMORY_INDEX_RULE_KEY: &str = "project-agent-memory-index";

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
        vec![Arc::new(DevMarketplaceFixtureSource)]
    }
}

/// 开源版默认 ProjectAgent memory discovery 集成。
pub struct ProjectAgentMemoryIntegration;

impl AgentDashIntegration for ProjectAgentMemoryIntegration {
    fn name(&self) -> &str {
        "builtin.project_agent_memory"
    }

    fn memory_discovery_providers(&self) -> Vec<Arc<dyn MemoryDiscoveryProvider>> {
        vec![Arc::new(ProjectAgentMemoryDiscoveryProvider)]
    }
}

struct ProjectAgentMemoryDiscoveryProvider;

#[async_trait]
impl MemoryDiscoveryProvider for ProjectAgentMemoryDiscoveryProvider {
    fn provider_key(&self) -> &str {
        PROJECT_AGENT_MEMORY_PROVIDER_KEY
    }

    fn vfs_discovery_rules(&self) -> Vec<MemoryDiscoveryVfsRule> {
        let mut rule = MemoryDiscoveryVfsRule::new(PROJECT_AGENT_MEMORY_INDEX_RULE_KEY);
        rule.exact_paths = vec![PROJECT_AGENT_MEMORY_INDEX_PATH.to_string()];
        rule.max_size_bytes = 16 * 1024;
        vec![rule]
    }

    async fn discover_from_vfs(
        &self,
        _context: MemoryDiscoveryContext,
        mounts: Vec<MemoryDiscoveryMount>,
        files: Vec<MemoryDiscoveryVfsFile>,
    ) -> Result<MemoryDiscoveryOutput, MemoryDiscoveryError> {
        let Some(agent_mount) = mounts
            .into_iter()
            .find(|mount| mount.mount_id == PROJECT_AGENT_MEMORY_MOUNT_ID)
        else {
            return Ok(MemoryDiscoveryOutput::default());
        };

        let index_file = files.into_iter().find(|file| {
            file.mount_id == PROJECT_AGENT_MEMORY_MOUNT_ID
                && file.path == PROJECT_AGENT_MEMORY_INDEX_PATH
        });
        let index_status = if index_file.is_some() {
            MemoryIndexStatus::Present
        } else {
            MemoryIndexStatus::Missing
        };

        let source = DiscoveredMemorySource {
            provider_key: PROJECT_AGENT_MEMORY_PROVIDER_KEY.to_string(),
            source_key: "agent".to_string(),
            display_name: agent_mount.display_name,
            source_uri: "agent://".to_string(),
            index_uri: "agent://MEMORY.md".to_string(),
            mount_id: PROJECT_AGENT_MEMORY_MOUNT_ID.to_string(),
            scope: MemorySourceScope::Agent,
            capabilities: agent_mount.capabilities,
            format: MemorySourceFormat::AgentDash,
            index_status,
            trust_level: MemorySourceTrustLevel::FirstParty,
            summary: Some(
                "ProjectAgent shared memory home backed by the active agent VFS mount.".to_string(),
            ),
            bounded_index_content: index_file.map(|file| file.content),
        };

        Ok(MemoryDiscoveryOutput {
            clusters: vec![MemoryDiscoveryCluster {
                provider_key: PROJECT_AGENT_MEMORY_PROVIDER_KEY.to_string(),
                display_name: "ProjectAgent Memory".to_string(),
                model_summary: Some(
                    "ProjectAgent memory sources discovered from active VFS mounts.".to_string(),
                ),
                ui_summary: Some("ProjectAgent memory sources.".to_string()),
                inventory_hint: Some("Default source: agent://".to_string()),
                inventory_count: Some(1),
                sources: vec![source],
            }],
            diagnostics: Vec::new(),
        })
    }
}

struct DevMarketplaceFixtureSource;

const FIXTURE_MARKETPLACE_SOURCE_KEY: &str = "agentdash.dev.marketplace";
const FIXTURE_MCP_EXTERNAL_ID: &str = "workspace-http-mcp";
const FIXTURE_MCP_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";

#[async_trait]
impl MarketplaceSourceProvider for DevMarketplaceFixtureSource {
    fn descriptor(&self) -> MarketplaceSourceDescriptor {
        MarketplaceSourceDescriptor {
            source_key: FIXTURE_MARKETPLACE_SOURCE_KEY.to_string(),
            display_name: "AgentDash Dev Marketplace".to_string(),
            description: Some(
                "First-party contract fixture for external MCP marketplace import validation."
                    .to_string(),
            ),
            provider_kind: MarketplaceSourceProviderKind::Builtin,
            supported_asset_types: vec![LibraryAssetType::McpServerTemplate],
            trust_level: MarketplaceSourceTrustLevel::Curated,
            enabled: true,
        }
    }

    async fn list_assets(
        &self,
        query: MarketplaceAssetQuery,
    ) -> Result<MarketplaceAssetPage, MarketplaceSourceError> {
        if query
            .asset_type
            .is_some_and(|asset_type| asset_type != LibraryAssetType::McpServerTemplate)
            || query.cursor.is_some()
        {
            return Ok(MarketplaceAssetPage {
                items: vec![],
                next_cursor: None,
            });
        }
        let listing = fixture_mcp_listing();
        let matches_query = query
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none_or(|query| {
                let query = query.to_ascii_lowercase();
                listing.key.to_ascii_lowercase().contains(&query)
                    || listing.display_name.to_ascii_lowercase().contains(&query)
            });
        let mut items = if matches_query { vec![listing] } else { vec![] };
        if let Some(limit) = query.limit {
            items.truncate(limit as usize);
        }
        Ok(MarketplaceAssetPage {
            items,
            next_cursor: None,
        })
    }

    async fn get_asset_detail(
        &self,
        external_id: &str,
    ) -> Result<MarketplaceAssetDetail, MarketplaceSourceError> {
        if external_id != FIXTURE_MCP_EXTERNAL_ID {
            return Err(MarketplaceSourceError::NotFound {
                source_key: self.descriptor().source_key,
                external_id: external_id.to_string(),
            });
        }
        Ok(MarketplaceAssetDetail {
            listing: fixture_mcp_listing(),
            detail_markdown: Some(
                "Development fixture for HTTP MCP template import and install.".to_string(),
            ),
            homepage_url: Some("https://mcp.example.com".to_string()),
            repository_url: None,
        })
    }

    async fn fetch_asset_payload(
        &self,
        external_id: &str,
    ) -> Result<MarketplaceFetchedAsset, MarketplaceSourceError> {
        if external_id != FIXTURE_MCP_EXTERNAL_ID {
            return Err(MarketplaceSourceError::NotFound {
                source_key: self.descriptor().source_key,
                external_id: external_id.to_string(),
            });
        }
        Ok(MarketplaceFetchedAsset::McpServerTemplate(
            MarketplaceFetchedAssetPayload {
                source_key: FIXTURE_MARKETPLACE_SOURCE_KEY.to_string(),
                external_id: FIXTURE_MCP_EXTERNAL_ID.to_string(),
                key: "workspace-http-mcp".to_string(),
                display_name: "Workspace HTTP MCP".to_string(),
                description: Some("HTTP MCP template with workspace slug parameter.".to_string()),
                version: "0.1.0".to_string(),
                digest: Some(FIXTURE_MCP_DIGEST.to_string()),
                payload: json!({
                    "transport_template": {
                        "type": "http",
                        "url_template": "https://mcp.example.com/${workspace}/mcp"
                    },
                    "route_policy": "direct",
                    "parameter_schema": {
                        "type": "object",
                        "required": ["workspace"],
                        "properties": {
                            "workspace": {
                                "type": "string",
                                "description": "Workspace slug"
                            }
                        },
                        "additionalProperties": false
                    },
                    "capabilities": ["search", "read"]
                }),
            },
        ))
    }
}

fn fixture_mcp_listing() -> MarketplaceAssetListing {
    MarketplaceAssetListing {
        source_key: FIXTURE_MARKETPLACE_SOURCE_KEY.to_string(),
        external_id: FIXTURE_MCP_EXTERNAL_ID.to_string(),
        asset_type: LibraryAssetType::McpServerTemplate,
        key: "workspace-http-mcp".to_string(),
        display_name: "Workspace HTTP MCP".to_string(),
        description: Some("HTTP MCP template with workspace slug parameter.".to_string()),
        version: "0.1.0".to_string(),
        tags: vec!["mcp".to_string(), "fixture".to_string()],
        author: Some("AgentDash".to_string()),
        digest: Some(FIXTURE_MCP_DIGEST.to_string()),
        updated_at: None,
        install_requirements: vec![],
    }
}

pub fn builtin_integrations() -> Vec<Box<dyn AgentDashIntegration>> {
    vec![
        Box::new(PersonalAuthIntegration),
        Box::new(CodexRuntimeIntegration),
        Box::new(ConnectorCatalogIntegration),
        Box::new(ProjectAgentMemoryIntegration),
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
        // 按 `AuthProvider::authorize` 契约，领域级授权（Project grant、owner/editor/member、
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
    use agentdash_integration_api::MountCapability;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

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
                "builtin.codex_runtime".to_string(),
                "builtin.connector_catalog".to_string(),
                "builtin.project_agent_memory".to_string()
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
    fn codex_runtime_is_a_functional_driver_contribution() {
        let integration = CodexRuntimeIntegration;
        let contributions = integration.agent_runtime_drivers();
        assert_eq!(contributions.len(), 1);
        assert_eq!(
            contributions[0]
                .definition
                .provenance
                .definition_id
                .as_str(),
            "builtin.codex-app-server"
        );
        assert_eq!(
            contributions[0].definition.supported_protocol_revisions,
            vec![140]
        );
    }

    #[test]
    fn connector_catalog_declares_fixture_marketplace_source() {
        let integration = ConnectorCatalogIntegration;
        let providers = integration.marketplace_source_providers();

        assert_eq!(providers.len(), 1);
        let descriptor = providers[0].descriptor();
        assert_eq!(descriptor.source_key, FIXTURE_MARKETPLACE_SOURCE_KEY);
        assert_eq!(
            descriptor.supported_asset_types,
            vec![LibraryAssetType::McpServerTemplate]
        );
        assert!(descriptor.enabled);
    }

    #[test]
    fn project_agent_memory_provider_declares_bounded_index_rule() {
        let integration = ProjectAgentMemoryIntegration;
        let providers = integration.memory_discovery_providers();

        assert_eq!(providers.len(), 1);
        assert_eq!(
            providers[0].provider_key(),
            PROJECT_AGENT_MEMORY_PROVIDER_KEY
        );

        let rules = providers[0].vfs_discovery_rules();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].exact_paths, vec!["MEMORY.md".to_string()]);
        assert_eq!(rules[0].max_size_bytes, 16 * 1024);
    }

    #[test]
    fn project_agent_memory_provider_returns_source_without_index_file() {
        let provider = ProjectAgentMemoryDiscoveryProvider;

        let output = block_on(provider.discover_from_vfs(
            MemoryDiscoveryContext::default(),
            vec![MemoryDiscoveryMount::new(
                PROJECT_AGENT_MEMORY_MOUNT_ID,
                "inline_fs",
                "Agent Memory",
                vec![
                    MountCapability::Read,
                    MountCapability::Write,
                    MountCapability::List,
                    MountCapability::Search,
                ],
            )],
            Vec::new(),
        ))
        .expect("memory discovery");

        let source = &output.clusters[0].sources[0];
        assert_eq!(source.source_uri, "agent://");
        assert_eq!(source.index_uri, "agent://MEMORY.md");
        assert_eq!(source.index_status, MemoryIndexStatus::Missing);
        assert_eq!(
            source.capabilities,
            vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
                MountCapability::Search,
            ]
        );
        assert!(source.bounded_index_content.is_none());
    }

    #[test]
    fn project_agent_memory_provider_attaches_bounded_index_only() {
        let provider = ProjectAgentMemoryDiscoveryProvider;

        let output = block_on(provider.discover_from_vfs(
            MemoryDiscoveryContext::default(),
            vec![MemoryDiscoveryMount::new(
                PROJECT_AGENT_MEMORY_MOUNT_ID,
                "inline_fs",
                "Agent Memory",
                vec![MountCapability::Read],
            )],
            vec![
                MemoryDiscoveryVfsFile {
                    rule_key: PROJECT_AGENT_MEMORY_INDEX_RULE_KEY.to_string(),
                    mount_id: PROJECT_AGENT_MEMORY_MOUNT_ID.to_string(),
                    path: PROJECT_AGENT_MEMORY_INDEX_PATH.to_string(),
                    content: "index body".to_string(),
                    size_bytes: Some(10),
                },
                MemoryDiscoveryVfsFile {
                    rule_key: PROJECT_AGENT_MEMORY_INDEX_RULE_KEY.to_string(),
                    mount_id: PROJECT_AGENT_MEMORY_MOUNT_ID.to_string(),
                    path: "topics/project.md".to_string(),
                    content: "topic body must not be injected".to_string(),
                    size_bytes: Some(31),
                },
            ],
        ))
        .expect("memory discovery");

        let source = &output.clusters[0].sources[0];
        assert_eq!(source.index_status, MemoryIndexStatus::Present);
        assert_eq!(source.bounded_index_content.as_deref(), Some("index body"));
        assert!(
            !source
                .bounded_index_content
                .as_deref()
                .unwrap_or_default()
                .contains("topic body")
        );
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
