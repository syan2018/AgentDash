mod builder;
mod builtins;
pub mod rendering;
pub mod slot_orders;
pub mod source_resolver;
pub mod vfs_discovery;
mod workflow_bindings;
pub mod workspace_sources;

pub use agentdash_application_runtime_session::context::{
    AuditFilter, AuditTrigger, ContextAuditBus, ContextAuditEvent, InMemoryContextAuditBus,
    NoopContextAuditBus, SharedContextAuditBus, emit_bundle_fragments, emit_fragment,
};
pub use builder::{
    ContextBuildPhase, Contribution, SessionContextConfig, TaskExecutionPhase,
    build_continuation_bundle_from_markdown, build_continuation_transcript_fragment,
    build_declared_source_warning_fragment, build_session_context_bundle,
};
pub use builtins::build_owner_context_resource_block;
pub use builtins::contribute_mcp;
pub(crate) use builtins::{trim_or_dash, workspace_context_fragment};
pub use source_resolver::{
    SourceResolverRegistry, resolve_declared_sources, resolve_declared_sources_with_registry,
};
pub use vfs_discovery::{VfsDiscoveryRegistry, builtin_vfs_registry};
pub use workflow_bindings::contribute_workflow_binding;
pub use workspace_sources::{
    contribute_workspace_static_sources, resolve_workspace_declared_sources,
};
