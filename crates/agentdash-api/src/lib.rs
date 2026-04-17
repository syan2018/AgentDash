#[cfg(test)]
mod vfs_access;
pub mod app_state;
pub mod auth;
pub mod bootstrap;
pub mod dto;
pub mod mount_providers;
pub mod plugins;
pub mod relay;
pub mod routes;
pub mod rpc;
pub mod runtime_bridge;
pub mod stream;
pub mod task_agent_context;
pub mod title_generator;
pub mod workspace_resolution;

use anyhow::Result;

use agentdash_plugin_api::AgentDashPlugin;

use app_state::AppState;
pub use plugins::builtin_plugins;

/// 启动 AgentDash API 服务
///
/// 接受插件列表，在 DI 组装完成后启动 HTTP 服务。
/// 开源版通常传入 `builtin_plugins()`；企业版在此基础上追加私有插件。
pub async fn run_server(plugins: Vec<Box<dyn AgentDashPlugin>>) -> Result<()> {
    let db_runtime =
        agentdash_infrastructure::postgres_runtime::PostgresRuntime::resolve("agentdash_api", 5)
            .await?;
    tracing::info!(database_url = %db_runtime.connection_url, "数据库已就绪");
    agentdash_infrastructure::migration::run_postgres_migrations(&db_runtime.pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let state = AppState::new_with_plugins(db_runtime.pool.clone(), plugins).await?;

    let app = routes::create_router(state);

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("{}:{}", host, port);

    tracing::info!("AgentDash API 服务启动: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
