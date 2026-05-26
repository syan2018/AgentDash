//! AgentDash 本机 runtime library。
//!
//! CLI 与后续 Tauri desktop 都应把这里作为本机能力入口；二进制入口只负责参数解析和宿主启动。

mod extension_artifact_cache;
mod extension_host;
mod handlers;
pub use handlers::browse_directory;
pub mod local_backend_config;
mod machine_identity;
mod materialization;
mod mcp_client_manager;
mod terminal_manager;
mod tool_executor;
mod workspace_prepare;
mod workspace_probe;
mod ws_client;

pub mod runtime;

pub use extension_artifact_cache::{
    ExtensionArtifactCacheEntry, ExtensionArtifactCacheError, ExtensionArtifactDownloadRequest,
    download_and_cache_extension_artifact,
};
pub use extension_host::{
    LocalExtensionHostActivation, LocalExtensionHostError, LocalExtensionHostHealth,
    LocalExtensionHostManager, LocalExtensionHostProfile, LocalExtensionHostWorkspaceRoot,
    LocalTsExtensionHostConfig,
};

pub use runtime::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeHandle, LocalRuntimeManager,
    LocalRuntimeSnapshot, LocalRuntimeState, LocalRuntimeStatus, McpProbeResult, StopReason,
    canonicalize_workspace_roots, load_mcp_servers_for_root, probe_mcp_server, run_standalone,
    save_mcp_servers_for_root,
};

pub use machine_identity::{LocalMachineIdentity, load_or_create_machine_identity};
