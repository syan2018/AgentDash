use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use chrono::{Duration, Utc};
use uuid::Uuid;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, ControlPlaneProjection, ControlPlaneProjectionChangeReason,
    ControlPlaneProjectionChanged, PlatformEvent, SourceInfo, TraceInfo, UserInputBlock,
    UserInputSubmissionKind, user_input_blocks_to_content_parts,
};
use agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress;
use agentdash_application_ports::lifecycle_surface_projection::{
    MessageStreamProjectionRef, MessageStreamTraceKind,
};
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxMessage, AgentRunMailboxRepository,
    AgentRunMailboxState, ConsumptionBarrier, MAILBOX_DELIVERY_RESULT_UNKNOWN, MailboxDelivery,
    MailboxDrainMode, MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity,
    NewAgentRunMailboxMessage, SteeringStopEffect,
};
use agentdash_domain::backend::ProjectBackendAccessRepository;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentRunAcceptedRefs, AgentRunCommandKind,
    AgentRunCommandReceiptRepository, AgentRunDeliveryBindingRepository, LifecycleAgent,
    LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::platform::auth::AuthIdentity;
use agentdash_spi::{AgentConfig, AgentMessage, ContentPart};

use crate::agent_run::runtime_session_boundary::{
    SessionControlService, SessionCoreService, SessionEventingService, SessionExecutionState,
    SessionLaunchService, SessionTurnSteerCommand,
};
use crate::agent_run::{
    AgentRunCommandReceiptView, AgentRunMessageDelivery, AgentRunMessageDeliveryPort,
    DeliveryRuntimeSelectionError, DeliveryRuntimeSelectionRepositories,
    DeliveryRuntimeSelectionService, SessionTurnMessageDeliveryPort,
    command_receipt::{
        claim_agent_run_command_receipt, digest_command_request, mark_command_terminal_failed,
    },
};
use crate::error::WorkflowApplicationError;

const CLAIM_LEASE_SECONDS: i64 = 300;
const AGENT_LOOP_DRAIN_LIMIT: i64 = 100;
const PROMOTE_PRIORITY: i32 = 10_000;

mod commands;
mod controls;
mod delivery;
mod payload;
mod policy;
mod receipts;
mod scheduler;
mod target;

#[cfg(test)]
mod tests;

pub use commands::{
    AgentRunMailboxCommandOutcome, AgentRunMailboxCommandResult, AgentRunMailboxCommandTarget,
    AgentRunMailboxControlCommand, AgentRunMailboxControlTargetCommand,
    AgentRunMailboxIntakeCommand, AgentRunMailboxIntakeTargetCommand,
    AgentRunMailboxScheduleOutcome, AgentRunMailboxScheduleTrigger,
    AgentRunMailboxUserMessageCommand, AgentRunMailboxUserMessageTargetCommand,
    mailbox_source_identity_dedup_key,
};
pub(crate) use receipts::{outcome_from_message, outcome_from_result_json};

pub struct AgentRunMailboxService<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    project_agent_repo: &'a dyn ProjectAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
    project_backend_access_repo: &'a dyn ProjectBackendAccessRepository,
    command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
    mailbox_repo: &'a dyn AgentRunMailboxRepository,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    session_eventing: SessionEventingService,
    session_launch: SessionLaunchService,
}

impl<'a> AgentRunMailboxService<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        project_agent_repo: &'a dyn ProjectAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
        delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
        project_backend_access_repo: &'a dyn ProjectBackendAccessRepository,
        command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
        mailbox_repo: &'a dyn AgentRunMailboxRepository,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        session_eventing: SessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_agent_repo,
            project_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            delivery_binding_repo,
            project_backend_access_repo,
            command_receipt_repo,
            mailbox_repo,
            session_core,
            session_control,
            session_eventing,
            session_launch,
        }
    }
}
