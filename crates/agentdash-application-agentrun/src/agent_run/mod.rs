mod applied_resource_surface;
mod conversation_snapshot;
mod display_title;
mod execution_state;
pub mod frame;
pub(crate) mod lifecycle_read_model_facade;
mod product_command_facade;
mod product_input_delivery;
mod product_launch;
mod product_mailbox_facade;
mod product_projection_gateway;
pub mod product_protocol;
mod product_runtime_change_observer;
mod product_runtime_provisioning;
pub mod project_agent_context;
pub mod runtime_capability;
pub mod runtime_capability_projection;
pub mod runtime_target;
pub mod terminal_projection_protocol;
pub mod terminal_registry;
pub mod workspace;

pub use applied_resource_surface::*;
pub use conversation_snapshot::{
    AgentConversationFrameRefModel, AgentConversationIdentityModel,
    AgentConversationLifecycleContextModel, AgentConversationSnapshotInput,
    AgentConversationSnapshotModel, AgentConversationSnapshotResolver,
    AgentRunCommandPreconditionModel, AgentRunOwnershipModel, ConversationCommandAvailability,
    ConversationCommandAvailabilityInput, ConversationCommandAvailabilityResolver,
    ConversationCommandKindModel, ConversationCommandModel, ConversationCommandPlacementModel,
    ConversationCommandSetModel, ConversationCommandStaleGuardModel, ConversationDiagnosticModel,
    ConversationEffectiveExecutorConfigModel, ConversationExecutionModel,
    ConversationExecutionStatusModel, ConversationKeyboardMapModel,
    ConversationMailboxSnapshotModel, ConversationModelConfigInput, ConversationModelConfigModel,
    ConversationModelConfigResolution, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, ConversationModelConfigStatusModel,
    ConversationWaitingItemModel, ValidationSeverityModel, conversation_command_id_for,
    conversation_execution_state_code, conversation_snapshot_id,
};
pub use display_title::{AgentRunDisplayTitle, resolve_agent_run_display_title};
pub use execution_state::AgentRunExecutionState;
pub use frame::{
    AgentFrameSurfaceExt, PromptLaunchPath, RuntimeTraceLaunchState,
    SessionRepositoryRehydrateMode, TerminalHookEffectBinding, resolve_prompt_launch_path,
};
pub use product_command_facade::*;
pub use product_input_delivery::*;
pub use product_launch::*;
pub use product_mailbox_facade::*;
pub use product_projection_gateway::*;
pub use product_protocol::*;
pub use product_runtime_change_observer::*;
pub use product_runtime_provisioning::*;
pub use project_agent_context::{
    ResolvedProjectAgentContext, build_project_agent_context, merge_executor_config_fields,
    resolve_project_workspace,
};
pub use terminal_projection_protocol::*;
pub use terminal_registry::*;
