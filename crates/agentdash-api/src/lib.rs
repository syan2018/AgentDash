pub mod app_state;
pub mod auth;
pub mod bootstrap;
pub mod context;
pub mod dto;
pub mod integrations;
pub mod mount_providers;
pub mod oauth_flow;
mod project_projection_notification;
pub mod relay;
pub mod routes;
pub mod rpc;
pub mod runtime_bridge;
pub mod stream;
pub mod vfs_materialization;
mod vfs_surface_runtime;
pub mod workspace_placement_runtime;
pub mod workspace_resolution;

use agentdash_diagnostics::{Subsystem, diag};
use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;

use agentdash_integration_api::AgentDashIntegration;

pub use agentdash_diagnostics::DiagnosticBuffer;
use app_state::AppState;
pub use integrations::builtin_integrations;

const DEFAULT_POSTGRES_MAX_CONNECTIONS: u32 = 20;

#[derive(Debug, Clone)]
pub struct ApiServerOptions {
    pub service_name: String,
    pub host: String,
    pub port: u16,
    pub max_connections: u32,
}

impl ApiServerOptions {
    pub fn from_env() -> Result<Self> {
        let host = read_env("AGENTDASH_BIND_HOST")
            .or_else(|| read_env("HOST"))
            .unwrap_or_else(|| "0.0.0.0".into());
        let port = read_env("AGENTDASH_PORT")
            .or_else(|| read_env("PORT"))
            .unwrap_or_else(|| "3001".into())
            .parse::<u16>()?;

        Ok(Self {
            service_name: "agentdash_api".to_string(),
            host,
            port,
            max_connections: DEFAULT_POSTGRES_MAX_CONNECTIONS,
        })
    }

    pub fn desktop_localhost(port: u16) -> Self {
        Self {
            service_name: "agentdash_desktop_api".to_string(),
            host: "127.0.0.1".to_string(),
            port,
            max_connections: DEFAULT_POSTGRES_MAX_CONNECTIONS,
        }
    }
}

fn read_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone)]
pub struct ApiServerReady {
    pub addr: String,
    pub origin: String,
    pub database_url: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseReady {
    pub database_url: String,
    pub schema_version: i64,
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
/// 接受 Host Integration 列表，在 DI 组装完成后启动 HTTP 服务。
/// 开源版通常传入 `builtin_integrations()`；企业版在此基础上追加私有集成。
///
/// `diagnostics` 为统一诊断环形缓冲句柄：调用方（main）先把它接进 tracing
/// 订阅器（[`DiagnosticBuffer::layer`]），再透传到这里供 `GET /api/diagnostics`
/// 查询。订阅器装配只在 main，本函数不 `.init()`。
pub async fn run_server(
    integrations: Vec<Box<dyn AgentDashIntegration>>,
    diagnostics: DiagnosticBuffer,
) -> Result<()> {
    run_server_with_options(integrations, ApiServerOptions::from_env()?, diagnostics).await
}

pub async fn run_server_with_options(
    integrations: Vec<Box<dyn AgentDashIntegration>>,
    options: ApiServerOptions,
    diagnostics: DiagnosticBuffer,
) -> Result<()> {
    let server = build_server(integrations, options, diagnostics).await?;
    let ready = server.ready().clone();
    diag!(
        Info,
        Subsystem::Api,
        "AgentDash API 服务启动: {}",
        ready.origin
    );
    server.serve().await
}

pub async fn run_postgres_migrations_with_options(
    options: ApiServerOptions,
) -> Result<DatabaseReady> {
    let (ready, _db_runtime) = prepare_database(&options, SchemaPreparation::RunMigrations).await?;
    Ok(ready)
}

pub async fn check_postgres_ready_with_options(options: ApiServerOptions) -> Result<DatabaseReady> {
    let (ready, _db_runtime) = prepare_database(&options, SchemaPreparation::CheckReady).await?;
    Ok(ready)
}

pub async fn build_server_with_migrations(
    integrations: Vec<Box<dyn AgentDashIntegration>>,
    options: ApiServerOptions,
    diagnostics: DiagnosticBuffer,
) -> Result<ApiServer> {
    build_server_with_schema_preparation(
        integrations,
        options,
        diagnostics,
        SchemaPreparation::RunMigrations,
    )
    .await
}

pub async fn build_server(
    integrations: Vec<Box<dyn AgentDashIntegration>>,
    options: ApiServerOptions,
    diagnostics: DiagnosticBuffer,
) -> Result<ApiServer> {
    build_server_with_schema_preparation(
        integrations,
        options,
        diagnostics,
        SchemaPreparation::CheckReady,
    )
    .await
}

async fn build_server_with_schema_preparation(
    integrations: Vec<Box<dyn AgentDashIntegration>>,
    options: ApiServerOptions,
    diagnostics: DiagnosticBuffer,
    preparation: SchemaPreparation,
) -> Result<ApiServer> {
    let (db_ready, db_runtime) = prepare_database(&options, preparation).await?;

    let state =
        AppState::new_with_integrations(db_runtime.pool.clone(), integrations, diagnostics).await?;

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
            database_url: db_ready.database_url,
        },
        listener,
        app,
        _db_runtime: db_runtime,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaPreparation {
    RunMigrations,
    CheckReady,
}

async fn prepare_database(
    options: &ApiServerOptions,
    preparation: SchemaPreparation,
) -> Result<(
    DatabaseReady,
    agentdash_infrastructure::postgres_runtime::PostgresRuntime,
)> {
    let db_runtime = agentdash_infrastructure::postgres_runtime::PostgresRuntime::resolve(
        &options.service_name,
        options.max_connections,
    )
    .await?;
    diag!(
        Info,
        Subsystem::Infra,
        database = %redact_database_url(&db_runtime.connection_url),
        "数据库已就绪"
    );
    if matches!(preparation, SchemaPreparation::RunMigrations) {
        agentdash_infrastructure::migration::run_postgres_migrations(&db_runtime.pool)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    agentdash_infrastructure::migration::assert_postgres_schema_ready(&db_runtime.pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok((
        DatabaseReady {
            database_url: db_runtime.connection_url.clone(),
            schema_version: schema_version(),
        },
        db_runtime,
    ))
}

fn schema_version() -> i64 {
    env!("AGENTDASH_SCHEMA_VERSION")
        .parse::<i64>()
        .unwrap_or_default()
}

pub fn redact_database_url(database_url: &str) -> String {
    let Some((scheme, rest)) = database_url.split_once("://") else {
        return database_url.to_string();
    };
    let Some((userinfo, host_and_path)) = rest.split_once('@') else {
        return database_url.to_string();
    };
    let redacted_userinfo = match userinfo.split_once(':') {
        Some((username, _password)) => format!("{username}:***"),
        None => userinfo.to_string(),
    };
    format!("{scheme}://{redacted_userinfo}@{host_and_path}")
}

#[cfg(test)]
mod tests {
    use super::redact_database_url;

    #[test]
    fn redact_database_url_masks_password() {
        assert_eq!(
            redact_database_url("postgres://agentdash:secret@postgres:5432/agentdash"),
            "postgres://agentdash:***@postgres:5432/agentdash"
        );
    }

    #[test]
    fn redact_database_url_keeps_passwordless_url_unchanged() {
        assert_eq!(
            redact_database_url("postgres://agentdash@postgres:5432/agentdash"),
            "postgres://agentdash@postgres:5432/agentdash"
        );
    }
}
