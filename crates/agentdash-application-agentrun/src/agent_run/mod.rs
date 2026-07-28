mod applied_resource_surface;
mod conversation_snapshot;
mod display_title;
pub mod frame;
pub(crate) mod lifecycle_read_model_facade;
mod product_command_facade;
mod product_delete;
mod product_fork;
mod product_input_delivery;
mod product_launch;
mod product_projection_gateway;
pub mod product_protocol;
mod product_runtime_provisioning;
pub mod project_agent_context;
pub mod runtime_capability;
pub mod runtime_capability_projection;
pub mod runtime_target;
mod session_context_projection;
pub mod terminal_projection_protocol;
pub mod terminal_registry;
pub mod workspace;

pub use applied_resource_surface::*;
pub use conversation_snapshot::{
    AgentConversationFrameRefModel, AgentConversationIdentityModel,
    AgentConversationLifecycleContextModel, AgentConversationSnapshotInput,
    AgentConversationSnapshotModel, AgentConversationSnapshotResolver, AgentRunOwnershipModel,
    ConversationCommandKindModel, ConversationCommandModel, ConversationCommandPlacementModel,
    ConversationCommandSetModel, ConversationDiagnosticModel,
    ConversationEffectiveExecutorConfigModel, ConversationKeyboardMapModel,
    ConversationModelConfigInput, ConversationModelConfigModel, ConversationModelConfigResolution,
    ConversationModelConfigResolver, ConversationModelConfigSourceModel,
    ConversationModelConfigStatusModel, ConversationWaitingItemModel, ValidationSeverityModel,
    conversation_command_id_for,
};
pub use display_title::{AgentRunDisplayTitle, resolve_agent_run_display_title};
pub use frame::{
    AgentFrameSurfaceExt, PromptLaunchPath, RuntimeTraceLaunchState,
    SessionRepositoryRehydrateMode, TerminalHookEffectBinding, resolve_prompt_launch_path,
};
pub use lifecycle_read_model_facade::LifecycleSubjectAssociationView;
pub use product_command_facade::*;
pub use product_delete::*;
pub use product_fork::*;
pub use product_input_delivery::*;
pub use product_launch::*;
pub use product_projection_gateway::*;
pub use product_protocol::*;
pub use product_runtime_provisioning::*;
pub use project_agent_context::{
    ResolvedProjectAgentContext, build_project_agent_context, merge_executor_config_fields,
    resolve_project_workspace,
};
pub use session_context_projection::project_agent_runtime_context;
pub use terminal_projection_protocol::*;
pub use terminal_registry::*;
