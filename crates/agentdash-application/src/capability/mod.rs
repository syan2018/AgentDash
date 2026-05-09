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
    AgentMcpServerEntry, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    CapabilityResolverOutput, CompanionContribution, ContextContributionSource,
    ContextContributions, McpCandidates, ToolContribution,
};
pub use session_workflow_context::{
    SessionWorkflowOwner, SessionWorkflowRepos, resolve_session_workflow_context,
    tool_directives_from_active_workflow,
};
pub use tool_catalog::query_tool_catalog;
