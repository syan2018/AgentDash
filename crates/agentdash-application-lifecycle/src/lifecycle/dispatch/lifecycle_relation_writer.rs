use std::sync::Arc;

use agentdash_application_ports::project_projection_notification::{
    ControlPlaneProjectionChangeReason, ProjectProjectionInvalidation,
    ProjectProjectionNotificationPort,
};
use uuid::Uuid;

use agentdash_application_workflow::gate::{LifecycleGateResolver, OpenCompanionGateCommand};
use agentdash_domain::workflow::{
    AgentLineage, AgentLineageRepository, AgentPolicy, GatePolicy, LifecycleAgent,
    LifecycleGateRepository, LifecycleRun,
};

use crate::lifecycle::WorkflowApplicationError;

use super::plan::DispatchPlan;

pub(crate) struct LifecycleRelationWriter<'a> {
    gate_repo: &'a dyn LifecycleGateRepository,
    lineage_repo: &'a dyn AgentLineageRepository,
    project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
}

pub(crate) struct RelationWriteResult {
    pub(crate) gate_ref: Option<Uuid>,
}

impl<'a> LifecycleRelationWriter<'a> {
    pub(crate) fn new(
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
        project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
    ) -> Self {
        Self {
            gate_repo,
            lineage_repo,
            project_projection_notifications,
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
            if let Some(port) = self.project_projection_notifications.as_ref() {
                let _ = port
                    .publish_project_projection_invalidated(
                        ProjectProjectionInvalidation::agent_run_list(
                            run.project_id,
                            run.id,
                            agent.id,
                            Some(frame_id),
                            ControlPlaneProjectionChangeReason::AgentRunLineageChanged,
                            None,
                        ),
                    )
                    .await;
            }
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
        let outcome = LifecycleGateResolver::open_companion_gate_with_repo(
            self.gate_repo,
            OpenCompanionGateCommand {
                run_id: run.id,
                agent_id: agent.id,
                frame_id: Some(frame_id),
                gate_kind: policy.gate_kind.clone(),
                correlation_id: correlation,
                payload: policy.payload.clone(),
                wait_policy: policy.wait_policy.clone(),
            },
        )
        .await?;
        Ok(outcome.gate.id)
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
