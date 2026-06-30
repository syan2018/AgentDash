use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentLineage, AgentLineageRepository, AgentPolicy, GatePolicy, LifecycleAgent, LifecycleGate,
    LifecycleGateRepository, LifecycleRun,
};

use crate::lifecycle::WorkflowApplicationError;

use super::plan::DispatchPlan;

pub(crate) struct LifecycleRelationWriter<'a> {
    gate_repo: &'a dyn LifecycleGateRepository,
    lineage_repo: &'a dyn AgentLineageRepository,
}

pub(crate) struct RelationWriteResult {
    pub(crate) gate_ref: Option<Uuid>,
}

impl<'a> LifecycleRelationWriter<'a> {
    pub(crate) fn new(
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
    ) -> Self {
        Self {
            gate_repo,
            lineage_repo,
        }
    }

    pub(crate) async fn write_for_dispatch(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        frame_id: Uuid,
        plan: &DispatchPlan,
    ) -> Result<RelationWriteResult, WorkflowApplicationError> {
        if let Some(parent_agent_id) = plan.parent_agent_id {
            let lineage = AgentLineage::new(
                run.id,
                Some(parent_agent_id),
                agent.id,
                lineage_relation_kind(&plan.agent_policy),
                Some(frame_id),
                None,
            );
            self.lineage_repo.create(&lineage).await?;
        }

        let gate_ref = if let Some(gate_policy) = &plan.gate_policy {
            Some(self.open_gate(run, agent, frame_id, gate_policy).await?)
        } else {
            None
        };

        Ok(RelationWriteResult { gate_ref })
    }

    async fn open_gate(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        frame_id: Uuid,
        policy: &GatePolicy,
    ) -> Result<Uuid, WorkflowApplicationError> {
        let correlation = policy
            .correlation_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let gate = LifecycleGate::open(
            run.id,
            Some(agent.id),
            Some(frame_id),
            &policy.gate_kind,
            correlation,
            policy.payload.clone(),
        );
        let gate_id = gate.id;
        self.gate_repo.create(&gate).await?;
        Ok(gate_id)
    }
}

fn lineage_relation_kind(policy: &AgentPolicy) -> &'static str {
    match policy {
        AgentPolicy::SpawnChild => "spawn",
        AgentPolicy::Create => "delegation",
        AgentPolicy::Resume => "resume",
        AgentPolicy::Reuse => "reuse",
    }
}
