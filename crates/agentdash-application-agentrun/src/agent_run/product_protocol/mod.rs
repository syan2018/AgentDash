//! AgentRun 产品协议与 durable orchestration。
//!
//! 本模块是 Product owner 面向 Runtime Contract 投影、AgentRun fork 与 Companion dispatch
//! 的唯一生产入口。具体 Runtime、Host、Business Surface 与持久化 adapter 由 S5
//! composition root 注入，产品层不选择 legacy runtime 路径。

mod activation;
mod companion;
mod companion_continuation;
mod feed;
mod fork_saga;
mod production_adapters;
mod recovery;
mod thread_name_projection;
mod workflow_agent_call;

use agentdash_domain::workflow::{AgentFrame, AgentRunLineage, LifecycleAgent, LifecycleRun};

#[derive(Debug, Clone)]
pub struct AgentRunForkGraph {
    pub child_run: LifecycleRun,
    pub child_agent: LifecycleAgent,
    pub child_frame: AgentFrame,
    pub lineage: AgentRunLineage,
}

pub use activation::*;
pub use companion::*;
pub use companion_continuation::*;
pub use feed::*;
pub use fork_saga::*;
pub use production_adapters::*;
pub use recovery::*;
pub use thread_name_projection::*;
pub use workflow_agent_call::*;
