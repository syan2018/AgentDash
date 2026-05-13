//! AgentDash 本机 runtime library。
//!
//! CLI 与后续 Tauri desktop 都应把这里作为本机能力入口；二进制入口只负责参数解析和宿主启动。

mod handlers;
mod local_backend_config;
mod materialization;
mod mcp_client_manager;
mod terminal_manager;
mod tool_executor;
mod workspace_prepare;
mod workspace_probe;
mod ws_client;

pub mod runtime;

pub use runtime::{
    LocalRuntimeConfig, LocalRuntimeHandle, LocalRuntimeManager, LocalRuntimeSnapshot,
    LocalRuntimeState, LocalRuntimeStatus, StopReason, canonicalize_accessible_roots,
    run_standalone,
};
