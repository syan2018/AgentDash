mod operation_authority;
mod operation_core;
mod operation_error;
mod operation_gateway;
mod operation_hosts;
mod operation_provider;
mod operation_script_adapter;
mod operation_types;
mod schema;

pub use agentdash_domain::operation::{
    OperationEffect, OperationOriginRef, OperationPrincipalRef, OperationProviderRef, OperationRef,
    OperationReplayPolicy, OperationScopeRef,
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
