use sqlx::types::Json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::extension_package::{
    ExtensionPackageArtifact, ExtensionPackageArtifactRepository,
};
use agentdash_domain::shared_library::ExtensionTemplatePayload;

use super::parse_pg_timestamp_checked;

#[derive(Clone)]
pub struct PostgresExtensionPackageArtifactRepository {
    pool: PgPool,
}

impl PostgresExtensionPackageArtifactRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["extension_package_artifacts"])
            .await
    }
}

const COLS: &str = "id,project_id,extension_id,package_name,package_version,asset_version,source_version,storage_ref,archive_digest,manifest_digest,manifest,byte_size,created_at,updated_at";

#[async_trait::async_trait]
impl ExtensionPackageArtifactRepository for PostgresExtensionPackageArtifactRepository {
    async fn create(&self, artifact: &ExtensionPackageArtifact) -> Result<(), DomainError> {
        artifact.manifest.validate()?;
        let manifest =
            serde_json::to_value(&artifact.manifest).map_err(DomainError::Serialization)?;
        sqlx::query(&format!(
            "INSERT INTO extension_package_artifacts ({COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"
        ))
        .bind(artifact.id.to_string())
        .bind(artifact.project_id.to_string())
        .bind(&artifact.extension_id)
        .bind(&artifact.package_name)
        .bind(&artifact.package_version)
        .bind(&artifact.asset_version)
        .bind(&artifact.source_version)
        .bind(&artifact.storage_ref)
        .bind(&artifact.archive_digest)
        .bind(&artifact.manifest_digest)
        .bind(Json(manifest))
        .bind(artifact.byte_size)
        .bind(artifact.created_at.to_rfc3339())
        .bind(artifact.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<ExtensionPackageArtifact>, DomainError> {
        sqlx::query(&format!(
            "SELECT {COLS} FROM extension_package_artifacts WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(row_to_artifact)
        .transpose()
    }

    async fn get_by_project_and_digest(
        &self,
        project_id: Uuid,
        archive_digest: &str,
    ) -> Result<Option<ExtensionPackageArtifact>, DomainError> {
        sqlx::query(&format!(
            "SELECT {COLS} FROM extension_package_artifacts WHERE project_id = $1 AND archive_digest = $2"
        ))
        .bind(project_id.to_string())
        .bind(archive_digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(row_to_artifact)
        .transpose()
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ExtensionPackageArtifact>, DomainError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM extension_package_artifacts WHERE project_id = $1 ORDER BY created_at DESC, id ASC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        rows.into_iter().map(row_to_artifact).collect()
    }
}

fn row_to_artifact(row: sqlx::postgres::PgRow) -> Result<ExtensionPackageArtifact, DomainError> {
    let manifest: Json<serde_json::Value> = row.try_get("manifest").map_err(db_err)?;
    let manifest: ExtensionTemplatePayload =
        serde_json::from_value(manifest.0).map_err(DomainError::Serialization)?;
    manifest.validate()?;

    let artifact = ExtensionPackageArtifact {
        id: parse_uuid(&row, "id")?,
        project_id: parse_uuid(&row, "project_id")?,
        extension_id: row.try_get("extension_id").map_err(db_err)?,
        package_name: row.try_get("package_name").map_err(db_err)?,
        package_version: row.try_get("package_version").map_err(db_err)?,
        asset_version: row.try_get("asset_version").map_err(db_err)?,
        source_version: row.try_get("source_version").map_err(db_err)?,
        storage_ref: row.try_get("storage_ref").map_err(db_err)?,
        archive_digest: row.try_get("archive_digest").map_err(db_err)?,
        manifest_digest: row.try_get("manifest_digest").map_err(db_err)?,
        manifest,
        byte_size: row.try_get("byte_size").map_err(db_err)?,
        created_at: parse_pg_timestamp_checked(
            row.try_get::<String, _>("created_at")
                .map_err(db_err)?
                .as_str(),
            "extension_package_artifacts.created_at",
        )?,
        updated_at: parse_pg_timestamp_checked(
            row.try_get::<String, _>("updated_at")
                .map_err(db_err)?
                .as_str(),
            "extension_package_artifacts.updated_at",
        )?,
    };
    artifact.package_ref().validate()?;
    Ok(artifact)
}

fn parse_uuid(row: &sqlx::postgres::PgRow, field: &str) -> Result<Uuid, DomainError> {
    let raw: String = row.try_get(field).map_err(db_err)?;
    Uuid::parse_str(&raw).map_err(|error| {
        DomainError::InvalidConfig(format!("extension_package_artifacts.{field}: {error}"))
    })
}

fn db_err(error: sqlx::Error) -> DomainError {
    DomainError::InvalidConfig(format!("extension_package_artifacts: {error}"))
}

#[cfg(test)]
mod tests {
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionTemplatePayload,
    };

    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo() -> Option<PostgresExtensionPackageArtifactRepository> {
        let pool = test_pg_pool("extension_package_artifact_repository").await?;
        let repo = PostgresExtensionPackageArtifactRepository::new(pool);
        repo.initialize()
            .await
            .expect("extension package artifact schema should be ready");
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

    fn sample_artifact(project_id: Uuid) -> ExtensionPackageArtifact {
        ExtensionPackageArtifact::new(
            project_id,
            format!(
                "extension-packages/{project_id}/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.agentdash-extension.tgz"
            ),
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            sample_manifest(),
            128,
        )
        .expect("valid artifact")
    }

    #[tokio::test]
    async fn create_and_lookup_extension_package_artifact_roundtrip() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let artifact = sample_artifact(project_id);

        repo.create(&artifact).await.expect("create artifact");

        let loaded = repo.get(artifact.id).await.expect("get").expect("exists");
        assert_eq!(loaded.project_id, project_id);
        assert_eq!(loaded.package_name, "@agentdash/local-hello");
        assert_eq!(loaded.manifest.package.version, "0.1.0");

        let by_digest = repo
            .get_by_project_and_digest(project_id, &artifact.archive_digest)
            .await
            .expect("get by digest")
            .expect("digest match");
        assert_eq!(by_digest.id, artifact.id);

        let listed = repo
            .list_by_project(project_id)
            .await
            .expect("list project");
        assert_eq!(
            listed.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![artifact.id]
        );
    }
}
