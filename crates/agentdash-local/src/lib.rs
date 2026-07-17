//! AgentDash 本机 runtime library。
//!
//! CLI 与后续 Tauri desktop 都应把这里作为本机能力入口；二进制入口只负责参数解析和宿主启动。

mod desktop_claim;
mod desktop_profile;
mod desktop_runner_host;
mod desktop_settings;
mod extensions;
mod file_discovery_policy;
mod handlers;
pub use handlers::{
    HostRuntimeDriverEndpointResolver, RuntimeDriverEndpointResolver, RuntimeWireCommandHandler,
    browse_directory,
};
mod agent_runtime_host;
pub mod local_backend_config;
mod machine_identity;
mod materialization;
mod mcp_client_manager;
mod mcp_connect;
mod process_executor;
pub mod runner_claim;
pub mod runner_config;
mod runner_redaction;
pub mod runner_service;
pub mod runner_status;
mod search_executor;
mod shell_session_manager;
mod tool_executor;
mod workspace_identity_discovery;
mod workspace_probe;
mod workspace_root_guard;
mod ws_client;

pub mod runtime;
pub mod runtime_paths;

pub use desktop_claim::{
    DesktopClaimError, DesktopEnsureLocalRuntimePayload, DesktopEnsureLocalRuntimeResponse,
    DesktopEnsureRetryEvent, DesktopEnsureRetryPolicy, desktop_claim_error_from_http,
    desktop_ensure_payload_from_request, desktop_runtime_config_from_ensure,
    ensure_desktop_local_runtime, ensure_desktop_runtime_config, validate_desktop_ensure_response,
};
pub use desktop_profile::{
    DesktopRuntimeStartRequest, LocalRuntimeProfile, delete_desktop_runtime_profile,
    load_desktop_runtime_profile, load_desktop_runtime_profile_with_server_origin,
    normalize_desktop_runtime_profile, normalize_desktop_runtime_profile_with_server_origin,
    normalize_desktop_runtime_start_request,
    normalize_desktop_runtime_start_request_with_server_origin, save_desktop_runtime_profile,
    save_desktop_runtime_profile_with_server_origin,
};
pub use desktop_runner_host::DesktopRunnerHost;
pub use desktop_settings::{
    DesktopAppSettings, load_desktop_app_settings, normalize_desktop_app_settings,
    save_desktop_app_settings,
};
pub use extensions::{
    ExtensionArtifactCacheEntry, ExtensionArtifactCacheError, ExtensionArtifactDownloadRequest,
    ExtensionBackendServiceArtifact, ExtensionBackendServiceError,
    ExtensionBackendServiceInstanceIdentity, ExtensionBackendServiceInvokeError,
    ExtensionBackendServiceInvokeMetadata, ExtensionBackendServiceInvokeRequest,
    ExtensionBackendServiceInvokeResponse, ExtensionBackendServiceLogLine,
    ExtensionBackendServiceMaterialization, ExtensionBackendServiceMaterializeRequest,
    ExtensionBackendServiceReadiness, ExtensionBackendServiceStartRequest,
    ExtensionBackendServiceStatus, LocalExtensionBackendServiceManager,
    LocalExtensionBackendServiceManagerConfig, LocalExtensionHostActivation,
    LocalExtensionHostError, LocalExtensionHostHealth, LocalExtensionHostManager,
    LocalExtensionHostProfile, LocalExtensionHostWorkspaceRoot, LocalTsExtensionHostConfig,
    download_and_cache_extension_artifact,
};

pub use runtime::{
    LocalAgentRuntimeInstanceConfig, LocalLogEvent, LocalRuntimeConfig, LocalRuntimeHandle,
    LocalRuntimeManager, LocalRuntimeSnapshot, LocalRuntimeState, LocalRuntimeStatus,
    McpProbeResult, StopReason, canonicalize_workspace_roots, load_mcp_servers_for_root,
    probe_mcp_server, run_standalone, run_standalone_with_status,
    run_standalone_with_status_and_shutdown, save_mcp_servers_for_root,
};

pub use machine_identity::{LocalMachineIdentity, load_or_create_machine_identity};
pub use runner_config::{ResolvedRunnerConfig, RunnerCliOverrides, RunnerCredentials};
pub use runner_redaction::{redact_optional, redact_secret};
pub use runner_status::RunnerStatusSnapshot;
pub use runtime_paths::{
    desktop_app_settings_path, local_mcp_servers_path, local_runtime_config_dir,
    local_runtime_data_dir, local_runtime_profile_path, machine_identity_path,
};
