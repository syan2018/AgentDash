use sqlx::{PgPool, Row};

use agentdash_domain::common::error::DomainError;
use agentdash_domain::story::{
    ChangeKind, Story, StoryContext, StoryPriority, StoryRepository, StoryStatus, StoryType,
};

use super::state_change_store::{append_state_change_in_tx, initialize_state_changes_schema};

pub struct PostgresStoryRepository {
    pool: PgPool,
}

impl PostgresStoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS stories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                default_workspace_id TEXT,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'created',
                priority TEXT NOT NULL DEFAULT 'p2',
                story_type TEXT NOT NULL DEFAULT 'feature',
                tags TEXT NOT NULL DEFAULT '[]',
                task_count INTEGER NOT NULL DEFAULT 0,
                context TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_stories_project ON stories(project_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.ensure_story_column("default_workspace_id", "TEXT")
            .await?;

        initialize_state_changes_schema(&self.pool).await?;

        Ok(())
    }

    async fn ensure_story_column(
        &self,
        column_name: &str,
        column_definition: &str,
    ) -> Result<(), DomainError> {
        let rows = sqlx::query(
            "SELECT column_name AS name
             FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'stories'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let column_names = rows
            .iter()
            .map(|row| {
                row.try_get::<String, _>("name")
                    .map_err(|e| DomainError::InvalidConfig(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let column_exists = column_names.iter().any(|name| name == column_name);

        if !column_exists {
            sqlx::query(&format!(
                "ALTER TABLE stories ADD COLUMN {column_name} {column_definition}"
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl StoryRepository for PostgresStoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "INSERT INTO stories (id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(story.id.to_string())
        .bind(story.project_id.to_string())
        .bind(story.default_workspace_id.map(|id| id.to_string()))
        .bind(&story.title)
        .bind(&story.description)
        .bind(serde_json::to_string(&story.status)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.priority)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.story_type)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.tags)?)
        .bind(story.task_count as i32)
        .bind(serde_json::to_string(&story.context)?)
        .bind(story.created_at.to_rfc3339())
        .bind(story.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        append_state_change_in_tx(
            &mut tx,
            story.project_id,
            story.id,
            ChangeKind::StoryCreated,
            story_payload(story)?,
            None,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Story>, DomainError> {
        let row = sqlx::query_as::<_, StoryRow>(
            "SELECT id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at
             FROM stories WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_by_project(&self, project_id: uuid::Uuid) -> Result<Vec<Story>, DomainError> {
        let rows = sqlx::query_as::<_, StoryRow>(
            "SELECT id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at
             FROM stories WHERE project_id = $1 ORDER BY created_at DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn update(&self, story: &Story) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE stories SET project_id = $1, default_workspace_id = $2, title = $3, description = $4, status = $5, priority = $6, story_type = $7, tags = $8, task_count = $9, context = $10, updated_at = $11
             WHERE id = $12",
        )
        .bind(story.project_id.to_string())
        .bind(story.default_workspace_id.map(|id| id.to_string()))
        .bind(&story.title)
        .bind(&story.description)
        .bind(serde_json::to_string(&story.status)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.priority)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.story_type)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.tags)?)
        .bind(story.task_count as i32)
        .bind(serde_json::to_string(&story.context)?)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(story.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "story",
                id: story.id.to_string(),
            });
        }

        append_state_change_in_tx(
            &mut tx,
            story.project_id,
            story.id,
            ChangeKind::StoryUpdated,
            story_payload(story)?,
            None,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM stories WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "story",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct StoryRow {
    id: String,
    project_id: String,
    default_workspace_id: Option<String>,
    title: String,
    description: String,
    status: String,
    priority: String,
    story_type: String,
    tags: String,
    task_count: i32,
    context: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<StoryRow> for Story {
    type Error = DomainError;

    fn try_from(row: StoryRow) -> Result<Self, Self::Error> {
        let project_id = row.project_id.parse().map_err(|_| DomainError::NotFound {
            entity: "story",
            id: row.id.clone(),
        })?;

        let context: StoryContext = parse_json_column(&row.context, "stories.context")?;
        let tags: Vec<String> = parse_json_column(&row.tags, "stories.tags")?;

        let default_workspace_id = row
            .default_workspace_id
            .as_deref()
            .map(|value| {
                value.parse().map_err(|error| {
                    DomainError::InvalidConfig(format!("stories.default_workspace_id: {error}"))
                })
            })
            .transpose()?;

        Ok(Story {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "story",
                id: row.id.clone(),
            })?,
            project_id,
            default_workspace_id,
            title: row.title,
            description: row.description,
            status: parse_story_status(&row.status)?,
            priority: parse_story_priority(&row.priority)?,
            story_type: parse_story_type(&row.story_type)?,
            tags,
            task_count: row.task_count.max(0) as u32,
            context,
            created_at: super::parse_pg_timestamp_checked(&row.created_at, "stories.created_at")?,
            updated_at: super::parse_pg_timestamp_checked(&row.updated_at, "stories.updated_at")?,
        })
    }
}

fn story_payload(story: &Story) -> Result<serde_json::Value, DomainError> {
    serde_json::to_value(story)
        .map_err(|error| DomainError::InvalidConfig(format!("stories.state_payload: {error}")))
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_story_status(raw: &str) -> Result<StoryStatus, DomainError> {
    match raw {
        "created" => Ok(StoryStatus::Created),
        "context_ready" => Ok(StoryStatus::ContextReady),
        "decomposed" => Ok(StoryStatus::Decomposed),
        "executing" => Ok(StoryStatus::Executing),
        "completed" => Ok(StoryStatus::Completed),
        "failed" => Ok(StoryStatus::Failed),
        "cancelled" | "canceled" => Ok(StoryStatus::Cancelled),
        _ => Err(DomainError::InvalidConfig(format!(
            "stories.status: 未知状态 `{raw}`"
        ))),
    }
}

fn parse_story_priority(raw: &str) -> Result<StoryPriority, DomainError> {
    match raw {
        "p0" => Ok(StoryPriority::P0),
        "p1" => Ok(StoryPriority::P1),
        "p2" => Ok(StoryPriority::P2),
        "p3" => Ok(StoryPriority::P3),
        _ => Err(DomainError::InvalidConfig(format!(
            "stories.priority: 未知优先级 `{raw}`"
        ))),
    }
}

fn parse_story_type(raw: &str) -> Result<StoryType, DomainError> {
    match raw {
        "feature" => Ok(StoryType::Feature),
        "bugfix" => Ok(StoryType::Bugfix),
        "refactor" => Ok(StoryType::Refactor),
        "docs" => Ok(StoryType::Docs),
        "test" => Ok(StoryType::Test),
        "other" => Ok(StoryType::Other),
        _ => Err(DomainError::InvalidConfig(format!(
            "stories.story_type: 未知类型 `{raw}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo_with_legacy_story_table() -> Option<PostgresStoryRepository> {
        let pool = match test_pg_pool("story_repository").await {
            Some(pool) => pool,
            None => return None,
        };

        sqlx::query(
            r#"
            CREATE TABLE projects (
                id TEXT PRIMARY KEY
            );

            CREATE TABLE stories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'created',
                priority TEXT NOT NULL DEFAULT 'p2',
                story_type TEXT NOT NULL DEFAULT 'feature',
                tags TEXT NOT NULL DEFAULT '[]',
                task_count INTEGER NOT NULL DEFAULT 0,
                context TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("应能创建旧版 stories 表");

        let repo = PostgresStoryRepository::new(pool);
        repo.initialize().await.expect("初始化时应能自动补齐缺失列");
        Some(repo)
    }

    #[tokio::test]
    async fn initialize_adds_default_workspace_id_for_legacy_story_table() {
        let Some(repo) = new_repo_with_legacy_story_table().await else {
            return;
        };
        let columns = sqlx::query(
            "SELECT column_name AS name
             FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'stories'",
        )
        .fetch_all(&repo.pool)
        .await
        .expect("应能读取 stories 表结构");

        let has_default_workspace_id = columns.iter().any(|row| {
            row.try_get::<String, _>("name")
                .map(|name| name == "default_workspace_id")
                .unwrap_or(false)
        });

        assert!(
            has_default_workspace_id,
            "initialize 后 stories 表应包含 default_workspace_id 列"
        );
    }

    #[tokio::test]
    async fn legacy_story_table_can_read_story_after_initialize() {
        let Some(repo) = new_repo_with_legacy_story_table().await else {
            return;
        };
        let project_id = uuid::Uuid::new_v4();
        let story = Story::new(project_id, "Story".to_string(), "desc".to_string());

        sqlx::query("INSERT INTO projects (id) VALUES ($1)")
            .bind(project_id.to_string())
            .execute(&repo.pool)
            .await
            .expect("应能插入 project");

        repo.create(&story)
            .await
            .expect("补齐列后应能按新 schema 写入 story");

        let loaded = repo
            .get_by_id(story.id)
            .await
            .expect("应能查询 story")
            .expect("story 应存在");

        assert_eq!(loaded.id, story.id);
        assert_eq!(loaded.default_workspace_id, None);
    }
}
