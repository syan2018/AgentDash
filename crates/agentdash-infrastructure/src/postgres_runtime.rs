use anyhow::Result;
use postgresql_embedded::{PostgreSQL, Settings};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// PostgreSQL 运行时句柄。
///
/// - external 模式：仅持有连接池
/// - embedded 模式：额外持有 PostgreSQL 实例，保证进程生命周期内数据库持续运行
pub struct PostgresRuntime {
    pub pool: PgPool,
    pub connection_url: String,
    embedded: Option<PostgreSQL>,
}

impl PostgresRuntime {
    pub async fn resolve(service_name: &str, max_connections: u32) -> Result<Self> {
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            let lower = database_url.to_ascii_lowercase();
            if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
                tracing::info!("检测到 DATABASE_URL，使用外部 PostgreSQL");
                let pool = PgPoolOptions::new()
                    .max_connections(max_connections)
                    .connect(&database_url)
                    .await?;
                return Ok(Self {
                    pool,
                    connection_url: database_url,
                    embedded: None,
                });
            }
            tracing::warn!("DATABASE_URL 不是 PostgreSQL 协议，回退到 embedded 模式");
        }

        let mut settings = Settings::new();
        settings.host = "127.0.0.1".to_string();
        settings.port = 0;

        let mut postgres = PostgreSQL::new(settings.clone());
        postgres.setup().await?;
        postgres.start().await?;

        let database_name = service_name.replace('-', "_");
        if !postgres.database_exists(&database_name).await? {
            postgres.create_database(&database_name).await?;
        }

        let database_url = postgres.settings().url(&database_name);
        tracing::info!(database = %database_name, url = %database_url, "使用 embedded PostgreSQL");

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(&database_url)
            .await?;

        Ok(Self {
            pool,
            connection_url: database_url,
            embedded: Some(postgres),
        })
    }
}

impl Drop for PostgresRuntime {
    fn drop(&mut self) {
        if let Some(embedded) = self.embedded.clone() {
            tokio::spawn(async move {
                if let Err(err) = embedded.stop().await {
                    tracing::warn!(error = %err, "停止 embedded PostgreSQL 失败");
                }
            });
        }
    }
}
