//! 工具能力解析器 — 统一计算 session 的有效工具集
//!
//! `CapabilityResolver` 根据 (session owner type, agent config, active workflow)
//! 计算出 `FlowCapabilities`（内置工具簇）和 `Vec<McpInjectionConfig>`（平台 MCP 端点）。
//!
//! 设计原则：
//! - 所有平台 well-known 能力由 visibility rule 过滤
//! - `mcp:*` 自定义能力从 agent config 的 mcp_servers 中按 name 查找
//! - 本模块取代各处散落的硬编码 FlowCapabilities 和 MCP injection 列表

mod resolver;

pub use resolver::{
    AgentMcpServerEntry, CapabilityResolver, CapabilityResolverInput, CapabilityResolverOutput,
    CompanionSliceMode,
};
