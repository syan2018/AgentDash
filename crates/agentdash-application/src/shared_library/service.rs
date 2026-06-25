use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::shared_library::{
    IntegrationLibraryAssetSeed, LibraryAsset, LibraryAssetListFilter, LibraryAssetRepository,
    LibraryAssetScope, LibraryAssetSource, LibraryAssetType,
};

use super::{builtin_library_seeds, seed_digest};

#[derive(Debug, Clone, Default)]
pub struct SeedBuiltinLibraryAssetsInput {
    pub asset_type: Option<LibraryAssetType>,
    pub key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IntegrationEmbeddedLibraryAssetSeed {
    pub integration_name: String,
    pub seed: IntegrationLibraryAssetSeed,
}

pub struct SharedLibraryService<'a> {
    repo: &'a dyn LibraryAssetRepository,
}

impl<'a> SharedLibraryService<'a> {
    pub fn new(repo: &'a dyn LibraryAssetRepository) -> Self {
        Self { repo }
    }

    pub async fn get(&self, id: Uuid) -> Result<LibraryAsset, DomainError> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "library_asset",
                id: id.to_string(),
            })
    }

    pub async fn list(
        &self,
        filter: LibraryAssetListFilter,
    ) -> Result<Vec<LibraryAsset>, DomainError> {
        self.repo.list(filter).await
    }

    pub async fn seed_builtin_assets(
        &self,
        input: SeedBuiltinLibraryAssetsInput,
    ) -> Result<Vec<LibraryAsset>, DomainError> {
        let is_unfiltered_seed = input.asset_type.is_none() && input.key.is_none();
        let seeds = builtin_library_seeds()?;
        let active_builtin_identities = seeds
            .iter()
            .map(|seed| (seed.asset_type, seed.key.clone()))
            .collect::<std::collections::HashSet<_>>();
        let mut seeded = Vec::new();
        for seed in seeds {
            if input
                .asset_type
                .is_some_and(|asset_type| asset_type != seed.asset_type)
            {
                continue;
            }
            if input
                .key
                .as_deref()
                .is_some_and(|key| key.trim() != seed.key)
            {
                continue;
            }
            seed.validate()?;
            let asset = LibraryAsset::new(
                seed.asset_type,
                LibraryAssetScope::Builtin,
                None,
                seed.key.clone(),
                seed.display_name,
                seed.description,
                seed.version,
                LibraryAssetSource::Builtin,
                Some(seed.source_ref),
                seed.payload_digest,
                seed.payload,
            )?;
            self.validate_seed_version_progression(&asset).await?;
            seeded.push(self.repo.upsert(&asset).await?);
        }

        if seeded.is_empty() {
            return Err(DomainError::NotFound {
                entity: "builtin_library_asset",
                id: input.key.unwrap_or_else(|| "matching_filter".to_string()),
            });
        }

        if is_unfiltered_seed {
            self.deprecate_removed_builtin_assets(&active_builtin_identities)
                .await?;
        }

        Ok(seeded)
    }

    pub async fn seed_integration_embedded_assets(
        &self,
        seeds: Vec<IntegrationEmbeddedLibraryAssetSeed>,
    ) -> Result<Vec<LibraryAsset>, DomainError> {
        let mut seen = std::collections::HashMap::<(LibraryAssetType, String), String>::new();
        for item in &seeds {
            let key = (item.seed.asset_type, item.seed.key.clone());
            if let Some(first_integration) = seen.insert(key, item.integration_name.clone()) {
                return Err(DomainError::InvalidConfig(format!(
                    "integration embedded LibraryAsset 重复: `{}` 与 `{}` 声明了相同的 {}:{}",
                    first_integration,
                    item.integration_name,
                    item.seed.asset_type.as_str(),
                    item.seed.key
                )));
            }
        }

        let mut seeded = Vec::new();
        for item in seeds {
            item.seed.validate()?;
            let source_ref = format!(
                "integration:{}:{}:{}",
                item.integration_name,
                item.seed.asset_type.as_str(),
                item.seed.key
            );
            let payload_digest = seed_digest(&item.seed.payload)?;
            let asset = LibraryAsset::new(
                item.seed.asset_type,
                LibraryAssetScope::System,
                None,
                item.seed.key.clone(),
                item.seed.display_name.clone(),
                item.seed.description.clone(),
                item.seed.version.clone(),
                LibraryAssetSource::IntegrationEmbedded,
                Some(source_ref.clone()),
                payload_digest,
                item.seed.payload.clone(),
            )?;
            validate_seed_version(&asset.version, source_ref.clone())?;

            match self
                .repo
                .find_by_identity(asset.asset_type, asset.scope, None, &asset.key)
                .await?
            {
                Some(existing)
                    if existing.source == LibraryAssetSource::IntegrationEmbedded
                        && existing.source_ref.as_deref() == Some(source_ref.as_str()) =>
                {
                    validate_seed_asset_progression(&existing, &asset, &source_ref)?;
                    let mut updated = asset;
                    updated.id = existing.id;
                    updated.created_at = existing.created_at;
                    updated.updated_at = chrono::Utc::now();
                    self.repo.update(&updated).await?;
                    seeded.push(updated);
                }
                Some(existing) => {
                    return Err(DomainError::InvalidConfig(format!(
                        "integration embedded LibraryAsset identity 冲突: {}:{} 已由 {:?} {:?} 占用",
                        existing.asset_type.as_str(),
                        existing.key,
                        existing.source,
                        existing.source_ref
                    )));
                }
                None => {
                    self.repo.create(&asset).await?;
                    seeded.push(asset);
                }
            }
        }

        Ok(seeded)
    }

    async fn validate_seed_version_progression(
        &self,
        asset: &LibraryAsset,
    ) -> Result<(), DomainError> {
        let Some(existing) = self
            .repo
            .find_by_identity(
                asset.asset_type,
                asset.scope,
                asset.owner_id.as_deref(),
                &asset.key,
            )
            .await?
        else {
            validate_seed_version(&asset.version, asset_identity(asset))?;
            return Ok(());
        };

        validate_seed_asset_progression(
            &existing,
            asset,
            asset.source_ref.as_deref().unwrap_or(asset.key.as_str()),
        )
    }

    async fn deprecate_removed_builtin_assets(
        &self,
        active_identities: &std::collections::HashSet<(LibraryAssetType, String)>,
    ) -> Result<(), DomainError> {
        let existing_assets = self
            .repo
            .list(LibraryAssetListFilter {
                scope: Some(LibraryAssetScope::Builtin),
                include_deprecated: true,
                ..Default::default()
            })
            .await?;

        for mut asset in existing_assets {
            if asset.source != LibraryAssetSource::Builtin
                || active_identities.contains(&(asset.asset_type, asset.key.clone()))
                || asset.deprecated
            {
                continue;
            }
            asset.mark_deprecated();
            self.repo.update(&asset).await?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemverCore {
    major: u64,
    minor: u64,
    patch: u64,
}

fn validate_seed_asset_progression(
    existing: &LibraryAsset,
    current: &LibraryAsset,
    source_ref: &str,
) -> Result<(), DomainError> {
    let existing_version = parse_seed_version(&existing.version, source_ref)?;
    let current_version = parse_seed_version(&current.version, source_ref)?;

    match (
        existing.payload == current.payload,
        existing.version == current.version,
    ) {
        (true, true) => Ok(()),
        (true, false) if current_version > existing_version => Ok(()),
        (true, false) => Err(DomainError::InvalidConfig(format!(
            "Shared Library seed version 不能回退: {source_ref} {} -> {}",
            existing.version, current.version
        ))),
        (false, _) if current_version > existing_version => Ok(()),
        (false, _) => Err(DomainError::InvalidConfig(format!(
            "Shared Library seed payload 变化但 version 未提升: {source_ref} {} -> {}",
            existing.version, current.version
        ))),
    }
}

fn validate_seed_version(version: &str, source_ref: String) -> Result<(), DomainError> {
    parse_seed_version(version, &source_ref).map(|_| ())
}

fn parse_seed_version(version: &str, source_ref: &str) -> Result<SemverCore, DomainError> {
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(invalid_seed_version(version, source_ref));
    }

    let parse_part = |raw: &str| -> Result<u64, DomainError> {
        if raw.is_empty() || !raw.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(invalid_seed_version(version, source_ref));
        }
        raw.parse::<u64>()
            .map_err(|_| invalid_seed_version(version, source_ref))
    };

    Ok(SemverCore {
        major: parse_part(parts[0])?,
        minor: parse_part(parts[1])?,
        patch: parse_part(parts[2])?,
    })
}

fn invalid_seed_version(version: &str, source_ref: &str) -> DomainError {
    DomainError::InvalidConfig(format!(
        "Shared Library seed version 必须使用 major.minor.patch: {source_ref} version={version}"
    ))
}

fn asset_identity(asset: &LibraryAsset) -> String {
    asset
        .source_ref
        .as_deref()
        .unwrap_or(&asset.key)
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct InMemoryLibraryAssetRepository {
        assets: Mutex<Vec<LibraryAsset>>,
    }

    #[async_trait::async_trait]
    impl LibraryAssetRepository for InMemoryLibraryAssetRepository {
        async fn create(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
            asset.typed_payload()?;
            self.assets.lock().expect("lock").push(asset.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LibraryAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .expect("lock")
                .iter()
                .find(|asset| asset.id == id)
                .cloned())
        }

        async fn find_by_identity(
            &self,
            asset_type: LibraryAssetType,
            scope: LibraryAssetScope,
            owner_id: Option<&str>,
            key: &str,
        ) -> Result<Option<LibraryAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .expect("lock")
                .iter()
                .find(|asset| {
                    asset.asset_type == asset_type
                        && asset.scope == scope
                        && asset.owner_id.as_deref() == owner_id
                        && asset.key == key
                })
                .cloned())
        }

        async fn list(
            &self,
            filter: LibraryAssetListFilter,
        ) -> Result<Vec<LibraryAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .expect("lock")
                .iter()
                .filter(|asset| {
                    filter
                        .asset_type
                        .is_none_or(|value| value == asset.asset_type)
                        && filter.scope.is_none_or(|value| value == asset.scope)
                        && filter
                            .owner_id
                            .as_deref()
                            .is_none_or(|value| asset.owner_id.as_deref() == Some(value))
                        && (filter.include_deprecated || !asset.deprecated)
                })
                .cloned()
                .collect())
        }

        async fn update(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
            asset.typed_payload()?;
            let mut assets = self.assets.lock().expect("lock");
            let existing = assets
                .iter_mut()
                .find(|existing| existing.id == asset.id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "library_asset",
                    id: asset.id.to_string(),
                })?;
            *existing = asset.clone();
            Ok(())
        }

        async fn upsert(&self, asset: &LibraryAsset) -> Result<LibraryAsset, DomainError> {
            asset.typed_payload()?;
            let mut assets = self.assets.lock().expect("lock");
            if let Some(existing) = assets.iter_mut().find(|existing| {
                existing.asset_type == asset.asset_type
                    && existing.scope == asset.scope
                    && existing.owner_id == asset.owner_id
                    && existing.key == asset.key
            }) {
                let mut merged = asset.clone();
                merged.id = existing.id;
                merged.created_at = existing.created_at;
                merged.updated_at = chrono::Utc::now();
                *existing = merged.clone();
                return Ok(merged);
            }

            assets.push(asset.clone());
            Ok(asset.clone())
        }
    }

    #[tokio::test]
    async fn integration_embedded_seeds_can_register_marketplace_builtin_asset_types() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);

        let seeded = service
            .seed_integration_embedded_assets(vec![IntegrationEmbeddedLibraryAssetSeed {
                integration_name: "corp.catalog".to_string(),
                seed: IntegrationLibraryAssetSeed {
                    asset_type: LibraryAssetType::McpServerTemplate,
                    key: "corp-search".to_string(),
                    display_name: "Corp Search".to_string(),
                    description: Some("企业搜索 MCP 模板".to_string()),
                    version: "0.2.0".to_string(),
                    payload: json!({
                        "transport_template": {
                            "type": "http",
                            "url_template": "https://mcp.example.com/search"
                        },
                        "route_policy": "direct",
                        "capabilities": ["search"]
                    }),
                },
            }])
            .await
            .expect("integration seed should be accepted");

        assert_eq!(seeded.len(), 1);
        let asset = &seeded[0];
        assert_eq!(asset.asset_type, LibraryAssetType::McpServerTemplate);
        assert_eq!(asset.scope, LibraryAssetScope::System);
        assert_eq!(asset.source, LibraryAssetSource::IntegrationEmbedded);
        assert_eq!(
            asset.source_ref.as_deref(),
            Some("integration:corp.catalog:mcp_server_template:corp-search")
        );
        assert!(asset.payload_digest.starts_with("sha256:"));
    }

    #[tokio::test]
    async fn integration_embedded_seed_rejects_payload_change_without_version_bump() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);
        let base_seed = IntegrationEmbeddedLibraryAssetSeed {
            integration_name: "corp.catalog".to_string(),
            seed: IntegrationLibraryAssetSeed {
                asset_type: LibraryAssetType::McpServerTemplate,
                key: "corp-search".to_string(),
                display_name: "Corp Search".to_string(),
                description: None,
                version: "0.2.0".to_string(),
                payload: json!({
                    "transport_template": {
                        "type": "http",
                        "url_template": "https://mcp.example.com/search"
                    },
                    "route_policy": "direct",
                    "capabilities": ["search"]
                }),
            },
        };

        service
            .seed_integration_embedded_assets(vec![base_seed.clone()])
            .await
            .expect("initial seed");

        let mut changed_seed = base_seed;
        changed_seed.seed.payload = json!({
            "transport_template": {
                "type": "http",
                "url_template": "https://mcp.example.com/search-v2"
            },
            "route_policy": "direct",
            "capabilities": ["search"]
        });

        let error = service
            .seed_integration_embedded_assets(vec![changed_seed])
            .await
            .expect_err("payload change without version bump must fail");

        assert!(
            matches!(error, DomainError::InvalidConfig(message) if message.contains("payload 变化但 version 未提升"))
        );
    }

    #[tokio::test]
    async fn integration_embedded_seed_repairs_digest_when_payload_and_version_are_unchanged() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);
        let payload = json!({
            "transport_template": {
                "type": "http",
                "url_template": "https://mcp.example.com/search"
            },
            "route_policy": "direct",
            "capabilities": ["search"]
        });
        let existing = LibraryAsset::new(
            LibraryAssetType::McpServerTemplate,
            LibraryAssetScope::System,
            None,
            "corp-search",
            "Corp Search",
            None,
            "0.2.0",
            LibraryAssetSource::IntegrationEmbedded,
            Some("integration:corp.catalog:mcp_server_template:corp-search".to_string()),
            "sha256:stale",
            payload.clone(),
        )
        .expect("existing asset");
        repo.create(&existing).await.expect("insert existing");

        let seeded = service
            .seed_integration_embedded_assets(vec![IntegrationEmbeddedLibraryAssetSeed {
                integration_name: "corp.catalog".to_string(),
                seed: IntegrationLibraryAssetSeed {
                    asset_type: LibraryAssetType::McpServerTemplate,
                    key: "corp-search".to_string(),
                    display_name: "Corp Search".to_string(),
                    description: None,
                    version: "0.2.0".to_string(),
                    payload,
                },
            }])
            .await
            .expect("unchanged payload/version should repair digest");

        assert_eq!(seeded[0].id, existing.id);
        assert_ne!(seeded[0].payload_digest, "sha256:stale");
    }

    #[tokio::test]
    async fn integration_embedded_seed_accepts_payload_change_with_version_bump() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);
        let base_seed = IntegrationEmbeddedLibraryAssetSeed {
            integration_name: "corp.catalog".to_string(),
            seed: IntegrationLibraryAssetSeed {
                asset_type: LibraryAssetType::McpServerTemplate,
                key: "corp-search".to_string(),
                display_name: "Corp Search".to_string(),
                description: None,
                version: "0.2.0".to_string(),
                payload: json!({
                    "transport_template": {
                        "type": "http",
                        "url_template": "https://mcp.example.com/search"
                    },
                    "route_policy": "direct",
                    "capabilities": ["search"]
                }),
            },
        };

        let first = service
            .seed_integration_embedded_assets(vec![base_seed.clone()])
            .await
            .expect("initial seed");

        let mut changed_seed = base_seed;
        changed_seed.seed.version = "0.2.1".to_string();
        changed_seed.seed.payload = json!({
            "transport_template": {
                "type": "http",
                "url_template": "https://mcp.example.com/search-v2"
            },
            "route_policy": "direct",
            "capabilities": ["search"]
        });

        let second = service
            .seed_integration_embedded_assets(vec![changed_seed])
            .await
            .expect("payload change with version bump should update");

        assert_eq!(first[0].id, second[0].id);
        assert_eq!(second[0].version, "0.2.1");
        assert_ne!(first[0].payload_digest, second[0].payload_digest);
    }

    #[tokio::test]
    async fn integration_embedded_seed_rejects_non_semver_version() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);

        let error = service
            .seed_integration_embedded_assets(vec![IntegrationEmbeddedLibraryAssetSeed {
                integration_name: "corp.catalog".to_string(),
                seed: IntegrationLibraryAssetSeed {
                    asset_type: LibraryAssetType::McpServerTemplate,
                    key: "corp-search".to_string(),
                    display_name: "Corp Search".to_string(),
                    description: None,
                    version: "next".to_string(),
                    payload: json!({
                        "transport_template": {
                            "type": "http",
                            "url_template": "https://mcp.example.com/search"
                        },
                        "route_policy": "direct",
                        "capabilities": ["search"]
                    }),
                },
            }])
            .await
            .expect_err("non-semver version must fail");

        assert!(
            matches!(error, DomainError::InvalidConfig(message) if message.contains("major.minor.patch"))
        );
    }

    #[tokio::test]
    async fn builtin_seeds_are_idempotent_and_filterable() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);

        let first = service
            .seed_builtin_assets(Default::default())
            .await
            .expect("builtin seeds should load");
        let second = service
            .seed_builtin_assets(Default::default())
            .await
            .expect("builtin seeds should upsert");

        assert_eq!(first.len(), second.len());
        let ids_by_identity = first
            .iter()
            .map(|asset| ((asset.asset_type, asset.key.clone()), asset.id))
            .collect::<std::collections::HashMap<_, _>>();
        for asset in &second {
            assert_eq!(
                ids_by_identity.get(&(asset.asset_type, asset.key.clone())),
                Some(&asset.id),
                "builtin seed upsert 应保留资产 identity 的稳定 id"
            );
            let expected_source_ref =
                format!("builtin:{}:{}", asset.asset_type.as_str(), asset.key);
            assert_eq!(
                asset.source_ref.as_deref(),
                Some(expected_source_ref.as_str())
            );
        }

        let missing_skill_templates = service
            .seed_builtin_assets(SeedBuiltinLibraryAssetsInput {
                asset_type: Some(LibraryAssetType::SkillTemplate),
                key: None,
            })
            .await;
        assert!(
            matches!(missing_skill_templates, Err(DomainError::NotFound { .. })),
            "内置系统 Skill 由 runtime bootstrap 管理，不进入 Shared Library 市场 seed"
        );
    }

    #[tokio::test]
    async fn builtin_seed_rejects_payload_change_without_version_bump() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);
        let payload = json!({
            "config": {
                "executor": "PI_AGENT",
                "system_prompt": "旧 payload",
                "system_prompt_mode": "append",
                "capability_directives": []
            }
        });
        let existing = LibraryAsset::new(
            LibraryAssetType::AgentTemplate,
            LibraryAssetScope::Builtin,
            None,
            "pi_agent_general",
            "Pi Agent General",
            Some("平台内置通用 Agent 模板".to_string()),
            "1.0.0",
            LibraryAssetSource::Builtin,
            Some("pi_agent_general".to_string()),
            seed_digest(&payload).expect("digest"),
            payload,
        )
        .expect("existing asset");
        repo.create(&existing).await.expect("insert existing");

        let error = service
            .seed_builtin_assets(SeedBuiltinLibraryAssetsInput {
                asset_type: Some(LibraryAssetType::AgentTemplate),
                key: Some("pi_agent_general".to_string()),
            })
            .await
            .expect_err("builtin payload change without version bump must fail");

        assert!(
            matches!(error, DomainError::InvalidConfig(message) if message.contains("payload 变化但 version 未提升"))
        );
    }

    #[tokio::test]
    async fn builtin_seed_marks_removed_assets_deprecated() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);
        let payload = json!({
            "config": {
                "executor": "PI_AGENT",
                "system_prompt": "已移除模板",
                "system_prompt_mode": "append",
                "capability_directives": []
            }
        });
        let stale = LibraryAsset::new(
            LibraryAssetType::AgentTemplate,
            LibraryAssetScope::Builtin,
            None,
            "removed_agent",
            "Removed Agent",
            None,
            "1.0.0",
            LibraryAssetSource::Builtin,
            Some("builtin:agent_template:removed_agent".to_string()),
            seed_digest(&payload).expect("digest"),
            payload,
        )
        .expect("stale asset");
        let stale_id = stale.id;
        repo.create(&stale).await.expect("insert stale asset");

        service
            .seed_builtin_assets(Default::default())
            .await
            .expect("seed all builtins");

        let stale = repo
            .get(stale_id)
            .await
            .expect("load stale")
            .expect("stale asset still exists");
        assert!(stale.deprecated);
    }
}
