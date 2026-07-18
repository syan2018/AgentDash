use std::sync::Arc;

use agentdash_application_ports::agent_run_fork::AgentRunForkGraph;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunLineage, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository,
};
use async_trait::async_trait;
use serde_json::json;
use thiserror::Error;

use super::{AgentRunForkProductGraphPort, AgentRunForkSaga, PreparedAgentRunForkGraph};

#[derive(Debug, Error)]
enum ProductAgentRunForkGraphError {
    #[error("parent LifecycleRun {0} was not found")]
    ParentRunNotFound(uuid::Uuid),
    #[error("parent LifecycleAgent {0} was not found")]
    ParentAgentNotFound(uuid::Uuid),
    #[error("parent AgentFrame for agent {0} was not found")]
    ParentFrameNotFound(uuid::Uuid),
    #[error("parent Product graph is inconsistent: {0}")]
    InconsistentParent(&'static str),
    #[error("Product repository failed while reading {entity}: {reason}")]
    Repository {
        entity: &'static str,
        reason: String,
    },
    #[error("prepared Product graph is invalid: {0}")]
    InvalidPreparedGraph(String),
}

/// Product-owned production adapter that prepares the immutable child graph for a fork.
///
/// This adapter is deliberately read-only. The prepared graph is published only by
/// `AgentRunForkSagaRepository::commit_product_graph`, which owns the atomic graph + saga
/// transition required by the fork protocol.
pub struct ProductAgentRunForkGraphAdapter {
    runs: Arc<dyn LifecycleRunRepository>,
    agents: Arc<dyn LifecycleAgentRepository>,
    frames: Arc<dyn AgentFrameRepository>,
}

impl ProductAgentRunForkGraphAdapter {
    pub fn new(
        runs: Arc<dyn LifecycleRunRepository>,
        agents: Arc<dyn LifecycleAgentRepository>,
        frames: Arc<dyn AgentFrameRepository>,
    ) -> Self {
        Self {
            runs,
            agents,
            frames,
        }
    }

    async fn prepare(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<PreparedAgentRunForkGraph, ProductAgentRunForkGraphError> {
        let parent = saga.parent();
        let parent_run = self
            .runs
            .get_by_id(parent.run_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "LifecycleRun",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentRunNotFound(
                parent.run_id,
            ))?;
        let parent_agent = self
            .agents
            .get(parent.agent_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "LifecycleAgent",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentAgentNotFound(
                parent.agent_id,
            ))?;
        let parent_frame = self
            .frames
            .get_latest(parent.agent_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "AgentFrame",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentFrameNotFound(
                parent.agent_id,
            ))?;

        if parent_agent.run_id != parent_run.id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "LifecycleAgent does not belong to LifecycleRun",
            ));
        }
        if parent_agent.project_id != parent_run.project_id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "LifecycleAgent and LifecycleRun have different projects",
            ));
        }
        if parent_frame.agent_id != parent_agent.id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "AgentFrame does not belong to LifecycleAgent",
            ));
        }

        let child = saga.child();
        let mut child_run = LifecycleRun::new_plain_for_user(
            parent_run.project_id,
            parent_run.created_by_user_id.clone(),
        );
        child_run.id = child.run_id;
        // The saga currently owns stable identities but not a separate requested-at timestamp.
        // Reusing the immutable parent graph timestamp keeps retry preparation byte-identical.
        child_run.created_at = parent_run.last_activity_at;
        child_run.updated_at = parent_run.last_activity_at;
        child_run.last_activity_at = parent_run.last_activity_at;

        let mut child_agent = LifecycleAgent::new_root_for_user(
            child.run_id,
            parent_run.project_id,
            parent_agent.source,
            parent_agent.created_by_user_id.clone(),
        );
        child_agent.id = child.agent_id;
        child_agent.project_agent_id = parent_agent.project_agent_id;
        child_agent.bootstrap_status = parent_agent.bootstrap_status.clone();
        child_agent.workspace_title = parent_agent.workspace_title.clone();
        child_agent.workspace_title_source = parent_agent
            .workspace_title
            .as_ref()
            .map(|_| "source".to_owned());
        child_agent.created_at = parent_agent.updated_at;
        child_agent.updated_at = parent_agent.updated_at;

        let mut child_frame = parent_frame.clone();
        child_frame.id = child.frame_id;
        child_frame.agent_id = child.agent_id;
        child_frame.revision = 1;
        child_frame.created_by_kind = "agent_run_fork_product_protocol".to_owned();
        child_frame.created_by_id = Some(parent_agent.created_by_user_id.clone());
        child_frame.created_at = parent_frame.created_at;

        let mut lineage = AgentRunLineage::new_fork(
            parent.run_id,
            parent.agent_id,
            child.run_id,
            child.agent_id,
            None,
            Some(json!({
                "kind": "completed_turn",
                "runtime_thread_id": parent.runtime_thread_id,
                "turn_id": parent.through_turn_id,
            })),
            parent_agent.created_by_user_id.clone(),
            Some(json!({
                "agent_run_id": child.agent_run_id,
                "runtime_thread_id": child.runtime_thread_id,
            })),
        )
        .with_frame_baseline(
            parent_frame.id,
            parent_frame.revision,
            child.frame_id,
            child_frame.revision,
        );
        lineage.id = saga.request_id().0;
        lineage.created_at = parent_frame.created_at;

        PreparedAgentRunForkGraph::prepare(
            saga,
            AgentRunForkGraph {
                child_run,
                child_agent,
                child_frame,
                lineage,
            },
        )
        .map_err(|error| ProductAgentRunForkGraphError::InvalidPreparedGraph(error.to_string()))
    }
}

#[async_trait]
impl AgentRunForkProductGraphPort for ProductAgentRunForkGraphAdapter {
    async fn prepare_child_graph_commit(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<PreparedAgentRunForkGraph, String> {
        self.prepare(saga).await.map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workflow::{AgentFrame, AgentSource, LifecycleAgent, LifecycleRun};
    use agentdash_test_support::workflow::{
        MemoryAgentFrameRepository, MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository,
    };
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::product_protocol::{
        AcceptedRuntimeOperation, AgentRunForkParent, AgentRunForkRequestId, AgentRunForkSagaStep,
        PreallocatedAgentRunChild, RuntimeForkPhaseEvidence, RuntimeOperationOutcome,
    };

    async fn fixture() -> (
        ProductAgentRunForkGraphAdapter,
        AgentRunForkSaga,
        Arc<MemoryLifecycleRunRepository>,
        Arc<MemoryLifecycleAgentRepository>,
        Arc<MemoryAgentFrameRepository>,
    ) {
        let runs = Arc::new(MemoryLifecycleRunRepository::default());
        let agents = Arc::new(MemoryLifecycleAgentRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let parent_run = LifecycleRun::new_plain_for_user(Uuid::new_v4(), "user-1");
        let parent_agent = LifecycleAgent::new_root_for_user(
            parent_run.id,
            parent_run.project_id,
            AgentSource::ProjectAgent,
            "user-1",
        );
        let mut parent_frame = AgentFrame::new_initial(parent_agent.id);
        parent_frame.execution_profile_json = Some(json!({"model": "test"}));
        runs.create(&parent_run).await.expect("store parent run");
        agents
            .create(&parent_agent)
            .await
            .expect("store parent agent");
        frames
            .create(&parent_frame)
            .await
            .expect("store parent frame");

        let mut saga = AgentRunForkSaga::requested(
            AgentRunForkRequestId(Uuid::new_v4()),
            AgentRunForkParent {
                run_id: parent_run.id,
                agent_id: parent_agent.id,
                runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    "runtime-parent",
                )
                .expect("parent thread"),
                through_turn_id: agentdash_agent_runtime_contract::RuntimeTurnId::new("turn-7")
                    .expect("turn id"),
            },
            PreallocatedAgentRunChild {
                agent_run_id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                frame_id: Uuid::new_v4(),
                runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    "runtime-child",
                )
                .expect("child thread"),
            },
        );
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork dispatch");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("mark dispatch");
        let receipt = AcceptedRuntimeOperation {
            operation_id: identity.runtime_operation_id.clone(),
            accepted_revision: agentdash_agent_runtime_contract::RuntimeProjectionRevision(2),
        };
        saga.record_runtime_outcome(
            identity.clone(),
            RuntimeOperationOutcome::Accepted(receipt.clone()),
        )
        .expect("record admission");
        let child_thread_id = saga.child().runtime_thread_id.clone();
        saga.record_runtime_outcome(
            identity,
            RuntimeOperationOutcome::Applied(RuntimeForkPhaseEvidence::ForkProvisioned {
                child_thread_id,
                child_history_digest: agentdash_agent_runtime_contract::RuntimePayloadDigest::new(
                    "sha256:history",
                )
                .expect("history digest"),
                context: None,
                receipt,
            }),
        )
        .expect("record fork provisioning");

        (
            ProductAgentRunForkGraphAdapter::new(runs.clone(), agents.clone(), frames.clone()),
            saga,
            runs,
            agents,
            frames,
        )
    }

    #[tokio::test]
    async fn prepares_complete_stable_graph_without_publishing_rows() {
        let (adapter, saga, runs, agents, frames) = fixture().await;

        let first = adapter
            .prepare_child_graph_commit(&saga)
            .await
            .expect("prepare first graph");
        let second = adapter
            .prepare_child_graph_commit(&saga)
            .await
            .expect("prepare retry graph");

        assert_eq!(first.payload_digest(), second.payload_digest());
        assert_eq!(first.agent_run_id(), saga.child().agent_run_id);
        assert_eq!(first.runtime_thread_id().as_str(), "runtime-child");
        let graph = first.graph();
        assert_eq!(graph.child_run.id, saga.child().run_id);
        assert_eq!(graph.child_agent.id, saga.child().agent_id);
        assert_eq!(graph.child_frame.id, saga.child().frame_id);
        assert_eq!(
            graph.child_frame.execution_profile_json,
            Some(json!({"model": "test"}))
        );
        assert_eq!(graph.lineage.parent_run_id, saga.parent().run_id);
        assert_eq!(graph.lineage.child_frame_id, Some(saga.child().frame_id));

        assert!(runs.get_by_id(saga.child().run_id).await.unwrap().is_none());
        assert!(agents.get(saga.child().agent_id).await.unwrap().is_none());
        assert!(frames.get(saga.child().frame_id).await.unwrap().is_none());
    }
}
