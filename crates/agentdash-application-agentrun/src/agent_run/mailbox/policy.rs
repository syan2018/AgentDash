use super::*;

#[derive(Debug, Clone)]
pub(super) struct UserMessagePolicy {
    pub(super) delivery: MailboxDelivery,
    pub(super) barrier: ConsumptionBarrier,
    pub(super) drain_mode: MailboxDrainMode,
    pub(super) queued_agent_run_turn_id: Option<String>,
    pub(super) expected_active_agent_run_turn_id: Option<String>,
}

pub(super) fn user_message_policy(
    execution_state: &SessionExecutionState,
    supports_steering: bool,
    delivery_intent: Option<&str>,
) -> UserMessagePolicy {
    let user_wants_steer = delivery_intent == Some("steer");
    match execution_state {
        SessionExecutionState::Running {
            turn_id: Some(active_turn_id),
        } if supports_steering && user_wants_steer => UserMessagePolicy {
            delivery: MailboxDelivery::SteerActiveTurn {
                stop_effect: SteeringStopEffect::None,
            },
            barrier: ConsumptionBarrier::AgentLoopTurnBoundary,
            drain_mode: MailboxDrainMode::All,
            queued_agent_run_turn_id: Some(active_turn_id.clone()),
            expected_active_agent_run_turn_id: Some(active_turn_id.clone()),
        },
        SessionExecutionState::Running { turn_id }
        | SessionExecutionState::Cancelling { turn_id } => UserMessagePolicy {
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier: ConsumptionBarrier::AgentRunTurnBoundary,
            drain_mode: MailboxDrainMode::One,
            queued_agent_run_turn_id: turn_id.clone(),
            expected_active_agent_run_turn_id: turn_id.clone(),
        },
        SessionExecutionState::Idle
        | SessionExecutionState::Completed { .. }
        | SessionExecutionState::Failed { .. }
        | SessionExecutionState::Interrupted { .. }
        | SessionExecutionState::Lost { .. } => UserMessagePolicy {
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier: ConsumptionBarrier::ImmediateIfIdle,
            drain_mode: MailboxDrainMode::One,
            queued_agent_run_turn_id: None,
            expected_active_agent_run_turn_id: None,
        },
    }
}

pub(super) fn runtime_can_launch(execution_state: &SessionExecutionState) -> bool {
    matches!(
        execution_state,
        SessionExecutionState::Idle
            | SessionExecutionState::Completed { .. }
            | SessionExecutionState::Failed { .. }
            | SessionExecutionState::Interrupted { .. }
            | SessionExecutionState::Lost { .. }
    )
}
