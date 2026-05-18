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
