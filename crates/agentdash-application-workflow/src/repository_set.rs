use std::sync::Arc;

use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisioner;
use agentdash_application_ports::lifecycle_materialization::WorkflowAgentNodeMaterializationPort;
use agentdash_domain::workflow::{
    AgentProcedureRepository, LifecycleGateRepository, LifecycleRunRepository,
};

#[derive(Clone)]
pub struct WorkflowRepositorySet {
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    pub lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    pub workflow_agent_node_materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
    pub agent_run_runtime_provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
}
