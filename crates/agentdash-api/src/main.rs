use std::sync::Arc;

use anyhow::Result;
use sqlx::sqlite::SqlitePoolOptions;
use tracing_subscriber::EnvFilter;

mod address_space_access;
mod app_state;
mod bootstrap;
mod dto;
mod relay;
mod routes;
mod rpc;
mod session_plan;
mod stream;
mod task_agent_context;
mod workflow_runtime;

use app_state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:agentdash.db?mode=rwc".into());

    tracing::info!("连接数据库: {}", db_url);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let state = Arc::new(AppState::new(pool).await?);

    let app = routes::create_router(state);

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("{}:{}", host, port);

    tracing::info!("AgentDash API 服务启动: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
