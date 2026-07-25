mod error;
mod extension_actions;
mod extension_workspace;
mod gateway;
mod provider;
mod schema;
mod setup_actions;
mod types;

pub use agentdash_application_ports::extension_gateway_setup::{
    ExtensionGatewaySetupError, MCP_PROBE_TRANSPORT_ACTION, McpProbeSetupPort, McpProbeTarget,
    McpProbeToolOutput, McpProbeTransportInput, McpProbeTransportOutput,
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WORKSPACE_DETECT_ACTION, WORKSPACE_DETECT_GIT_ACTION,
    WORKSPACE_DISCOVER_BY_IDENTITY_ACTION, WorkspaceBrowseDirectoryEntry,
    WorkspaceBrowseDirectoryInput, WorkspaceBrowseDirectoryOutput,
    WorkspaceBrowseDirectorySetupPort, WorkspaceDetectGitInput, WorkspaceDetectGitOutput,
    WorkspaceDetectGitSetupPort, WorkspaceDetectInput, WorkspaceDetectOutput,
    WorkspaceDetectSetupPort, WorkspaceDiscoverByIdentityCandidateOutput,
    WorkspaceDiscoverByIdentityInput, WorkspaceDiscoverByIdentityOutput,
    WorkspaceDiscoverByIdentitySetupPort, WorkspaceDiscoverByIdentitySkippedOutput,
    WorkspaceDiscoverByIdentityWorkspaceInput,
};
pub use agentdash_application_ports::extension_runtime::{
    ExtensionBackendServiceTransport, ExtensionRuntimeActionTransport,
    ExtensionRuntimeActionTransportError, ExtensionRuntimeChannelTransport,
};
pub use error::{RuntimeInvocationError, RuntimeInvocationErrorKind};
pub use extension_actions::{
    EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_ID_METADATA,
    EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_KEY_METADATA,
    EXTENSION_RUNTIME_DESCRIPTOR_INSTALLATION_ID_METADATA, ExtensionInvocationWorkspaceContext,
    ExtensionRuntimeActionProvider, ExtensionRuntimeBackendServiceInvokeRequest,
    ExtensionRuntimeBackendServiceInvokeResult, ExtensionRuntimeBackendServiceInvoker,
    ExtensionRuntimeChannelConsumer, ExtensionRuntimeChannelInvokeRequest,
    ExtensionRuntimeChannelInvokeResult, ExtensionRuntimeChannelInvoker,
    attach_extension_invocation_workspace,
};
pub use extension_workspace::{
    ExtensionInvocationWorkspaceResolution, ExtensionInvocationWorkspaceUnavailableReason,
    resolve_extension_invocation_workspace,
};
pub use gateway::ExtensionGateway;
pub use provider::RuntimeProvider;
pub use schema::validate_json_schema_subset;
pub use setup_actions::{
    McpProbeTransportProvider, WorkspaceBrowseDirectoryProvider, WorkspaceDetectGitProvider,
    WorkspaceDetectProvider, WorkspaceDiscoverByIdentityProvider,
};
pub use types::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKeyError, RuntimeActionKind,
    RuntimeActor, RuntimeContext, RuntimeInvocationOutput, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimePolicy, RuntimeSurface, RuntimeTarget, RuntimeTrace,
};
