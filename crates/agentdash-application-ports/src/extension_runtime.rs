use agentdash_relay::{
    CommandExtensionActionInvokePayload, CommandExtensionChannelInvokePayload,
    ResponseExtensionActionInvokePayload, ResponseExtensionChannelInvokePayload,
};
use async_trait::async_trait;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtensionRuntimeActionTransportError {
    #[error("backend offline: {backend_id}")]
    Offline { backend_id: String },
    #[error("backend command timeout: {backend_id}")]
    Timeout { backend_id: String },
    #[error("backend response dropped: {backend_id}")]
    ResponseDropped { backend_id: String },
    #[error("extension action relay failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait ExtensionRuntimeActionTransport: Send + Sync {
    async fn invoke_extension_action(
        &self,
        backend_id: &str,
        payload: CommandExtensionActionInvokePayload,
    ) -> Result<ResponseExtensionActionInvokePayload, ExtensionRuntimeActionTransportError>;
}

#[async_trait]
pub trait ExtensionRuntimeChannelTransport: Send + Sync {
    async fn invoke_extension_channel(
        &self,
        backend_id: &str,
        payload: CommandExtensionChannelInvokePayload,
    ) -> Result<ResponseExtensionChannelInvokePayload, ExtensionRuntimeActionTransportError>;
}
