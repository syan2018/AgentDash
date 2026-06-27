use anyhow::{Result, bail};
use serde::Serialize;
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

use agentdash_diagnostics::{DEFAULT_CAPACITY, DiagnosticBuffer};

/// JSON line 滚动日志目录环境变量；缺省落地到 `./logs/`。
const LOG_DIR_ENV: &str = "AGENTDASH_LOG_DIR";
const DEFAULT_LOG_DIR: &str = "./logs";
const LOG_FILE_PREFIX: &str = "agentdash-api.log";

#[tokio::main]
async fn main() -> Result<()> {
    // 统一诊断环形缓冲：既接进 tracing 订阅器（写入），又透传进 AppState（查询）。
    let diagnostics = DiagnosticBuffer::new(DEFAULT_CAPACITY);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // JSON line 滚动文件层：按天滚动，写入 AGENTDASH_LOG_DIR（默认 ./logs）。
    let log_dir = std::env::var(LOG_DIR_ENV).unwrap_or_else(|_| DEFAULT_LOG_DIR.into());
    let file_appender = tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_writer)
        .with_ansi(false);

    Registry::default()
        .with(env_filter)
        // stdout：保留现状观感（pretty / 默认 fmt）。
        .with(fmt::layer())
        // JSON line 滚动文件。
        .with(file_layer)
        // 有界环形缓冲，供 GET /api/diagnostics 查询近期诊断。
        .with(diagnostics.layer())
        .init();

    let result = match ServerCommand::parse(std::env::args().skip(1))? {
        ServerCommand::Serve => {
            agentdash_api::run_server(agentdash_api::builtin_integrations(), diagnostics).await
        }
        ServerCommand::Migrate => {
            let ready = agentdash_api::run_postgres_migrations_with_options(
                agentdash_api::ApiServerOptions::from_env()?,
            )
            .await?;
            print_report("migrate", ready)?;
            Ok(())
        }
        ServerCommand::Doctor => {
            let ready = agentdash_api::check_postgres_ready_with_options(
                agentdash_api::ApiServerOptions::from_env()?,
            )
            .await?;
            print_report("doctor", ready)?;
            Ok(())
        }
        ServerCommand::Help => {
            print_help();
            Ok(())
        }
    };

    // `file_guard`（tracing_appender WorkerGuard）必须在 main 的整个生命周期内持有：
    // 它一旦 drop，后台写线程会提前退出并丢弃尚未刷盘的日志。上面的 await 期间 guard
    // 一直在作用域内，进程退出时才随 main 一起 drop，刷出剩余日志。
    drop(file_guard);
    result
}

enum ServerCommand {
    Serve,
    Migrate,
    Doctor,
    Help,
}

impl ServerCommand {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self> {
        let values = args.collect::<Vec<_>>();
        if values.is_empty() {
            return Ok(Self::Serve);
        }
        if values.len() > 1 {
            bail!(
                "agentdash-server 只接受一个子命令，收到: {}",
                values.join(" ")
            );
        }
        match values[0].as_str() {
            "serve" => Ok(Self::Serve),
            "migrate" => Ok(Self::Migrate),
            "doctor" => Ok(Self::Doctor),
            "--help" | "-h" | "help" => Ok(Self::Help),
            command => bail!("未知 agentdash-server 子命令: {command}"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct CommandReport {
    command: &'static str,
    status: &'static str,
    version: &'static str,
    schema_version: i64,
    database: String,
}

fn print_report(command: &'static str, ready: agentdash_api::DatabaseReady) -> Result<()> {
    let report = CommandReport {
        command,
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        schema_version: ready.schema_version,
        database: agentdash_api::redact_database_url(&ready.database_url),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn print_help() {
    println!("Usage: agentdash-server [serve|migrate|doctor]");
    println!();
    println!("Commands:");
    println!("  serve    启动 AgentDash API 服务（默认）");
    println!("  migrate  执行 PostgreSQL migrations 并检查 schema readiness");
    println!("  doctor   检查 PostgreSQL 连接与 schema readiness，不执行 migration");
}
