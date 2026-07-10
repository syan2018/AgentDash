mod error;
mod extension_actions;
mod extension_workspace;
mod gateway;
mod mcp_access;
mod mcp_operations;
mod operation_authority;
mod operation_core;
mod operation_error;
mod operation_gateway;
mod operation_hosts;
mod operation_provider;
mod operation_script_adapter;
mod operation_types;
mod provider;
mod schema;
mod session_actions;
mod setup_operations;
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
pub use agentdash_domain::operation::{
    OperationEffect, OperationOriginRef, OperationPrincipalRef, OperationReplayPolicy,
    OperationScopeRef,
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
pub use mcp_operations::{
    MCP_OPERATION_NAMESPACE, McpOperationProvider, OperationMcpAccess, OperationMcpTool,
};
pub use operation_authority::CompositeOperationAuthorityResolver;
pub use operation_core::{
    OperationAuditSink, OperationDispatcher, OperationExecutionCore, OperationPlacementResolver,
    OperationResultStore, OperationSurfaceResolver, result_access_matches, scope_project_id,
};
pub use operation_error::{OperationExecutionError, OperationExecutionErrorKind};
pub use operation_gateway::{
    InMemoryOperationResultStore, OperationGateway, TracingOperationAuditSink,
};
pub use operation_hosts::{
    AgentRunOperationHost, BoundOperationHost, ExtensionServiceOperationHost,
    HostInvocationOptions, HostOperationInvocation, UserWorkshopOperationHost,
};
pub use operation_provider::{
    DynamicOperationProvider, OperationAuthorityGrant, OperationAuthorityResolver,
    OperationProvider,
};
pub use operation_script_adapter::GatewayOperationScriptExecutor;
pub use operation_types::{
    ActorOperationSurface, OperationActorKind, OperationAuditEvent, OperationAuditStage,
    OperationAuthorizationScope, OperationCatalog, OperationDescriptor, OperationDispatch,
    OperationExecutionPolicy, OperationExecutionRequest, OperationExecutionResult,
    OperationInvocationCommand, OperationInvocationEnvelope, OperationPlacement,
    OperationPrincipal, OperationProvenance, OperationReadiness, OperationResultAccess,
    OperationResultRef, OperationResultValue, OperationTraceContext, ScopedOperationResult,
};
pub use provider::RuntimeProvider;
pub use schema::{validate_json_schema_definition, validate_json_schema_subset};
pub(crate) use session_actions::execute_runtime_mcp_tool;
pub use session_actions::{
    MCP_CALL_TOOL_ACTION, MCP_LIST_TOOLS_ACTION, McpCallToolInput, McpCallToolProvider,
    McpListToolsInput, McpListToolsOutput, McpListToolsProvider, RuntimeMcpToolDescriptor,
    RuntimeSessionMcpAccess, RuntimeSessionMcpError,
};
pub use setup_operations::{
    SETUP_OPERATION_NAMESPACE, SETUP_OPERATION_PROVIDER_KEY, SetupOperationAccessPort,
    SetupOperationAuthorityResolver, SetupOperationProvider, setup_operation_ref,
};
pub use tool_adapter::{RuntimeActionToolAdapter, RuntimeActionToolSpec};
pub use types::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKeyError, RuntimeActionKind,
    RuntimeActor, RuntimeContext, RuntimeInvocationOutput, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimePolicy, RuntimeSurface, RuntimeTarget, RuntimeTrace,
};
