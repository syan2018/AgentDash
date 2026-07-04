pub(crate) mod dispatch;
pub mod gate_control;
pub mod notifications;
pub mod payload_types;
pub mod runtime_tool_provider;
pub(crate) mod skill_projection;
pub(crate) mod tool_context;
pub mod tools;

pub use gate_control::{
    CompanionGateControlDeps, CompanionGateControlRepos, CompanionGateControlService,
    CompanionGateRespondResult, CompanionHumanResponseMailboxDelivery,
    CompanionHumanResponseMailboxDeliveryCommand, CompanionParentMailboxDelivery,
    CompanionParentMailboxDeliveryCommand, CompanionParentMailboxDeliveryResult,
    CompanionParentRequestMailboxDeliveryCommand, CompanionParentResponseMailboxDeliveryCommand,
    CompleteCompanionChildResultCommand, OpenCompanionParentRequestCommand,
    ResolveCompanionParentRequestCommand, RespondCompanionGateCommand,
    SessionEventingCompanionGateDelivery,
};
pub use notifications::{
    build_companion_event_notification, build_companion_human_response_notification,
};
pub use payload_types::PayloadTypeRegistry;
pub use runtime_tool_provider::CollaborationRuntimeToolProvider;
pub use tools::{
    AgentRunCompanionMailboxDelivery, CompanionRequestTool, CompanionRespondTool,
    build_hook_action_resolved_notification,
};
