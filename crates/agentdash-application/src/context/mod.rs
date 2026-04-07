mod builder;
mod builtins;
mod contributor;
pub mod workspace_sources;

pub use builder::{build_declared_source_warning_fragment, build_task_agent_context};
pub use builtins::{McpContextContributor, StaticFragmentsContributor};
pub(crate) use builtins::{clean_text, trim_or_dash};
pub use contributor::{
    BuiltTaskAgentContext, ContextContributor, ContextContributorRegistry, Contribution,
    ContributorInput, TaskAgentBuildInput, TaskExecutionPhase,
};
pub use workspace_sources::resolve_workspace_declared_sources;
