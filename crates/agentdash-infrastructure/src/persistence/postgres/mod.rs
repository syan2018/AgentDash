mod agent_repository;
mod auth_session_repository;
mod backend_repository;
mod canvas_repository;
mod project_repository;
mod session_binding_repository;
mod session_repository;
mod settings_repository;
mod state_change_repository;
mod state_change_store;
mod story_repository;
mod task_repository;
mod user_directory_repository;
mod workflow_repository;
mod workspace_repository;

use agentdash_domain::common::error::DomainError;

#[cfg(test)]
pub(crate) fn test_database_url() -> Option<String> {
    use std::sync::OnceLock;

    static DOTENV_INIT: OnceLock<()> = OnceLock::new();
    DOTENV_INIT.get_or_init(|| {
        let _ = dotenvy::dotenv();
    });

    std::env::var("TEST_DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

#[cfg(test)]
pub(crate) async fn test_pg_pool(suite: &str) -> Option<sqlx::PgPool> {
    let Some(database_url) = test_database_url() else {
        eprintln!("跳过 PostgreSQL {suite} 测试：未设置 TEST_DATABASE_URL / DATABASE_URL");
        return None;
    };

    Some(
        sqlx::PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL"),
    )
}

/// PostgreSQL `TEXT` 时间戳 → `DateTime<Utc>` 健壮解析。
///
/// PG 的 `CURRENT_TIMESTAMP` 输出格式多变，常见：
/// - `2026-04-01T18:17:53.927979+08:00` (RFC 3339)
/// - `2026-04-01 18:17:53.927979+08`    (短时区偏移，无冒号)
/// - `2026-04-01 18:17:53.927979+08:00`
/// - `2026-04-01 18:17:53.927979`       (无时区)
/// - `2026-04-01 18:17:53`              (无时区无小数)
fn parse_pg_timestamp(raw: &str) -> chrono::DateTime<chrono::Utc> {
    use chrono::{DateTime, NaiveDateTime, Utc};

    if let Ok(v) = DateTime::parse_from_rfc3339(raw) {
        return v.with_timezone(&Utc);
    }
    // +08:00 / -05:30  (冒号分隔的时区偏移)
    if let Ok(v) = DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f%:z") {
        return v.with_timezone(&Utc);
    }
    // +0800 / -0530 (四位数字无冒号)
    if let Ok(v) = DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f%z") {
        return v.with_timezone(&Utc);
    }
    // +08 / -05 (PG 常见的两位数字短时区) — 手动补 :00
    {
        let trimmed = raw.trim();
        if let Some(idx) = trimmed.rfind('+').or_else(|| {
            let bytes = trimmed.as_bytes();
            // 跳过开头的负号场景：找最后一个 '-' 且它不是日期分隔符
            (10..trimmed.len()).rev().find(|&i| bytes[i] == b'-')
        }) {
            let tz_part = &trimmed[idx..];
            if tz_part.len() == 3 {
                let patched = format!("{}:00", trimmed);
                if let Ok(v) = DateTime::parse_from_str(&patched, "%Y-%m-%d %H:%M:%S%.f%:z") {
                    return v.with_timezone(&Utc);
                }
            }
        }
    }
    // 无时区，含小数秒
    if let Ok(v) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f") {
        return DateTime::from_naive_utc_and_offset(v, chrono::Utc);
    }
    // 无时区，无小数秒
    if let Ok(v) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return DateTime::from_naive_utc_and_offset(v, chrono::Utc);
    }
    chrono::Utc::now()
}

pub(crate) fn parse_pg_timestamp_checked(
    raw: &str,
    field: &str,
) -> Result<chrono::DateTime<chrono::Utc>, DomainError> {
    use chrono::{DateTime, NaiveDateTime, Utc};

    if let Ok(v) = DateTime::parse_from_rfc3339(raw) {
        return Ok(v.with_timezone(&Utc));
    }
    if let Ok(v) = DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f%:z") {
        return Ok(v.with_timezone(&Utc));
    }
    if let Ok(v) = DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f%z") {
        return Ok(v.with_timezone(&Utc));
    }
    {
        let trimmed = raw.trim();
        if let Some(idx) = trimmed.rfind('+').or_else(|| {
            let bytes = trimmed.as_bytes();
            (10..trimmed.len()).rev().find(|&i| bytes[i] == b'-')
        }) {
            let tz_part = &trimmed[idx..];
            if tz_part.len() == 3 {
                let patched = format!("{}:00", trimmed);
                if let Ok(v) = DateTime::parse_from_str(&patched, "%Y-%m-%d %H:%M:%S%.f%:z") {
                    return Ok(v.with_timezone(&Utc));
                }
            }
        }
    }
    if let Ok(v) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(DateTime::from_naive_utc_and_offset(v, chrono::Utc));
    }
    if let Ok(v) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::from_naive_utc_and_offset(v, chrono::Utc));
    }

    Err(DomainError::InvalidConfig(format!(
        "{field}: 无法解析 PostgreSQL 时间戳 `{raw}`"
    )))
}

pub use agent_repository::PostgresAgentRepository;
pub use auth_session_repository::PostgresAuthSessionRepository;
pub use backend_repository::PostgresBackendRepository;
pub use canvas_repository::PostgresCanvasRepository;
pub use project_repository::PostgresProjectRepository;
pub use session_binding_repository::PostgresSessionBindingRepository;
pub use session_repository::PostgresSessionRepository;
pub use settings_repository::PostgresSettingsRepository;
pub use state_change_repository::PostgresStateChangeRepository;
pub use story_repository::PostgresStoryRepository;
pub use task_repository::PostgresTaskRepository;
pub use user_directory_repository::PostgresUserDirectoryRepository;
pub use workflow_repository::PostgresWorkflowRepository;
pub use workspace_repository::PostgresWorkspaceRepository;
