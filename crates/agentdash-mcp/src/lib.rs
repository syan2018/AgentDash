//! # agentdash-mcp
//!
//! AgentDashboard MCP 服务层：提供三层级工具暴露框架。
//!
//! ## 架构设计
//!
//! 三层 MCP Server，每层独立暴露工具集，支持不同来源的调用隔离：
//!
//! - **Relay 层** (`RelayMcpServer`)：面向用户，支持看板全局操作
//! - **Story 层** (`StoryMcpServer`)：面向编排 Agent，支持 Story 上下文管理
//! - **Task 层** (`TaskMcpServer`)：面向执行 Agent，支持 Task 粒度操作
//!
//! ## 传输
//!
//! 通过 Streamable HTTP 与现有 Axum 服务集成，不同路径映射到不同层级的 Server：
//! - `POST /mcp/relay` → Relay 工具
//! - `POST /mcp/story/{story_id}` → Story 工具
//! - `POST /mcp/task/{task_id}` → Task 工具

pub mod error;
pub mod scope;
pub mod servers;
pub mod services;
pub mod transport;
