use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DomainError;
use crate::shared_library::ExtensionTemplatePayload;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionPackageMetadata {
    pub name: String,
    pub version: String,
}

impl ExtensionPackageMetadata {
    pub fn validate(&self) -> Result<(), DomainError> {
        require_non_empty("extension_template.package.name", &self.name)?;
        require_non_empty("extension_template.package.version", &self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPackageArtifactRef {
    pub artifact_id: Uuid,
    pub package_name: String,
    pub package_version: String,
    pub asset_version: String,
    pub source_version: String,
    pub storage_ref: String,
    pub archive_digest: String,
    pub manifest_digest: String,
}

impl ExtensionPackageArtifactRef {
    pub fn from_artifact(artifact: &ExtensionPackageArtifact) -> Self {
        Self {
            artifact_id: artifact.id,
            package_name: artifact.package_name.clone(),
            package_version: artifact.package_version.clone(),
            asset_version: artifact.asset_version.clone(),
            source_version: artifact.source_version.clone(),
            storage_ref: artifact.storage_ref.clone(),
            archive_digest: artifact.archive_digest.clone(),
            manifest_digest: artifact.manifest_digest.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        require_non_empty(
            "project_extension_installations.package_name",
            &self.package_name,
        )?;
        require_non_empty(
            "project_extension_installations.package_version",
            &self.package_version,
        )?;
        require_non_empty(
            "project_extension_installations.asset_version",
            &self.asset_version,
        )?;
        require_non_empty(
            "project_extension_installations.source_version",
            &self.source_version,
        )?;
        require_non_empty(
            "project_extension_installations.storage_ref",
            &self.storage_ref,
        )?;
        validate_sha256_digest(
            "project_extension_installations.archive_digest",
            &self.archive_digest,
        )?;
        validate_sha256_digest(
            "project_extension_installations.manifest_digest",
            &self.manifest_digest,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionPackageArtifact {
    pub id: Uuid,
    pub project_id: Uuid,
    pub extension_id: String,
    pub package_name: String,
    pub package_version: String,
    pub asset_version: String,
    pub source_version: String,
    pub storage_ref: String,
    pub archive_digest: String,
    pub manifest_digest: String,
    pub manifest: ExtensionTemplatePayload,
    pub byte_size: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ExtensionPackageArtifact {
    pub fn new(
        project_id: Uuid,
        storage_ref: impl Into<String>,
        archive_digest: impl Into<String>,
        manifest_digest: impl Into<String>,
        manifest: ExtensionTemplatePayload,
        byte_size: i64,
    ) -> Result<Self, DomainError> {
        manifest.validate()?;
        let storage_ref = storage_ref.into();
        require_non_empty("extension_package_artifacts.storage_ref", &storage_ref)?;
        let archive_digest = archive_digest.into();
        validate_sha256_digest(
            "extension_package_artifacts.archive_digest",
            &archive_digest,
        )?;
        let manifest_digest = manifest_digest.into();
        validate_sha256_digest(
            "extension_package_artifacts.manifest_digest",
            &manifest_digest,
        )?;
        if byte_size <= 0 {
            return Err(DomainError::InvalidConfig(
                "extension_package_artifacts.byte_size 必须大于 0".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            extension_id: manifest.extension_id.clone(),
            package_name: manifest.package.name.clone(),
            package_version: manifest.package.version.clone(),
            asset_version: manifest.asset_version.clone(),
            source_version: manifest.asset_version.clone(),
            storage_ref,
            archive_digest,
            manifest_digest,
            manifest,
            byte_size,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn package_ref(&self) -> ExtensionPackageArtifactRef {
        ExtensionPackageArtifactRef::from_artifact(self)
    }
}

#[async_trait::async_trait]
pub trait ExtensionPackageArtifactRepository: Send + Sync {
    async fn create(&self, artifact: &ExtensionPackageArtifact) -> Result<(), DomainError>;
    async fn get(&self, id: Uuid) -> Result<Option<ExtensionPackageArtifact>, DomainError>;
    async fn get_by_project_and_digest(
        &self,
        project_id: Uuid,
        archive_digest: &str,
    ) -> Result<Option<ExtensionPackageArtifact>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ExtensionPackageArtifact>, DomainError>;
}

pub fn validate_sha256_digest(field: &str, value: &str) -> Result<(), DomainError> {
    require_non_empty(field, value)?;
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(DomainError::InvalidConfig(format!(
            "{field} 必须使用 sha256:<hex> 格式"
        )));
    };
    let valid = hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidConfig(format!(
            "{field} 必须包含 64 位 sha256 十六进制摘要"
        )))
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        Err(DomainError::InvalidConfig(format!("{field} 不能为空")))
    } else {
        Ok(())
    }
}
