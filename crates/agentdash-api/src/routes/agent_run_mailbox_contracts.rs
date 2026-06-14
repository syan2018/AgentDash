use agentdash_application::session::{
    AgentRunMailboxCommandOutcome as AppMailboxCommandOutcome, AgentRunMailboxCommandResult,
};
use agentdash_application::workflow::{
    AgentRunCommandReceiptView, agent_run_workspace as app_workspace,
};
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandReceipt, AgentRunMessageAcceptedRefs, AgentRunMessageCommandOutcome,
    AgentRunMessageCommandResponse, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
    MailboxMessageOrigin, MailboxMessageSource, MailboxMessageStatus, MailboxMessageView,
    MailboxStateView, RuntimeSessionCommandStateDto, SteeringStopEffect,
};
use agentdash_contracts::workflow::{AgentRunRefDto, LifecycleRunRefDto, RuntimeSessionRefDto};

pub(crate) fn agent_run_message_command_response(
    result: AgentRunMailboxCommandResult,
) -> AgentRunMessageCommandResponse {
    AgentRunMessageCommandResponse {
        command_receipt: command_receipt_view(result.command_receipt),
        outcome: mailbox_command_outcome_view(result.outcome),
        mailbox_message: result.mailbox_message.map(mailbox_message_view),
        accepted_refs: result.accepted_refs.map(agent_run_message_accepted_refs),
        runtime_state: result.runtime_state.map(|state| {
            runtime_command_state_dto(
                app_workspace::AgentRunWorkspaceProjection::runtime_command_state(&state),
            )
        }),
    }
}

pub(crate) fn command_receipt_view(receipt: AgentRunCommandReceiptView) -> AgentRunCommandReceipt {
    AgentRunCommandReceipt {
        client_command_id: receipt.client_command_id,
        status: receipt.status,
        duplicate: receipt.duplicate,
        message: receipt.message,
    }
}

pub(crate) fn agent_run_message_accepted_refs(
    refs: agentdash_domain::workflow::AgentRunAcceptedRefs,
) -> AgentRunMessageAcceptedRefs {
    AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: refs.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: refs.run_id.to_string(),
            agent_id: refs.agent_id.to_string(),
        },
        frame_ref: refs
            .frame_id
            .map(|frame_id| agentdash_contracts::workflow::AgentFrameRefDto {
                agent_id: refs.agent_id.to_string(),
                frame_id: frame_id.to_string(),
                revision: refs.frame_revision,
            }),
        runtime_session_ref: refs
            .runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        agent_run_turn_id: refs.agent_run_turn_id,
        protocol_turn_id: refs.protocol_turn_id,
    }
}

pub(crate) fn mailbox_command_outcome_view(
    outcome: AppMailboxCommandOutcome,
) -> AgentRunMessageCommandOutcome {
    match outcome {
        AppMailboxCommandOutcome::Launched => AgentRunMessageCommandOutcome::Launched,
        AppMailboxCommandOutcome::Queued => AgentRunMessageCommandOutcome::Queued,
        AppMailboxCommandOutcome::Steered => AgentRunMessageCommandOutcome::Steered,
        AppMailboxCommandOutcome::Deleted => AgentRunMessageCommandOutcome::Deleted,
        AppMailboxCommandOutcome::Resumed => AgentRunMessageCommandOutcome::Resumed,
        AppMailboxCommandOutcome::Blocked => AgentRunMessageCommandOutcome::Blocked,
        AppMailboxCommandOutcome::Failed => AgentRunMessageCommandOutcome::Failed,
    }
}

pub(crate) fn runtime_command_state_dto(
    state: app_workspace::AgentRunWorkspaceRuntimeCommandStateModel,
) -> RuntimeSessionCommandStateDto {
    RuntimeSessionCommandStateDto {
        status: state.status.as_str().to_string(),
        turn_id: state.turn_id,
        message: state.message,
    }
}

pub(crate) fn mailbox_message_view(
    message: agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
) -> MailboxMessageView {
    let can_delete = matches!(
        message.status,
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Accepted
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Queued
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::ReadyToConsume
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Paused
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Blocked
    );
    let can_promote = can_delete
        && message.delivery
            == agentdash_domain::agent_run_mailbox::MailboxDelivery::LaunchOrContinueTurn
        && message.last_error.as_deref()
            != Some(agentdash_domain::agent_run_mailbox::MAILBOX_DELIVERY_RESULT_UNKNOWN);
    let can_reorder = can_delete
        && message.origin == agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User
        && message.delivery
            == agentdash_domain::agent_run_mailbox::MailboxDelivery::LaunchOrContinueTurn;
    let can_recall = can_delete
        && message.origin == agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User
        && message.payload_json.is_some();
    MailboxMessageView {
        id: message.id.to_string(),
        origin: mailbox_origin_view(message.origin),
        source: mailbox_source_view(message.source),
        delivery: mailbox_delivery_view(message.delivery.clone()),
        barrier: mailbox_barrier_view(message.barrier),
        drain_mode: mailbox_drain_mode_view(message.drain_mode),
        status: mailbox_status_view(message.status),
        preview: message.preview.clone(),
        has_images: message.has_images,
        attempt_count: message.attempt_count,
        accepted_refs: mailbox_message_accepted_refs(&message),
        last_error: message.last_error.clone(),
        created_at: message.created_at.to_rfc3339(),
        updated_at: message.updated_at.to_rfc3339(),
        can_promote,
        can_delete,
        can_reorder,
        can_recall,
    }
}

pub(crate) fn mailbox_message_visible(
    message: &agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
) -> bool {
    !matches!(
        message.status,
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Dispatched
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Steered
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Deleted
    )
}

pub(crate) fn mailbox_state_view(
    state: Option<&agentdash_domain::agent_run_mailbox::AgentRunMailboxState>,
    can_resume: bool,
    visible_message_count: usize,
    hide_system_steer_messages: bool,
) -> MailboxStateView {
    let paused = state.is_some_and(|state| state.paused) && visible_message_count > 0;
    MailboxStateView {
        paused,
        pause_reason: state.and_then(|state| state.pause_reason.clone()),
        message: state.and_then(|state| state.pause_message.clone()),
        can_resume: can_resume && paused,
        hide_system_steer_messages,
    }
}

fn mailbox_status_view(
    status: agentdash_domain::agent_run_mailbox::MailboxMessageStatus,
) -> MailboxMessageStatus {
    match status {
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Accepted => {
            MailboxMessageStatus::Accepted
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Queued => {
            MailboxMessageStatus::Queued
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::ReadyToConsume => {
            MailboxMessageStatus::ReadyToConsume
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Consuming => {
            MailboxMessageStatus::Consuming
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Dispatched => {
            MailboxMessageStatus::Dispatched
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Steered => {
            MailboxMessageStatus::Steered
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Paused => {
            MailboxMessageStatus::Paused
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Blocked => {
            MailboxMessageStatus::Blocked
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Failed => {
            MailboxMessageStatus::Failed
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Deleted => {
            MailboxMessageStatus::Deleted
        }
    }
}

fn mailbox_origin_view(
    origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin,
) -> MailboxMessageOrigin {
    match origin {
        agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User => {
            MailboxMessageOrigin::User
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::System => {
            MailboxMessageOrigin::System
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::Hook => {
            MailboxMessageOrigin::Hook
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::Companion => {
            MailboxMessageOrigin::Companion
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::Workflow => {
            MailboxMessageOrigin::Workflow
        }
    }
}

fn mailbox_source_view(
    source: agentdash_domain::agent_run_mailbox::MailboxMessageSource,
) -> MailboxMessageSource {
    match source {
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::Composer => {
            MailboxMessageSource::Composer
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::DraftStart => {
            MailboxMessageSource::DraftStart
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::HookAfterTurn => {
            MailboxMessageSource::HookAfterTurn
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::HookBeforeStop => {
            MailboxMessageSource::HookBeforeStop
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::HookAutoResume => {
            MailboxMessageSource::HookAutoResume
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::CompanionParentResume => {
            MailboxMessageSource::CompanionParentResume
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::WorkflowOrchestrator => {
            MailboxMessageSource::WorkflowOrchestrator
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::RoutineExecutor => {
            MailboxMessageSource::RoutineExecutor
        }
        agentdash_domain::agent_run_mailbox::MailboxMessageSource::LocalRelayPrompt => {
            MailboxMessageSource::LocalRelayPrompt
        }
    }
}

fn mailbox_delivery_view(
    delivery: agentdash_domain::agent_run_mailbox::MailboxDelivery,
) -> MailboxDelivery {
    match delivery {
        agentdash_domain::agent_run_mailbox::MailboxDelivery::LaunchOrContinueTurn => {
            MailboxDelivery::LaunchOrContinueTurn
        }
        agentdash_domain::agent_run_mailbox::MailboxDelivery::SteerActiveTurn { stop_effect } => {
            MailboxDelivery::SteerActiveTurn {
                stop_effect: match stop_effect {
                    agentdash_domain::agent_run_mailbox::SteeringStopEffect::None => {
                        SteeringStopEffect::None
                    }
                    agentdash_domain::agent_run_mailbox::SteeringStopEffect::ContinueOnStop => {
                        SteeringStopEffect::ContinueOnStop
                    }
                },
            }
        }
        agentdash_domain::agent_run_mailbox::MailboxDelivery::ResumeLaunchSource {
            launch_source,
        } => MailboxDelivery::ResumeLaunchSource { launch_source },
    }
}

fn mailbox_barrier_view(
    barrier: agentdash_domain::agent_run_mailbox::ConsumptionBarrier,
) -> ConsumptionBarrier {
    match barrier {
        agentdash_domain::agent_run_mailbox::ConsumptionBarrier::ImmediateIfIdle => {
            ConsumptionBarrier::ImmediateIfIdle
        }
        agentdash_domain::agent_run_mailbox::ConsumptionBarrier::AgentLoopTurnBoundary => {
            ConsumptionBarrier::AgentLoopTurnBoundary
        }
        agentdash_domain::agent_run_mailbox::ConsumptionBarrier::AgentRunTurnBoundary => {
            ConsumptionBarrier::AgentRunTurnBoundary
        }
        agentdash_domain::agent_run_mailbox::ConsumptionBarrier::ManualResume => {
            ConsumptionBarrier::ManualResume
        }
    }
}

fn mailbox_drain_mode_view(
    drain_mode: agentdash_domain::agent_run_mailbox::MailboxDrainMode,
) -> MailboxDrainMode {
    match drain_mode {
        agentdash_domain::agent_run_mailbox::MailboxDrainMode::One => MailboxDrainMode::One,
        agentdash_domain::agent_run_mailbox::MailboxDrainMode::All => MailboxDrainMode::All,
    }
}

fn mailbox_message_accepted_refs(
    message: &agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
) -> Option<AgentRunMessageAcceptedRefs> {
    if message.accepted_agent_run_turn_id.is_none() && message.accepted_protocol_turn_id.is_none() {
        return None;
    }
    Some(AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: message.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: message.run_id.to_string(),
            agent_id: message.agent_id.to_string(),
        },
        frame_ref: None,
        runtime_session_ref: Some(RuntimeSessionRefDto {
            runtime_session_id: message.runtime_session_id.clone(),
        }),
        agent_run_turn_id: message.accepted_agent_run_turn_id.clone(),
        protocol_turn_id: message.accepted_protocol_turn_id.clone(),
    })
}
