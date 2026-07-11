mod operation_authority;
mod operation_core;
mod operation_error;
mod operation_gateway;
mod operation_hosts;
mod operation_provider;
mod operation_script_adapter;
mod operation_types;
mod schema;
mod setup_operations;

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
    OperationEffect, OperationOriginRef, OperationPrincipalRef, OperationProviderRef, OperationRef,
    OperationReplayPolicy, OperationScopeRef,
};
pub use extension_operations::{
    EXTENSION_OPERATION_NAMESPACE, ExtensionOperationContextPort, ExtensionOperationProvider,
    ExtensionOperationRuntimeContext,
};
pub use extension_workspace::{
    ExtensionInvocationWorkspaceContext, ExtensionInvocationWorkspaceResolution,
    ExtensionInvocationWorkspaceUnavailableReason, resolve_extension_invocation_workspace,
};
pub use interaction_operations::{
    INTERACTION_OPERATION_NAMESPACE, InteractionCommandOperation, InteractionOperationAccess,
    InteractionOperationProvider,
};
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
    EphemeralOperationResultStore, OperationGateway, TracingOperationAuditSink,
};
pub use operation_hosts::{
    AgentRunOperationHost, BoundOperationHost, BoundOperationScriptHost,
    ExtensionServiceOperationHost, HostInvocationOptions, HostOperationInvocation,
    HostOperationScriptProgram, UserWorkshopOperationHost,
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
pub use schema::{validate_json_schema_definition, validate_json_schema_subset};
pub use setup_operations::{
    SETUP_OPERATION_NAMESPACE, SETUP_OPERATION_PROVIDER_KEY, SetupOperationAccessPort,
    SetupOperationAuthorityResolver, SetupOperationProvider, setup_operation_ref,
};
mod extension_operations;
mod extension_workspace;
mod interaction_operations;
mod mcp_operations;
