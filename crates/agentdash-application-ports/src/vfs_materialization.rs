use agentdash_relay::{VfsMaterializePayload, VfsMaterializeResponse};
use async_trait::async_trait;

#[async_trait]
pub trait VfsMaterializationTransport: Send + Sync {
    async fn materialize(
        &self,
        backend_id: &str,
        payload: VfsMaterializePayload,
    ) -> Result<VfsMaterializeResponse, String>;
}
