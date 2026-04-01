use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::session_binding::{
    ProjectSessionBinding, SessionBinding, SessionBindingRepository, SessionOwnerType,
};

pub struct PostgresSessionBindingRepository {
    pool: PgPool,
}

impl PostgresSessionBindingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_bindings (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL DEFAULT '',
                session_id TEXT NOT NULL,
                owner_type TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_sb_unique
                ON session_bindings(session_id, owner_type, owner_id);
            CREATE INDEX IF NOT EXISTS idx_sb_project
                ON session_bindings(project_id);
            CREATE INDEX IF NOT EXISTS idx_sb_owner
                ON session_bindings(owner_type, owner_id);
            CREATE INDEX IF NOT EXISTS idx_sb_session
                ON session_bindings(session_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl SessionBindingRepository for PostgresSessionBindingRepository {
    async fn create(&self, binding: &SessionBinding) -> Result<(), DomainError> {
        let existing_project_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT project_id FROM session_bindings WHERE session_id = $1",
        )
        .bind(&binding.session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if existing_project_ids
            .iter()
            .any(|(project_id,)| project_id != &binding.project_id.to_string())
        {
            return Err(DomainError::InvalidConfig(format!(
                "session `{}` 已绑定到其他 Project，禁止跨 Project 复用",
                binding.session_id
            )));
        }

        sqlx::query(
            "INSERT INTO session_bindings (id, project_id, session_id, owner_type, owner_id, label, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(binding.id.to_string())
        .bind(binding.project_id.to_string())
        .bind(&binding.session_id)
        .bind(binding.owner_type.to_string())
        .bind(binding.owner_id.to_string())
        .bind(&binding.label)
        .bind(binding.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM session_bindings WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "session_binding",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete_by_session_and_owner(
        &self,
        session_id: &str,
        owner_type: SessionOwnerType,
        owner_id: uuid::Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "DELETE FROM session_bindings WHERE session_id = $1 AND owner_type = $2 AND owner_id = $3",
        )
        .bind(session_id)
        .bind(owner_type.to_string())
        .bind(owner_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn list_by_owner(
        &self,
        owner_type: SessionOwnerType,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<SessionBinding>, DomainError> {
        let rows = sqlx::query_as::<_, BindingRow>(
            "SELECT id, project_id, session_id, owner_type, owner_id, label, created_at
             FROM session_bindings
             WHERE owner_type = $1 AND owner_id = $2
             ORDER BY created_at ASC",
        )
        .bind(owner_type.to_string())
        .bind(owner_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn list_by_session(&self, session_id: &str) -> Result<Vec<SessionBinding>, DomainError> {
        let rows = sqlx::query_as::<_, BindingRow>(
            "SELECT id, project_id, session_id, owner_type, owner_id, label, created_at
             FROM session_bindings
             WHERE session_id = $1
             ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn find_by_owner_and_label(
        &self,
        owner_type: SessionOwnerType,
        owner_id: uuid::Uuid,
        label: &str,
    ) -> Result<Option<SessionBinding>, DomainError> {
        let row = sqlx::query_as::<_, BindingRow>(
            "SELECT id, project_id, session_id, owner_type, owner_id, label, created_at
             FROM session_bindings
             WHERE owner_type = $1 AND owner_id = $2 AND label = $3
             LIMIT 1",
        )
        .bind(owner_type.to_string())
        .bind(owner_id.to_string())
        .bind(label)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_bound_session_ids(&self) -> Result<Vec<String>, DomainError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT DISTINCT session_id FROM session_bindings")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// 一次 SQL 获取项目下所有层级的 bindings，内联归属上下文。
    ///
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectSessionBinding>, DomainError> {
        let pid = project_id.to_string();

        #[derive(sqlx::FromRow)]
        struct ProjectBindingRow {
            id: String,
            project_id: String,
            session_id: String,
            owner_type: String,
            owner_id: String,
            label: String,
            created_at: String,
            owner_title: Option<String>,
            story_id: Option<String>,
            story_title: Option<String>,
        }

        let rows = sqlx::query_as::<_, ProjectBindingRow>(
            r#"
            SELECT
                sb.id,
                sb.project_id,
                sb.session_id,
                sb.owner_type,
                sb.owner_id,
                sb.label,
                sb.created_at,
                CASE
                    WHEN sb.owner_type = 'project' THEN p.name
                    WHEN sb.owner_type = 'story' THEN s.title
                    WHEN sb.owner_type = 'task' THEN t.title
                    ELSE NULL
                END AS owner_title,
                CASE
                    WHEN sb.owner_type = 'task' THEN s.id
                    ELSE NULL
                END AS story_id,
                CASE
                    WHEN sb.owner_type = 'task' THEN s.title
                    ELSE NULL
                END AS story_title
            FROM session_bindings sb
            LEFT JOIN projects p ON sb.owner_type = 'project' AND sb.owner_id = p.id
            LEFT JOIN stories s ON (
                (sb.owner_type = 'story' AND sb.owner_id = s.id)
                OR
                (sb.owner_type = 'task' AND EXISTS (
                    SELECT 1
                    FROM tasks tx
                    WHERE tx.id = sb.owner_id AND tx.story_id = s.id
                ))
            )
            LEFT JOIN tasks t ON sb.owner_type = 'task' AND sb.owner_id = t.id
            WHERE sb.project_id = $1
            ORDER BY sb.created_at ASC
            "#,
        )
        .bind(&pid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let owner_type = row.owner_type.parse::<SessionOwnerType>().map_err(|_| {
                    DomainError::InvalidConfig(format!("无效的 owner_type: {}", row.owner_type))
                })?;
                let binding = SessionBinding {
                    id: row.id.parse().map_err(|_| DomainError::NotFound {
                        entity: "session_binding",
                        id: row.id.clone(),
                    })?,
                    project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                        entity: "project",
                        id: row.project_id.clone(),
                    })?,
                    session_id: row.session_id,
                    owner_type,
                    owner_id: row.owner_id.parse().map_err(|_| DomainError::NotFound {
                        entity: "session_binding",
                        id: row.owner_id.clone(),
                    })?,
                    label: row.label,
                    created_at: super::parse_pg_timestamp_checked(
                        &row.created_at,
                        "session_bindings.created_at",
                    )?,
                };
                let story_id = row
                    .story_id
                    .as_deref()
                    .map(|value| {
                        value.parse::<Uuid>().map_err(|error| {
                            DomainError::InvalidConfig(format!(
                                "session_bindings.story_id: {error}"
                            ))
                        })
                    })
                    .transpose()?;
                Ok(ProjectSessionBinding {
                    binding,
                    story_title: row.story_title,
                    story_id,
                    owner_title: row.owner_title,
                })
            })
            .collect()
    }
}

#[derive(sqlx::FromRow)]
struct BindingRow {
    id: String,
    project_id: String,
    session_id: String,
    owner_type: String,
    owner_id: String,
    label: String,
    created_at: String,
}

impl TryFrom<BindingRow> for SessionBinding {
    type Error = DomainError;

    fn try_from(row: BindingRow) -> Result<Self, Self::Error> {
        let owner_type = row.owner_type.parse::<SessionOwnerType>().map_err(|_| {
            DomainError::InvalidConfig(format!(
                "无效的 session_binding owner_type: {}",
                row.owner_type
            ))
        })?;

        Ok(SessionBinding {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "session_binding",
                id: row.id.clone(),
            })?,
            project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.project_id.clone(),
            })?,
            session_id: row.session_id,
            owner_type,
            owner_id: row.owner_id.parse().map_err(|_| DomainError::NotFound {
                entity: "session_binding",
                id: row.owner_id.clone(),
            })?,
            label: row.label,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "session_bindings.created_at",
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo() -> Option<PostgresSessionBindingRepository> {
        let pool = match test_pg_pool("session_binding_repository").await {
            Some(pool) => pool,
            None => return None,
        };
        sqlx::query(
            r#"
            CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                story_id TEXT NOT NULL,
                session_id TEXT,
                updated_at TEXT
            );
            CREATE TABLE stories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("应能创建 session_binding 初始化依赖表");

        let repo = PostgresSessionBindingRepository::new(pool);
        repo.initialize()
            .await
            .expect("应能初始化 session_binding schema");
        Some(repo)
    }

    #[tokio::test]
    async fn allows_reusing_session_within_same_project() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();

        let story_binding = SessionBinding::new(
            project_id,
            "sess-shared".to_string(),
            SessionOwnerType::Story,
            story_id,
            "companion",
        );
        let task_binding = SessionBinding::new(
            project_id,
            "sess-shared".to_string(),
            SessionOwnerType::Task,
            task_id,
            "execution",
        );

        SessionBindingRepository::create(&repo, &story_binding)
            .await
            .expect("同一 project 内应允许复用 session");
        SessionBindingRepository::create(&repo, &task_binding)
            .await
            .expect("同一 project 内第二个 owner 也应允许绑定");
    }

    #[tokio::test]
    async fn rejects_cross_project_session_reuse() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let first_binding = SessionBinding::new(
            Uuid::new_v4(),
            "sess-cross-project".to_string(),
            SessionOwnerType::Story,
            Uuid::new_v4(),
            "companion",
        );
        let second_binding = SessionBinding::new(
            Uuid::new_v4(),
            "sess-cross-project".to_string(),
            SessionOwnerType::Task,
            Uuid::new_v4(),
            "execution",
        );

        SessionBindingRepository::create(&repo, &first_binding)
            .await
            .expect("首个 binding 应成功");
        let error = SessionBindingRepository::create(&repo, &second_binding)
            .await
            .expect_err("跨 project 复用同一 session 应失败");

        match error {
            DomainError::InvalidConfig(message) => {
                assert!(message.contains("禁止跨 Project 复用"));
            }
            other => panic!("预期 InvalidConfig，实际得到: {other:?}"),
        }
    }
}
