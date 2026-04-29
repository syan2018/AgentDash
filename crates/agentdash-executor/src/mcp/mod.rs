//! 通用 MCP 工具发现与适配层
//!
//! 此模块将 MCP server 声明转化为 `AgentTool` 实例，供任意 connector 使用。
//! 包含两种模式：
//! - **direct**: 云端直连 HTTP MCP server（Streamable HTTP 传输）
//! - **relay**: 通过 relay 信道代理到本机 backend 上的 MCP server

pub mod direct;
pub mod relay;

pub use direct::{discover_mcp_tools, namespaced_tool_name};
pub use relay::discover_relay_mcp_tools;
