use agentdash_domain::DomainError;
use agentdash_domain::identity::{Group, User, UserDirectoryRepository};
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};

pub struct PostgresUserDirectoryRepository {
    pool: PgPool,
}

impl PostgresUserDirectoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["users", "groups", "group_memberships"],
        )
        .await
    }
}

#[async_trait::async_trait]
impl UserDirectoryRepository for PostgresUserDirectoryRepository {
    async fn upsert_user(&self, user: &User) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO users (
                user_id, subject, auth_mode, display_name, email, avatar_url, is_admin, provider, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT(user_id) DO UPDATE SET
                subject = excluded.subject,
                auth_mode = excluded.auth_mode,
                display_name = excluded.display_name,
                email = excluded.email,
                avatar_url = excluded.avatar_url,
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
        .bind(&user.avatar_url)
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
            SELECT user_id, subject, auth_mode, display_name, email, avatar_url, is_admin, provider, created_at, updated_at
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
            SELECT user_id, subject, auth_mode, display_name, email, avatar_url, is_admin, provider, created_at, updated_at
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

        let now = Utc::now().to_rfc3339();
        if !groups.is_empty() {
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO groups (group_id, display_name, created_at, updated_at) ",
            );
            builder.push_values(groups, |mut row, group| {
                row.push_bind(&group.group_id)
                    .push_bind(&group.display_name)
                    .push_bind(group.created_at.to_rfc3339())
                    .push_bind(&now);
            });
            builder.push(
                " ON CONFLICT(group_id) DO UPDATE SET display_name = excluded.display_name, updated_at = excluded.updated_at",
            );
            builder
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        sqlx::query("DELETE FROM group_memberships WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if !groups.is_empty() {
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO group_memberships (user_id, group_id, created_at, updated_at) ",
            );
            builder.push_values(groups, |mut row, group| {
                row.push_bind(user_id)
                    .push_bind(&group.group_id)
                    .push_bind(&now)
                    .push_bind(&now);
            });
            builder
                .build()
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
    avatar_url: Option<String>,
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
            avatar_url: row.avatar_url,
            is_admin: row.is_admin,
            provider: row.provider,
            created_at: super::parse_pg_timestamp_checked(&row.created_at, "users.created_at")?,
            updated_at: super::parse_pg_timestamp_checked(&row.updated_at, "users.updated_at")?,
        })
    }
}

impl TryFrom<GroupRow> for Group {
    type Error = DomainError;

    fn try_from(row: GroupRow) -> Result<Self, Self::Error> {
        Ok(Group {
            group_id: row.group_id,
            display_name: row.display_name,
            created_at: super::parse_pg_timestamp_checked(&row.created_at, "groups.created_at")?,
            updated_at: super::parse_pg_timestamp_checked(&row.updated_at, "groups.updated_at")?,
        })
    }
}
