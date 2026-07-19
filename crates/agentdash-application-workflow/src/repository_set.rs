use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentProcedureRepository, LifecycleGateRepository, LifecycleRunRepository,
};

#[derive(Clone)]
pub struct WorkflowRepositorySet {
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    pub agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
}
