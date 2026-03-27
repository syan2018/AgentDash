/// 薄 re-export — Contributor 框架和内置实现已迁移到 agentdash-application::context
pub use agentdash_application::context::*;

use std::sync::Arc;

use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::workspace::Workspace;
use agentdash_injection::ResolveSourcesOutput;

use crate::app_state::AppState;
use crate::workspace_resolution::AppStateBackendAvailability;

/// API 层适配器 — 委托给 application 层的核心实现，注入 AppState 的 BackendAvailability
pub async fn resolve_workspace_declared_sources(
    state: &Arc<AppState>,
    sources: &[ContextSourceRef],
    workspace: Option<&Workspace>,
    base_order: i32,
) -> Result<ResolveSourcesOutput, String> {
    let availability = AppStateBackendAvailability::new(state.clone());
    agentdash_application::context::resolve_workspace_declared_sources(
        &availability,
        &state.services.address_space_service,
        sources,
        workspace,
        base_order,
    )
    .await
}
