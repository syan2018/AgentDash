use agentdash_agent_runtime_contract::ManagedRuntimePlatformChange;
use async_trait::async_trait;

use super::AgentRunProductRuntimeBinding;

/// A Product-fenced Managed Runtime change delivered to durable Product consumers.
///
/// The delivery worker resolves the immutable RuntimeThread -> AgentRun binding before
/// constructing this value. Consumers must additionally validate any source-binding or
/// projection facts they need before applying Product effects.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunProductRuntimeChange {
    pub binding: AgentRunProductRuntimeBinding,
    pub change: ManagedRuntimePlatformChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunProductRuntimeChangeOutcome {
    Ignored,
    Applied,
}

#[async_trait]
pub trait AgentRunProductRuntimeChangeObserver: Send + Sync {
    fn consumer_name(&self) -> &'static str;

    async fn observe_product_runtime_change(
        &self,
        input: &AgentRunProductRuntimeChange,
    ) -> Result<AgentRunProductRuntimeChangeOutcome, String>;
}
