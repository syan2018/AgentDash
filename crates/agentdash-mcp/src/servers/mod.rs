//! MCP Server 实现
//!
//! 三层 MCP Server，每层独立暴露工具集：
//! - `relay`: 面向用户的看板全局操作
//! - `story`: 面向编排 Agent 的 Story 上下文管理
//! - `task`: 面向执行 Agent 的 Task 粒度操作

pub mod relay;
pub mod story;
pub mod task;

pub use relay::RelayMcpServer;
pub use story::StoryMcpServer;
pub use task::TaskMcpServer;
