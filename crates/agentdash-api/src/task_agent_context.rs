/// 薄 re-export — Contributor 框架和内置实现已迁移到 agentdash-application::context
pub use agentdash_application::context::*;

use std::sync::Arc;

use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::workspace::Workspace;
use agentdash_spi::ResolveSourcesOutput;

use crate::app_state::AppState;

/// API 层适配器 — 委托给 application 层的核心实现，BackendRegistry 通过 blanket impl 满足 BackendAvailability
pub async fn resolve_workspace_declared_sources(
    state: &Arc<AppState>,
    sources: &[ContextSourceRef],
    workspace: Option<&Workspace>,
    base_order: i32,
) -> Result<ResolveSourcesOutput, String> {
    agentdash_application::context::resolve_workspace_declared_sources(
        state.services.backend_registry.as_ref(),
        &state.services.vfs_service,
        sources,
        workspace,
        base_order,
    )
    .await
}
