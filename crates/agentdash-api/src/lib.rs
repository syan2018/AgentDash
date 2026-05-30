pub mod app_state;
pub mod auth;
pub mod bootstrap;
pub mod dto;
pub mod mount_providers;
pub mod oauth_flow;
pub mod plugins;
pub mod relay;
pub mod routes;
pub mod rpc;
pub mod runtime_bridge;
pub mod session_construction;
pub mod stream;
pub mod task_agent_context;
#[cfg(test)]
mod vfs_access;
pub mod vfs_materialization;
mod vfs_surface_runtime;
pub mod workspace_resolution;

use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;

use agentdash_plugin_api::AgentDashPlugin;

use app_state::AppState;
pub use plugins::builtin_plugins;

#[derive(Debug, Clone)]
pub struct ApiServerOptions {
    pub service_name: String,
    pub host: String,
    pub port: u16,
    pub max_connections: u32,
}

impl ApiServerOptions {
    pub fn from_env() -> Result<Self> {
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "3001".into())
            .parse::<u16>()?;

        Ok(Self {
            service_name: "agentdash_api".to_string(),
            host,
            port,
            max_connections: 5,
        })
    }

    pub fn desktop_localhost(port: u16) -> Self {
        Self {
            service_name: "agentdash_desktop_api".to_string(),
            host: "127.0.0.1".to_string(),
            port,
            max_connections: 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiServerReady {
    pub addr: String,
    pub origin: String,
    pub database_url: String,
}

pub struct ApiServer {
    ready: ApiServerReady,
    listener: TcpListener,
    app: Router,
    _db_runtime: agentdash_infrastructure::postgres_runtime::PostgresRuntime,
}

impl ApiServer {
    pub fn ready(&self) -> &ApiServerReady {
        &self.ready
    }

    pub async fn serve(self) -> Result<()> {
        axum::serve(self.listener, self.app).await?;
        Ok(())
    }
}

/// 启动 AgentDash API 服务
///
/// 接受插件列表，在 DI 组装完成后启动 HTTP 服务。
/// 开源版通常传入 `builtin_plugins()`；企业版在此基础上追加私有插件。
pub async fn run_server(plugins: Vec<Box<dyn AgentDashPlugin>>) -> Result<()> {
    run_server_with_options(plugins, ApiServerOptions::from_env()?).await
}

pub async fn run_server_with_options(
    plugins: Vec<Box<dyn AgentDashPlugin>>,
    options: ApiServerOptions,
) -> Result<()> {
    let server = build_server(plugins, options).await?;
    let ready = server.ready().clone();
    tracing::info!("AgentDash API 服务启动: {}", ready.origin);
    server.serve().await
}

pub async fn build_server(
    plugins: Vec<Box<dyn AgentDashPlugin>>,
    options: ApiServerOptions,
) -> Result<ApiServer> {
    let db_runtime = agentdash_infrastructure::postgres_runtime::PostgresRuntime::resolve(
        &options.service_name,
        options.max_connections,
    )
    .await?;
    tracing::info!(database_url = %db_runtime.connection_url, "数据库已就绪");
    agentdash_infrastructure::migration::run_postgres_migrations(&db_runtime.pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    agentdash_infrastructure::migration::assert_postgres_schema_ready(&db_runtime.pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let state = AppState::new_with_plugins(db_runtime.pool.clone(), plugins).await?;

    let app = routes::create_router(state);

    let addr = format!("{}:{}", options.host, options.port);
    let origin_host = if options.host == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else {
        options.host.clone()
    };
    let origin = format!("http://{}:{}", origin_host, options.port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    Ok(ApiServer {
        ready: ApiServerReady {
            addr,
            origin,
            database_url: db_runtime.connection_url.clone(),
        },
        listener,
        app,
        _db_runtime: db_runtime,
    })
}
