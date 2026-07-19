mod applied_resource_surface;
pub mod frame;
mod product_command_facade;
mod product_mailbox_facade;
mod product_launch;
mod product_projection_gateway;
pub mod product_protocol;
mod product_runtime_provisioning;
pub mod project_agent_context;
pub mod runtime_capability;
pub mod runtime_capability_projection;
pub mod runtime_target;
pub mod terminal_projection_protocol;

pub use applied_resource_surface::*;
pub use frame::{
    AgentFrameSurfaceExt, PromptLaunchPath, RuntimeTraceLaunchState,
    SessionRepositoryRehydrateMode, TerminalHookEffectBinding, resolve_prompt_launch_path,
};
pub use project_agent_context::{
    ResolvedProjectAgentContext, build_project_agent_context, merge_executor_config_fields,
    resolve_project_workspace,
};
pub use product_command_facade::*;
pub use product_mailbox_facade::*;
pub use product_launch::*;
pub use product_projection_gateway::*;
pub use product_protocol::*;
pub use product_runtime_provisioning::*;
pub use terminal_projection_protocol::*;
