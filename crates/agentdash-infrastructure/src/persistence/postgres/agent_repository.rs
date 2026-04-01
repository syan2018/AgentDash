use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::agent::{
    Agent, AgentRepository, ProjectAgentLink, ProjectAgentLinkRepository,
};
use agentdash_domain::common::error::DomainError;

pub struct SqliteAgentRepository {
    pool: PgPool,
}

impl SqliteAgentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                agent_type TEXT NOT NULL,
                base_config TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS project_agent_links (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                config_override TEXT,
                default_lifecycle_key TEXT,
                is_default_for_story BOOLEAN NOT NULL DEFAULT FALSE,
                is_default_for_task BOOLEAN NOT NULL DEFAULT FALSE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(project_id, agent_id)
            );

            CREATE INDEX IF NOT EXISTS idx_pal_project ON project_agent_links(project_id);
            CREATE INDEX IF NOT EXISTS idx_pal_agent ON project_agent_links(agent_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct AgentRow {
    id: String,
    name: String,
    agent_type: String,
    base_config: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<AgentRow> for Agent {
    type Error = DomainError;

    fn try_from(row: AgentRow) -> Result<Self, Self::Error> {
        Ok(Agent {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| DomainError::InvalidConfig(format!("agents.id: {e}")))?,
            name: row.name,
            agent_type: row.agent_type,
            base_config: serde_json::from_str(&row.base_config).unwrap_or_default(),
            created_at: super::parse_pg_timestamp(&row.created_at),
            updated_at: super::parse_pg_timestamp(&row.updated_at),
        })
    }
}

#[async_trait::async_trait]
impl AgentRepository for SqliteAgentRepository {
    async fn create(&self, agent: &Agent) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO agents (id, name, agent_type, base_config, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(agent.id.to_string())
        .bind(&agent.name)
        .bind(&agent.agent_type)
        .bind(serde_json::to_string(&agent.base_config).unwrap_or_default())
        .bind(agent.created_at.to_rfc3339())
        .bind(agent.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Agent>, DomainError> {
        let row: Option<AgentRow> =
            sqlx::query_as("SELECT id, name, agent_type, base_config, created_at, updated_at FROM agents WHERE id = $1")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(Agent::try_from).transpose()
    }

    async fn list_all(&self) -> Result<Vec<Agent>, DomainError> {
        let rows: Vec<AgentRow> =
            sqlx::query_as("SELECT id, name, agent_type, base_config, created_at, updated_at FROM agents ORDER BY name")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(Agent::try_from).collect()
    }

    async fn update(&self, agent: &Agent) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE agents SET name = $1, agent_type = $2, base_config = $3, updated_at = $4 WHERE id = $5",
        )
        .bind(&agent.name)
        .bind(&agent.agent_type)
        .bind(serde_json::to_string(&agent.base_config).unwrap_or_default())
        .bind(agent.updated_at.to_rfc3339())
        .bind(agent.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }
}

// ─── ProjectAgentLink ───

#[derive(sqlx::FromRow)]
struct LinkRow {
    id: String,
    project_id: String,
    agent_id: String,
    config_override: Option<String>,
    default_lifecycle_key: Option<String>,
    is_default_for_story: bool,
    is_default_for_task: bool,
    created_at: String,
    updated_at: String,
}

impl TryFrom<LinkRow> for ProjectAgentLink {
    type Error = DomainError;

    fn try_from(row: LinkRow) -> Result<Self, Self::Error> {
        Ok(ProjectAgentLink {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| DomainError::InvalidConfig(format!("project_agent_links.id: {e}")))?,
            project_id: Uuid::parse_str(&row.project_id).map_err(|e| {
                DomainError::InvalidConfig(format!("project_agent_links.project_id: {e}"))
            })?,
            agent_id: Uuid::parse_str(&row.agent_id).map_err(|e| {
                DomainError::InvalidConfig(format!("project_agent_links.agent_id: {e}"))
            })?,
            config_override: row
                .config_override
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
            default_lifecycle_key: row.default_lifecycle_key,
            is_default_for_story: row.is_default_for_story,
            is_default_for_task: row.is_default_for_task,
            created_at: super::parse_pg_timestamp(&row.created_at),
            updated_at: super::parse_pg_timestamp(&row.updated_at),
        })
    }
}

const LINK_COLUMNS: &str = "id, project_id, agent_id, config_override, default_lifecycle_key, is_default_for_story, is_default_for_task, created_at, updated_at";

#[async_trait::async_trait]
impl ProjectAgentLinkRepository for SqliteAgentRepository {
    async fn create(&self, link: &ProjectAgentLink) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO project_agent_links (id, project_id, agent_id, config_override, default_lifecycle_key, is_default_for_story, is_default_for_task, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(link.id.to_string())
        .bind(link.project_id.to_string())
        .bind(link.agent_id.to_string())
        .bind(link.config_override.as_ref().and_then(|v| serde_json::to_string(v).ok()))
        .bind(&link.default_lifecycle_key)
        .bind(link.is_default_for_story)
        .bind(link.is_default_for_task)
        .bind(link.created_at.to_rfc3339())
        .bind(link.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgentLink>, DomainError> {
        let sql = format!("SELECT {LINK_COLUMNS} FROM project_agent_links WHERE id = $1");
        let row: Option<LinkRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(ProjectAgentLink::try_from).transpose()
    }

    async fn find_by_project_and_agent(
        &self,
        project_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<ProjectAgentLink>, DomainError> {
        let sql = format!(
            "SELECT {LINK_COLUMNS} FROM project_agent_links WHERE project_id = $1 AND agent_id = $2"
        );
        let row: Option<LinkRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .bind(agent_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(ProjectAgentLink::try_from).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectAgentLink>, DomainError> {
        let sql = format!(
            "SELECT {LINK_COLUMNS} FROM project_agent_links WHERE project_id = $1 ORDER BY created_at"
        );
        let rows: Vec<LinkRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(ProjectAgentLink::try_from).collect()
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<ProjectAgentLink>, DomainError> {
        let sql = format!(
            "SELECT {LINK_COLUMNS} FROM project_agent_links WHERE agent_id = $1 ORDER BY created_at"
        );
        let rows: Vec<LinkRow> = sqlx::query_as(&sql)
            .bind(agent_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(ProjectAgentLink::try_from).collect()
    }

    async fn update(&self, link: &ProjectAgentLink) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE project_agent_links SET config_override = $1, default_lifecycle_key = $2, is_default_for_story = $3, is_default_for_task = $4, updated_at = $5 WHERE id = $6",
        )
        .bind(link.config_override.as_ref().and_then(|v| serde_json::to_string(v).ok()))
        .bind(&link.default_lifecycle_key)
        .bind(link.is_default_for_story)
        .bind(link.is_default_for_task)
        .bind(link.updated_at.to_rfc3339())
        .bind(link.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM project_agent_links WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete_by_project_and_agent(
        &self,
        project_id: Uuid,
        agent_id: Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM project_agent_links WHERE project_id = $1 AND agent_id = $2")
            .bind(project_id.to_string())
            .bind(agent_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }
}
