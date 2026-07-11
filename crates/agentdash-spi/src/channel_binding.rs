use agentdash_domain::channel::{
    ChannelBinding, ChannelMessage, ChannelOperation, ChannelParticipantRef, ChannelReplyTarget,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelProviderInboundEvent {
    pub provider: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedChannelIngress {
    pub external_workspace_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_room_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_thread_ref: Option<String>,
    pub provider_event_ref: String,
    pub sender: ChannelParticipantRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_target: Option<ChannelReplyTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelOutboundRequest {
    pub binding: ChannelBinding,
    pub operation: ChannelOperation,
    pub message: ChannelMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelProviderReceipt {
    pub provider: String,
    pub provider_event_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChannelBindingError {
    #[error("channel binding provider `{provider}` is unavailable")]
    Unavailable { provider: String },
    #[error("channel binding provider rejected request: {0}")]
    Rejected(String),
    #[error("channel binding provider failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait ChannelBindingProvider: Send + Sync {
    fn provider_key(&self) -> &str;

    async fn normalize_inbound(
        &self,
        event: ChannelProviderInboundEvent,
    ) -> Result<NormalizedChannelIngress, ChannelBindingError>;

    async fn publish(
        &self,
        request: ChannelOutboundRequest,
    ) -> Result<ChannelProviderReceipt, ChannelBindingError>;
}
