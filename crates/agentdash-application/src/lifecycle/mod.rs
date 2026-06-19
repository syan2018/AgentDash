pub(crate) mod activity_activation;
mod completion;
pub mod dispatch_service;
mod error;
pub mod execution_log;
pub mod gate_service;
pub mod orchestrator;
pub mod projection;
pub(crate) mod run;
pub mod run_view_builder;
mod session_association;
mod session_run_context_resolver;
mod subject_context_assignment;
mod subject_execution_control;
pub mod surface;
pub mod tools;

#[cfg(test)]
pub(crate) use activity_activation::KickoffPromptFragment;
pub(crate) use activity_activation::{
    ActivityActivation, ActivityActivationInput, activate_activity_with_platform,
};
pub use completion::{session_terminal_state_tag, session_terminal_summary};
pub use dispatch_service::{
    LifecycleDispatchService, RuntimeSessionCreationRequest, RuntimeSessionCreator,
    SessionPersistenceRuntimeSessionCreator, WorkflowAgentNodeFrameComposer,
    WorkflowAgentNodeMaterializationRequest, WorkflowAgentNodeMaterializationResult,
};
pub use error::WorkflowApplicationError;
pub use execution_log::{
    RuntimeNodeArtifactScope, RuntimeNodePortArtifactRef, load_scoped_port_output_map,
    materialize_activity_summary,
};
pub use gate_service::LifecycleGateService;
pub use orchestrator::{
    AdvanceCurrentActivityInput, AdvanceCurrentNodeResult, AdvanceCurrentNodeStatus,
    LifecycleNodeAdvanceOutcome, LifecycleOrchestrator,
};
#[cfg(test)]
pub(crate) use projection::activity_projection;
pub use projection::{
    ActiveWorkflowProjection, resolve_active_workflow_projection_for_target,
    resolve_active_workflow_projection_from_message_stream_trace,
};
pub use run::select_active_run;
pub use session_association::{
    LIFECYCLE_ACTIVITY_LABEL_PREFIX, LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_activity_label,
    build_lifecycle_node_label, lifecycle_activity_parts_from_label,
    resolve_activity_runtime_association_from_message_stream_trace,
    resolve_current_frame_from_delivery_trace_ref,
};
pub use session_run_context_resolver::{SubjectRunContextResolver, build_subject_run_context};
pub use subject_context_assignment::{
    SubjectContextAssignment, SubjectContextAssignmentRequest, SubjectContextAssignmentResolver,
    SubjectWorkspacePolicy,
};
pub use subject_execution_control::{
    CancelSubjectExecutionCommand, RuntimeCancelDeliveryCommand, SubjectExecutionCancelResult,
    SubjectExecutionControlService,
};
pub use surface::mount::{
    LifecycleMountSurface, append_active_workflow_lifecycle_mount,
    lifecycle_mount_surface_for_active_workflow, project_active_workflow_lifecycle_vfs,
    writable_port_keys_for_active_workflow,
};
pub use surface::surface_projector::{
    AgentRunLifecycleProjectionSet, AgentRunLifecycleSurface, AgentRunLifecycleSurfaceInput,
    AgentRunLifecycleSurfaceMode, AgentRunLifecycleSurfaceProjector, AgentRunRuntimeAddress,
    BuiltinLifecycleSkill, BuiltinLifecycleSkillPolicy, MessageStreamProjectionFacts,
    MessageStreamProjectionRef, MessageStreamTraceKind, OrchestrationNodeProjectionFacts,
    OrchestrationNodeProjectionInput,
};
