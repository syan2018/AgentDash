pub mod address_space_discovery;
mod builder;
mod builtins;
pub mod context_composer;
mod contributor;
pub mod source_resolver;
mod workflow_bindings;
pub mod workspace_sources;

pub use address_space_discovery::{AddressSpaceDiscoveryRegistry, builtin_address_space_registry};
pub use builder::{build_declared_source_warning_fragment, build_task_agent_context};
pub use builtins::{McpContextContributor, StaticFragmentsContributor};
pub use builtins::{build_owner_context_resource_block, build_owner_prompt_blocks};
pub(crate) use builtins::{clean_text, trim_or_dash, workspace_context_fragment};
pub use context_composer::ContextComposer;
pub use contributor::{
    BuiltTaskAgentContext, ContextContributor, ContextContributorRegistry, Contribution,
    ContributorInput, TaskAgentBuildInput, TaskExecutionPhase,
};
pub use source_resolver::{
    SourceResolverRegistry, resolve_declared_sources, resolve_declared_sources_with_registry,
};
pub use workflow_bindings::WorkflowContextBindingsContributor;
pub use workspace_sources::resolve_workspace_declared_sources;
