use async_trait::async_trait;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_application_ports::launch::{
    LaunchCommand, LaunchInputSource, LaunchPlanningInput, LaunchPromptInput,
};
use agentdash_domain::agent_run_mailbox::MailboxMessageOrigin;
use agentdash_spi::platform::auth::AuthIdentity;
use agentdash_spi::{AgentConfig, PromptPayload};

use crate::agent_run::runtime_session_boundary::SessionLaunchService;
use crate::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct AgentRunMessageDelivery {
    pub runtime_thread_id: String,
    pub origin: MailboxMessageOrigin,
    pub input: Vec<UserInputBlock>,
    pub input_source: Option<LaunchInputSource>,
    pub executor_config: Option<AgentConfig>,
    pub planning_input: LaunchPlanningInput,
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
        let user_input = LaunchPromptInput {
            input: Some(delivery.input),
            environment_variables: Default::default(),
            executor_config: delivery.executor_config,
        };
        validate_launch_prompt_input(&user_input)?;
        let command = launch_command_for_mailbox_origin(
            delivery.origin,
            user_input,
            delivery.identity,
            delivery.input_source,
        );
        self.session_launch
            .launch_command_in_task(
                delivery.runtime_thread_id.clone(),
                command,
                delivery.planning_input,
            )
            .await
    }
}

fn launch_command_for_mailbox_origin(
    origin: MailboxMessageOrigin,
    input: LaunchPromptInput,
    identity: Option<AuthIdentity>,
    input_source: Option<LaunchInputSource>,
) -> LaunchCommand {
    let command = match origin {
        MailboxMessageOrigin::User => {
            LaunchCommand::lifecycle_agent_user_message_input(input, identity)
        }
        MailboxMessageOrigin::Companion => LaunchCommand::companion_parent_resume_input(input),
        MailboxMessageOrigin::Hook => LaunchCommand::hook_auto_resume_input(input),
        MailboxMessageOrigin::Workflow => LaunchCommand::workflow_orchestrator_input(input),
        MailboxMessageOrigin::System => LaunchCommand::system_delivery_input(input),
    };
    if let Some(source) = input_source {
        command.with_input_source(source)
    } else {
        command
    }
}

fn validate_launch_prompt_input(input: &LaunchPromptInput) -> Result<(), WorkflowApplicationError> {
    let blocks = input
        .input
        .as_ref()
        .ok_or_else(|| WorkflowApplicationError::BadRequest("必须提供 input".to_string()))?;
    if blocks.is_empty() {
        return Err(WorkflowApplicationError::BadRequest(
            "input 不能为空数组".to_string(),
        ));
    }
    if PromptPayload::Input(blocks.clone())
        .to_fallback_text()
        .trim()
        .is_empty()
    {
        return Err(WorkflowApplicationError::BadRequest(
            "input 中没有有效内容".to_string(),
        ));
    }
    Ok(())
}
