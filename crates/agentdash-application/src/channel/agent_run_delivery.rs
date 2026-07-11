use agentdash_agent_runtime_contract::{RuntimeActor, RuntimeInput};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductDeliveryPort, DeliverAgentRunProductInput,
};
use async_trait::async_trait;

use super::{
    AdmittedChannelDelivery, ChannelAgentDeliveryPort, ChannelAgentDeliveryReceipt,
    ChannelAgentDeliveryTarget,
};
use crate::ApplicationError;
use agentdash_domain::channel::ChannelParticipantRef;

pub struct AgentRunChannelDeliveryAdapter<'a> {
    product_delivery: &'a dyn AgentRunProductDeliveryPort,
}

impl<'a> AgentRunChannelDeliveryAdapter<'a> {
    pub fn new(product_delivery: &'a dyn AgentRunProductDeliveryPort) -> Self {
        Self { product_delivery }
    }
}

#[async_trait]
impl ChannelAgentDeliveryPort for AgentRunChannelDeliveryAdapter<'_> {
    async fn deliver(
        &self,
        delivery: AdmittedChannelDelivery,
        target: ChannelAgentDeliveryTarget,
    ) -> Result<ChannelAgentDeliveryReceipt, ApplicationError> {
        let intent = delivery.intent();
        let mut input = Vec::new();
        if let Some(text) = &intent.message.payload.text {
            input.push(RuntimeInput::Text { text: text.clone() });
        }
        if let Some(value) = &intent.message.payload.data {
            input.push(RuntimeInput::Structured {
                schema: format!("agentdash.channel/{}", intent.message.payload.kind),
                value: value.clone(),
            });
        }
        if input.is_empty() {
            return Err(ApplicationError::InvalidConfig(
                "admitted channel delivery contains no runtime input".to_string(),
            ));
        }
        let actor = match &intent.message.sender {
            ChannelParticipantRef::Agent { agent_id, .. } => RuntimeActor::Agent {
                name: agent_id.to_string(),
            },
            ChannelParticipantRef::User { user_id } => RuntimeActor::User {
                subject: user_id.clone(),
            },
            sender => RuntimeActor::System {
                component: format!(
                    "channel:{}:{}",
                    intent.message.origin.namespace,
                    sender.stable_key()
                ),
            },
        };
        let result = self
            .product_delivery
            .deliver(DeliverAgentRunProductInput {
                run_id: target.run_id,
                agent_id: target.agent_id,
                input,
                actor,
                client_command_id: format!("channel-delivery:{}", intent.id),
            })
            .await
            .map_err(|error| {
                ApplicationError::Unavailable(format!("AgentRun channel delivery failed: {error}"))
            })?;
        let duplicate = result
            .operation_receipt
            .as_ref()
            .is_some_and(|receipt| receipt.duplicate);
        Ok(ChannelAgentDeliveryReceipt {
            mailbox_message_id: result.mailbox_message_id,
            accepted_runtime_operation_id: result
                .operation_receipt
                .map(|receipt| receipt.operation_id.to_string()),
            queued: result.queued,
            duplicate,
        })
    }
}
