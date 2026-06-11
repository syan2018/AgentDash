use anyhow::{Result, bail};
use serde::Serialize;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match ServerCommand::parse(std::env::args().skip(1))? {
        ServerCommand::Serve => {
            agentdash_api::run_server(agentdash_api::builtin_integrations()).await
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
    }
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
    database_url: String,
}

fn print_report(command: &'static str, ready: agentdash_api::DatabaseReady) -> Result<()> {
    let report = CommandReport {
        command,
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        schema_version: ready.schema_version,
        database_url: ready.database_url,
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
