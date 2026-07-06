use agentdash_domain::DomainError;
use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetListFilter, LibraryAssetRepository, LibraryAssetScope,
    LibraryAssetType, ProjectExtensionInstallation, ProjectExtensionInstallationRepository,
};
use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct MemoryLibraryAssetRepository {
    assets: Mutex<Vec<LibraryAsset>>,
}

#[async_trait::async_trait]
impl LibraryAssetRepository for MemoryLibraryAssetRepository {
    async fn create(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
        asset.typed_payload()?;
        self.assets.lock().await.push(asset.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LibraryAsset>, DomainError> {
        Ok(self
            .assets
            .lock()
            .await
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
            .await
            .iter()
            .find(|asset| {
                asset.asset_type == asset_type
                    && asset.scope == scope
                    && asset.owner_id.as_deref() == owner_id
                    && asset.key == key
            })
            .cloned())
    }

    async fn list(&self, filter: LibraryAssetListFilter) -> Result<Vec<LibraryAsset>, DomainError> {
        Ok(self
            .assets
            .lock()
            .await
            .iter()
            .filter(|asset| {
                filter
                    .asset_type
                    .is_none_or(|value| asset.asset_type == value)
            })
            .filter(|asset| filter.scope.is_none_or(|value| asset.scope == value))
            .filter(|asset| {
                filter
                    .owner_id
                    .as_ref()
                    .is_none_or(|owner_id| asset.owner_id.as_deref() == Some(owner_id.as_str()))
            })
            .filter(|asset| filter.include_deprecated || !asset.deprecated)
            .cloned()
            .collect())
    }

    async fn update(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
        asset.typed_payload()?;
        let mut assets = self.assets.lock().await;
        if let Some(existing) = assets.iter_mut().find(|existing| existing.id == asset.id) {
            *existing = asset.clone();
        }
        Ok(())
    }

    async fn upsert(&self, asset: &LibraryAsset) -> Result<LibraryAsset, DomainError> {
        asset.typed_payload()?;
        let mut assets = self.assets.lock().await;
        if let Some(existing) = assets.iter_mut().find(|existing| {
            existing.asset_type == asset.asset_type
                && existing.scope == asset.scope
                && existing.owner_id == asset.owner_id
                && existing.key == asset.key
        }) {
            let mut merged = asset.clone();
            merged.id = existing.id;
            merged.created_at = existing.created_at;
            merged.updated_at = Utc::now();
            *existing = merged.clone();
            return Ok(merged);
        } else {
            assets.push(asset.clone());
        }
        Ok(asset.clone())
    }
}

impl MemoryLibraryAssetRepository {
    pub fn new_with_assets(assets: Vec<LibraryAsset>) -> Self {
        Self {
            assets: Mutex::new(assets),
        }
    }

    pub async fn debug_list(&self) -> Vec<LibraryAsset> {
        self.assets.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryProjectExtensionInstallationRepository {
    installations: Mutex<Vec<ProjectExtensionInstallation>>,
}

#[async_trait::async_trait]
impl ProjectExtensionInstallationRepository for MemoryProjectExtensionInstallationRepository {
    async fn create(&self, installation: &ProjectExtensionInstallation) -> Result<(), DomainError> {
        self.installations.lock().await.push(installation.clone());
        Ok(())
    }

    async fn update(&self, installation: &ProjectExtensionInstallation) -> Result<(), DomainError> {
        let mut installations = self.installations.lock().await;
        if let Some(existing) = installations
            .iter_mut()
            .find(|existing| existing.id == installation.id)
        {
            *existing = installation.clone();
        }
        Ok(())
    }

    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        extension_key: &str,
    ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
        Ok(self
            .installations
            .lock()
            .await
            .iter()
            .find(|installation| {
                installation.project_id == project_id && installation.extension_key == extension_key
            })
            .cloned())
    }

    async fn get_by_project_and_id(
        &self,
        project_id: Uuid,
        installation_id: Uuid,
    ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
        Ok(self
            .installations
            .lock()
            .await
            .iter()
            .find(|installation| {
                installation.project_id == project_id && installation.id == installation_id
            })
            .cloned())
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
        Ok(self
            .installations
            .lock()
            .await
            .iter()
            .filter(|installation| installation.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn list_enabled_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
        Ok(self
            .list_by_project(project_id)
            .await?
            .into_iter()
            .filter(|installation| installation.enabled)
            .collect())
    }

    async fn delete(&self, project_id: Uuid, installation_id: Uuid) -> Result<bool, DomainError> {
        let mut installations = self.installations.lock().await;
        let before = installations.len();
        installations.retain(|installation| {
            installation.project_id != project_id || installation.id != installation_id
        });
        Ok(installations.len() != before)
    }
}

impl MemoryProjectExtensionInstallationRepository {
    pub fn new_with_installations(installations: Vec<ProjectExtensionInstallation>) -> Self {
        Self {
            installations: Mutex::new(installations),
        }
    }

    pub async fn debug_list(&self) -> Vec<ProjectExtensionInstallation> {
        self.installations.lock().await.clone()
    }
}
