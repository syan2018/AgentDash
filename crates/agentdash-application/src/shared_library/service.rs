use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetListFilter, LibraryAssetRepository, LibraryAssetScope,
    LibraryAssetSource, LibraryAssetType, PluginLibraryAssetSeed,
};

use super::{builtin_library_seeds, seed_digest};

#[derive(Debug, Clone, Default)]
pub struct SeedBuiltinLibraryAssetsInput {
    pub asset_type: Option<LibraryAssetType>,
    pub key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PluginEmbeddedLibraryAssetSeed {
    pub plugin_name: String,
    pub seed: PluginLibraryAssetSeed,
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
        let mut seeded = Vec::new();
        for seed in builtin_library_seeds()? {
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
                Some(seed.key),
                seed.payload_digest,
                seed.payload,
            )?;
            seeded.push(self.repo.upsert(&asset).await?);
        }

        if seeded.is_empty() {
            return Err(DomainError::NotFound {
                entity: "builtin_library_asset",
                id: input.key.unwrap_or_else(|| "matching_filter".to_string()),
            });
        }

        Ok(seeded)
    }

    pub async fn seed_plugin_embedded_assets(
        &self,
        seeds: Vec<PluginEmbeddedLibraryAssetSeed>,
    ) -> Result<Vec<LibraryAsset>, DomainError> {
        let mut seen = std::collections::HashMap::<(LibraryAssetType, String), String>::new();
        for item in &seeds {
            let key = (item.seed.asset_type, item.seed.key.clone());
            if let Some(first_plugin) = seen.insert(key, item.plugin_name.clone()) {
                return Err(DomainError::InvalidConfig(format!(
                    "plugin embedded LibraryAsset 重复: `{}` 与 `{}` 声明了相同的 {}:{}",
                    first_plugin,
                    item.plugin_name,
                    item.seed.asset_type.as_str(),
                    item.seed.key
                )));
            }
        }

        let mut seeded = Vec::new();
        for item in seeds {
            item.seed.validate()?;
            let source_ref = format!(
                "plugin:{}:{}:{}",
                item.plugin_name,
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
                LibraryAssetSource::PluginEmbedded,
                Some(source_ref.clone()),
                payload_digest,
                item.seed.payload.clone(),
            )?;

            match self
                .repo
                .find_by_identity(asset.asset_type, asset.scope, None, &asset.key)
                .await?
            {
                Some(existing)
                    if existing.source == LibraryAssetSource::PluginEmbedded
                        && existing.source_ref.as_deref() == Some(source_ref.as_str()) =>
                {
                    let mut updated = asset;
                    updated.id = existing.id;
                    updated.created_at = existing.created_at;
                    updated.updated_at = chrono::Utc::now();
                    self.repo.update(&updated).await?;
                    seeded.push(updated);
                }
                Some(existing) => {
                    return Err(DomainError::InvalidConfig(format!(
                        "plugin embedded LibraryAsset identity 冲突: {}:{} 已由 {:?} {:?} 占用",
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
    async fn plugin_embedded_seeds_can_register_marketplace_builtin_asset_types() {
        let repo = InMemoryLibraryAssetRepository::default();
        let service = SharedLibraryService::new(&repo);

        let seeded = service
            .seed_plugin_embedded_assets(vec![PluginEmbeddedLibraryAssetSeed {
                plugin_name: "corp.catalog".to_string(),
                seed: PluginLibraryAssetSeed {
                    asset_type: LibraryAssetType::McpServerTemplate,
                    key: "corp-search".to_string(),
                    display_name: "Corp Search".to_string(),
                    description: Some("企业搜索 MCP 模板".to_string()),
                    version: "0.2.0".to_string(),
                    payload: json!({
                        "transport": {
                            "type": "http",
                            "url": "https://mcp.example.com/search"
                        },
                        "route_policy": "direct",
                        "capabilities": ["search"]
                    }),
                },
            }])
            .await
            .expect("plugin seed should be accepted");

        assert_eq!(seeded.len(), 1);
        let asset = &seeded[0];
        assert_eq!(asset.asset_type, LibraryAssetType::McpServerTemplate);
        assert_eq!(asset.scope, LibraryAssetScope::System);
        assert_eq!(asset.source, LibraryAssetSource::PluginEmbedded);
        assert_eq!(
            asset.source_ref.as_deref(),
            Some("plugin:corp.catalog:mcp_server_template:corp-search")
        );
        assert!(asset.payload_digest.starts_with("sha256:"));
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
        }

        let skill_templates = service
            .seed_builtin_assets(SeedBuiltinLibraryAssetsInput {
                asset_type: Some(LibraryAssetType::SkillTemplate),
                key: None,
            })
            .await
            .expect("skill template seeds should load");
        assert!(!skill_templates.is_empty());
        assert!(
            skill_templates
                .iter()
                .all(|asset| asset.asset_type == LibraryAssetType::SkillTemplate)
        );
    }
}
