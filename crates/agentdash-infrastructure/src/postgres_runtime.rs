use anyhow::{Result, anyhow};
use postgresql_embedded::{PostgreSQL, Settings};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// PostgreSQL 运行时句柄。
///
/// - external 模式：仅持有连接池
/// - embedded 模式：额外持有 PostgreSQL 实例，保证进程生命周期内数据库持续运行
///
/// 启动时会自动探测已存在的 embedded 实例并尝试复用，避免
/// 上次非正常退出遗留的 postgres 进程阻塞新实例启动。
pub struct PostgresRuntime {
    pub pool: PgPool,
    pub connection_url: String,
    embedded: Option<PostgreSQL>,
}

struct RunningPgInfo {
    pid: u32,
    port: u16,
}

impl PostgresRuntime {
    pub async fn resolve(service_name: &str, max_connections: u32) -> Result<Self> {
        // ── External PostgreSQL ──────────────────────────────────────────
        if let Some(database_url) = resolve_external_database_url()? {
            tracing::info!("检测到 DATABASE_URL，使用外部 PostgreSQL");
            let pool = PgPoolOptions::new()
                .max_connections(max_connections)
                .connect(&database_url)
                .await
                .map_err(|e| anyhow!("连接外部 PostgreSQL 失败: {e}"))?;
            return Ok(Self {
                pool,
                connection_url: database_url,
                embedded: None,
            });
        }

        // ── Embedded PostgreSQL ──────────────────────────────────────────
        let database_name = service_name.replace('-', "_");
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let service_dir = workspace_root
            .join(".agentdash")
            .join("embedded-postgres")
            .join(service_name);
        let data_dir = service_dir.join("data");
        let password_file = service_dir.join("pgpass");

        std::fs::create_dir_all(&service_dir)
            .map_err(|e| anyhow!("创建 service_dir 失败: {} ({e})", service_dir.display()))?;

        let saved_password = read_saved_password(&password_file);

        // 1) 尝试复用上次遗留但仍在运行的 PG 实例
        if let Some((pool, url)) =
            try_reuse_running(&data_dir, &database_name, &saved_password, max_connections).await
        {
            return Ok(Self {
                pool,
                connection_url: url,
                embedded: None,
            });
        }

        // 2) 清理残留的 postmaster.pid / 僵尸进程
        cleanup_stale_instance(&data_dir);

        // 3) 启动新 embedded 实例
        let mut settings = Settings::new();
        settings.host = "127.0.0.1".to_string();
        settings.port = 0;
        settings.temporary = false;
        settings.data_dir = data_dir;
        settings.password_file = password_file;
        if let Some(ref pw) = saved_password {
            settings.password = pw.clone();
            tracing::info!("复用已存在的 pgpass 密码");
        }

        let mut postgres = PostgreSQL::new(settings);
        tracing::info!("执行 embedded PostgreSQL setup");
        postgres
            .setup()
            .await
            .map_err(|e| anyhow!("embedded PostgreSQL setup 失败: {e}"))?;
        tracing::info!("执行 embedded PostgreSQL start");
        postgres
            .start()
            .await
            .map_err(|e| anyhow!("embedded PostgreSQL start 失败: {e}"))?;
        tracing::info!("embedded PostgreSQL 已启动");

        if !postgres
            .database_exists(&database_name)
            .await
            .map_err(|e| anyhow!("检查数据库存在性失败: {e}"))?
        {
            postgres
                .create_database(&database_name)
                .await
                .map_err(|e| anyhow!("创建数据库失败: {e}"))?;
        }

        let database_url = postgres.settings().url(&database_name);
        tracing::info!(database = %database_name, url = %database_url, "embedded PostgreSQL 就绪");

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(&database_url)
            .await
            .map_err(|e| anyhow!("连接 embedded PostgreSQL 失败: {e}"))?;

        Ok(Self {
            pool,
            connection_url: database_url,
            embedded: Some(postgres),
        })
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn resolve_external_database_url() -> Result<Option<String>> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return Ok(None);
    };
    let trimmed = database_url.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        return Ok(Some(trimmed.to_string()));
    }
    Err(anyhow!(
        "DATABASE_URL 必须使用 PostgreSQL 协议（postgres:// 或 postgresql://），当前值不合法: {trimmed}"
    ))
}

fn read_saved_password(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|s| {
        let p = s.trim().to_string();
        if p.is_empty() { None } else { Some(p) }
    })
}

/// 解析 data_dir/postmaster.pid，提取 PID 和监听端口。
fn read_postmaster_pid(data_dir: &Path) -> Option<RunningPgInfo> {
    let content = std::fs::read_to_string(data_dir.join("postmaster.pid")).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() < 4 {
        return None;
    }
    let pid: u32 = lines[0].trim().parse().ok()?;
    let port: u16 = lines[3].trim().parse().ok()?;
    if pid == 0 || port == 0 {
        return None;
    }
    Some(RunningPgInfo { pid, port })
}

/// 尝试复用已在运行的 embedded PostgreSQL 实例。
/// 通过 postmaster.pid → TCP 探测 → sqlx 连接 三步验证。
async fn try_reuse_running(
    data_dir: &Path,
    database_name: &str,
    password: &Option<String>,
    max_connections: u32,
) -> Option<(PgPool, String)> {
    let info = read_postmaster_pid(data_dir)?;

    let addr = SocketAddr::from(([127, 0, 0, 1], info.port));
    if TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_err() {
        tracing::info!(
            port = info.port,
            "postmaster.pid 存在但端口不可达，判定为残留"
        );
        return None;
    }

    let pw = password.as_deref().unwrap_or("");
    let options = PgConnectOptions::new()
        .host("127.0.0.1")
        .port(info.port)
        .username("postgres")
        .password(pw)
        .database(database_name);
    let display_url = format!(
        "postgres://postgres@127.0.0.1:{}/{}",
        info.port, database_name
    );

    tracing::info!(
        port = info.port,
        pid = info.pid,
        "检测到已运行的 embedded PostgreSQL，尝试复用"
    );

    match PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
    {
        Ok(pool) => {
            tracing::info!("成功复用已运行的 embedded PostgreSQL");
            Some((pool, display_url))
        }
        Err(e) => {
            tracing::warn!("复用连接失败: {e}");
            None
        }
    }
}

/// 清理残留的 postgres 进程和过期的 postmaster.pid。
fn cleanup_stale_instance(data_dir: &Path) {
    let Some(info) = read_postmaster_pid(data_dir) else {
        return;
    };

    tracing::info!(pid = info.pid, "停止残留 PostgreSQL 进程");

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &info.pid.to_string()])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .arg(info.pid.to_string())
            .output();
    }

    std::thread::sleep(Duration::from_secs(2));
    let _ = std::fs::remove_file(data_dir.join("postmaster.pid"));
}

impl Drop for PostgresRuntime {
    fn drop(&mut self) {
        if let Some(embedded) = self.embedded.take() {
            tokio::spawn(async move {
                if let Err(err) = embedded.stop().await {
                    tracing::warn!(error = %err, "停止 embedded PostgreSQL 失败");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_external_database_url;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_external_database_url_accepts_postgres_scheme() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://localhost/test");
        }
        let result = resolve_external_database_url().expect("postgres 协议应通过");
        assert_eq!(result.as_deref(), Some("postgres://localhost/test"));
        unsafe {
            std::env::remove_var("DATABASE_URL");
        }
    }

    #[test]
    fn resolve_external_database_url_accepts_postgresql_scheme() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        unsafe {
            std::env::set_var("DATABASE_URL", "postgresql://localhost/test");
        }
        let result = resolve_external_database_url().expect("postgresql 协议应通过");
        assert_eq!(result.as_deref(), Some("postgresql://localhost/test"));
        unsafe {
            std::env::remove_var("DATABASE_URL");
        }
    }

    #[test]
    fn resolve_external_database_url_rejects_non_postgres_scheme() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        unsafe {
            std::env::set_var("DATABASE_URL", "sqlite://agentdash.db");
        }
        let error = resolve_external_database_url().expect_err("非 postgres 协议应直接失败");
        assert!(
            error
                .to_string()
                .contains("DATABASE_URL 必须使用 PostgreSQL 协议")
        );
        unsafe {
            std::env::remove_var("DATABASE_URL");
        }
    }
}
