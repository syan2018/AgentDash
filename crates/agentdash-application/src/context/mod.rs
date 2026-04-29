mod builder;
mod builtins;
pub mod context_composer;
mod contributor;
pub mod source_resolver;
pub mod vfs_discovery;
mod workflow_bindings;
pub mod workspace_sources;

pub use builder::{
    ContextBuildPhase, Contribution, SessionContextConfig, build_declared_source_warning_fragment,
    build_session_context_bundle, build_task_agent_context,
};
pub use builtins::{McpContextContributor, StaticFragmentsContributor};
pub use builtins::build_owner_context_resource_block;
pub use builtins::{
    contribute_binding_initial_context, contribute_core_context, contribute_declared_sources,
    contribute_instruction, contribute_mcp,
};
pub(crate) use builtins::{trim_or_dash, workspace_context_fragment};
pub use context_composer::ContextComposer;
pub use contributor::{
    BuiltTaskAgentContext, ContextContributor, ContextContributorRegistry, ContributorInput,
    TaskAgentBuildInput, TaskExecutionPhase,
};
pub use source_resolver::{
    SourceResolverRegistry, resolve_declared_sources, resolve_declared_sources_with_registry,
};
pub use vfs_discovery::{VfsDiscoveryRegistry, builtin_vfs_registry};
pub use workflow_bindings::{WorkflowContextBindingsContributor, contribute_workflow_binding};
pub use workspace_sources::{
    contribute_workspace_static_sources, resolve_workspace_declared_sources,
};
