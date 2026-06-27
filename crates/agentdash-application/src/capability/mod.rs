//! 工具能力解析器 — 统一计算 session 的有效工具集
//!
//! `CapabilityResolver` 根据 (session owner type, agent config, active workflow)
//! 计算出唯一运行态 `CapabilityState`，其中包含能力 key、本地工具簇、工具策略、
//! MCP server 与 VFS 投影。
//!
//! 设计原则：
//! - 所有平台 well-known 能力由 visibility rule 过滤
//! - `mcp:*` 自定义能力优先从 project MCP preset 按 key 查找
//! - 本模块取代各处散落的硬编码能力状态和 MCP injection 列表

mod notification;
mod resolver;
mod session_workflow_context;
pub mod tool_catalog;

#[cfg(test)]
mod pipeline_tests;

pub use agentdash_spi::CompanionSliceMode;
pub use notification::{
    build_capability_delta_markdown, capability_description, is_known_capability_key,
};
pub use resolver::{
    AuthorityState, AvailableMcpPresets, CapabilityContext, CapabilityResolver,
    CapabilityResolverInput, CapabilityResolverOutput, CompanionContribution,
    ContextContributionSource, ContextContributions, McpCandidates, OperationAuthorityStatus,
    ToolContribution,
};
pub use session_workflow_context::{
    SessionWorkflowOwner, SessionWorkflowRepos, resolve_session_workflow_context,
    tool_directives_from_active_workflow, tool_directives_from_active_workflow_projection,
};
pub use tool_catalog::{query_capability_catalog, query_tool_catalog};

use crate::repository_set::RepositorySet;
use agentdash_diagnostics::{Subsystem, diag};

/// 加载 project 级 MCP Preset 并展开为 resolver 消费的 map。
///
/// 能力解析调用方共享同一份 project preset 视图，避免 session / workflow /
/// task 各自维护不同的 MCP preset 查询路径。
pub async fn load_available_presets(
    repos: &RepositorySet,
    project_id: uuid::Uuid,
) -> AvailableMcpPresets {
    match repos.mcp_preset_repo.list_by_project(project_id).await {
        Ok(presets) => presets.into_iter().map(|p| (p.key.clone(), p)).collect(),
        Err(error) => {
            diag!(Warn, Subsystem::AgentRun,

                project_id = %project_id,
                error = %error,
                "加载 project MCP Preset 列表失败,mcp:<X> 能力无法解析为 RuntimeMcpServer"
            );
            Default::default()
        }
    }
}
