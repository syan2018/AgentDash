use std::sync::Arc;

use agentdash_agent_runtime_contract::{ManagedRuntimeSnapshot, RuntimeThreadId};
use async_trait::async_trait;

use super::{
    AgentRunForkProductGraphPort, AgentRunForkRuntimePort, AgentRunForkSagaRepository,
    CompanionFreshRuntimePort, CompanionFreshSagaRepository,
};

/// Product 组合根所需的 concrete Agent 权威快照读取边界。
#[async_trait]
pub trait AgentRunRuntimeSnapshotPort: Send + Sync {
    async fn load_snapshot(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeSnapshot, String>;
}

/// Product 协议的完整生产依赖；所有端口都必须由 composition root 显式注入。
pub struct AgentRunProductProtocolPorts {
    pub fork_sagas: Arc<dyn AgentRunForkSagaRepository>,
    pub fork_runtime: Arc<dyn AgentRunForkRuntimePort>,
    pub fork_product_graph: Arc<dyn AgentRunForkProductGraphPort>,
    pub companion_fresh_sagas: Arc<dyn CompanionFreshSagaRepository>,
    pub companion_fresh_runtime: Arc<dyn CompanionFreshRuntimePort>,
    pub product_launch: Arc<crate::agent_run::AgentRunProductLaunchService>,
    pub runtime_snapshot: Arc<dyn AgentRunRuntimeSnapshotPort>,
}

impl AgentRunProductProtocolPorts {
    pub fn new(
        fork_sagas: Arc<dyn AgentRunForkSagaRepository>,
        fork_runtime: Arc<dyn AgentRunForkRuntimePort>,
        fork_product_graph: Arc<dyn AgentRunForkProductGraphPort>,
        companion_fresh_sagas: Arc<dyn CompanionFreshSagaRepository>,
        companion_fresh_runtime: Arc<dyn CompanionFreshRuntimePort>,
        product_launch: Arc<crate::agent_run::AgentRunProductLaunchService>,
        runtime_snapshot: Arc<dyn AgentRunRuntimeSnapshotPort>,
    ) -> Self {
        Self {
            fork_sagas,
            fork_runtime,
            fork_product_graph,
            companion_fresh_sagas,
            companion_fresh_runtime,
            product_launch,
            runtime_snapshot,
        }
    }
}
