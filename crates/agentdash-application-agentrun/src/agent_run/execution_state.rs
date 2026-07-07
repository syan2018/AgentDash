use agentdash_domain::workflow::{AgentRunDeliveryBinding, DeliveryBindingStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunExecutionState {
    Idle,
    Running {
        turn_id: Option<String>,
    },
    Cancelling {
        turn_id: Option<String>,
    },
    Completed {
        turn_id: String,
    },
    Failed {
        turn_id: String,
        message: Option<String>,
    },
    Interrupted {
        turn_id: Option<String>,
        message: Option<String>,
    },
    Lost {
        turn_id: Option<String>,
        message: Option<String>,
    },
}

impl AgentRunExecutionState {
    pub fn from_delivery_binding(binding: Option<&AgentRunDeliveryBinding>) -> Self {
        let Some(binding) = binding else {
            return Self::Idle;
        };
        Self::from_delivery_parts(
            binding.status,
            binding.active_turn_id.clone(),
            binding.last_turn_id.clone(),
            binding.terminal_state.clone(),
            binding.terminal_message.clone(),
        )
    }

    pub fn from_delivery_parts(
        status: DeliveryBindingStatus,
        active_turn_id: Option<String>,
        last_turn_id: Option<String>,
        terminal_state: Option<String>,
        terminal_message: Option<String>,
    ) -> Self {
        match status {
            DeliveryBindingStatus::Ready | DeliveryBindingStatus::DeliveryMissing => Self::Idle,
            DeliveryBindingStatus::Running => Self::Running {
                turn_id: active_turn_id,
            },
            DeliveryBindingStatus::Terminal => {
                terminal_state_from_parts(last_turn_id, terminal_state, terminal_message)
            }
            DeliveryBindingStatus::Lost => Self::Lost {
                turn_id: last_turn_id,
                message: terminal_message,
            },
            DeliveryBindingStatus::FrameMissing => Self::Idle,
        }
    }
}

fn terminal_state_from_parts(
    last_turn_id: Option<String>,
    terminal_state: Option<String>,
    terminal_message: Option<String>,
) -> AgentRunExecutionState {
    let turn_id = last_turn_id.clone().unwrap_or_default();
    match terminal_state.as_deref() {
        Some("failed") => AgentRunExecutionState::Failed {
            turn_id,
            message: terminal_message,
        },
        Some("interrupted") => AgentRunExecutionState::Interrupted {
            turn_id: last_turn_id,
            message: terminal_message,
        },
        Some("lost") => AgentRunExecutionState::Lost {
            turn_id: last_turn_id,
            message: terminal_message,
        },
        _ => AgentRunExecutionState::Completed { turn_id },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{DeliveryBindingStatus, RuntimeSessionExecutionAnchor};
    use chrono::Utc;
    use uuid::Uuid;

    fn binding(status: DeliveryBindingStatus) -> AgentRunDeliveryBinding {
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-a",
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        AgentRunDeliveryBinding::from_anchor(&anchor, status, Utc::now())
    }

    #[test]
    fn running_state_comes_from_agent_run_binding_turn() {
        let binding = binding(DeliveryBindingStatus::Ready).mark_running("turn-1", Utc::now());

        assert_eq!(
            AgentRunExecutionState::from_delivery_binding(Some(&binding)),
            AgentRunExecutionState::Running {
                turn_id: Some("turn-1".to_string())
            }
        );
    }

    #[test]
    fn terminal_state_comes_from_agent_run_binding_terminal_fields() {
        let binding = binding(DeliveryBindingStatus::Ready).mark_terminal(
            "turn-1",
            "failed",
            Some("provider failed".to_string()),
            None,
            Utc::now(),
        );

        assert_eq!(
            AgentRunExecutionState::from_delivery_binding(Some(&binding)),
            AgentRunExecutionState::Failed {
                turn_id: "turn-1".to_string(),
                message: Some("provider failed".to_string())
            }
        );
    }
}
