use agentdash_domain::common::error::DomainError;
use sqlx::PgPool;

pub async fn run_postgres_migrations(pool: &PgPool) -> Result<(), DomainError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|err| DomainError::InvalidConfig(format!("数据库迁移失败: {err}")))?;
    Ok(())
}
