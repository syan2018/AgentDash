use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentFrame, AgentLineage, AgentPolicy, ExecutionDispatchResult, ExecutionIntent,
    ExecutionSource, GatePolicy, LifecycleAgent, LifecycleGate, LifecycleRun,
    LifecycleSubjectAssociation, RunPolicy, RuntimePolicy, SubjectExecutionRef, SubjectRef,
    WorkflowGraphInstance, WorkflowGraphRef,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, LifecycleAgentRepository,
    LifecycleGateRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    WorkflowGraphInstanceRepository,
};

use super::frame_builder::AgentFrameBuilder;
use super::WorkflowApplicationError;

/// 业务执行进入控制面的统一入口 service。
///
/// 接收 `ExecutionIntent`，根据 policy 决定：
/// - 复用 / 创建 LifecycleRun
/// - 创建 / 复用 WorkflowGraphInstance
/// - 创建 LifecycleSubjectAssociation（如果有 subject_ref）
/// - 创建 / 复用 LifecycleAgent
/// - 创建 AgentFrame initial revision
/// - 按需创建 LifecycleGate / AgentLineage
///
/// **不拥有** AgentFrame 内部构造细节（T4 AgentFrameBuilder 的工作）
/// 和 connector launch（T4 的工作）。
pub struct LifecycleDispatchService<'a> {
    run_repo: &'a dyn LifecycleRunRepository,
    graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
    agent_repo: &'a dyn LifecycleAgentRepository,
    frame_repo: &'a dyn AgentFrameRepository,
    association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    gate_repo: &'a dyn LifecycleGateRepository,
    lineage_repo: &'a dyn AgentLineageRepository,
}

impl<'a> LifecycleDispatchService<'a> {
    pub fn new(
        run_repo: &'a dyn LifecycleRunRepository,
        graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
    ) -> Self {
        Self {
            run_repo,
            graph_instance_repo,
            agent_repo,
            frame_repo,
            association_repo,
            gate_repo,
            lineage_repo,
        }
    }

    /// 按 ExecutionIntent 的 policy 编排所有目标锚点。
    pub async fn dispatch(
        &self,
        intent: &ExecutionIntent,
    ) -> Result<ExecutionDispatchResult, WorkflowApplicationError> {
        // 1. 解析目标 graph_id（简化：直接从 ref 取 ID）
        let graph_id = resolve_graph_id(&intent.workflow_graph_ref)?;

        // 2. 按 run_policy 选择或创建 LifecycleRun
        let run = self.resolve_or_create_run(intent).await?;

        // 3. 创建或复用 WorkflowGraphInstance
        let graph_instance = self
            .resolve_or_create_graph_instance(&run, graph_id, intent)
            .await?;

        // 4. 创建 LifecycleSubjectAssociation（如果有 subject_ref）
        let association = if let Some(subject_ref) = &intent.subject_ref {
            Some(
                self.create_subject_association(run.id, subject_ref, &intent.source)
                    .await?,
            )
        } else {
            None
        };

        // 5. 创建或复用 LifecycleAgent
        let agent = self.resolve_or_create_agent(&run, intent).await?;

        // 6. 创建 AgentFrame initial revision
        let runtime_session_ref = resolve_runtime_session_ref(&intent.runtime_policy);
        let frame = self.create_initial_frame(&agent, runtime_session_ref).await?;

        // 7. 更新 agent.current_frame_id
        let mut agent = agent;
        agent.set_current_frame(frame.id);
        self.agent_repo
            .update(&agent)
            .await
?;

        // 8. 按需创建 AgentLineage
        if let Some(parent_agent_id) = intent.parent_agent_id {
            let lineage = AgentLineage::new(
                run.id,
                Some(parent_agent_id),
                agent.id,
                lineage_relation_kind(&intent.agent_policy),
                Some(frame.id),
                None,
            );
            self.lineage_repo.create(&lineage).await?;
        }

        // 9. 按需创建 LifecycleGate
        let gate_ref = if let Some(gate_policy) = &intent.gate_policy {
            Some(self.create_gate(&run, &agent, &frame, gate_policy).await?)
        } else {
            None
        };

        // 10. 组装结果
        let subject_execution_ref = association.as_ref().map(|assoc| SubjectExecutionRef {
            subject_ref: intent.subject_ref.clone().unwrap(),
            association_id: assoc.id,
        });

        Ok(ExecutionDispatchResult {
            run_ref: run.id,
            graph_instance_ref: graph_instance.id,
            agent_ref: agent.id,
            frame_ref: frame.id,
            runtime_session_ref: resolve_runtime_session_ref(&intent.runtime_policy),
            assignment_ref: None,
            gate_ref,
            subject_execution_ref,
            trace_ref: None,
        })
    }

    // ─── Run Resolution ──────────────────────────────────────────────────

    async fn resolve_or_create_run(
        &self,
        intent: &ExecutionIntent,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        match (&intent.run_policy, intent.parent_run_id) {
            // same-run: 复用现有 run 或追加 graph
            (RunPolicy::ReuseExisting | RunPolicy::AppendGraph, Some(run_id)) => {
                let run = self
                    .run_repo
                    .get_by_id(run_id)
                    .await
                    ?
                    .ok_or_else(|| {
                        WorkflowApplicationError::BadRequest(format!(
                            "parent_run_id {run_id} 不存在"
                        ))
                    })?;
                Ok(run)
            }
            // 创建新 run
            _ => {
                let graph_id =
                    resolve_graph_id(&intent.workflow_graph_ref)?.unwrap_or(Uuid::nil());
                let run = create_lifecycle_run(intent.project_id, graph_id);
                self.run_repo
                    .create(&run)
                    .await
                    ?;
                Ok(run)
            }
        }
    }

    // ─── Graph Instance Resolution ───────────────────────────────────────

    async fn resolve_or_create_graph_instance(
        &self,
        run: &LifecycleRun,
        graph_id: Option<Uuid>,
        intent: &ExecutionIntent,
    ) -> Result<WorkflowGraphInstance, WorkflowApplicationError> {
        let effective_graph_id = graph_id.unwrap_or(run.lifecycle_id);

        match intent.run_policy {
            RunPolicy::ReuseExisting => {
                // 尝试复用现有 root graph instance
                let instances = self
                    .graph_instance_repo
                    .list_by_run(run.id)
                    .await
                    ?;
                if let Some(existing) = instances.into_iter().find(|gi| gi.is_root()) {
                    return Ok(existing);
                }
                let instance = WorkflowGraphInstance::new_root(run.id, effective_graph_id);
                self.graph_instance_repo
                    .create(&instance)
                    .await
                    ?;
                Ok(instance)
            }
            RunPolicy::AppendGraph => {
                let role = graph_instance_role_from_source(&intent.source);
                let instance = WorkflowGraphInstance::new(run.id, effective_graph_id, role);
                self.graph_instance_repo
                    .create(&instance)
                    .await
                    ?;
                Ok(instance)
            }
            RunPolicy::CreateLinkedRun => {
                // 新 run 创建 root graph instance
                let instance = WorkflowGraphInstance::new_root(run.id, effective_graph_id);
                self.graph_instance_repo
                    .create(&instance)
                    .await
                    ?;
                Ok(instance)
            }
        }
    }

    // ─── Subject Association ─────────────────────────────────────────────

    async fn create_subject_association(
        &self,
        run_id: Uuid,
        subject_ref: &SubjectRef,
        source: &ExecutionSource,
    ) -> Result<LifecycleSubjectAssociation, WorkflowApplicationError> {
        let role = association_role_from_source(source);
        let assoc = LifecycleSubjectAssociation::new_run_scoped(run_id, subject_ref, role, None);
        self.association_repo
            .create(&assoc)
            .await
            ?;
        Ok(assoc)
    }

    // ─── Agent Resolution ────────────────────────────────────────────────

    async fn resolve_or_create_agent(
        &self,
        run: &LifecycleRun,
        intent: &ExecutionIntent,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        match intent.agent_policy {
            AgentPolicy::Reuse | AgentPolicy::Resume => {
                let agents = self
                    .agent_repo
                    .list_by_run(run.id)
                    .await
                    ?;
                if let Some(existing) = agents.into_iter().find(|a| a.status == "active") {
                    return Ok(existing);
                }
                Ok(self.create_agent(run, intent).await?)
            }
            AgentPolicy::Create | AgentPolicy::SpawnChild => {
                Ok(self.create_agent(run, intent).await?)
            }
        }
    }

    async fn create_agent(
        &self,
        run: &LifecycleRun,
        intent: &ExecutionIntent,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        let agent_kind = agent_kind_from_source(&intent.source);
        let agent = LifecycleAgent::new_root(run.id, intent.project_id, agent_kind);
        self.agent_repo
            .create(&agent)
            .await
            ?;
        Ok(agent)
    }

    // ─── Frame Creation ──────────────────────────────────────────────────

    async fn create_initial_frame(
        &self,
        agent: &LifecycleAgent,
        runtime_session_ref: Option<Uuid>,
    ) -> Result<AgentFrame, WorkflowApplicationError> {
        let mut builder = AgentFrameBuilder::new(agent.id)
            .with_created_by("dispatch", None);
        if let Some(session_id) = runtime_session_ref {
            builder = builder.with_runtime_session(session_id);
        }
        let frame = builder.build(self.frame_repo).await?;
        Ok(frame)
    }

    // ─── Gate Creation ───────────────────────────────────────────────────

    async fn create_gate(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        frame: &AgentFrame,
        policy: &GatePolicy,
    ) -> Result<Uuid, WorkflowApplicationError> {
        let correlation = policy
            .correlation_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let gate = LifecycleGate::open(
            run.id,
            Some(agent.id),
            Some(frame.id),
            &policy.gate_kind,
            correlation,
            policy.payload.clone(),
        );
        let gate_id = gate.id;
        self.gate_repo
            .create(&gate)
            .await
            ?;
        Ok(gate_id)
    }
}

// ─── Helper Functions ────────────────────────────────────────────────────────

fn resolve_graph_id(
    graph_ref: &Option<WorkflowGraphRef>,
) -> Result<Option<Uuid>, WorkflowApplicationError> {
    match graph_ref {
        Some(WorkflowGraphRef::ById(id)) => Ok(Some(*id)),
        Some(WorkflowGraphRef::ByKey { .. }) => {
            // ByKey 解析需要 repository 查询；dispatch service 简化版先返回 None，
            // caller 应在构造 intent 时解析为 ID。
            Ok(None)
        }
        None => Ok(None),
    }
}

fn resolve_runtime_session_ref(policy: &RuntimePolicy) -> Option<Uuid> {
    match policy {
        RuntimePolicy::AttachExisting(id) | RuntimePolicy::ContinueCurrent(id) => Some(*id),
        RuntimePolicy::CreateRuntimeSession => None,
    }
}

fn create_lifecycle_run(project_id: Uuid, lifecycle_id: Uuid) -> LifecycleRun {
    let now = chrono::Utc::now();
    LifecycleRun {
        id: Uuid::new_v4(),
        project_id,
        lifecycle_id,
        session_id: None,
        status: agentdash_domain::workflow::LifecycleRunStatus::Ready,
        active_node_keys: Vec::new(),
        execution_log: Vec::new(),
        activity_state: None,
        created_at: now,
        updated_at: now,
        last_activity_at: now,
    }
}

fn graph_instance_role_from_source(source: &ExecutionSource) -> &'static str {
    match source {
        ExecutionSource::ParentAgent => "task_execution",
        ExecutionSource::Routine => "routine_execution",
        _ => "append",
    }
}

fn association_role_from_source(source: &ExecutionSource) -> &'static str {
    match source {
        ExecutionSource::User => "user_initiated",
        ExecutionSource::Routine => "routine_source",
        ExecutionSource::ParentAgent => "parent_delegated",
        ExecutionSource::ProjectAgent => "project_agent",
        ExecutionSource::Api => "api_triggered",
        ExecutionSource::Migration => "migration",
    }
}

fn agent_kind_from_source(source: &ExecutionSource) -> &'static str {
    match source {
        ExecutionSource::User | ExecutionSource::ProjectAgent | ExecutionSource::Api => {
            "project_agent"
        }
        ExecutionSource::Routine => "routine_agent",
        ExecutionSource::ParentAgent => "child_agent",
        ExecutionSource::Migration => "migration_agent",
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::*;

    use super::*;

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
        async fn list_by_lifecycle(
            &self,
            _lifecycle_id: Uuid,
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
    struct InMemoryGraphInstanceRepo {
        items: Mutex<Vec<WorkflowGraphInstance>>,
    }
    #[async_trait::async_trait]
    impl WorkflowGraphInstanceRepository for InMemoryGraphInstanceRepo {
        async fn create(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(instance.clone());
            Ok(())
        }
        async fn get(&self, id: Uuid) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|i| i.id == id)
                .cloned())
        }
        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|i| i.run_id == run_id)
                .cloned()
                .collect())
        }
        async fn update(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|i| i.id == instance.id) {
                *existing = instance.clone();
            }
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
        async fn find_by_runtime_session(
            &self,
            _runtime_session_id: &str,
        ) -> Result<Option<AgentFrame>, DomainError> {
            Ok(None)
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
    }

    // ─── Helper ──────────────────────────────────────────────────────────

    fn make_service<'a>(
        run_repo: &'a dyn LifecycleRunRepository,
        graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
    ) -> LifecycleDispatchService<'a> {
        LifecycleDispatchService::new(
            run_repo,
            graph_instance_repo,
            agent_repo,
            frame_repo,
            association_repo,
            gate_repo,
            lineage_repo,
        )
    }

    fn new_project_agent_intent(project_id: Uuid) -> ExecutionIntent {
        ExecutionIntent {
            project_id,
            source: ExecutionSource::ProjectAgent,
            subject_ref: Some(SubjectRef::new("project", project_id)),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            agent_procedure_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
            gate_policy: None,
        }
    }

    // ─── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dispatch_creates_new_run_with_graph_instance_agent_and_frame() {
        let run_repo = InMemoryRunRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let service = make_service(
            &run_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
        );

        let project_id = Uuid::new_v4();
        let intent = new_project_agent_intent(project_id);
        let result = service.dispatch(&intent).await.expect("dispatch");

        assert_eq!(run_repo.items.lock().unwrap().len(), 1);
        assert_eq!(gi_repo.items.lock().unwrap().len(), 1);
        assert!(gi_repo.items.lock().unwrap()[0].is_root());
        assert_eq!(agent_repo.items.lock().unwrap().len(), 1);
        assert_eq!(frame_repo.items.lock().unwrap().len(), 1);
        assert_eq!(assoc_repo.items.lock().unwrap().len(), 1);
        assert!(result.subject_execution_ref.is_some());
        assert!(result.gate_ref.is_none());
    }

    #[tokio::test]
    async fn append_graph_adds_to_existing_run() {
        let run_repo = InMemoryRunRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();

        let project_id = Uuid::new_v4();
        let existing_run = create_lifecycle_run(project_id, Uuid::new_v4());
        run_repo
            .items
            .lock()
            .unwrap()
            .push(existing_run.clone());

        let service = make_service(
            &run_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
        );

        let intent = ExecutionIntent {
            project_id,
            source: ExecutionSource::ParentAgent,
            subject_ref: None,
            parent_run_id: Some(existing_run.id),
            parent_agent_id: None,
            workflow_graph_ref: None,
            agent_procedure_ref: None,
            run_policy: RunPolicy::AppendGraph,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Inherit,
            capability_policy: CapabilityPolicy::InheritedSlice,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
            gate_policy: None,
        };

        let result = service.dispatch(&intent).await.expect("dispatch");

        // 没有新建 run
        assert_eq!(run_repo.items.lock().unwrap().len(), 1);
        assert_eq!(result.run_ref, existing_run.id);
        // 新建了一个 graph instance（role=task_execution for ParentAgent）
        let instances = gi_repo.items.lock().unwrap().clone();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].role, "task_execution");
    }

    #[tokio::test]
    async fn dispatch_with_gate_policy_creates_gate() {
        let run_repo = InMemoryRunRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let service = make_service(
            &run_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
        );

        let project_id = Uuid::new_v4();
        let mut intent = new_project_agent_intent(project_id);
        intent.gate_policy = Some(GatePolicy {
            gate_kind: "human_review".to_string(),
            correlation_id: Some("test-corr".to_string()),
            payload: None,
        });

        let result = service.dispatch(&intent).await.expect("dispatch");

        assert!(result.gate_ref.is_some());
        let gates = gate_repo.items.lock().unwrap().clone();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].gate_kind, "human_review");
        assert_eq!(gates[0].correlation_id, "test-corr");
    }

    #[tokio::test]
    async fn dispatch_with_parent_agent_creates_lineage() {
        let run_repo = InMemoryRunRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let service = make_service(
            &run_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
        );

        let project_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let mut intent = new_project_agent_intent(project_id);
        intent.parent_agent_id = Some(parent_agent_id);
        intent.agent_policy = AgentPolicy::SpawnChild;

        let result = service.dispatch(&intent).await.expect("dispatch");

        let lineages = lineage_repo.items.lock().unwrap().clone();
        assert_eq!(lineages.len(), 1);
        assert_eq!(lineages[0].parent_agent_id, Some(parent_agent_id));
        assert_eq!(lineages[0].child_agent_id, result.agent_ref);
        assert_eq!(lineages[0].relation_kind, "spawn");
    }
}
