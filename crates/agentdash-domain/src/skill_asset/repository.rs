use async_trait::async_trait;
use uuid::Uuid;

use crate::DomainError;

use super::entity::SkillAsset;

#[async_trait]
pub trait SkillAssetRepository: Send + Sync {
    async fn create(&self, asset: &SkillAsset) -> Result<(), DomainError>;
    async fn get(&self, id: Uuid) -> Result<Option<SkillAsset>, DomainError>;
    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<SkillAsset>, DomainError>;
    async fn get_by_project_and_builtin_key(
        &self,
        project_id: Uuid,
        builtin_key: &str,
    ) -> Result<Option<SkillAsset>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<SkillAsset>, DomainError>;
    async fn update(&self, asset: &SkillAsset) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
