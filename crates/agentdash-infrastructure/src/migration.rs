use agentdash_domain::common::error::DomainError;
use sqlx::PgPool;

const REQUIRED_POSTGRES_TABLES: &[&str] = &[
    "activity_execution_claims",
    "agent_assignments",
    "agent_frames",
    "agent_lineages",
    "agent_procedures",
    "auth_sessions",
    "backend_execution_leases",
    "backend_workspace_inventory",
    "backends",
    "canvas_bindings",
    "canvas_files",
    "canvases",
    "extension_package_artifacts",
    "group_memberships",
    "groups",
    "inline_fs_files",
    "lifecycle_agents",
    "lifecycle_gates",
    "lifecycle_runs",
    "lifecycle_subject_associations",
    "lifecycle_workflow_instances",
    "library_assets",
    "llm_providers",
    "llm_provider_user_credentials",
    "mcp_presets",
    "permission_grants",
    "project_agents",
    "project_backend_access",
    "project_extension_installations",
    "project_subject_grants",
    "project_vfs_mounts",
    "projects",
    "routine_executions",
    "routines",
    "runtime_health",
    "session_compactions",
    "session_events",
    "session_lineage",
    "session_projection_heads",
    "session_projection_segments",
    "session_runtime_commands",
    "session_terminal_effects",
    "sessions",
    "settings",
    "skill_assets",
    "state_changes",
    "stories",
    "user_preferences",
    "users",
    "views",
    "workflow_graphs",
    "workspace_bindings",
    "workspaces",
];

pub async fn run_postgres_migrations(pool: &PgPool) -> Result<(), DomainError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|err| DomainError::InvalidConfig(format!("数据库迁移失败: {err}")))?;
    Ok(())
}

pub async fn assert_postgres_schema_ready(pool: &PgPool) -> Result<(), DomainError> {
    assert_postgres_tables_ready(pool, REQUIRED_POSTGRES_TABLES).await
}

pub async fn assert_postgres_tables_ready(
    pool: &PgPool,
    tables: &[&str],
) -> Result<(), DomainError> {
    let mut missing = Vec::new();
    for table in tables {
        let regclass: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::TEXT")
            .bind(format!("public.{table}"))
            .fetch_one(pool)
            .await
            .map_err(|err| {
                DomainError::InvalidConfig(format!("schema readiness 检查失败: {err}"))
            })?;
        if regclass.is_none() {
            missing.push(*table);
        }
    }

    if missing.is_empty() {
        return Ok(());
    }

    Err(DomainError::InvalidConfig(format!(
        "PostgreSQL schema 未完成 migration，缺少表: {}",
        missing.join(", ")
    )))
}
