mod agent_repository;
mod agent_run_command_receipt_repository;
mod agent_run_lineage_repository;
mod agent_run_mailbox_repository;
mod agent_run_message_submission_store;
mod agent_run_product_persistence;
mod agent_run_product_projection_repository;
mod agent_run_product_saga_repository;
mod auth_session_repository;
mod backend_execution_lease_repository;
mod backend_repository;
mod canvas_repository;
mod canvas_runtime_state_repository;
mod complete_agent_repositories;
mod dash_complete_agent_store;
mod extension_package_artifact_repository;
mod inline_file_repository;
mod json_document;
mod lifecycle_anchor_repository;
mod llm_provider_repository;
mod mcp_preset_repository;
mod owner_document;
mod project_backend_access_repository;
mod project_extension_installation_repository;
mod project_repository;
mod project_vfs_mount_repository;
mod routine_repository;
mod runner_registration_token_repository;
mod runtime_health_repository;
mod settings_repository;
mod shared_library_repository;
mod skill_asset_repository;
mod state_change_repository;
mod state_change_store;
mod story_repository;
mod user_directory_repository;
mod workflow_agent_call_repository;
mod workflow_executor_effect_repository;
mod workflow_recovery_repository;
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

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("应能连接测试 PostgreSQL");
    crate::migration::run_postgres_migrations(&pool)
        .await
        .expect("测试 PostgreSQL 应能运行 migrations");
    crate::migration::assert_postgres_schema_ready(&pool)
        .await
        .expect("测试 PostgreSQL schema 应已就绪");
    Some(pool)
}

/// 统一的 sqlx 错误映射 helper。
///
/// Repository 在基础设施边界保留数据库错误语义，API 层再根据 `DomainError`
/// 映射 HTTP status，避免靠字符串嗅探恢复唯一约束冲突。
pub(crate) fn db_err(error: sqlx::Error) -> DomainError {
    map_sqlx_error("database", "database", error)
}

/// 带表名前缀的 sqlx 错误映射，便于定位出错的表。
///
/// 与 [`db_err`] 共用一处实现，仅多一个可选的表名前缀；命名刻意不以 `db_err`
/// 开头，以保证 `db_err` helper 在工作区内唯一。
pub(crate) fn sql_err_for(table: &'static str, error: sqlx::Error) -> DomainError {
    map_sqlx_error(table, table, error)
}

fn map_sqlx_error(
    entity: &'static str,
    operation: &'static str,
    error: sqlx::Error,
) -> DomainError {
    match error {
        sqlx::Error::RowNotFound => DomainError::NotFound {
            entity,
            id: "row_not_found".to_string(),
        },
        sqlx::Error::Database(error) => map_database_error(entity, operation, error.as_ref()),
        other => DomainError::Database {
            operation,
            message: other.to_string(),
        },
    }
}

fn map_database_error(
    entity: &'static str,
    operation: &'static str,
    error: &(dyn sqlx::error::DatabaseError + 'static),
) -> DomainError {
    let code = error.code().map(|code| code.into_owned());
    match code.as_deref() {
        Some("23505") => DomainError::Conflict {
            entity,
            constraint: "unique",
            message: database_constraint_message("唯一约束冲突", error),
        },
        Some("23503") => DomainError::Conflict {
            entity,
            constraint: "foreign_key",
            message: database_constraint_message("外键约束冲突", error),
        },
        Some("23P01") => DomainError::Conflict {
            entity,
            constraint: "exclusion",
            message: database_constraint_message("排他约束冲突", error),
        },
        _ => DomainError::Database {
            operation,
            message: error.message().to_string(),
        },
    }
}

fn database_constraint_message(
    fallback: &'static str,
    error: &(dyn sqlx::error::DatabaseError + 'static),
) -> String {
    error
        .constraint()
        .map(|constraint| format!("{fallback}: {constraint}"))
        .unwrap_or_else(|| fallback.to_string())
}

pub use agent_repository::PostgresProjectAgentRepository;
pub use agent_run_command_receipt_repository::PostgresAgentRunCommandReceiptRepository;
pub use agent_run_fork_graph_store::PostgresAgentRunForkGraphStore;
pub use agent_run_lineage_repository::PostgresAgentRunLineageRepository;
pub use agent_run_mailbox_repository::PostgresAgentRunMailboxRepository;
pub use agent_run_message_submission_store::PostgresAgentRunMessageSubmissionStore;
pub use agent_run_product_persistence::{
    PostgresAgentRunAppliedResourceSurfaceRepository, PostgresProductMailboxRepository,
    PostgresProductRuntimeCommandClaimRepository,
};
pub use agent_run_product_projection_repository::{
    PostgresAgentRunProductRuntimeBindingRepository, PostgresAgentRunTerminalProjectionStore,
    PostgresWorkspaceModulePresentationStore, product_runtime_binding_digest,
};
pub use agent_run_product_saga_repository::{
    PostgresAgentRunForkSagaRepository, PostgresCompanionFreshSagaRepository,
};
pub use auth_session_repository::PostgresAuthSessionRepository;
pub use backend_execution_lease_repository::PostgresBackendExecutionLeaseRepository;
pub use backend_repository::PostgresBackendRepository;
pub use canvas_repository::PostgresCanvasRepository;
pub use canvas_runtime_state_repository::PostgresCanvasRuntimeStateRepository;
pub use complete_agent_repositories::{
    PostgresCompleteAgentCallbackRepository, PostgresCompleteAgentHostRepository,
    PostgresManagedRuntimeStateRepository,
};
pub use dash_complete_agent_store::{
    PostgresDashAgentRepositoryStore, PostgresDashCompleteAgentStore,
};
pub use extension_package_artifact_repository::PostgresExtensionPackageArtifactRepository;
pub use inline_file_repository::PostgresInlineFileRepository;
pub use lifecycle_anchor_repository::{
    PostgresAgentFrameRepository, PostgresAgentLineageRepository, PostgresLifecycleAgentRepository,
    PostgresLifecycleGateRepository, PostgresLifecycleSubjectAssociationRepository,
};
pub use llm_provider_repository::{
    PostgresLlmProviderCredentialRepository, PostgresLlmProviderRepository,
};
pub use mcp_preset_repository::PostgresMcpPresetRepository;
pub use project_backend_access_repository::PostgresProjectBackendAccessRepository;
pub use project_extension_installation_repository::PostgresProjectExtensionInstallationRepository;
pub use project_repository::PostgresProjectRepository;
pub use project_vfs_mount_repository::PostgresProjectVfsMountRepository;
pub use routine_repository::{PostgresRoutineExecutionRepository, PostgresRoutineRepository};
pub use runner_registration_token_repository::PostgresRunnerRegistrationTokenRepository;
pub use runtime_health_repository::PostgresRuntimeHealthRepository;
pub use settings_repository::PostgresSettingsRepository;
pub use shared_library_repository::PostgresSharedLibraryRepository;
pub use skill_asset_repository::PostgresSkillAssetRepository;
pub use state_change_repository::PostgresStateChangeRepository;
pub use story_repository::PostgresStoryRepository;
pub use user_directory_repository::PostgresUserDirectoryRepository;
pub use workflow_agent_call_repository::PostgresWorkflowAgentCallRepository;
pub use workflow_executor_effect_repository::PostgresWorkflowExecutorEffectRepository;
pub use workflow_recovery_repository::PostgresWorkflowRecoveryRepository;
pub use workflow_repository::PostgresWorkflowRepository;
pub use workspace_repository::PostgresWorkspaceRepository;
mod agent_run_fork_graph_store;
