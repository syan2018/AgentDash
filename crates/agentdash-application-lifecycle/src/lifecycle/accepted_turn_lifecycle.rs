use std::sync::Arc;

use agentdash_application_ports::accepted_turn_lifecycle::{
    AcceptedTurnLifecycleAdvanceInput, AcceptedTurnLifecycleAdvancePort,
};
use agentdash_application_workflow::orchestration::{
    OrchestrationRuntimeEvent, apply_orchestration_event_to_run,
};
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleRunRepository, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::ConnectorError;
use async_trait::async_trait;

#[derive(Clone)]
pub struct AcceptedTurnLifecycleAdvanceService {
    anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    run_repo: Arc<dyn LifecycleRunRepository>,
}

impl AcceptedTurnLifecycleAdvanceService {
    pub fn new(
        anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        run_repo: Arc<dyn LifecycleRunRepository>,
    ) -> Self {
        Self {
            anchor_repo,
            run_repo,
        }
    }
}

#[async_trait]
impl AcceptedTurnLifecycleAdvancePort for AcceptedTurnLifecycleAdvanceService {
    async fn advance_node_started_for_accepted_turn(
        &self,
        input: AcceptedTurnLifecycleAdvanceInput,
    ) -> Result<(), ConnectorError> {
        let anchor = self
            .anchor_repo
            .find_by_session(&input.runtime_session_id)
            .await
            .map_err(|error| {
                connector_error(format!(
                    "accepted turn lifecycle 查询 RuntimeSession anchor 失败: {error}"
                ))
            })?
            .ok_or_else(|| {
                connector_error(format!(
                    "accepted turn lifecycle 缺少 RuntimeSession anchor: runtime_session_id={}, turn_id={}",
                    input.runtime_session_id, input.turn_id
                ))
            })?;

        let Some((orchestration_id, node_path, attempt)) = orchestration_binding(&anchor) else {
            return Ok(());
        };
        let run = self
            .run_repo
            .get_by_id(anchor.run_id)
            .await
            .map_err(|error| {
                connector_error(format!(
                    "accepted turn lifecycle 读取 LifecycleRun 失败: run_id={}, error={error}",
                    anchor.run_id
                ))
            })?
            .ok_or_else(|| {
                connector_error(format!(
                    "accepted turn lifecycle 缺少 LifecycleRun: run_id={}, runtime_session_id={}",
                    anchor.run_id, input.runtime_session_id
                ))
            })?;

        let (updated_run, _) = apply_orchestration_event_to_run(
            run,
            orchestration_id,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path,
                attempt,
                executor_run_ref: Some(ExecutorRunRef::RuntimeSession {
                    session_id: input.runtime_session_id.clone(),
                }),
                timestamp: chrono::Utc::now(),
            },
        )
        .map_err(|error| {
            connector_error(format!(
                "accepted turn lifecycle 提交 NodeStarted 失败: run_id={}, orchestration_id={}, runtime_session_id={}, turn_id={}, error={error}",
                anchor.run_id, orchestration_id, input.runtime_session_id, input.turn_id
            ))
        })?;
        self.run_repo.update(&updated_run).await.map_err(|error| {
            connector_error(format!(
                "accepted turn lifecycle 保存 LifecycleRun 失败: run_id={}, runtime_session_id={}, turn_id={}, error={error}",
                anchor.run_id, input.runtime_session_id, input.turn_id
            ))
        })?;
        Ok(())
    }
}

pub fn accepted_turn_lifecycle_advance_port(
    anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    run_repo: Arc<dyn LifecycleRunRepository>,
) -> Arc<dyn AcceptedTurnLifecycleAdvancePort> {
    Arc::new(AcceptedTurnLifecycleAdvanceService::new(
        anchor_repo,
        run_repo,
    ))
}

fn orchestration_binding(
    anchor: &RuntimeSessionExecutionAnchor,
) -> Option<(uuid::Uuid, String, u32)> {
    Some((
        anchor.orchestration_id?,
        anchor.node_path.clone()?,
        anchor.node_attempt?,
    ))
}

fn connector_error(message: String) -> ConnectorError {
    ConnectorError::Runtime(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_workflow::orchestration::activate_root_orchestration;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivationRule, AgentFrame, ExecutorRunRef, LifecycleRun, OrchestrationLimits,
        OrchestrationPlanSnapshot, OrchestrationSourceRef, PlanNode, PlanNodeKind,
        RuntimeNodeStatus, RuntimeTraceRef,
    };
    use chrono::Utc;
    use std::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct MemoryAnchorRepo {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for MemoryAnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            self.anchors.lock().unwrap().push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors
                .lock()
                .unwrap()
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct MemoryRunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for MemoryRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().unwrap();
            if let Some(existing) = runs.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().retain(|run| run.id != id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn accepted_turn_advances_claimed_orchestration_node_to_started() {
        let project_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id);
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id: Uuid::new_v4(),
            graph_version: Some(1),
        };
        let mut orchestration =
            activate_root_orchestration(source_ref.clone(), plan_snapshot(source_ref));
        let orchestration_id = orchestration.orchestration_id;
        orchestration.node_tree[0].status = RuntimeNodeStatus::Claiming;
        orchestration.activation.ready_node_ids.clear();
        orchestration.dispatch.ready_node_ids.clear();
        run.add_orchestration(orchestration);
        let run_id = run.id;
        let agent_id = Uuid::new_v4();
        let launch_frame = AgentFrame::new_initial(agent_id);
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "runtime-accepted",
            run_id,
            launch_frame.id,
            agent_id,
            orchestration_id,
            "entry",
            1,
        );
        let anchor_repo = Arc::new(MemoryAnchorRepo::default());
        anchor_repo.create_once(&anchor).await.unwrap();
        let run_repo = Arc::new(MemoryRunRepo::default());
        run_repo.create(&run).await.unwrap();
        let service = AcceptedTurnLifecycleAdvanceService::new(anchor_repo, run_repo.clone());

        service
            .advance_node_started_for_accepted_turn(AcceptedTurnLifecycleAdvanceInput {
                runtime_session_id: "runtime-accepted".to_string(),
                turn_id: "turn-accepted".to_string(),
            })
            .await
            .expect("accepted lifecycle advance");

        let updated = run_repo.get_by_id(run_id).await.unwrap().unwrap();
        let node = &updated.orchestrations[0].node_tree[0];
        assert_eq!(node.status, RuntimeNodeStatus::Running);
        assert!(node.started_at.is_some());
        assert_eq!(
            node.executor_run_ref,
            Some(ExecutorRunRef::RuntimeSession {
                session_id: "runtime-accepted".to_string(),
            })
        );
        assert_eq!(
            node.trace_refs,
            vec![RuntimeTraceRef::RuntimeSession {
                session_id: "runtime-accepted".to_string(),
            }]
        );
    }

    fn plan_snapshot(source_ref: OrchestrationSourceRef) -> OrchestrationPlanSnapshot {
        OrchestrationPlanSnapshot {
            plan_digest: "sha256:accepted-turn-lifecycle-test".to_string(),
            plan_version: 1,
            source_ref,
            nodes: vec![PlanNode {
                node_id: "entry".to_string(),
                node_path: "entry".to_string(),
                parent_node_id: None,
                kind: PlanNodeKind::AgentCall,
                label: None,
                executor: None,
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec!["entry".to_string()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: "entry".to_string(),
            }],
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        }
    }
}
