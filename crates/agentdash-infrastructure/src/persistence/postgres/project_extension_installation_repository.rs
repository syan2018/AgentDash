use sqlx::types::Json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::extension_package::ExtensionPackageArtifactRef;
use agentdash_domain::shared_library::{
    ExtensionTemplatePayload, InstalledAssetSource, ProjectExtensionInstallation,
    ProjectExtensionInstallationRepository,
};

use super::parse_pg_timestamp_checked;

#[derive(Clone)]
pub struct PostgresProjectExtensionInstallationRepository {
    pool: PgPool,
}

impl PostgresProjectExtensionInstallationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["project_extension_installations"],
        )
        .await
    }
}

#[async_trait::async_trait]
impl ProjectExtensionInstallationRepository for PostgresProjectExtensionInstallationRepository {
    async fn create(&self, installation: &ProjectExtensionInstallation) -> Result<(), DomainError> {
        let manifest =
            serde_json::to_value(&installation.manifest).map_err(DomainError::Serialization)?;
        sqlx::query(
            "INSERT INTO project_extension_installations (
                id, project_id, extension_key, display_name, enabled, config, manifest,
                installed_library_asset_id, installed_source_ref, installed_source_version,
                installed_source_digest, installed_at,
                package_artifact_id, package_name, package_version, package_asset_version,
                package_source_version, artifact_storage_ref, artifact_archive_digest,
                artifact_manifest_digest, created_at, updated_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                $13, $14, $15, $16, $17, $18, $19, $20, $21, $22
             )",
        )
        .bind(installation.id.to_string())
        .bind(installation.project_id.to_string())
        .bind(&installation.extension_key)
        .bind(&installation.display_name)
        .bind(installation.enabled)
        .bind(Json(installation.config.clone()))
        .bind(Json(manifest))
        .bind(installed_source_library_asset_id(
            &installation.installed_source,
        ))
        .bind(installed_source_ref(&installation.installed_source))
        .bind(installed_source_version(&installation.installed_source))
        .bind(installed_source_digest(&installation.installed_source))
        .bind(installed_source_installed_at(
            &installation.installed_source,
        ))
        .bind(package_artifact_id(&installation.package_artifact))
        .bind(package_name(&installation.package_artifact))
        .bind(package_version(&installation.package_artifact))
        .bind(package_asset_version(&installation.package_artifact))
        .bind(package_source_version(&installation.package_artifact))
        .bind(artifact_storage_ref(&installation.package_artifact))
        .bind(artifact_archive_digest(&installation.package_artifact))
        .bind(artifact_manifest_digest(&installation.package_artifact))
        .bind(installation.created_at.to_rfc3339())
        .bind(installation.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn update(&self, installation: &ProjectExtensionInstallation) -> Result<(), DomainError> {
        let manifest =
            serde_json::to_value(&installation.manifest).map_err(DomainError::Serialization)?;
        let result = sqlx::query(
            "UPDATE project_extension_installations SET
                project_id=$1, extension_key=$2, display_name=$3, enabled=$4, config=$5, manifest=$6,
                installed_library_asset_id=$7, installed_source_ref=$8, installed_source_version=$9,
                installed_source_digest=$10, installed_at=$11,
                package_artifact_id=$12, package_name=$13, package_version=$14,
                package_asset_version=$15, package_source_version=$16, artifact_storage_ref=$17,
                artifact_archive_digest=$18, artifact_manifest_digest=$19, updated_at=$20
             WHERE id=$21",
        )
        .bind(installation.project_id.to_string())
        .bind(&installation.extension_key)
        .bind(&installation.display_name)
        .bind(installation.enabled)
        .bind(Json(installation.config.clone()))
        .bind(Json(manifest))
        .bind(installed_source_library_asset_id(&installation.installed_source))
        .bind(installed_source_ref(&installation.installed_source))
        .bind(installed_source_version(&installation.installed_source))
        .bind(installed_source_digest(&installation.installed_source))
        .bind(installed_source_installed_at(&installation.installed_source))
        .bind(package_artifact_id(&installation.package_artifact))
        .bind(package_name(&installation.package_artifact))
        .bind(package_version(&installation.package_artifact))
        .bind(package_asset_version(&installation.package_artifact))
        .bind(package_source_version(&installation.package_artifact))
        .bind(artifact_storage_ref(&installation.package_artifact))
        .bind(artifact_archive_digest(&installation.package_artifact))
        .bind(artifact_manifest_digest(&installation.package_artifact))
        .bind(installation.updated_at.to_rfc3339())
        .bind(installation.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "project_extension_installation",
                id: installation.id.to_string(),
            });
        }
        Ok(())
    }

    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        extension_key: &str,
    ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM project_extension_installations
             WHERE project_id = $1 AND extension_key = $2",
        )
        .bind(project_id.to_string())
        .bind(extension_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        row.map(row_to_installation).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM project_extension_installations
             WHERE project_id = $1 ORDER BY extension_key ASC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        rows.into_iter().map(row_to_installation).collect()
    }

    async fn list_enabled_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM project_extension_installations
             WHERE project_id = $1 AND enabled = TRUE ORDER BY extension_key ASC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        rows.into_iter().map(row_to_installation).collect()
    }
}

fn row_to_installation(
    row: sqlx::postgres::PgRow,
) -> Result<ProjectExtensionInstallation, DomainError> {
    let id = parse_uuid(&row, "id")?;
    let project_id = parse_uuid(&row, "project_id")?;
    let manifest: Json<serde_json::Value> = row.try_get("manifest").map_err(db_err)?;
    let manifest: ExtensionTemplatePayload =
        serde_json::from_value(manifest.0).map_err(DomainError::Serialization)?;
    manifest.validate()?;
    let installed_source = row_to_installed_source(&row)?;
    let package_artifact = row_to_package_artifact(&row)?;
    Ok(ProjectExtensionInstallation {
        id,
        project_id,
        extension_key: row.try_get("extension_key").map_err(db_err)?,
        display_name: row.try_get("display_name").map_err(db_err)?,
        enabled: row.try_get("enabled").map_err(db_err)?,
        config: row
            .try_get::<Json<serde_json::Value>, _>("config")
            .map_err(db_err)?
            .0,
        manifest,
        installed_source,
        package_artifact,
        created_at: parse_pg_timestamp_checked(
            row.try_get::<String, _>("created_at")
                .map_err(db_err)?
                .as_str(),
            "project_extension_installations.created_at",
        )?,
        updated_at: parse_pg_timestamp_checked(
            row.try_get::<String, _>("updated_at")
                .map_err(db_err)?
                .as_str(),
            "project_extension_installations.updated_at",
        )?,
    })
}

fn installed_source_library_asset_id(source: &Option<InstalledAssetSource>) -> Option<String> {
    source
        .as_ref()
        .map(|source| source.library_asset_id.to_string())
}

fn installed_source_ref(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_ref.as_str())
}

fn installed_source_version(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_version.as_str())
}

fn installed_source_digest(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_digest.as_str())
}

fn installed_source_installed_at(source: &Option<InstalledAssetSource>) -> Option<String> {
    source
        .as_ref()
        .map(|source| source.installed_at.to_rfc3339())
}

fn package_artifact_id(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<String> {
    artifact
        .as_ref()
        .map(|artifact| artifact.artifact_id.to_string())
}

fn package_name(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<&str> {
    artifact
        .as_ref()
        .map(|artifact| artifact.package_name.as_str())
}

fn package_version(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<&str> {
    artifact
        .as_ref()
        .map(|artifact| artifact.package_version.as_str())
}

fn package_asset_version(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<&str> {
    artifact
        .as_ref()
        .map(|artifact| artifact.asset_version.as_str())
}

fn package_source_version(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<&str> {
    artifact
        .as_ref()
        .map(|artifact| artifact.source_version.as_str())
}

fn artifact_storage_ref(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<&str> {
    artifact
        .as_ref()
        .map(|artifact| artifact.storage_ref.as_str())
}

fn artifact_archive_digest(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<&str> {
    artifact
        .as_ref()
        .map(|artifact| artifact.archive_digest.as_str())
}

fn artifact_manifest_digest(artifact: &Option<ExtensionPackageArtifactRef>) -> Option<&str> {
    artifact
        .as_ref()
        .map(|artifact| artifact.manifest_digest.as_str())
}

fn row_to_installed_source(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<InstalledAssetSource>, DomainError> {
    let library_asset_id: Option<String> =
        row.try_get("installed_library_asset_id").map_err(db_err)?;
    let Some(library_asset_id) = library_asset_id else {
        return Ok(None);
    };
    let source_ref = required_optional_column(row, "installed_source_ref")?;
    let source_version = required_optional_column(row, "installed_source_version")?;
    let source_digest = required_optional_column(row, "installed_source_digest")?;
    let installed_at = required_optional_column(row, "installed_at")?;
    Ok(Some(InstalledAssetSource {
        library_asset_id: parse_uuid_value(&library_asset_id, "installed_library_asset_id")?,
        source_ref,
        source_version,
        source_digest,
        installed_at: parse_pg_timestamp_checked(
            &installed_at,
            "project_extension_installations.installed_at",
        )?,
    }))
}

fn row_to_package_artifact(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<ExtensionPackageArtifactRef>, DomainError> {
    let artifact_id: Option<String> = row.try_get("package_artifact_id").map_err(db_err)?;
    let Some(artifact_id) = artifact_id else {
        return Ok(None);
    };
    let artifact = ExtensionPackageArtifactRef {
        artifact_id: parse_uuid_value(&artifact_id, "package_artifact_id")?,
        package_name: required_optional_column(row, "package_name")?,
        package_version: required_optional_column(row, "package_version")?,
        asset_version: required_optional_column(row, "package_asset_version")?,
        source_version: required_optional_column(row, "package_source_version")?,
        storage_ref: required_optional_column(row, "artifact_storage_ref")?,
        archive_digest: required_optional_column(row, "artifact_archive_digest")?,
        manifest_digest: required_optional_column(row, "artifact_manifest_digest")?,
    };
    artifact.validate()?;
    Ok(Some(artifact))
}

fn required_optional_column(
    row: &sqlx::postgres::PgRow,
    field: &str,
) -> Result<String, DomainError> {
    row.try_get::<Option<String>, _>(field)
        .map_err(db_err)?
        .ok_or_else(|| {
            DomainError::InvalidConfig(format!("project_extension_installations.{field} 为空"))
        })
}

fn parse_uuid(row: &sqlx::postgres::PgRow, field: &str) -> Result<Uuid, DomainError> {
    let raw: String = row.try_get(field).map_err(db_err)?;
    parse_uuid_value(&raw, field)
}

fn parse_uuid_value(raw: &str, field: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(&raw).map_err(|error| {
        DomainError::InvalidConfig(format!("project_extension_installations.{field}: {error}"))
    })
}

fn db_err(error: sqlx::Error) -> DomainError {
    DomainError::InvalidConfig(format!("project_extension_installations: {error}"))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use agentdash_domain::extension_package::{
        ExtensionPackageArtifactRef, ExtensionPackageMetadata,
    };
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionTemplatePayload,
        ProjectExtensionInstallation, ProjectExtensionInstallationRepository,
    };

    use super::PostgresProjectExtensionInstallationRepository;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo() -> Option<PostgresProjectExtensionInstallationRepository> {
        let pool = test_pg_pool("project_extension_installation_repository").await?;
        let repo = PostgresProjectExtensionInstallationRepository::new(pool);
        repo.initialize()
            .await
            .expect("project extension installation schema should be ready");
        Some(repo)
    }

    fn sample_manifest() -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "local-hello".to_string(),
            package: ExtensionPackageMetadata {
                name: "@agentdash/local-hello".to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![],
            workspace_tabs: vec![],
            permissions: vec![],
            bundles: vec![ExtensionBundleRef {
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            }],
        }
    }

    fn sample_package_ref(artifact_id: Uuid) -> ExtensionPackageArtifactRef {
        ExtensionPackageArtifactRef {
            artifact_id,
            package_name: "@agentdash/local-hello".to_string(),
            package_version: "0.1.0".to_string(),
            asset_version: "0.1.0".to_string(),
            source_version: "0.1.0".to_string(),
            storage_ref: format!(
                "extension-packages/{artifact_id}/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.agentdash-extension.tgz"
            ),
            archive_digest:
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            manifest_digest:
                "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                    .to_string(),
        }
    }

    #[tokio::test]
    async fn packaged_installation_roundtrips_artifact_ref() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let installation = ProjectExtensionInstallation::new_packaged(
            project_id,
            format!("local-hello-{artifact_id}"),
            "Local Hello",
            sample_manifest(),
            sample_package_ref(artifact_id),
        )
        .expect("valid packaged installation");

        repo.create(&installation)
            .await
            .expect("create packaged installation");

        let loaded = repo
            .get_by_project_and_key(project_id, &installation.extension_key)
            .await
            .expect("load")
            .expect("exists");
        assert!(loaded.installed_source.is_none());
        let package_artifact = loaded.package_artifact.expect("package artifact");
        assert_eq!(package_artifact.artifact_id, artifact_id);
        assert_eq!(package_artifact.package_name, "@agentdash/local-hello");

        let enabled = repo
            .list_enabled_by_project(project_id)
            .await
            .expect("list enabled");
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].extension_key, installation.extension_key);
    }
}
