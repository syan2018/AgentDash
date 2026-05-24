use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::shared_library::InstalledAssetSource;

pub struct PostgresProjectAgentRepository {
    pool: PgPool,
}

impl PostgresProjectAgentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["project_agents"]).await
    }
}

#[derive(sqlx::FromRow)]
struct ProjectAgentRow {
    id: String,
    project_id: String,
    name: String,
    agent_type: String,
    config: String,
    installed_library_asset_id: Option<String>,
    installed_source_ref: Option<String>,
    installed_source_version: Option<String>,
    installed_source_digest: Option<String>,
    installed_at: Option<String>,
    default_lifecycle_key: Option<String>,
    is_default_for_story: bool,
    is_default_for_task: bool,
    knowledge_enabled: bool,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ProjectAgentRow> for ProjectAgent {
    type Error = DomainError;

    fn try_from(row: ProjectAgentRow) -> Result<Self, Self::Error> {
        Ok(ProjectAgent {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| DomainError::InvalidConfig(format!("project_agents.id: {e}")))?,
            project_id: Uuid::parse_str(&row.project_id).map_err(|e| {
                DomainError::InvalidConfig(format!("project_agents.project_id: {e}"))
            })?,
            name: row.name,
            agent_type: row.agent_type,
            config: parse_json_column(&row.config, "project_agents.config")?,
            installed_source: parse_installed_source(
                row.installed_library_asset_id,
                row.installed_source_ref,
                row.installed_source_version,
                row.installed_source_digest,
                row.installed_at,
            )?,
            default_lifecycle_key: row.default_lifecycle_key,
            is_default_for_story: row.is_default_for_story,
            is_default_for_task: row.is_default_for_task,
            knowledge_enabled: row.knowledge_enabled,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "project_agents.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "project_agents.updated_at",
            )?,
        })
    }
}

const PROJECT_AGENT_COLUMNS: &str = "id, project_id, name, agent_type, config, installed_library_asset_id, installed_source_ref, installed_source_version, installed_source_digest, installed_at, default_lifecycle_key, is_default_for_story, is_default_for_task, knowledge_enabled, created_at, updated_at";

#[async_trait::async_trait]
impl ProjectAgentRepository for PostgresProjectAgentRepository {
    async fn create(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
        let config_json = serialize_json_column(&agent.config, "project_agents.config")?;
        sqlx::query(
            "INSERT INTO project_agents (id, project_id, name, agent_type, config, installed_library_asset_id, installed_source_ref, installed_source_version, installed_source_digest, installed_at, default_lifecycle_key, is_default_for_story, is_default_for_task, knowledge_enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        )
        .bind(agent.id.to_string())
        .bind(agent.project_id.to_string())
        .bind(&agent.name)
        .bind(&agent.agent_type)
        .bind(config_json)
        .bind(installed_library_asset_id(&agent.installed_source))
        .bind(installed_source_ref(&agent.installed_source))
        .bind(installed_source_version(&agent.installed_source))
        .bind(installed_source_digest(&agent.installed_source))
        .bind(installed_at(&agent.installed_source))
        .bind(&agent.default_lifecycle_key)
        .bind(agent.is_default_for_story)
        .bind(agent.is_default_for_task)
        .bind(agent.knowledge_enabled)
        .bind(agent.created_at.to_rfc3339())
        .bind(agent.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgent>, DomainError> {
        let sql = format!("SELECT {PROJECT_AGENT_COLUMNS} FROM project_agents WHERE id = $1");
        let row: Option<ProjectAgentRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(ProjectAgent::try_from).transpose()
    }

    async fn get_by_project_and_id(
        &self,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ProjectAgent>, DomainError> {
        let sql = format!(
            "SELECT {PROJECT_AGENT_COLUMNS} FROM project_agents WHERE project_id = $1 AND id = $2"
        );
        let row: Option<ProjectAgentRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(ProjectAgent::try_from).transpose()
    }

    async fn get_by_project_and_name(
        &self,
        project_id: Uuid,
        name: &str,
    ) -> Result<Option<ProjectAgent>, DomainError> {
        let sql = format!(
            "SELECT {PROJECT_AGENT_COLUMNS} FROM project_agents WHERE project_id = $1 AND name = $2"
        );
        let row: Option<ProjectAgentRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(ProjectAgent::try_from).transpose()
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectAgent>, DomainError> {
        let sql = format!(
            "SELECT {PROJECT_AGENT_COLUMNS} FROM project_agents WHERE project_id = $1 ORDER BY created_at"
        );
        let rows: Vec<ProjectAgentRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(ProjectAgent::try_from).collect()
    }

    async fn update(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
        let config_json = serialize_json_column(&agent.config, "project_agents.config")?;
        sqlx::query(
            "UPDATE project_agents SET name = $1, agent_type = $2, config = $3, installed_library_asset_id = $4, installed_source_ref = $5, installed_source_version = $6, installed_source_digest = $7, installed_at = $8, default_lifecycle_key = $9, is_default_for_story = $10, is_default_for_task = $11, knowledge_enabled = $12, updated_at = $13 WHERE id = $14 AND project_id = $15",
        )
        .bind(&agent.name)
        .bind(&agent.agent_type)
        .bind(config_json)
        .bind(installed_library_asset_id(&agent.installed_source))
        .bind(installed_source_ref(&agent.installed_source))
        .bind(installed_source_version(&agent.installed_source))
        .bind(installed_source_digest(&agent.installed_source))
        .bind(installed_at(&agent.installed_source))
        .bind(&agent.default_lifecycle_key)
        .bind(agent.is_default_for_story)
        .bind(agent.is_default_for_task)
        .bind(agent.knowledge_enabled)
        .bind(agent.updated_at.to_rfc3339())
        .bind(agent.id.to_string())
        .bind(agent.project_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM project_agents WHERE project_id = $1 AND id = $2")
            .bind(project_id.to_string())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn serialize_json_column<T: serde::Serialize>(
    value: &T,
    field: &str,
) -> Result<String, DomainError> {
    serde_json::to_string(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn installed_library_asset_id(source: &Option<InstalledAssetSource>) -> Option<String> {
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

fn installed_at(source: &Option<InstalledAssetSource>) -> Option<String> {
    source
        .as_ref()
        .map(|source| source.installed_at.to_rfc3339())
}

fn parse_installed_source(
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<String>,
) -> Result<Option<InstalledAssetSource>, DomainError> {
    let Some(library_asset_id) = library_asset_id else {
        return Ok(None);
    };
    Ok(Some(InstalledAssetSource {
        library_asset_id: Uuid::parse_str(&library_asset_id).map_err(|_| {
            DomainError::InvalidConfig("installed_source.library_asset_id 无效".to_string())
        })?,
        source_ref: required_installed_source_field(source_ref, "installed_source.source_ref")?,
        source_version: required_installed_source_field(
            source_version,
            "installed_source.source_version",
        )?,
        source_digest: required_installed_source_field(
            source_digest,
            "installed_source.source_digest",
        )?,
        installed_at: super::parse_pg_timestamp_checked(
            &required_installed_source_field(installed_at, "installed_source.installed_at")?,
            "installed_source.installed_at",
        )?,
    }))
}

fn required_installed_source_field(
    value: Option<String>,
    field: &str,
) -> Result<String, DomainError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DomainError::InvalidConfig(format!("{field} 为空")))
}
