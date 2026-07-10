mod error;
mod extension_actions;
mod extension_workspace;
mod gateway;
mod mcp_access;
mod operation_core;
mod operation_error;
mod operation_types;
mod provider;
mod schema;
mod session_actions;
mod setup_actions;
mod tool_adapter;
mod types;

pub use agentdash_application_ports::extension_runtime::{
    ExtensionBackendServiceTransport, ExtensionRuntimeActionTransport,
    ExtensionRuntimeActionTransportError, ExtensionRuntimeProtocolTransport,
};
pub use agentdash_application_ports::runtime_gateway_setup::{
    MCP_PROBE_TRANSPORT_ACTION, McpProbeSetupPort, McpProbeTarget, McpProbeToolOutput,
    McpProbeTransportInput, McpProbeTransportOutput, RuntimeGatewaySetupError,
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
pub use error::{RuntimeInvocationError, RuntimeInvocationErrorKind};
pub use extension_actions::{
    EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_ID_METADATA,
    EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_KEY_METADATA,
    EXTENSION_RUNTIME_DESCRIPTOR_INSTALLATION_ID_METADATA, ExtensionInvocationWorkspaceContext,
    ExtensionRuntimeActionProvider, ExtensionRuntimeBackendServiceInvokeRequest,
    ExtensionRuntimeBackendServiceInvokeResult, ExtensionRuntimeBackendServiceInvoker,
    ExtensionRuntimeProtocolConsumer, ExtensionRuntimeProtocolInvokeRequest,
    ExtensionRuntimeProtocolInvokeResult, ExtensionRuntimeProtocolInvoker,
    attach_extension_invocation_workspace,
};
pub use extension_workspace::{
    ExtensionInvocationWorkspaceResolution, ExtensionInvocationWorkspaceUnavailableReason,
    resolve_extension_invocation_workspace,
};
pub use gateway::RuntimeGateway;
pub use mcp_access::CurrentSurfaceRuntimeMcpAccess;
pub use operation_core::{
    OperationAuditSink, OperationDispatcher, OperationExecutionCore, OperationPlacementResolver,
    OperationResultStore, OperationSurfaceResolver, result_access_matches, scope_project_id,
};
pub use operation_error::{OperationExecutionError, OperationExecutionErrorKind};
pub use operation_types::{
    ActorOperationSurface, OperationActorKind, OperationAuditEvent, OperationAuditStage,
    OperationAuthorizationScope, OperationCatalog, OperationDescriptor, OperationDispatch,
    OperationEffect, OperationExecutionPolicy, OperationExecutionRequest, OperationExecutionResult,
    OperationInvocationEnvelope, OperationOrigin, OperationPlacement, OperationPrincipal,
    OperationProvenance, OperationReadiness, OperationReplayPolicy, OperationResultAccess,
    OperationResultRef, OperationResultValue, OperationScopeRef, OperationTraceContext,
    ScopedOperationResult,
};
pub use provider::RuntimeProvider;
pub use schema::{validate_json_schema_definition, validate_json_schema_subset};
pub(crate) use session_actions::execute_runtime_mcp_tool;
pub use session_actions::{
    MCP_CALL_TOOL_ACTION, MCP_LIST_TOOLS_ACTION, McpCallToolInput, McpCallToolProvider,
    McpListToolsInput, McpListToolsOutput, McpListToolsProvider, RuntimeMcpToolDescriptor,
    RuntimeSessionMcpAccess, RuntimeSessionMcpError,
};
pub use setup_actions::{
    McpProbeTransportProvider, WorkspaceBrowseDirectoryProvider, WorkspaceDetectGitProvider,
    WorkspaceDetectProvider, WorkspaceDiscoverByIdentityProvider,
};
pub use tool_adapter::{RuntimeActionToolAdapter, RuntimeActionToolSpec};
pub use types::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKeyError, RuntimeActionKind,
    RuntimeActor, RuntimeContext, RuntimeInvocationOutput, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimePolicy, RuntimeSurface, RuntimeTarget, RuntimeTrace,
};
