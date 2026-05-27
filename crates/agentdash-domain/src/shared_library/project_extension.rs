use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::DomainError;
use crate::extension_package::ExtensionPackageArtifactRef;

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
    pub installed_source: Option<InstalledAssetSource>,
    pub package_artifact: Option<ExtensionPackageArtifactRef>,
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
        Self::build(
            project_id,
            extension_key,
            display_name,
            manifest,
            Some(installed_source),
            None,
        )
    }

    pub fn new_packaged(
        project_id: Uuid,
        extension_key: impl Into<String>,
        display_name: impl Into<String>,
        manifest: ExtensionTemplatePayload,
        package_artifact: ExtensionPackageArtifactRef,
    ) -> Result<Self, DomainError> {
        Self::build(
            project_id,
            extension_key,
            display_name,
            manifest,
            None,
            Some(package_artifact),
        )
    }

    fn build(
        project_id: Uuid,
        extension_key: impl Into<String>,
        display_name: impl Into<String>,
        manifest: ExtensionTemplatePayload,
        installed_source: Option<InstalledAssetSource>,
        package_artifact: Option<ExtensionPackageArtifactRef>,
    ) -> Result<Self, DomainError> {
        manifest.validate()?;
        if let Some(package_artifact) = &package_artifact {
            package_artifact.validate()?;
        }
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
            package_artifact,
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
    async fn get_by_project_and_id(
        &self,
        project_id: Uuid,
        installation_id: Uuid,
    ) -> Result<Option<ProjectExtensionInstallation>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError>;
    async fn list_enabled_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError>;
    /// 删除指定 Project 下的 installation。返回 true 表示删除成功；
    /// 返回 false 表示该 (project_id, installation_id) 不存在。
    async fn delete(&self, project_id: Uuid, installation_id: Uuid) -> Result<bool, DomainError>;
}
