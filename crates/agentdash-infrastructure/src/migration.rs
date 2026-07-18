use agentdash_domain::common::error::DomainError;
use sqlx::PgPool;

const REQUIRED_POSTGRES_TABLES: &[&str] = &[
    "agent_frame_transitions",
    "agent_frames",
    "agent_lineages",
    "agent_procedures",
    "agent_run_mailbox_messages",
    "agent_run_mailbox_states",
    "agent_run_product_command_receipts",
    "auth_sessions",
    "backend_execution_leases",
    "backend_workspace_inventory",
    "backends",
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
    "library_assets",
    "llm_providers",
    "llm_provider_user_credentials",
    "mcp_presets",
    "project_agents",
    "project_backend_access",
    "project_extension_installations",
    "project_subject_grants",
    "project_vfs_mounts",
    "projects",
    "routine_executions",
    "routines",
    "runner_registration_tokens",
    "runtime_health",
    "agent_run_fork_saga",
    "agent_run_fork_graph",
    "companion_fresh_saga",
    "agent_runtime_state_revision",
    "agent_runtime_source_projection",
    "agent_runtime_source_identity",
    "agent_runtime_source_change",
    "agent_runtime_projection",
    "agent_runtime_thread_binding",
    "agent_runtime_operation",
    "agent_runtime_idempotency",
    "agent_runtime_pending_command",
    "agent_runtime_change",
    "agent_runtime_outbox",
    "agent_runtime_surface_snapshot",
    "agent_runtime_host_revision",
    "agent_service_instance",
    "agent_runtime_offer",
    "agent_runtime_placement",
    "agent_runtime_lifecycle_target",
    "agent_runtime_lifecycle_effect",
    "agent_runtime_binding",
    "agent_runtime_source_coordinate",
    "agent_runtime_callback_route",
    "agent_runtime_callback_route_tombstone",
    "agent_runtime_effect",
    "agent_runtime_effect_attempt_history",
    "agent_runtime_lease",
    "agent_runtime_lease_epoch",
    "agent_runtime_callback_revision",
    "agent_runtime_callback_reservation",
    "agent_runtime_callback_outcome",
    "dash_agent_session",
    "dash_agent_branch",
    "dash_agent_history",
    "dash_agent_command",
    "dash_agent_effect",
    "dash_agent_change",
    "dash_complete_source",
    "dash_complete_effect",
    "agent_run_control_effects",
    "settings",
    "skill_assets",
    "state_changes",
    "stories",
    "users",
    "views",
    "workflow_graphs",
    "workspace_bindings",
    "workspaces",
];

const RETIRED_POSTGRES_TABLES: &[&str] = &[
    "agent_run_runtime_binding",
    "agent_run_command_receipts",
    "agent_run_delivery_bindings",
    "runtime_session_compaction_requests",
    "runtime_session_execution_anchors",
    "runtime_session_delivery_commands",
    "runtime_session_projection_segments",
    "runtime_session_projection_heads",
    "runtime_session_lineage",
    "runtime_session_compactions",
    "runtime_session_events",
    "runtime_sessions",
    "agent_runtime_thread",
    "agent_runtime_event",
    "agent_runtime_terminal_application_effect_outbox",
    "agent_runtime_turn",
    "agent_runtime_item",
    "agent_runtime_interaction",
    "agent_runtime_quarantine",
    "agent_runtime_hook_plan",
    "agent_runtime_hook_run",
    "agent_runtime_hook_effect",
    "agent_runtime_tool_call",
    "agent_runtime_service_instance",
    "agent_runtime_service_instance_revision",
    "agent_runtime_service_activation",
    "agent_runtime_host_binding",
    "agent_runtime_driver_lease",
    "agent_runtime_driver_coordinate",
    "agent_run_runtime_thread_anchor",
    "agent_run_runtime_binding_lineage",
    "agent_run_runtime_recovery_intent",
    "agent_context_checkpoint",
    "agent_context_preparation",
    "agent_context_candidate",
    "agent_context_head",
    "agent_context_activation",
    "agent_context_activation_dispatch",
];

pub async fn run_postgres_migrations(pool: &PgPool) -> Result<(), DomainError> {
    // sqlx::migrate! 在编译期收集 migration 元数据；迁移文件变更时同步刷新本模块。
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|err| DomainError::InvalidConfig(format!("数据库迁移失败: {err}")))?;
    Ok(())
}

pub async fn assert_postgres_schema_ready(pool: &PgPool) -> Result<(), DomainError> {
    assert_postgres_tables_ready(pool, REQUIRED_POSTGRES_TABLES).await?;
    assert_postgres_tables_absent(pool, RETIRED_POSTGRES_TABLES).await
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

pub async fn assert_postgres_tables_absent(
    pool: &PgPool,
    tables: &[&str],
) -> Result<(), DomainError> {
    let mut present = Vec::new();
    for table in tables {
        let regclass: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::TEXT")
            .bind(format!("public.{table}"))
            .fetch_one(pool)
            .await
            .map_err(|err| {
                DomainError::InvalidConfig(format!("schema retirement 检查失败: {err}"))
            })?;
        if regclass.is_some() {
            present.push(*table);
        }
    }

    if present.is_empty() {
        return Ok(());
    }

    Err(DomainError::InvalidConfig(format!(
        "PostgreSQL schema 仍包含已退役表: {}",
        present.join(", ")
    )))
}
