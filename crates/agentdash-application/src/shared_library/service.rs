use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetListFilter, LibraryAssetRepository, LibraryAssetScope,
    LibraryAssetSource, LibraryAssetType,
};

use super::builtin_library_seeds;

#[derive(Debug, Clone, Default)]
pub struct SeedBuiltinLibraryAssetsInput {
    pub asset_type: Option<LibraryAssetType>,
    pub key: Option<String>,
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
}
