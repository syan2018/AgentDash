use anyhow::Result;
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

    // `file_guard`（tracing_appender WorkerGuard）必须在 main 的整个生命周期内持有：
    // 它一旦 drop，后台写线程会提前退出并丢弃尚未刷盘的日志。下面 await 期间 guard
    // 一直在作用域内，进程退出时才随 main 一起 drop，刷出剩余日志。
    let result =
        agentdash_api::run_server(agentdash_api::builtin_integrations(), diagnostics).await;
    drop(file_guard);
    result
}
