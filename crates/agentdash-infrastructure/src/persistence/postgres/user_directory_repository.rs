use agentdash_domain::DomainError;
use agentdash_domain::identity::{Group, User, UserDirectoryRepository};
use chrono::Utc;
use sqlx::PgPool;

pub struct SqliteUserDirectoryRepository {
    pool: PgPool,
}

impl SqliteUserDirectoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                user_id TEXT PRIMARY KEY,
                subject TEXT NOT NULL,
                auth_mode TEXT NOT NULL,
                display_name TEXT,
                email TEXT,
                is_admin INTEGER NOT NULL DEFAULT 0,
                provider TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS groups (
                group_id TEXT PRIMARY KEY,
                display_name TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS group_memberships (
                user_id TEXT NOT NULL,
                group_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (user_id, group_id),
                FOREIGN KEY(user_id) REFERENCES users(user_id) ON DELETE CASCADE,
                FOREIGN KEY(group_id) REFERENCES groups(group_id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_users_subject_auth_mode ON users(auth_mode, subject)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_group_memberships_user_id ON group_memberships(user_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_group_memberships_group_id ON group_memberships(group_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl UserDirectoryRepository for SqliteUserDirectoryRepository {
    async fn upsert_user(&self, user: &User) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO users (
                user_id, subject, auth_mode, display_name, email, is_admin, provider, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT(user_id) DO UPDATE SET
                subject = excluded.subject,
                auth_mode = excluded.auth_mode,
                display_name = excluded.display_name,
                email = excluded.email,
                is_admin = excluded.is_admin,
                provider = excluded.provider,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&user.user_id)
        .bind(&user.subject)
        .bind(&user.auth_mode)
        .bind(&user.display_name)
        .bind(&user.email)
        .bind(user.is_admin)
        .bind(&user.provider)
        .bind(user.created_at.to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, DomainError> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT user_id, subject, auth_mode, display_name, email, is_admin, provider, created_at, updated_at
            FROM users
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn get_group_by_id(&self, group_id: &str) -> Result<Option<Group>, DomainError> {
        let row = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT group_id, display_name, created_at, updated_at
            FROM groups
            WHERE group_id = $1
            "#,
        )
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_users(&self) -> Result<Vec<User>, DomainError> {
        let rows = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT user_id, subject, auth_mode, display_name, email, is_admin, provider, created_at, updated_at
            FROM users
            ORDER BY updated_at DESC, user_id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_groups(&self) -> Result<Vec<Group>, DomainError> {
        let rows = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT group_id, display_name, created_at, updated_at
            FROM groups
            ORDER BY updated_at DESC, group_id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_groups_for_user(&self, user_id: &str) -> Result<Vec<Group>, DomainError> {
        let rows = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT g.group_id, g.display_name, g.created_at, g.updated_at
            FROM groups g
            INNER JOIN group_memberships gm ON gm.group_id = g.group_id
            WHERE gm.user_id = $1
            ORDER BY g.group_id ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn replace_groups_for_user(
        &self,
        user_id: &str,
        groups: &[Group],
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        for group in groups {
            sqlx::query(
                r#"
                INSERT INTO groups (group_id, display_name, created_at, updated_at)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT(group_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(&group.group_id)
            .bind(&group.display_name)
            .bind(group.created_at.to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        sqlx::query("DELETE FROM group_memberships WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let now = Utc::now().to_rfc3339();
        for group in groups {
            sqlx::query(
                r#"
                INSERT INTO group_memberships (user_id, group_id, created_at, updated_at)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(user_id)
            .bind(&group.group_id)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    user_id: String,
    subject: String,
    auth_mode: String,
    display_name: Option<String>,
    email: Option<String>,
    is_admin: bool,
    provider: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct GroupRow {
    group_id: String,
    display_name: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<UserRow> for User {
    type Error = DomainError;

    fn try_from(row: UserRow) -> Result<Self, Self::Error> {
        Ok(User {
            user_id: row.user_id,
            subject: row.subject,
            auth_mode: row.auth_mode,
            display_name: row.display_name,
            email: row.email,
            is_admin: row.is_admin,
            provider: row.provider,
            created_at: super::parse_pg_timestamp(&row.created_at),
            updated_at: super::parse_pg_timestamp(&row.updated_at),
        })
    }
}

impl TryFrom<GroupRow> for Group {
    type Error = DomainError;

    fn try_from(row: GroupRow) -> Result<Self, Self::Error> {
        Ok(Group {
            group_id: row.group_id,
            display_name: row.display_name,
            created_at: super::parse_pg_timestamp(&row.created_at),
            updated_at: super::parse_pg_timestamp(&row.updated_at),
        })
    }
}
