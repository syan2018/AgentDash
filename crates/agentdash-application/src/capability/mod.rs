//! 工具能力解析器 — 统一计算 session 的有效工具集
//!
//! `CapabilityResolver` 根据 (session owner type, agent config, active workflow)
//! 计算出 `FlowCapabilities`（内置工具簇）和 `Vec<McpInjectionConfig>`（平台 MCP 端点）。
//!
//! 设计原则：
//! - 所有平台 well-known 能力由 visibility rule 过滤
//! - `mcp:*` 自定义能力从 agent config 的 mcp_servers 中按 name 查找
//! - 本模块取代各处散落的硬编码 FlowCapabilities 和 MCP injection 列表

mod notification;
mod resolver;
mod session_workflow_context;
pub mod tool_catalog;

#[cfg(test)]
mod pipeline_tests;

pub use notification::{
    build_capability_delta_markdown, capability_description, is_known_capability_key,
};
pub use resolver::{
    AgentMcpServerEntry, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    CapabilityResolverOutput, CompanionSliceMode,
};
pub use session_workflow_context::{
    capability_directives_from_active_workflow, resolve_session_workflow_context,
    SessionWorkflowContext, SessionWorkflowOwner, SessionWorkflowRepos,
};
pub use tool_catalog::query_tool_catalog;
