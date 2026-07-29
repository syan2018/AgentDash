mod projection;
mod provider;
mod system_providers;
mod tool_service;
mod tools;

pub use projection::{
    WorkspaceModulePresentationError, build_workspace_module_presentation,
    workspace_module_operation_from_descriptor,
};
pub use provider::{
    WorkspaceModuleActor, WorkspaceModuleOperateRequest, WorkspaceModulePresentationPreparation,
    WorkspaceModulePresentationRequest, WorkspaceModuleProvider, WorkspaceModuleProviderContext,
    WorkspaceModuleProviderRegistry,
};
pub use system_providers::{ExtensionWorkspaceModuleProvider, PlatformWorkspaceModuleProvider};
pub use tool_service::{
    ApplicationWorkspaceModuleRuntimeToolService, WorkspaceModuleRuntimeToolDeps,
    workspace_module_runtime_tool_schema,
};
