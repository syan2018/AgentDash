use uuid::Uuid;

use crate::DomainError;

use super::entity::LibraryAsset;
use super::value_objects::{LibraryAssetScope, LibraryAssetType};

#[derive(Debug, Clone, Default)]
pub struct LibraryAssetListFilter {
    pub asset_type: Option<LibraryAssetType>,
    pub scope: Option<LibraryAssetScope>,
    pub owner_id: Option<String>,
    pub include_deprecated: bool,
}

#[async_trait::async_trait]
pub trait LibraryAssetRepository: Send + Sync {
    async fn create(&self, asset: &LibraryAsset) -> Result<(), DomainError>;
    async fn get(&self, id: Uuid) -> Result<Option<LibraryAsset>, DomainError>;
    async fn find_by_identity(
        &self,
        asset_type: LibraryAssetType,
        scope: LibraryAssetScope,
        owner_id: Option<&str>,
        key: &str,
    ) -> Result<Option<LibraryAsset>, DomainError>;
    async fn list(&self, filter: LibraryAssetListFilter) -> Result<Vec<LibraryAsset>, DomainError>;
    async fn update(&self, asset: &LibraryAsset) -> Result<(), DomainError>;
    async fn upsert(&self, asset: &LibraryAsset) -> Result<LibraryAsset, DomainError>;
}
