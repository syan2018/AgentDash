use agentdash_contracts::workflow::{
    AgentRunMessageAcceptedRefs, AgentRunRefDto, ConsumptionBarrier, LifecycleRunRefDto,
    MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin, MailboxMessageSource,
    MailboxMessageStatus, MailboxMessageView, MailboxStateView, RuntimeSessionRefDto,
};

pub(crate) fn mailbox_message_view(
    message: agentdash_domain::workflow::AgentRunMailboxMessage,
) -> MailboxMessageView {
    let can_delete = matches!(
        message.status,
        agentdash_domain::workflow::MailboxMessageStatus::Accepted
            | agentdash_domain::workflow::MailboxMessageStatus::Queued
            | agentdash_domain::workflow::MailboxMessageStatus::ReadyToConsume
            | agentdash_domain::workflow::MailboxMessageStatus::Paused
            | agentdash_domain::workflow::MailboxMessageStatus::Blocked
    );
    let can_promote = can_delete
        && message.delivery == agentdash_domain::workflow::MailboxDelivery::LaunchOrContinueTurn
        && message.last_error.as_deref()
            != Some(agentdash_domain::workflow::MAILBOX_DELIVERY_RESULT_UNKNOWN);
    let can_reorder = can_delete
        && message.origin == agentdash_domain::workflow::MailboxMessageOrigin::User
        && message.delivery == agentdash_domain::workflow::MailboxDelivery::LaunchOrContinueTurn;
    let can_recall = can_delete
        && message.origin == agentdash_domain::workflow::MailboxMessageOrigin::User
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
    message: &agentdash_domain::workflow::AgentRunMailboxMessage,
) -> bool {
    !matches!(
        message.status,
        agentdash_domain::workflow::MailboxMessageStatus::Dispatched
            | agentdash_domain::workflow::MailboxMessageStatus::Steered
            | agentdash_domain::workflow::MailboxMessageStatus::Deleted
    )
}

pub(crate) fn mailbox_state_view(
    state: Option<&agentdash_domain::workflow::AgentRunMailboxState>,
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
    status: agentdash_domain::workflow::MailboxMessageStatus,
) -> MailboxMessageStatus {
    match status {
        agentdash_domain::workflow::MailboxMessageStatus::Accepted => {
            MailboxMessageStatus::Accepted
        }
        agentdash_domain::workflow::MailboxMessageStatus::Queued => MailboxMessageStatus::Queued,
        agentdash_domain::workflow::MailboxMessageStatus::ReadyToConsume => {
            MailboxMessageStatus::ReadyToConsume
        }
        agentdash_domain::workflow::MailboxMessageStatus::Consuming => {
            MailboxMessageStatus::Consuming
        }
        agentdash_domain::workflow::MailboxMessageStatus::Dispatched => {
            MailboxMessageStatus::Dispatched
        }
        agentdash_domain::workflow::MailboxMessageStatus::Steered => MailboxMessageStatus::Steered,
        agentdash_domain::workflow::MailboxMessageStatus::Paused => MailboxMessageStatus::Paused,
        agentdash_domain::workflow::MailboxMessageStatus::Blocked => MailboxMessageStatus::Blocked,
        agentdash_domain::workflow::MailboxMessageStatus::Failed => MailboxMessageStatus::Failed,
        agentdash_domain::workflow::MailboxMessageStatus::Deleted => MailboxMessageStatus::Deleted,
    }
}

fn mailbox_origin_view(
    origin: agentdash_domain::workflow::MailboxMessageOrigin,
) -> MailboxMessageOrigin {
    match origin {
        agentdash_domain::workflow::MailboxMessageOrigin::User => MailboxMessageOrigin::User,
        agentdash_domain::workflow::MailboxMessageOrigin::System => MailboxMessageOrigin::System,
        agentdash_domain::workflow::MailboxMessageOrigin::Hook => MailboxMessageOrigin::Hook,
        agentdash_domain::workflow::MailboxMessageOrigin::Companion => {
            MailboxMessageOrigin::Companion
        }
        agentdash_domain::workflow::MailboxMessageOrigin::Workflow => {
            MailboxMessageOrigin::Workflow
        }
    }
}

fn mailbox_source_view(
    source: agentdash_domain::workflow::MailboxMessageSource,
) -> MailboxMessageSource {
    match source {
        agentdash_domain::workflow::MailboxMessageSource::Composer => {
            MailboxMessageSource::Composer
        }
        agentdash_domain::workflow::MailboxMessageSource::DraftStart => {
            MailboxMessageSource::DraftStart
        }
        agentdash_domain::workflow::MailboxMessageSource::HookAfterTurn => {
            MailboxMessageSource::HookAfterTurn
        }
        agentdash_domain::workflow::MailboxMessageSource::HookBeforeStop => {
            MailboxMessageSource::HookBeforeStop
        }
        agentdash_domain::workflow::MailboxMessageSource::HookAutoResume => {
            MailboxMessageSource::HookAutoResume
        }
        agentdash_domain::workflow::MailboxMessageSource::CompanionParentResume => {
            MailboxMessageSource::CompanionParentResume
        }
        agentdash_domain::workflow::MailboxMessageSource::WorkflowOrchestrator => {
            MailboxMessageSource::WorkflowOrchestrator
        }
        agentdash_domain::workflow::MailboxMessageSource::RoutineExecutor => {
            MailboxMessageSource::RoutineExecutor
        }
        agentdash_domain::workflow::MailboxMessageSource::LocalRelayPrompt => {
            MailboxMessageSource::LocalRelayPrompt
        }
    }
}

fn mailbox_delivery_view(delivery: agentdash_domain::workflow::MailboxDelivery) -> MailboxDelivery {
    match delivery {
        agentdash_domain::workflow::MailboxDelivery::LaunchOrContinueTurn => {
            MailboxDelivery::LaunchOrContinueTurn
        }
        agentdash_domain::workflow::MailboxDelivery::SteerActiveTurn { stop_effect } => {
            MailboxDelivery::SteerActiveTurn {
                stop_effect: match stop_effect {
                    agentdash_domain::workflow::SteeringStopEffect::None => {
                        agentdash_contracts::workflow::SteeringStopEffect::None
                    }
                    agentdash_domain::workflow::SteeringStopEffect::ContinueOnStop => {
                        agentdash_contracts::workflow::SteeringStopEffect::ContinueOnStop
                    }
                },
            }
        }
        agentdash_domain::workflow::MailboxDelivery::ResumeLaunchSource { launch_source } => {
            MailboxDelivery::ResumeLaunchSource { launch_source }
        }
    }
}

fn mailbox_barrier_view(
    barrier: agentdash_domain::workflow::ConsumptionBarrier,
) -> ConsumptionBarrier {
    match barrier {
        agentdash_domain::workflow::ConsumptionBarrier::ImmediateIfIdle => {
            ConsumptionBarrier::ImmediateIfIdle
        }
        agentdash_domain::workflow::ConsumptionBarrier::AgentLoopTurnBoundary => {
            ConsumptionBarrier::AgentLoopTurnBoundary
        }
        agentdash_domain::workflow::ConsumptionBarrier::AgentRunTurnBoundary => {
            ConsumptionBarrier::AgentRunTurnBoundary
        }
        agentdash_domain::workflow::ConsumptionBarrier::ManualResume => {
            ConsumptionBarrier::ManualResume
        }
    }
}

fn mailbox_drain_mode_view(
    drain_mode: agentdash_domain::workflow::MailboxDrainMode,
) -> MailboxDrainMode {
    match drain_mode {
        agentdash_domain::workflow::MailboxDrainMode::One => MailboxDrainMode::One,
        agentdash_domain::workflow::MailboxDrainMode::All => MailboxDrainMode::All,
    }
}

fn mailbox_message_accepted_refs(
    message: &agentdash_domain::workflow::AgentRunMailboxMessage,
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
