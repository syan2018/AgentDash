use agentdash_contracts::agent_run_mailbox::{
    AgentRunMessageAcceptedRefs, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
    MailboxMessageOrigin, MailboxMessageStatus, MailboxMessageView, MailboxSourceIdentity,
    MailboxStateView, SteeringStopEffect,
};
use agentdash_contracts::workflow::{AgentRunRefDto, LifecycleRunRefDto};
use agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage;

pub(crate) fn mailbox_message_view(message: AgentRunMailboxMessage) -> MailboxMessageView {
    let accepted_refs = mailbox_message_accepted_refs(&message);
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
        delivery: mailbox_delivery_view(message.delivery),
        barrier: mailbox_barrier_view(message.barrier),
        drain_mode: mailbox_drain_mode_view(message.drain_mode),
        status: mailbox_status_view(message.status),
        preview: message.preview,
        has_images: message.has_images,
        attempt_count: message.attempt_count,
        accepted_refs,
        last_error: message.last_error,
        created_at: message.created_at.to_rfc3339(),
        updated_at: message.updated_at.to_rfc3339(),
        can_promote,
        can_delete,
        can_reorder,
        can_recall,
    }
}

pub(crate) fn mailbox_state_view(
    state: Option<&agentdash_domain::agent_run_mailbox::AgentRunMailboxState>,
    visible_message_count: usize,
) -> MailboxStateView {
    let paused = state.is_some_and(|state| state.paused) && visible_message_count > 0;
    MailboxStateView {
        paused,
        pause_reason: state.and_then(|state| state.pause_reason.clone()),
        message: state.and_then(|state| state.pause_message.clone()),
        can_resume: paused,
        hide_system_steer_messages: false,
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
    source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity,
) -> MailboxSourceIdentity {
    MailboxSourceIdentity {
        namespace: source.namespace,
        kind: source.kind,
        source_ref: source.source_ref,
        correlation_ref: source.correlation_ref,
        actor: source.actor,
        route: source.route,
        display_label_key: source.display_label_key,
        metadata: source.metadata,
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
    mode: agentdash_domain::agent_run_mailbox::MailboxDrainMode,
) -> MailboxDrainMode {
    match mode {
        agentdash_domain::agent_run_mailbox::MailboxDrainMode::One => MailboxDrainMode::One,
        agentdash_domain::agent_run_mailbox::MailboxDrainMode::All => MailboxDrainMode::All,
    }
}

fn mailbox_message_accepted_refs(
    message: &AgentRunMailboxMessage,
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
        agent_run_turn_id: message.accepted_agent_run_turn_id.clone(),
        protocol_turn_id: message.accepted_protocol_turn_id.clone(),
    })
}
