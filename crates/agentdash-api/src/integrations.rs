use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agentdash_application::shared_library::IntegrationEmbeddedLibraryAssetSeed;
use agentdash_integration_api::{AgentDashIntegration, AuthProvider};
use agentdash_spi::AgentConnector;
use agentdash_spi::VfsDiscoveryProvider;
use agentdash_spi::platform::mount::MountProvider;
use thiserror::Error;

/// 开源版内置 Host Integration 集合。
pub fn builtin_integrations() -> Vec<Box<dyn AgentDashIntegration>> {
    agentdash_first_party_integrations::builtin_integrations()
}

/// Host Integration 注册结果。
///
/// 宿主先汇总所有集成注册，再基于此统一构建运行时，避免“先构建、后塞集成”的假扩展点。
pub(crate) struct HostIntegrationRegistration {
    pub vfs_providers: Vec<Box<dyn VfsDiscoveryProvider>>,
    pub connectors: Vec<Arc<dyn AgentConnector>>,
    pub auth_provider: Option<Arc<dyn AuthProvider>>,
    pub mount_providers: Vec<Arc<dyn MountProvider>>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub library_asset_seeds: Vec<IntegrationEmbeddedLibraryAssetSeed>,
}

#[derive(Debug, Error)]
pub(crate) enum IntegrationRegistrationError {
    #[error("Host Integration `{integration_name}` 初始化失败: {message}")]
    IntegrationInit {
        integration_name: String,
        message: String,
    },
    #[error(
        "检测到多个 AuthProvider：`{first_integration}` 与 `{second_integration}`。当前宿主只允许注册一个认证集成"
    )]
    DuplicateAuthProvider {
        first_integration: String,
        second_integration: String,
    },
    #[error(
        "执行器 ID `{executor_id}` 重复注册：`{first_owner}` 与 `{second_owner}` 不能同时声明同一执行器"
    )]
    DuplicateExecutorId {
        executor_id: String,
        first_owner: String,
        second_owner: String,
    },
}

pub(crate) fn collect_integration_registration(
    integrations: Vec<Box<dyn AgentDashIntegration>>,
) -> Result<HostIntegrationRegistration, IntegrationRegistrationError> {
    let mut vfs_providers = Vec::new();
    let mut connectors = Vec::new();
    let mut auth_provider: Option<Arc<dyn AuthProvider>> = None;
    let mut auth_provider_integration: Option<String> = None;
    let mut executor_owners: HashMap<String, String> = HashMap::new();
    let mut mount_providers = Vec::new();
    let mut extra_skill_dirs = Vec::new();
    let mut library_asset_seeds = Vec::new();

    for integration in integrations {
        let integration_name = integration.name().to_string();
        tracing::info!("加载 Host Integration: {}", integration_name);

        integration
            .on_init()
            .map_err(|err| IntegrationRegistrationError::IntegrationInit {
                integration_name: integration_name.clone(),
                message: err.to_string(),
            })?;

        vfs_providers.extend(integration.vfs_providers());

        let mp = integration.mount_providers();
        if !mp.is_empty() {
            tracing::info!(
                "  Host Integration `{}` 注册了 {} 个 MountProvider",
                integration_name,
                mp.len()
            );
            mount_providers.extend(mp);
        }

        let skill_dirs = integration.extra_skill_dirs();
        if !skill_dirs.is_empty() {
            tracing::info!(
                "  Host Integration `{}` 注册了 {} 个 skill 扫描目录",
                integration_name,
                skill_dirs.len()
            );
            extra_skill_dirs.extend(skill_dirs);
        }

        let seeds = integration.library_asset_seeds();
        if !seeds.is_empty() {
            tracing::info!(
                "  Host Integration `{}` 声明了 {} 个 Shared Library asset",
                integration_name,
                seeds.len()
            );
            library_asset_seeds.extend(seeds.into_iter().map(|seed| {
                IntegrationEmbeddedLibraryAssetSeed {
                    integration_name: integration_name.clone(),
                    seed,
                }
            }));
        }

        for connector in integration.agent_connectors() {
            for executor in connector.list_executors() {
                if let Some(first_integration) =
                    executor_owners.insert(executor.id.clone(), integration_name.clone())
                {
                    return Err(IntegrationRegistrationError::DuplicateExecutorId {
                        executor_id: executor.id,
                        first_owner: first_integration,
                        second_owner: integration_name.clone(),
                    });
                }
            }
            connectors.push(connector);
        }

        if let Some(provider) = integration.auth_provider() {
            if let Some(first_integration) = auth_provider_integration {
                return Err(IntegrationRegistrationError::DuplicateAuthProvider {
                    first_integration,
                    second_integration: integration_name,
                });
            }
            auth_provider_integration = Some(integration_name);
            auth_provider = Some(Arc::from(provider));
        }
    }

    Ok(HostIntegrationRegistration {
        vfs_providers,
        connectors,
        auth_provider,
        mount_providers,
        extra_skill_dirs,
        library_asset_seeds,
    })
}

pub(crate) fn validate_connector_executor_ids(
    connectors: &[Arc<dyn AgentConnector>],
) -> Result<(), IntegrationRegistrationError> {
    let mut executor_owners: HashMap<String, String> = HashMap::new();

    for connector in connectors {
        let owner = connector.connector_id().to_string();
        for executor in connector.list_executors() {
            if let Some(first_owner) = executor_owners.insert(executor.id.clone(), owner.clone()) {
                return Err(IntegrationRegistrationError::DuplicateExecutorId {
                    executor_id: executor.id,
                    first_owner,
                    second_owner: owner.clone(),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use agentdash_integration_api::{
        AgentDashIntegration, AuthError, AuthIdentity, AuthMode, AuthProvider, AuthRequest,
        IntegrationLibraryAssetSeed, LibraryAssetType,
    };
    use agentdash_spi::{
        AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
        ExecutionContext, ExecutionStream, PromptPayload,
    };
    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};
    use serde_json::json;

    use super::*;

    struct TestIntegration {
        name: &'static str,
        auth: bool,
        executor_ids: Vec<&'static str>,
    }

    struct SeedIntegration;

    impl AgentDashIntegration for SeedIntegration {
        fn name(&self) -> &str {
            "seed-integration"
        }

        fn library_asset_seeds(&self) -> Vec<IntegrationLibraryAssetSeed> {
            vec![IntegrationLibraryAssetSeed {
                asset_type: LibraryAssetType::ExtensionTemplate,
                key: "seed-extension".to_string(),
                display_name: "Seed Extension".to_string(),
                description: None,
                version: "0.1.0".to_string(),
                payload: json!({
                    "manifest_version": "2",
                    "extension_id": "seed-extension",
                    "package": {
                        "name": "seed-extension",
                        "version": "0.1.0"
                    },
                    "asset_version": "0.1.0",
                    "commands": [{
                        "name": "seed-extension:run",
                        "description": "run",
                        "handler": { "kind": "inject_message", "content": "run" }
                    }]
                }),
            }]
        }
    }

    impl AgentDashIntegration for TestIntegration {
        fn name(&self) -> &str {
            self.name
        }

        fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> {
            self.auth
                .then(|| Box::new(TestAuthProvider) as Box<dyn AuthProvider>)
        }

        fn agent_connectors(&self) -> Vec<Arc<dyn AgentConnector>> {
            if self.executor_ids.is_empty() {
                return vec![];
            }
            vec![Arc::new(TestConnector {
                id: self.name,
                executors: self
                    .executor_ids
                    .iter()
                    .map(|id| (*id).to_string())
                    .collect(),
            })]
        }
    }

    struct TestAuthProvider;

    #[async_trait]
    impl AuthProvider for TestAuthProvider {
        async fn authenticate(&self, _req: &AuthRequest) -> Result<AuthIdentity, AuthError> {
            Ok(AuthIdentity {
                auth_mode: AuthMode::Enterprise,
                user_id: "test-user".to_string(),
                subject: "test-subject".to_string(),
                display_name: Some("Test User".to_string()),
                email: Some("test@example.com".to_string()),
                avatar_url: None,
                groups: vec![],
                is_admin: false,
                provider: Some("test.auth".to_string()),
                extra: serde_json::Value::Null,
            })
        }

        async fn authorize(
            &self,
            _identity: &AuthIdentity,
            _resource: &str,
            _action: &str,
        ) -> Result<bool, AuthError> {
            Ok(true)
        }
    }

    struct TestConnector {
        id: &'static str,
        executors: Vec<String>,
    }

    #[async_trait]
    impl AgentConnector for TestConnector {
        fn connector_id(&self) -> &'static str {
            self.id
        }

        fn connector_type(&self) -> ConnectorType {
            ConnectorType::LocalExecutor
        }

        fn capabilities(&self) -> ConnectorCapabilities {
            ConnectorCapabilities::default()
        }

        fn list_executors(&self) -> Vec<AgentInfo> {
            self.executors
                .iter()
                .map(|id| AgentInfo {
                    id: id.clone(),
                    name: id.clone(),
                    variants: vec![],
                    available: true,
                })
                .collect()
        }

        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
            Ok(Box::pin(stream::empty()))
        }

        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: ExecutionContext,
        ) -> Result<ExecutionStream, ConnectorError> {
            let stream: ExecutionStream = Box::pin(stream::empty());
            Ok(stream)
        }

        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    #[test]
    fn rejects_duplicate_auth_provider() {
        let err = match collect_integration_registration(vec![
            Box::new(TestIntegration {
                name: "auth-a",
                auth: true,
                executor_ids: vec![],
            }),
            Box::new(TestIntegration {
                name: "auth-b",
                auth: true,
                executor_ids: vec![],
            }),
        ]) {
            Ok(_) => panic!("重复 auth provider 应失败"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            IntegrationRegistrationError::DuplicateAuthProvider { .. }
        ));
    }

    #[test]
    fn rejects_duplicate_executor_id() {
        let err = match collect_integration_registration(vec![
            Box::new(TestIntegration {
                name: "connector-a",
                auth: false,
                executor_ids: vec!["CODEX"],
            }),
            Box::new(TestIntegration {
                name: "connector-b",
                auth: false,
                executor_ids: vec!["CODEX"],
            }),
        ]) {
            Ok(_) => panic!("重复执行器 ID 应失败"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            IntegrationRegistrationError::DuplicateExecutorId { .. }
        ));
    }

    #[test]
    fn collects_auth_and_connectors() {
        let registration = collect_integration_registration(vec![
            Box::new(TestIntegration {
                name: "auth-only",
                auth: true,
                executor_ids: vec![],
            }),
            Box::new(TestIntegration {
                name: "connector-only",
                auth: false,
                executor_ids: vec!["CODEX", "CLAUDE"],
            }),
        ])
        .expect("应成功聚合 Host Integration");

        assert!(registration.auth_provider.is_some());
        assert_eq!(registration.connectors.len(), 1);
        assert_eq!(registration.connectors[0].list_executors().len(), 2);
    }

    #[test]
    fn collects_integration_library_asset_seeds() {
        let registration =
            collect_integration_registration(vec![Box::new(SeedIntegration)]).expect("collect");

        assert_eq!(registration.library_asset_seeds.len(), 1);
        assert_eq!(
            registration.library_asset_seeds[0].integration_name,
            "seed-integration"
        );
        assert_eq!(
            registration.library_asset_seeds[0].seed.key,
            "seed-extension"
        );
    }

    #[test]
    fn validates_duplicate_executor_ids_across_combined_connectors() {
        let connectors: Vec<Arc<dyn AgentConnector>> = vec![
            Arc::new(TestConnector {
                id: "builtin-pi",
                executors: vec!["PI_AGENT".to_string()],
            }),
            Arc::new(TestConnector {
                id: "integration-codex",
                executors: vec!["PI_AGENT".to_string()],
            }),
        ];

        let err = validate_connector_executor_ids(&connectors)
            .expect_err("内置与 Host Integration 执行器重复时应失败");

        assert!(matches!(
            err,
            IntegrationRegistrationError::DuplicateExecutorId { .. }
        ));
    }
}
