use std::sync::Arc;

use agentdash_domain::workflow::{LifecycleGateRepository, LifecycleRunRepository};

#[derive(Clone)]
pub struct WorkflowRepositorySet {
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
}
