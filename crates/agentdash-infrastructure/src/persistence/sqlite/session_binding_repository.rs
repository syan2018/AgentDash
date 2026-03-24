use sqlx::SqlitePool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::session_binding::{
    ProjectSessionBinding, SessionBinding, SessionBindingRepository, SessionOwnerType,
};

pub struct SqliteSessionBindingRepository {
    pool: SqlitePool,
}

impl SqliteSessionBindingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_bindings (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                owner_type TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_sb_unique
                ON session_bindings(session_id, owner_type, owner_id);
            CREATE INDEX IF NOT EXISTS idx_sb_owner
                ON session_bindings(owner_type, owner_id);
            CREATE INDEX IF NOT EXISTS idx_sb_session
                ON session_bindings(session_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.backfill_from_tasks().await?;

        Ok(())
    }

    /// 启动时回填：将 tasks 表中已有 session_id 的记录同步到 session_bindings
    async fn backfill_from_tasks(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO session_bindings (id, session_id, owner_type, owner_id, label, created_at)
            SELECT
                lower(
                    hex(randomblob(4)) || '-' ||
                    hex(randomblob(2)) || '-4' ||
                    substr(hex(randomblob(2)),2) || '-' ||
                    substr('89ab', abs(random()) % 4 + 1, 1) ||
                    substr(hex(randomblob(2)),2) || '-' ||
                    hex(randomblob(6))
                ),
                session_id,
                'task',
                id,
                'execution',
                COALESCE(updated_at, datetime('now'))
            FROM tasks
            WHERE session_id IS NOT NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl SessionBindingRepository for SqliteSessionBindingRepository {
    async fn create(&self, binding: &SessionBinding) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO session_bindings (id, session_id, owner_type, owner_id, label, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(binding.id.to_string())
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
        let result = sqlx::query("DELETE FROM session_bindings WHERE id = ?")
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
            "DELETE FROM session_bindings WHERE session_id = ? AND owner_type = ? AND owner_id = ?",
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
            "SELECT id, session_id, owner_type, owner_id, label, created_at
             FROM session_bindings
             WHERE owner_type = ? AND owner_id = ?
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
            "SELECT id, session_id, owner_type, owner_id, label, created_at
             FROM session_bindings
             WHERE session_id = ?
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
            "SELECT id, session_id, owner_type, owner_id, label, created_at
             FROM session_bindings
             WHERE owner_type = ? AND owner_id = ? AND label = ?
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
    /// 查询逻辑：
    ///   - owner_type = 'project' → owner_id = project_id
    ///   - owner_type = 'story'   → JOIN stories ON owner_id = stories.id WHERE stories.project_id = ?
    ///   - owner_type = 'task'    → JOIN tasks ON owner_id = tasks.id
    ///                              JOIN stories ON tasks.story_id = stories.id WHERE stories.project_id = ?
    ///
    /// 用 UNION ALL 合并三段，避免复杂 LEFT JOIN 带来的行膨胀。
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectSessionBinding>, DomainError> {
        let pid = project_id.to_string();

        #[derive(sqlx::FromRow)]
        struct ProjectBindingRow {
            id: String,
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
            -- Project 级 bindings
            SELECT
                sb.id, sb.session_id, sb.owner_type, sb.owner_id, sb.label, sb.created_at,
                NULL AS owner_title,
                NULL AS story_id,
                NULL AS story_title
            FROM session_bindings sb
            WHERE sb.owner_type = 'project'
              AND sb.owner_id = ?

            UNION ALL

            -- Story 级 bindings
            SELECT
                sb.id, sb.session_id, sb.owner_type, sb.owner_id, sb.label, sb.created_at,
                s.title AS owner_title,
                NULL    AS story_id,
                NULL    AS story_title
            FROM session_bindings sb
            INNER JOIN stories s ON sb.owner_id = s.id
            WHERE sb.owner_type = 'story'
              AND s.project_id = ?

            UNION ALL

            -- Task 级 bindings
            SELECT
                sb.id, sb.session_id, sb.owner_type, sb.owner_id, sb.label, sb.created_at,
                t.title  AS owner_title,
                s.id     AS story_id,
                s.title  AS story_title
            FROM session_bindings sb
            INNER JOIN tasks    t ON sb.owner_id = t.id
            INNER JOIN stories  s ON t.story_id  = s.id
            WHERE sb.owner_type = 'task'
              AND s.project_id = ?
            "#,
        )
        .bind(&pid)
        .bind(&pid)
        .bind(&pid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let owner_type =
                    SessionOwnerType::from_str_loose(&row.owner_type).ok_or_else(|| {
                        DomainError::InvalidConfig(format!(
                            "无效的 owner_type: {}",
                            row.owner_type
                        ))
                    })?;
                let binding = SessionBinding {
                    id: row.id.parse().map_err(|_| DomainError::NotFound {
                        entity: "session_binding",
                        id: row.id.clone(),
                    })?,
                    session_id: row.session_id,
                    owner_type,
                    owner_id: row.owner_id.parse().map_err(|_| DomainError::NotFound {
                        entity: "session_binding",
                        id: row.owner_id.clone(),
                    })?,
                    label: row.label,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                };
                let story_id = row
                    .story_id
                    .as_deref()
                    .and_then(|s| s.parse::<Uuid>().ok());
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
    session_id: String,
    owner_type: String,
    owner_id: String,
    label: String,
    created_at: String,
}

impl TryFrom<BindingRow> for SessionBinding {
    type Error = DomainError;

    fn try_from(row: BindingRow) -> Result<Self, Self::Error> {
        let owner_type = SessionOwnerType::from_str_loose(&row.owner_type).ok_or_else(|| {
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
            session_id: row.session_id,
            owner_type,
            owner_id: row.owner_id.parse().map_err(|_| DomainError::NotFound {
                entity: "session_binding",
                id: row.owner_id.clone(),
            })?,
            label: row.label,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}
