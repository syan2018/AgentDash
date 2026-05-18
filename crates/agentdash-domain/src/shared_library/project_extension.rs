use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::DomainError;

use super::value_objects::{ExtensionTemplatePayload, InstalledAssetSource};

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectExtensionInstallation {
    pub id: Uuid,
    pub project_id: Uuid,
    pub extension_key: String,
    pub display_name: String,
    pub enabled: bool,
    pub config: Value,
    pub manifest: ExtensionTemplatePayload,
    pub installed_source: InstalledAssetSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectExtensionInstallation {
    pub fn new(
        project_id: Uuid,
        extension_key: impl Into<String>,
        display_name: impl Into<String>,
        manifest: ExtensionTemplatePayload,
        installed_source: InstalledAssetSource,
    ) -> Result<Self, DomainError> {
        manifest.validate()?;
        let extension_key = extension_key.into();
        if extension_key.trim().is_empty() {
            return Err(DomainError::InvalidConfig(
                "ProjectExtensionInstallation.extension_key 不能为空".to_string(),
            ));
        }
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(DomainError::InvalidConfig(
                "ProjectExtensionInstallation.display_name 不能为空".to_string(),
            ));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            extension_key,
            display_name,
            enabled: true,
            config: Value::Object(Default::default()),
            manifest,
            installed_source,
            created_at: now,
            updated_at: now,
        })
    }
}

#[async_trait::async_trait]
pub trait ProjectExtensionInstallationRepository: Send + Sync {
    async fn create(&self, installation: &ProjectExtensionInstallation) -> Result<(), DomainError>;
    async fn update(&self, installation: &ProjectExtensionInstallation) -> Result<(), DomainError>;
    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        extension_key: &str,
    ) -> Result<Option<ProjectExtensionInstallation>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError>;
    async fn list_enabled_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError>;
}
