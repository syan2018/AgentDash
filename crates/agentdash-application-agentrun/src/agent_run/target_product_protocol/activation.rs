use async_trait::async_trait;

use super::{
    AgentRunForkRuntimePort, AgentRunRuntimeChangePage, AgentRunRuntimeFeedSnapshot,
    RuntimeAgentChildIdentity, SubmitInput,
};

/// Production caller inventory frozen for the S5 activation commit.
pub const AGENT_RUN_TARGET_ACTIVATION_CALLERS: &[&str] = &[
    "Business Surface projection",
    "Tool Broker composition",
    "AgentHostCallbacks delivery",
    "Runtime snapshot/change owner",
];

/// Canonical artifacts remain owned by their generator and are intentionally
/// absent from the S4 target lane.
pub const AGENT_RUN_TARGET_GENERATED_ARTIFACTS: &[&str] = &[
    "canonical Rust wire DTOs",
    "canonical TypeScript API bindings",
];

/// Persistence shape is recorded for activation; S4 does not add a migration.
pub const AGENT_RUN_TARGET_SCHEMA_ACTIVATION: &[&str] = &[
    "agent_run_fork_saga durable repository",
    "runtime projection snapshot/change cursor store",
];

#[async_trait]
pub trait AgentRunTargetBusinessSurfacePort: Send + Sync {
    async fn apply_business_surface(
        &self,
        child: &RuntimeAgentChildIdentity,
        surface_facts: &serde_json::Value,
    ) -> Result<String, String>;
}

#[async_trait]
pub trait AgentRunTargetToolBrokerPort: Send + Sync {
    async fn bind_tool_broker(&self, child: &RuntimeAgentChildIdentity) -> Result<String, String>;
}

#[async_trait]
pub trait AgentRunTargetHostCallbacksPort: Send + Sync {
    async fn submit_input(
        &self,
        child: &RuntimeAgentChildIdentity,
        input: SubmitInput,
    ) -> Result<String, String>;
}

#[async_trait]
pub trait AgentRunTargetSnapshotPort: Send + Sync {
    async fn snapshot(
        &self,
        source_coordinate: &str,
    ) -> Result<AgentRunRuntimeFeedSnapshot, String>;

    async fn changes(
        &self,
        source_coordinate: &str,
        after_cursor: Option<&str>,
    ) -> Result<AgentRunRuntimeChangePage, String>;
}

/// Explicit composition boundary for S5. Constructing this value is the only
/// intended way to activate the target lane; S4 leaves it unconstructed.
pub struct AgentRunTargetActivation<'a> {
    pub runtime: &'a dyn AgentRunForkRuntimePort,
    pub business_surface: &'a dyn AgentRunTargetBusinessSurfacePort,
    pub tool_broker: &'a dyn AgentRunTargetToolBrokerPort,
    pub host_callbacks: &'a dyn AgentRunTargetHostCallbacksPort,
    pub snapshot_owner: &'a dyn AgentRunTargetSnapshotPort,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_inventory_has_the_four_required_consumer_cuts() {
        assert_eq!(AGENT_RUN_TARGET_ACTIVATION_CALLERS.len(), 4);
        assert!(AGENT_RUN_TARGET_ACTIVATION_CALLERS.contains(&"Business Surface projection"));
        assert!(AGENT_RUN_TARGET_ACTIVATION_CALLERS.contains(&"Tool Broker composition"));
        assert!(AGENT_RUN_TARGET_ACTIVATION_CALLERS.contains(&"AgentHostCallbacks delivery"));
        assert!(AGENT_RUN_TARGET_ACTIVATION_CALLERS.contains(&"Runtime snapshot/change owner"));
    }
}
