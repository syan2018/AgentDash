use agentdash_domain::DomainError;
use agentdash_domain::skill_asset::{SkillAsset, SkillAssetRepository, SkillAssetSource};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct MemorySkillAssetRepository {
    assets: Mutex<Vec<SkillAsset>>,
}

#[async_trait::async_trait]
impl SkillAssetRepository for MemorySkillAssetRepository {
    async fn create(&self, asset: &SkillAsset) -> Result<(), DomainError> {
        self.assets.lock().await.push(asset.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<SkillAsset>, DomainError> {
        Ok(self
            .assets
            .lock()
            .await
            .iter()
            .find(|asset| asset.id == id)
            .cloned())
    }

    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<SkillAsset>, DomainError> {
        Ok(self
            .assets
            .lock()
            .await
            .iter()
            .find(|asset| asset.project_id == project_id && asset.key == key)
            .cloned())
    }

    async fn get_by_project_and_builtin_key(
        &self,
        project_id: Uuid,
        builtin_key: &str,
    ) -> Result<Option<SkillAsset>, DomainError> {
        Ok(self
            .assets
            .lock()
            .await
            .iter()
            .find(|asset| {
                asset.project_id == project_id
                    && matches!(
                        &asset.source,
                        SkillAssetSource::BuiltinSeed { key } if key == builtin_key
                    )
            })
            .cloned())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<SkillAsset>, DomainError> {
        Ok(self
            .assets
            .lock()
            .await
            .iter()
            .filter(|asset| asset.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, asset: &SkillAsset) -> Result<(), DomainError> {
        let mut assets = self.assets.lock().await;
        if let Some(existing) = assets.iter_mut().find(|existing| existing.id == asset.id) {
            *existing = asset.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.assets.lock().await.retain(|asset| asset.id != id);
        Ok(())
    }
}

impl MemorySkillAssetRepository {
    pub fn new_with_assets(assets: Vec<SkillAsset>) -> Self {
        Self {
            assets: Mutex::new(assets),
        }
    }

    pub async fn debug_list(&self) -> Vec<SkillAsset> {
        self.assets.lock().await.clone()
    }
}
