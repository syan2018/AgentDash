use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::lifecycle_materialization::{
    WorkflowAgentNodeMaterializationRequest, WorkflowAgentNodeMaterializationResult,
};
use agentdash_application_ports::runtime_session_delivery as runtime_session_delivery_port;
use agentdash_application_ports::workflow_agent_frame_materialization as workflow_node_frame_port;
use agentdash_application_ports::workflow_graph_planning as workflow_graph_planning_port;
use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, AgentRunDeliveryBindingRepository,
    LifecycleAgentRepository, LifecycleGateRepository, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, RuntimeSessionExecutionAnchorRepository,
    WorkflowGraphRepository,
};
use agentdash_domain::workflow::{
    AgentLaunchDispatchResult, AgentLaunchIntent, ExecutionDispatchResult, ExecutionIntent,
    InteractionDispatchIntent, InteractionGateOpenedDispatchResult,
    LifecycleRunStartDispatchResult, LifecycleRunStartIntent, SubjectExecutionDispatchResult,
    SubjectExecutionIntent,
};

use super::WorkflowApplicationError;
use super::dispatch::{
    AgentRuntimeMaterializer, DispatchFacts, DispatchPlan, LifecycleRelationWriter,
    OrchestrationReducerBridge, RunOrchestrationStarter, SubjectAssociationWriter,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_spi::{ExecutionStatus, SessionMeta, SessionMetaStore};

#[derive(Clone)]
pub struct SessionMetaStoreRuntimeSessionCreator {
    session_meta_store: Arc<dyn SessionMetaStore>,
}

impl SessionMetaStoreRuntimeSessionCreator {
    pub fn new(session_meta_store: Arc<dyn SessionMetaStore>) -> Self {
        Self { session_meta_store }
    }
}

#[async_trait]
impl runtime_session_delivery_port::RuntimeSessionCreationPort
    for SessionMetaStoreRuntimeSessionCreator
{
    async fn create_runtime_session(
        &self,
        _request: runtime_session_delivery_port::RuntimeSessionCreationRequest,
    ) -> Result<
        runtime_session_delivery_port::RuntimeSessionCreationResult,
        runtime_session_delivery_port::RuntimeSessionDeliveryError,
    > {
        let session_id = Uuid::new_v4();
        let now = chrono::Utc::now().timestamp_millis();
        let meta = SessionMeta {
            id: session_id.to_string(),
            created_at: now,
            updated_at: now,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: None,
        };
        self.session_meta_store
            .create_session(&meta)
            .await
            .map_err(|error| {
                runtime_session_delivery_port::RuntimeSessionDeliveryError::Internal {
                    message: format!("RuntimeSession 创建失败: {error}"),
                }
            })?;
        Ok(
            runtime_session_delivery_port::RuntimeSessionCreationResult {
                runtime_session_id: session_id,
            },
        )
    }
}

/// 业务执行进入控制面的统一入口 service。
///
/// 接收 `ExecutionIntent`，根据 policy 决定：
/// - 复用 / 创建 LifecycleRun
/// - 创建 / 复用 OrchestrationInstance
/// - 创建 LifecycleSubjectAssociation（如果有 subject_ref）
/// - 创建 / 复用 LifecycleAgent
/// - 创建 AgentFrame initial revision
/// - 按需创建 LifecycleGate / AgentLineage
///
/// **不拥有** AgentFrame 内部构造细节（由 AgentRun frame materialization 边界处理）
/// 和 connector launch（T4 的工作）。
pub struct LifecycleDispatchService<'a> {
    run_repo: &'a dyn LifecycleRunRepository,
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    agent_repo: &'a dyn LifecycleAgentRepository,
    _frame_repo: &'a dyn AgentFrameRepository,
    association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    gate_repo: &'a dyn LifecycleGateRepository,
    lineage_repo: &'a dyn AgentLineageRepository,
    anchor_repo: Option<&'a dyn RuntimeSessionExecutionAnchorRepository>,
    delivery_binding_repo: Option<&'a dyn AgentRunDeliveryBindingRepository>,
    runtime_session_creator:
        Option<&'a dyn runtime_session_delivery_port::RuntimeSessionCreationPort>,
    frame_construction:
        Option<&'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort>,
    workflow_agent_frame_materialization:
        Option<&'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort>,
    workflow_graph_planner: Option<&'a dyn workflow_graph_planning_port::WorkflowGraphPlanningPort>,
}

impl<'a> LifecycleDispatchService<'a> {
    pub fn new(
        run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
    ) -> Self {
        Self {
            run_repo,
            workflow_graph_repo,
            agent_repo,
            _frame_repo: frame_repo,
            association_repo,
            gate_repo,
            lineage_repo,
            anchor_repo: None,
            delivery_binding_repo: None,
            runtime_session_creator: None,
            frame_construction: None,
            workflow_agent_frame_materialization: None,
            workflow_graph_planner: None,
        }
    }

    pub fn with_anchor_repo(
        mut self,
        repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    ) -> Self {
        self.anchor_repo = Some(repo);
        self
    }

    pub fn with_delivery_binding_repo(
        mut self,
        repo: &'a dyn AgentRunDeliveryBindingRepository,
    ) -> Self {
        self.delivery_binding_repo = Some(repo);
        self
    }

    pub fn with_runtime_session_creator(
        mut self,
        creator: &'a dyn runtime_session_delivery_port::RuntimeSessionCreationPort,
    ) -> Self {
        self.runtime_session_creator = Some(creator);
        self
    }

    pub fn with_frame_construction_port(
        mut self,
        port: &'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort,
    ) -> Self {
        self.frame_construction = Some(port);
        self
    }

    pub fn with_workflow_agent_frame_materialization_port(
        mut self,
        port: &'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort,
    ) -> Self {
        self.workflow_agent_frame_materialization = Some(port);
        self
    }

    pub fn with_workflow_graph_planner(
        mut self,
        planner: &'a dyn workflow_graph_planning_port::WorkflowGraphPlanningPort,
    ) -> Self {
        self.workflow_graph_planner = Some(planner);
        self
    }

    /// 按 typed ExecutionIntent 编排对应目标锚点。
    pub async fn dispatch(
        &self,
        intent: &ExecutionIntent,
    ) -> Result<ExecutionDispatchResult, WorkflowApplicationError> {
        match intent {
            ExecutionIntent::AgentLaunch(intent) => self
                .launch_agent(intent)
                .await
                .map(ExecutionDispatchResult::AgentLaunch),
            ExecutionIntent::SubjectExecution(intent) => self
                .execute_subject(intent)
                .await
                .map(ExecutionDispatchResult::SubjectExecution),
            ExecutionIntent::LifecycleRunStart(intent) => self
                .start_lifecycle_run(intent)
                .await
                .map(ExecutionDispatchResult::LifecycleRunStart),
            ExecutionIntent::InteractionDispatch(intent) => self
                .open_interaction_gate(intent)
                .await
                .map(ExecutionDispatchResult::InteractionGateOpened),
        }
    }

    pub async fn launch_agent(
        &self,
        intent: &AgentLaunchIntent,
    ) -> Result<AgentLaunchDispatchResult, WorkflowApplicationError> {
        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            source = ?intent.source,
            has_graph = intent.workflow_graph_ref.is_some(),
            "dispatch: launch_agent 进入"
        );
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        diag!(
            Info,
            Subsystem::Lifecycle,
            run_id = %facts.runtime_refs.run_ref,
            agent_id = %facts.runtime_refs.agent_ref,
            "dispatch: launch_agent 完成"
        );
        Ok(AgentLaunchDispatchResult {
            runtime_refs: facts.runtime_refs,
            delivery_runtime_ref: facts.runtime_session_ref,
        })
    }

    pub async fn execute_subject(
        &self,
        intent: &SubjectExecutionIntent,
    ) -> Result<SubjectExecutionDispatchResult, WorkflowApplicationError> {
        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            subject_kind = %intent.subject_ref.kind,
            subject_id = %intent.subject_ref.id,
            "dispatch: execute_subject 进入"
        );
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        let runtime_refs = facts.runtime_refs;
        let subject_execution_ref = facts.subject_execution_ref.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "SubjectExecutionIntent 未创建 subject_execution_ref".to_string(),
            )
        })?;
        Ok(SubjectExecutionDispatchResult {
            runtime_refs,
            subject_execution_ref,
            delivery_runtime_ref: facts.runtime_session_ref,
        })
    }

    pub async fn open_interaction_gate(
        &self,
        intent: &InteractionDispatchIntent,
    ) -> Result<InteractionGateOpenedDispatchResult, WorkflowApplicationError> {
        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            parent_run_id = %intent.parent_run_id,
            parent_agent_id = %intent.parent_agent_id,
            gate_kind = %intent.gate_policy.gate_kind,
            "dispatch: open_interaction_gate 进入"
        );
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        let gate_ref = facts.gate_ref.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "InteractionDispatchIntent 未创建 LifecycleGate".to_string(),
            )
        })?;
        Ok(InteractionGateOpenedDispatchResult {
            runtime_refs: facts.runtime_refs,
            gate_ref,
            delivery_runtime_ref: facts.runtime_session_ref,
        })
    }

    pub async fn start_lifecycle_run(
        &self,
        intent: &LifecycleRunStartIntent,
    ) -> Result<LifecycleRunStartDispatchResult, WorkflowApplicationError> {
        let result = self
            .run_orchestration_starter()
            .start_lifecycle_run(intent)
            .await?;

        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            run_id = %result.run_ref,
            orchestration_id = %result.orchestration_ref,
            "dispatch: start_lifecycle_run 创建 root orchestration"
        );

        Ok(result)
    }

    pub async fn materialize_workflow_agent_node(
        &self,
        request: WorkflowAgentNodeMaterializationRequest,
    ) -> Result<WorkflowAgentNodeMaterializationResult, WorkflowApplicationError> {
        let context = self
            .run_orchestration_starter()
            .workflow_agent_node_context(request.run_id, &request.orchestration_binding)
            .await?;
        let result = self
            .agent_runtime_materializer()
            .materialize_workflow_agent_node(context, request)
            .await?;

        diag!(
            Info,
            Subsystem::Lifecycle,
            run_id = %result.runtime_refs.run_ref,
            agent_id = %result.runtime_refs.agent_ref,
            orchestration_id = ?result.runtime_refs.orchestration_ref(),
            node_path = ?result.runtime_refs.node_path(),
            "dispatch: workflow agent node materialized，已绑定 delivery anchor"
        );

        Ok(result)
    }

    fn run_orchestration_starter(&self) -> RunOrchestrationStarter<'_> {
        RunOrchestrationStarter::new(
            self.run_repo,
            self.workflow_graph_repo,
            self.workflow_graph_planner,
        )
    }

    fn agent_runtime_materializer(&self) -> AgentRuntimeMaterializer<'_> {
        AgentRuntimeMaterializer::new(
            self.agent_repo,
            self.anchor_repo,
            self.delivery_binding_repo,
            self.runtime_session_creator,
            self.frame_construction,
            self.workflow_agent_frame_materialization,
        )
    }

    fn subject_association_writer(&self) -> SubjectAssociationWriter<'_> {
        SubjectAssociationWriter::new(self.association_repo)
    }

    fn lifecycle_relation_writer(&self) -> LifecycleRelationWriter<'_> {
        LifecycleRelationWriter::new(self.gate_repo, self.lineage_repo)
    }

    fn orchestration_reducer_bridge(&self) -> OrchestrationReducerBridge<'_> {
        OrchestrationReducerBridge::new(self.run_repo)
    }

    async fn dispatch_common(
        &self,
        plan: DispatchPlan,
    ) -> Result<DispatchFacts, WorkflowApplicationError> {
        if plan.workflow_graph_ref.is_none() {
            return self.dispatch_plain(plan).await;
        }

        let workflow_graph_ref = plan
            .workflow_graph_ref
            .as_ref()
            .expect("checked workflow graph ref");
        let prepared = self
            .run_orchestration_starter()
            .prepare_graph_dispatch(&plan, workflow_graph_ref)
            .await?;
        let materialized = self
            .agent_runtime_materializer()
            .materialize_dispatch_runtime(
                &prepared.run,
                &plan,
                Some(prepared.orchestration_binding.clone()),
            )
            .await?;
        let subject_result = self
            .subject_association_writer()
            .write_for_dispatch(prepared.run.id, materialized.agent.id, &plan)
            .await?;
        let relation_result = self
            .lifecycle_relation_writer()
            .write_for_dispatch(
                &prepared.run,
                &materialized.agent,
                materialized.frame_id,
                &plan,
            )
            .await?;
        self.orchestration_reducer_bridge()
            .mark_node_claimed(prepared.run, &prepared.orchestration_binding, &materialized)
            .await?;

        Ok(DispatchFacts {
            runtime_refs: materialized.runtime_refs,
            runtime_session_ref: materialized.runtime_session_ref,
            gate_ref: relation_result.gate_ref,
            subject_execution_ref: subject_result.subject_execution_ref,
        })
    }

    async fn dispatch_plain(
        &self,
        plan: DispatchPlan,
    ) -> Result<DispatchFacts, WorkflowApplicationError> {
        let run = self
            .run_orchestration_starter()
            .resolve_or_create_plain_run(&plan)
            .await?;
        let materialized = self
            .agent_runtime_materializer()
            .materialize_dispatch_runtime(&run, &plan, None)
            .await?;
        let subject_result = self
            .subject_association_writer()
            .write_for_dispatch(run.id, materialized.agent.id, &plan)
            .await?;
        let relation_result = self
            .lifecycle_relation_writer()
            .write_for_dispatch(&run, &materialized.agent, materialized.frame_id, &plan)
            .await?;

        Ok(DispatchFacts {
            runtime_refs: materialized.runtime_refs,
            runtime_session_ref: materialized.runtime_session_ref,
            gate_ref: relation_result.gate_ref,
            subject_execution_ref: subject_result.subject_execution_ref,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_application_workflow::orchestration::ROOT_ORCHESTRATION_ROLE;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::*;

    use super::*;

    const TEST_WORKFLOW_GRAPH_KEY: &str = "test.workflow_graph";
    const TEST_AGENT_PROCEDURE_KEY: &str = "test.agent_procedure";
    const TEST_ACTIVITY_KEY: &str = "main";

    // ─── In-Memory Repositories ──────────────────────────────────────────

    #[derive(Default)]
    struct InMemoryRunRepo {
        items: Mutex<Vec<LifecycleRun>>,
    }
    #[async_trait::async_trait]
    impl LifecycleRunRepository for InMemoryRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(run.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == id)
                .cloned())
        }
        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|r| ids.contains(&r.id))
                .cloned()
                .collect())
        }
        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(vec![])
        }
        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|r| r.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }
        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryWorkflowGraphRepo {
        items: Mutex<Vec<WorkflowGraph>>,
    }
    #[async_trait::async_trait]
    impl WorkflowGraphRepository for InMemoryWorkflowGraphRepo {
        async fn create(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(lifecycle.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|graph| graph.id == id)
                .cloned())
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|graph| graph.project_id == project_id && graph.key == key)
                .cloned())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|graph| graph.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn update(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|graph| graph.id == lifecycle.id) {
                *existing = lifecycle.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|graph| graph.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAgentRepo {
        items: Mutex<Vec<LifecycleAgent>>,
    }
    #[async_trait::async_trait]
    impl LifecycleAgentRepository for InMemoryAgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(agent.clone());
            Ok(())
        }
        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|a| a.id == id)
                .cloned())
        }
        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.run_id == run_id)
                .cloned()
                .collect())
        }
        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|a| a.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryFrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }
    #[async_trait::async_trait]
    impl AgentFrameRepository for InMemoryFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(frame.clone());
            Ok(())
        }
        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }
        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            let items = self.items.lock().unwrap();
            let mut frames: Vec<_> = items.iter().filter(|f| f.agent_id == agent_id).collect();
            frames.sort_by_key(|f| f.revision);
            Ok(frames.last().cloned().cloned())
        }
        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|f| f.agent_id == agent_id)
                .cloned()
                .collect())
        }
    }

    #[async_trait::async_trait]
    impl agent_frame_materialization_port::AgentRunFrameConstructionPort for InMemoryFrameRepo {
        async fn execute_frame_construction_command(
            &self,
            command: agent_frame_materialization_port::FrameConstructionCommand,
        ) -> Result<
            agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome,
            agent_frame_materialization_port::AgentRunFrameSurfaceError,
        > {
            let agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
                agent_id,
                runtime_session_id,
                created_by_id,
                ..
            } = command
            else {
                return Err(
                    agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                        message: "test frame repo only supports DispatchLaunchAnchor".to_string(),
                    },
                );
            };
            let next_revision = self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .map(|frame| frame.revision)
                .max()
                .unwrap_or(0)
                + 1;
            let mut frame = AgentFrame::new_revision(agent_id, next_revision, "frame_construction");
            frame.created_by_id = created_by_id;
            self.create(&frame).await.map_err(|error| {
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;
            let mut outcome =
                agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome::new(
                    agent_frame_materialization_port::AgentFrameWriteRole::FrameConstruction,
                );
            outcome.frame_id = Some(frame.id);
            outcome.agent_id = Some(frame.agent_id);
            outcome.runtime_session_id = Some(runtime_session_id);
            outcome.wrote_frame_revision = true;
            Ok(outcome)
        }
    }

    #[derive(Default)]
    struct InMemoryAssociationRepo {
        items: Mutex<Vec<LifecycleSubjectAssociation>>,
    }
    #[async_trait::async_trait]
    impl LifecycleSubjectAssociationRepository for InMemoryAssociationRepo {
        async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(assoc.clone());
            Ok(())
        }
        async fn list_by_subject(
            &self,
            subject: &SubjectRef,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.subject_kind == subject.kind && a.subject_id == subject.id)
                .cloned()
                .collect())
        }
        async fn list_by_anchor(
            &self,
            run_id: Uuid,
            agent_id: Option<Uuid>,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.anchor_run_id == run_id && a.anchor_agent_id == agent_id)
                .cloned()
                .collect())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|a| a.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryGateRepo {
        items: Mutex<Vec<LifecycleGate>>,
    }
    #[async_trait::async_trait]
    impl LifecycleGateRepository for InMemoryGateRepo {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(gate.clone());
            Ok(())
        }
        async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|g| g.id == id)
                .cloned())
        }
        async fn list_open_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|g| g.agent_id == Some(agent_id) && g.is_open())
                .cloned()
                .collect())
        }

        async fn find_by_agent_and_correlation(
            &self,
            agent_id: Uuid,
            correlation_id: &str,
        ) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|gate| {
                    gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id
                })
                .cloned())
        }

        async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|g| g.id == gate.id) {
                *existing = gate.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryLineageRepo {
        items: Mutex<Vec<AgentLineage>>,
    }
    #[async_trait::async_trait]
    impl AgentLineageRepository for InMemoryLineageRepo {
        async fn create(&self, lineage: &AgentLineage) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(lineage.clone());
            Ok(())
        }
        async fn list_children(&self, agent_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.parent_agent_id == Some(agent_id))
                .cloned()
                .collect())
        }
        async fn find_parent(
            &self,
            child_agent_id: Uuid,
        ) -> Result<Option<AgentLineage>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|l| l.child_agent_id == child_agent_id)
                .cloned())
        }
        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.run_id == run_id)
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct InMemoryRuntimeSessionCreator {
        items: Mutex<Vec<Uuid>>,
    }

    #[async_trait::async_trait]
    impl runtime_session_delivery_port::RuntimeSessionCreationPort for InMemoryRuntimeSessionCreator {
        async fn create_runtime_session(
            &self,
            _request: runtime_session_delivery_port::RuntimeSessionCreationRequest,
        ) -> Result<
            runtime_session_delivery_port::RuntimeSessionCreationResult,
            runtime_session_delivery_port::RuntimeSessionDeliveryError,
        > {
            let session_id = Uuid::new_v4();
            self.items.lock().unwrap().push(session_id);
            Ok(
                runtime_session_delivery_port::RuntimeSessionCreationResult {
                    runtime_session_id: session_id,
                },
            )
        }
    }

    #[derive(Default)]
    struct InMemoryExecutionAnchorRepo {
        items: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }
    #[async_trait::async_trait]
    impl RuntimeSessionExecutionAnchorRepository for InMemoryExecutionAnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items
                .iter()
                .find(|item| item.runtime_session_id == anchor.runtime_session_id)
            {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            items.push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.items
                .lock()
                .unwrap()
                .retain(|item| item.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| runtime_session_ids.contains(&item.runtime_session_id))
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct InMemoryDeliveryBindingRepo {
        items: Mutex<Vec<AgentRunDeliveryBinding>>,
    }

    #[async_trait::async_trait]
    impl AgentRunDeliveryBindingRepository for InMemoryDeliveryBindingRepo {
        async fn upsert(&self, binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items
                .iter_mut()
                .find(|item| item.run_id == binding.run_id && item.agent_id == binding.agent_id)
            {
                *existing = binding.clone();
            } else {
                items.push(binding.clone());
            }
            Ok(())
        }

        async fn get_current(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.run_id == run_id && item.agent_id == agent_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.items
                .lock()
                .unwrap()
                .retain(|item| item.runtime_session_id != runtime_session_id);
            Ok(())
        }
    }

    // ─── Helper ──────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn make_service<'a>(
        run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a InMemoryFrameRepo,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
        runtime_session_creator: &'a dyn runtime_session_delivery_port::RuntimeSessionCreationPort,
    ) -> LifecycleDispatchService<'a> {
        LifecycleDispatchService::new(
            run_repo,
            workflow_graph_repo,
            agent_repo,
            frame_repo,
            association_repo,
            gate_repo,
            lineage_repo,
        )
        .with_runtime_session_creator(runtime_session_creator)
        .with_frame_construction_port(frame_repo)
    }

    fn test_workflow_graph_ref(project_id: Uuid) -> WorkflowGraphRef {
        WorkflowGraphRef::ByKey {
            project_id,
            key: TEST_WORKFLOW_GRAPH_KEY.to_string(),
        }
    }

    fn new_project_agent_intent(project_id: Uuid) -> AgentLaunchIntent {
        AgentLaunchIntent {
            project_id,
            source: ExecutionSource::ProjectAgent,
            created_by_user_id: None,
            subject_ref: Some(SubjectRef::new("project", project_id)),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
        }
    }

    fn new_story_root_intent(project_id: Uuid, story_id: Uuid) -> AgentLaunchIntent {
        AgentLaunchIntent {
            project_id,
            source: ExecutionSource::User,
            created_by_user_id: None,
            subject_ref: Some(SubjectRef::new("story", story_id)),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
        }
    }

    fn new_task_execution_intent(project_id: Uuid, task_id: Uuid) -> SubjectExecutionIntent {
        SubjectExecutionIntent {
            project_id,
            source: ExecutionSource::User,
            created_by_user_id: None,
            subject_ref: SubjectRef::new("task", task_id),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
        }
    }

    fn seed_test_workflow_graph(
        repo: &InMemoryWorkflowGraphRepo,
        project_id: Uuid,
    ) -> WorkflowGraph {
        let graph =
            build_test_workflow_graph(project_id, TEST_WORKFLOW_GRAPH_KEY, TEST_ACTIVITY_KEY);
        repo.items.lock().unwrap().push(graph.clone());
        graph
    }

    fn build_test_workflow_graph(project_id: Uuid, key: &str, activity_key: &str) -> WorkflowGraph {
        WorkflowGraph::new(WorkflowGraphDraft {
            project_id,
            key: key.to_string(),
            name: key.to_string(),
            description: "test workflow graph".to_string(),
            source: DefinitionSource::UserAuthored,
            entry_activity_key: activity_key.to_string(),
            activities: vec![ActivityDefinition {
                key: activity_key.to_string(),
                description: "test activity".to_string(),
                executor: ActivityExecutorSpec::Agent(
                    AgentActivityExecutorSpec::continue_current_agent(TEST_AGENT_PROCEDURE_KEY),
                ),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: ActivityCompletionPolicy::OpenEnded,
                iteration_policy: ActivityIterationPolicy::default(),
                join_policy: ActivityJoinPolicy::All,
            }],
            transitions: Vec::new(),
        })
        .expect("test workflow graph")
    }

    // Tests

    #[tokio::test]
    async fn agent_launch_creates_plain_surface_without_orchestration_binding() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let anchor_repo = InMemoryExecutionAnchorRepo::default();
        let delivery_binding_repo = InMemoryDeliveryBindingRepo::default();
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        )
        .with_anchor_repo(&anchor_repo)
        .with_delivery_binding_repo(&delivery_binding_repo);

        let result = service
            .launch_agent(&new_project_agent_intent(project_id))
            .await
            .expect("dispatch");

        let runs = run_repo.items.lock().unwrap().clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].topology, LifecycleRunTopology::Plain);
        assert!(runs[0].orchestrations.is_empty());
        assert_eq!(result.runtime_refs.orchestration_ref(), None);
        assert_eq!(agent_repo.items.lock().unwrap().len(), 1);
        let frames = frame_repo.items.lock().unwrap().clone();
        assert_eq!(frames.len(), 1);
        assert_eq!(runtime_session_creator.items.lock().unwrap().len(), 1);
        assert_eq!(assoc_repo.items.lock().unwrap().len(), 1);
        assert!(result.delivery_runtime_ref.is_some());
        let anchors = anchor_repo.items.lock().unwrap().clone();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].orchestration_id, None);
        let bindings = delivery_binding_repo.items.lock().unwrap().clone();
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].runtime_session_id,
            anchors[0].runtime_session_id
        );
        assert_eq!(bindings[0].status, DeliveryBindingStatus::Ready);
    }

    #[tokio::test]
    async fn story_root_launch_creates_agent_scoped_story_association() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let result = service
            .launch_agent(&new_story_root_intent(project_id, story_id))
            .await
            .expect("story launch dispatch");

        let associations = assoc_repo.items.lock().unwrap().clone();
        assert_eq!(associations.len(), 1);
        assert_eq!(associations[0].subject_kind, "story");
        assert_eq!(associations[0].subject_id, story_id);
        assert_eq!(associations[0].anchor_run_id, result.runtime_refs.run_ref);
        assert_eq!(
            associations[0].anchor_agent_id,
            Some(result.runtime_refs.agent_ref)
        );
        assert!(associations[0].is_agent_scoped());
        assert!(result.delivery_runtime_ref.is_some());
    }

    #[tokio::test]
    async fn subject_execution_initializes_orchestration_node_and_anchor_binding() {
        let project_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let anchor_repo = InMemoryExecutionAnchorRepo::default();
        let delivery_binding_repo = InMemoryDeliveryBindingRepo::default();
        seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        )
        .with_anchor_repo(&anchor_repo)
        .with_delivery_binding_repo(&delivery_binding_repo);

        let mut intent = new_task_execution_intent(project_id, task_id);
        intent.workflow_graph_ref = Some(test_workflow_graph_ref(project_id));
        let result = service.execute_subject(&intent).await.expect("dispatch");

        let runs = run_repo.items.lock().unwrap().clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].orchestrations.len(), 1);
        let orchestration = &runs[0].orchestrations[0];
        assert_eq!(
            result.runtime_refs.orchestration_ref(),
            Some(orchestration.orchestration_id)
        );
        assert_eq!(result.runtime_refs.node_path(), Some(TEST_ACTIVITY_KEY));
        assert_eq!(result.runtime_refs.node_attempt(), Some(1));
        assert_eq!(orchestration.status, OrchestrationStatus::Running);
        assert_eq!(orchestration.node_tree.len(), 1);
        assert_eq!(orchestration.node_tree[0].node_path, TEST_ACTIVITY_KEY);
        assert!(orchestration.activation.ready_node_ids.is_empty());
        assert!(orchestration.dispatch.ready_node_ids.is_empty());
        let session_id = result.delivery_runtime_ref.expect("runtime session");
        assert_eq!(
            orchestration.node_tree[0].status,
            RuntimeNodeStatus::Claiming
        );
        assert_eq!(orchestration.node_tree[0].executor_run_ref, None);
        assert!(orchestration.node_tree[0].trace_refs.is_empty());
        assert!(orchestration.node_tree[0].started_at.is_none());

        let frames = frame_repo.items.lock().unwrap().clone();
        assert_eq!(frames.len(), 1);

        let anchors = anchor_repo.items.lock().unwrap().clone();
        assert_eq!(anchors.len(), 1);
        assert_eq!(
            anchors[0].orchestration_id,
            Some(orchestration.orchestration_id)
        );
        assert_eq!(anchors[0].node_path.as_deref(), Some(TEST_ACTIVITY_KEY));
        assert_eq!(anchors[0].node_attempt, Some(1));
        let bindings = delivery_binding_repo.items.lock().unwrap().clone();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].runtime_session_id, session_id.to_string());
        assert_eq!(bindings[0].node_path.as_deref(), Some(TEST_ACTIVITY_KEY));
        assert_eq!(bindings[0].node_attempt, Some(1));
        assert_eq!(result.subject_execution_ref.subject_ref.kind, "task");
        assert_eq!(result.subject_execution_ref.subject_ref.id, task_id);
    }

    #[tokio::test]
    async fn dispatch_resolves_workflow_graph_by_key_inside_service() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let mut intent = new_project_agent_intent(project_id);
        intent.workflow_graph_ref = Some(WorkflowGraphRef::ByKey {
            project_id,
            key: TEST_WORKFLOW_GRAPH_KEY.to_string(),
        });

        let result = service.launch_agent(&intent).await.expect("dispatch");

        let runs = run_repo.items.lock().unwrap().clone();
        assert_eq!(runs[0].orchestrations.len(), 1);
        let orchestration = &runs[0].orchestrations[0];
        assert_eq!(orchestration.role, ROOT_ORCHESTRATION_ROLE);
        assert_eq!(orchestration.status, OrchestrationStatus::Running);
        assert_eq!(
            result.runtime_refs.orchestration_ref(),
            Some(orchestration.orchestration_id)
        );
        assert!(matches!(
            orchestration.source_ref,
            OrchestrationSourceRef::WorkflowGraph {
                graph_id,
                graph_version: Some(1),
            } if graph_id == workflow_graph.id
        ));
        assert!(orchestration.activation.ready_node_ids.is_empty());
        assert!(orchestration.dispatch.ready_node_ids.is_empty());
        assert!(result.delivery_runtime_ref.is_some());
        assert_eq!(
            orchestration.node_tree[0].status,
            RuntimeNodeStatus::Claiming
        );
        assert_eq!(orchestration.node_tree[0].executor_run_ref, None);
        assert!(orchestration.node_tree[0].trace_refs.is_empty());
    }

    #[tokio::test]
    async fn lifecycle_run_start_intent_initializes_root_orchestration_state() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let result = service
            .start_lifecycle_run(&LifecycleRunStartIntent {
                project_id,
                source: ExecutionSource::Api,
                workflow_graph_ref: test_workflow_graph_ref(project_id),
            })
            .await
            .expect("start lifecycle run");

        let runs = run_repo.items.lock().unwrap().clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, result.run_ref);
        assert_eq!(runs[0].orchestrations.len(), 1);
        let orchestration = &runs[0].orchestrations[0];
        assert_eq!(orchestration.orchestration_id, result.orchestration_ref);
        assert_eq!(orchestration.role, "root");
        assert_eq!(orchestration.status, OrchestrationStatus::Running);
        assert!(matches!(
            orchestration.source_ref,
            OrchestrationSourceRef::WorkflowGraph {
                graph_id,
                graph_version: Some(1),
            } if graph_id == workflow_graph.id
        ));
        assert!(
            orchestration
                .plan_snapshot
                .plan_digest
                .starts_with("sha256:")
        );
        assert_eq!(
            orchestration.plan_snapshot.entry_node_ids,
            vec![workflow_graph.entry_activity_key.clone()]
        );
        assert_eq!(
            orchestration.dispatch.ready_node_ids,
            vec![workflow_graph.entry_activity_key.clone()]
        );
        assert_eq!(orchestration.node_tree.len(), 1);
        assert_eq!(
            orchestration.node_tree[0].node_id,
            workflow_graph.entry_activity_key
        );
        assert_eq!(orchestration.node_tree[0].kind, PlanNodeKind::AgentCall);
        assert_eq!(orchestration.node_tree[0].status, RuntimeNodeStatus::Ready);
        assert_eq!(orchestration.node_tree[0].executor_run_ref, None);
        assert!(orchestration.node_tree[0].trace_refs.is_empty());
        assert!(agent_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn lifecycle_run_start_rejects_blocking_compiler_diagnostics_without_creating_run() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let mut workflow_graph =
            build_test_workflow_graph(project_id, TEST_WORKFLOW_GRAPH_KEY, TEST_ACTIVITY_KEY);
        workflow_graph.activities[0].executor =
            ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                procedure_key: TEST_AGENT_PROCEDURE_KEY.to_string(),
                agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
                runtime_session_policy: RuntimeSessionPolicy::DeliverToCurrentTrace,
            });
        workflow_repo.items.lock().unwrap().push(workflow_graph);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let err = service
            .start_lifecycle_run(&LifecycleRunStartIntent {
                project_id,
                source: ExecutionSource::Api,
                workflow_graph_ref: test_workflow_graph_ref(project_id),
            })
            .await
            .expect_err("blocking compiler diagnostics should fail");

        assert!(matches!(err, WorkflowApplicationError::BadRequest(_)));
        assert!(
            err.to_string()
                .contains("unsupported_agent_executor_policy")
        );
        assert!(run_repo.items.lock().unwrap().is_empty());
        assert!(agent_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reuse_existing_with_parent_agent_id_resumes_explicit_agent() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        seed_test_workflow_graph(&workflow_repo, project_id);
        let existing_run = LifecycleRun::new_control(project_id);
        let first_agent =
            LifecycleAgent::new_root(existing_run.id, project_id, AgentSource::Routine);
        let target_agent =
            LifecycleAgent::new_root(existing_run.id, project_id, AgentSource::Routine);
        run_repo.items.lock().unwrap().push(existing_run.clone());
        agent_repo.items.lock().unwrap().push(first_agent.clone());
        agent_repo.items.lock().unwrap().push(target_agent.clone());
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let mut intent = new_task_execution_intent(project_id, Uuid::new_v4());
        intent.parent_run_id = Some(existing_run.id);
        intent.parent_agent_id = Some(target_agent.id);
        intent.run_policy = RunPolicy::ReuseExisting;
        intent.agent_policy = AgentPolicy::Resume;

        let result = service.execute_subject(&intent).await.expect("dispatch");

        assert_eq!(result.runtime_refs.agent_ref, target_agent.id);
        let frames = frame_repo.items.lock().unwrap().clone();
        let updated_target = frames
            .iter()
            .filter(|frame| frame.agent_id == target_agent.id)
            .max_by_key(|frame| frame.revision)
            .expect("target frame");
        assert_eq!(updated_target.id, result.runtime_refs.frame_ref);
        assert!(frames.iter().all(|frame| frame.agent_id != first_agent.id));
    }
}
