use async_trait::async_trait;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_spi::AgentConfig;
use agentdash_spi::platform::auth::AuthIdentity;

use crate::error::WorkflowApplicationError;
use crate::session::{LaunchCommand, SessionLaunchService, UserPromptInput};

#[derive(Debug, Clone)]
pub struct AgentRunMessageDelivery {
    pub delivery_runtime_session_id: String,
    pub input: Vec<UserInputBlock>,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
}

#[async_trait]
pub trait AgentRunMessageDeliveryPort: Send + Sync {
    async fn deliver_user_message(
        &self,
        delivery: AgentRunMessageDelivery,
    ) -> Result<String, WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct SessionTurnMessageDeliveryPort {
    session_launch: SessionLaunchService,
}

impl SessionTurnMessageDeliveryPort {
    pub fn new(session_launch: SessionLaunchService) -> Self {
        Self { session_launch }
    }
}

#[async_trait]
impl AgentRunMessageDeliveryPort for SessionTurnMessageDeliveryPort {
    async fn deliver_user_message(
        &self,
        delivery: AgentRunMessageDelivery,
    ) -> Result<String, WorkflowApplicationError> {
        let user_input = UserPromptInput {
            input: Some(delivery.input),
            env: Default::default(),
            executor_config: delivery.executor_config,
            backend_selection: None,
        };
        user_input
            .resolve_prompt_payload()
            .map_err(WorkflowApplicationError::BadRequest)?;
        let command =
            LaunchCommand::lifecycle_agent_user_message_input(user_input, delivery.identity);
        self.session_launch
            .launch_command_in_task(delivery.delivery_runtime_session_id.clone(), command)
            .await
            .map_err(WorkflowApplicationError::from)
    }
}
