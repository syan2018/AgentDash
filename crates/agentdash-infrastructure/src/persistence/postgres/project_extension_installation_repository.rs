use sqlx::types::Json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use agentdash_domain::DomainError;
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
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS project_extension_installations (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                extension_key TEXT NOT NULL,
                display_name TEXT NOT NULL,
                enabled BOOLEAN NOT NULL DEFAULT TRUE,
                config JSONB NOT NULL DEFAULT '{}',
                manifest JSONB NOT NULL,
                installed_library_asset_id TEXT NOT NULL,
                installed_source_ref TEXT NOT NULL,
                installed_source_version TEXT NOT NULL,
                installed_source_digest TEXT NOT NULL,
                installed_at TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                CONSTRAINT project_extension_installations_unique_key UNIQUE (project_id, extension_key)
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_project_extension_installations_project
             ON project_extension_installations(project_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
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
                installed_source_digest, installed_at, created_at, updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(installation.id.to_string())
        .bind(installation.project_id.to_string())
        .bind(&installation.extension_key)
        .bind(&installation.display_name)
        .bind(installation.enabled)
        .bind(Json(installation.config.clone()))
        .bind(Json(manifest))
        .bind(installation.installed_source.library_asset_id.to_string())
        .bind(&installation.installed_source.source_ref)
        .bind(&installation.installed_source.source_version)
        .bind(&installation.installed_source.source_digest)
        .bind(installation.installed_source.installed_at.to_rfc3339())
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
                installed_source_digest=$10, installed_at=$11, updated_at=$12
             WHERE id=$13",
        )
        .bind(installation.project_id.to_string())
        .bind(&installation.extension_key)
        .bind(&installation.display_name)
        .bind(installation.enabled)
        .bind(Json(installation.config.clone()))
        .bind(Json(manifest))
        .bind(installation.installed_source.library_asset_id.to_string())
        .bind(&installation.installed_source.source_ref)
        .bind(&installation.installed_source.source_version)
        .bind(&installation.installed_source.source_digest)
        .bind(installation.installed_source.installed_at.to_rfc3339())
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
    let installed_source = InstalledAssetSource {
        library_asset_id: parse_uuid(&row, "installed_library_asset_id")?,
        source_ref: row.try_get("installed_source_ref").map_err(db_err)?,
        source_version: row.try_get("installed_source_version").map_err(db_err)?,
        source_digest: row.try_get("installed_source_digest").map_err(db_err)?,
        installed_at: parse_pg_timestamp_checked(
            row.try_get::<String, _>("installed_at")
                .map_err(db_err)?
                .as_str(),
            "project_extension_installations.installed_at",
        )?,
    };
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

fn parse_uuid(row: &sqlx::postgres::PgRow, field: &str) -> Result<Uuid, DomainError> {
    let raw: String = row.try_get(field).map_err(db_err)?;
    Uuid::parse_str(&raw).map_err(|error| {
        DomainError::InvalidConfig(format!("project_extension_installations.{field}: {error}"))
    })
}

fn db_err(error: sqlx::Error) -> DomainError {
    DomainError::InvalidConfig(format!("project_extension_installations: {error}"))
}
