use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityBindingRefs, AgentAssignment, AgentFrame, AgentLaunchDispatchResult, AgentLaunchIntent,
    AgentLineage, AgentPolicy, AgentRuntimeRefs, ExecutionDispatchResult, ExecutionIntent,
    ExecutionSource, GatePolicy, InteractionDispatchIntent, InteractionGateOpenedDispatchResult,
    LifecycleAgent, LifecycleGate, LifecycleRun, LifecycleRunStartDispatchResult,
    LifecycleRunStartIntent, LifecycleSubjectAssociation, RunPolicy, RuntimePolicy,
    RuntimeSessionExecutionAnchor, SubjectExecutionDispatchResult, SubjectExecutionIntent,
    SubjectExecutionRef, SubjectRef, WorkflowGraph, WorkflowGraphInstance, WorkflowGraphRef,
};
use agentdash_domain::workflow::{
    AgentAssignmentRepository, AgentFrameRepository, AgentLineageRepository,
    LifecycleAgentRepository, LifecycleGateRepository, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, RuntimeSessionExecutionAnchorRepository,
    WorkflowGraphInstanceRepository, WorkflowGraphRepository,
};

use super::LifecycleEngine;
use super::WorkflowApplicationError;
use super::frame_builder::AgentFrameBuilder;
use super::graph_resolver::WorkflowGraphResolver;
use crate::session::{ExecutionStatus, SessionMeta, SessionPersistence, TitleSource};

#[derive(Debug, Clone)]
pub struct RuntimeSessionCreationRequest {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub source: ExecutionSource,
}

#[async_trait]
pub trait RuntimeSessionCreator: Send + Sync {
    async fn create_runtime_session(
        &self,
        request: RuntimeSessionCreationRequest,
    ) -> Result<Uuid, WorkflowApplicationError>;
}

#[derive(Clone)]
pub struct SessionPersistenceRuntimeSessionCreator {
    persistence: Arc<dyn SessionPersistence>,
}

impl SessionPersistenceRuntimeSessionCreator {
    pub fn new(persistence: Arc<dyn SessionPersistence>) -> Self {
        Self { persistence }
    }
}

#[async_trait]
impl RuntimeSessionCreator for SessionPersistenceRuntimeSessionCreator {
    async fn create_runtime_session(
        &self,
        request: RuntimeSessionCreationRequest,
    ) -> Result<Uuid, WorkflowApplicationError> {
        let session_id = Uuid::new_v4();
        let now = chrono::Utc::now().timestamp_millis();
        let meta = SessionMeta {
            id: session_id.to_string(),
            title: runtime_session_title(&request),
            title_source: TitleSource::Auto,
            created_at: now,
            updated_at: now,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: None,
        };
        self.persistence
            .create_session(&meta)
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!("RuntimeSession 创建失败: {error}"))
            })?;
        Ok(session_id)
    }
}

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
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
    agent_repo: &'a dyn LifecycleAgentRepository,
    frame_repo: &'a dyn AgentFrameRepository,
    assignment_repo: &'a dyn AgentAssignmentRepository,
    association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    gate_repo: &'a dyn LifecycleGateRepository,
    lineage_repo: &'a dyn AgentLineageRepository,
    anchor_repo: Option<&'a dyn RuntimeSessionExecutionAnchorRepository>,
    runtime_session_creator: Option<&'a dyn RuntimeSessionCreator>,
}

#[derive(Debug, Clone)]
struct DispatchPlan {
    project_id: Uuid,
    source: ExecutionSource,
    subject_ref: Option<SubjectRef>,
    parent_run_id: Option<Uuid>,
    parent_agent_id: Option<Uuid>,
    workflow_graph_ref: Option<WorkflowGraphRef>,
    run_policy: RunPolicy,
    agent_policy: AgentPolicy,
    runtime_policy: RuntimePolicy,
    gate_policy: Option<GatePolicy>,
    bind_entry_assignment: bool,
}

struct DispatchFacts {
    run: LifecycleRun,
    graph_instance: Option<WorkflowGraphInstance>,
    agent: LifecycleAgent,
    frame: AgentFrame,
    runtime_session_ref: Option<Uuid>,
    assignment: Option<AgentAssignment>,
    gate_ref: Option<Uuid>,
    subject_execution_ref: Option<SubjectExecutionRef>,
}

impl DispatchFacts {
    fn runtime_refs(&self) -> AgentRuntimeRefs {
        AgentRuntimeRefs::new(
            self.run.id,
            self.agent.id,
            self.frame.id,
            self.graph_instance.as_ref().map(|instance| {
                ActivityBindingRefs::new(
                    instance.id,
                    self.assignment.as_ref().map(|assignment| assignment.id),
                )
            }),
        )
    }
}

impl From<&AgentLaunchIntent> for DispatchPlan {
    fn from(intent: &AgentLaunchIntent) -> Self {
        Self {
            project_id: intent.project_id,
            source: intent.source.clone(),
            subject_ref: intent.subject_ref.clone(),
            parent_run_id: intent.parent_run_id,
            parent_agent_id: intent.parent_agent_id,
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: intent.run_policy.clone(),
            agent_policy: intent.agent_policy.clone(),
            runtime_policy: intent.runtime_policy.clone(),
            gate_policy: None,
            bind_entry_assignment: false,
        }
    }
}

impl From<&SubjectExecutionIntent> for DispatchPlan {
    fn from(intent: &SubjectExecutionIntent) -> Self {
        Self {
            project_id: intent.project_id,
            source: intent.source.clone(),
            subject_ref: Some(intent.subject_ref.clone()),
            parent_run_id: intent.parent_run_id,
            parent_agent_id: intent.parent_agent_id,
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: intent.run_policy.clone(),
            agent_policy: intent.agent_policy.clone(),
            runtime_policy: intent.runtime_policy.clone(),
            gate_policy: None,
            bind_entry_assignment: true,
        }
    }
}

impl From<&InteractionDispatchIntent> for DispatchPlan {
    fn from(intent: &InteractionDispatchIntent) -> Self {
        Self {
            project_id: intent.project_id,
            source: intent.source.clone(),
            subject_ref: None,
            parent_run_id: Some(intent.parent_run_id),
            parent_agent_id: Some(intent.parent_agent_id),
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: RunPolicy::AppendGraph,
            agent_policy: AgentPolicy::SpawnChild,
            runtime_policy: intent.runtime_policy.clone(),
            gate_policy: Some(intent.gate_policy.clone()),
            bind_entry_assignment: true,
        }
    }
}

impl<'a> LifecycleDispatchService<'a> {
    pub fn new(
        run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        assignment_repo: &'a dyn AgentAssignmentRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
    ) -> Self {
        Self {
            run_repo,
            workflow_graph_repo,
            graph_instance_repo,
            agent_repo,
            frame_repo,
            assignment_repo,
            association_repo,
            gate_repo,
            lineage_repo,
            anchor_repo: None,
            runtime_session_creator: None,
        }
    }

    pub fn with_anchor_repo(
        mut self,
        repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    ) -> Self {
        self.anchor_repo = Some(repo);
        self
    }

    pub fn with_runtime_session_creator(mut self, creator: &'a dyn RuntimeSessionCreator) -> Self {
        self.runtime_session_creator = Some(creator);
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
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        Ok(AgentLaunchDispatchResult {
            runtime_refs: facts.runtime_refs(),
            delivery_runtime_ref: facts.runtime_session_ref,
        })
    }

    pub async fn execute_subject(
        &self,
        intent: &SubjectExecutionIntent,
    ) -> Result<SubjectExecutionDispatchResult, WorkflowApplicationError> {
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        let runtime_refs = facts.runtime_refs();
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
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        let gate_ref = facts.gate_ref.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "InteractionDispatchIntent 未创建 LifecycleGate".to_string(),
            )
        })?;
        Ok(InteractionGateOpenedDispatchResult {
            runtime_refs: facts.runtime_refs(),
            gate_ref,
            delivery_runtime_ref: facts.runtime_session_ref,
        })
    }

    pub async fn start_lifecycle_run(
        &self,
        intent: &LifecycleRunStartIntent,
    ) -> Result<LifecycleRunStartDispatchResult, WorkflowApplicationError> {
        let workflow_graph = WorkflowGraphResolver::new(self.workflow_graph_repo)
            .resolve(intent.project_id, &intent.workflow_graph_ref)
            .await?
            .graph;
        let mut run = create_lifecycle_run(intent.project_id, workflow_graph.id);
        self.run_repo.create(&run).await?;

        let mut graph_instance = WorkflowGraphInstance::new_root(run.id, workflow_graph.id);
        let state = LifecycleEngine::initialize(&workflow_graph, graph_instance.id)
            .map_err(|error| WorkflowApplicationError::BadRequest(error.to_string()))?;
        graph_instance
            .replace_activity_state(state)
            .map_err(WorkflowApplicationError::BadRequest)?;
        self.graph_instance_repo.create(&graph_instance).await?;
        run.sync_graph_instance_activity_projections(
            graph_instance
                .activity_state
                .as_ref()
                .map(|state| (graph_instance.id, state))
                .into_iter(),
        );
        self.run_repo.update(&run).await?;

        Ok(LifecycleRunStartDispatchResult {
            run_ref: run.id,
            graph_instance_ref: graph_instance.id,
        })
    }

    async fn dispatch_common(
        &self,
        plan: DispatchPlan,
    ) -> Result<DispatchFacts, WorkflowApplicationError> {
        if plan.workflow_graph_ref.is_none() {
            return self.dispatch_graphless(plan).await;
        }

        let workflow_graph_ref = plan
            .workflow_graph_ref
            .as_ref()
            .expect("checked workflow graph ref");
        let workflow_graph = WorkflowGraphResolver::new(self.workflow_graph_repo)
            .resolve(plan.project_id, workflow_graph_ref)
            .await?
            .graph;
        let mut run = self.resolve_or_create_run(&plan, &workflow_graph).await?;
        let graph_instance = self
            .resolve_or_create_graph_instance(&run, &workflow_graph, &plan)
            .await?;
        let workflow_graph = self
            .align_workflow_graph_with_instance(&plan, workflow_graph, &graph_instance)
            .await?;
        let graph_instance = self
            .ensure_graph_instance_activity_state(&mut run, graph_instance, &workflow_graph)
            .await?;
        let agent = self.resolve_or_create_agent(&run, &plan).await?;
        let association = if let Some(subject_ref) = &plan.subject_ref {
            Some(
                self.create_subject_association(run.id, agent.id, subject_ref, &plan.source)
                    .await?,
            )
        } else {
            None
        };
        let runtime_session_ref = self
            .resolve_or_create_runtime_session(&plan, &run, &agent)
            .await?;
        let frame = self
            .create_initial_frame(
                &agent,
                &graph_instance,
                &workflow_graph,
                runtime_session_ref,
                plan.bind_entry_assignment,
            )
            .await?;
        let mut agent = agent;
        agent.set_current_frame(frame.id);
        self.agent_repo.update(&agent).await?;

        if let Some(parent_agent_id) = plan.parent_agent_id {
            let lineage = AgentLineage::new(
                run.id,
                Some(parent_agent_id),
                agent.id,
                lineage_relation_kind(&plan.agent_policy),
                Some(frame.id),
                None,
            );
            self.lineage_repo.create(&lineage).await?;
        }

        let gate_ref = if let Some(gate_policy) = &plan.gate_policy {
            Some(self.create_gate(&run, &agent, &frame, gate_policy).await?)
        } else {
            None
        };
        // ── 第一段 anchor 写入：frame 已创建，assignment 尚未创建 ──
        if let (Some(anchor_repo), Some(session_id)) = (self.anchor_repo, runtime_session_ref) {
            let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                session_id.to_string(),
                run.id,
                frame.id,
                agent.id,
                Some(graph_instance.id),
                frame
                    .activity_key
                    .clone()
                    .or_else(|| Some(workflow_graph.entry_activity_key.clone())),
            );
            anchor_repo.upsert(&anchor).await?;
        }

        let assignment = if plan.bind_entry_assignment {
            Some(
                self.resolve_or_create_entry_assignment(
                    &run,
                    &graph_instance,
                    &workflow_graph,
                    &agent,
                    &frame,
                )
                .await?,
            )
        } else {
            None
        };

        // ── 第二段 anchor 补写：assignment 创建后回填 ──
        if let (Some(anchor_repo), Some(session_id), Some(assignment)) =
            (self.anchor_repo, runtime_session_ref, &assignment)
        {
            anchor_repo
                .update_assignment(&session_id.to_string(), assignment.id, assignment.attempt)
                .await?;
        }

        let subject_execution_ref = association.as_ref().map(|assoc| SubjectExecutionRef {
            subject_ref: plan
                .subject_ref
                .clone()
                .expect("association requires subject"),
            association_id: assoc.id,
        });

        Ok(DispatchFacts {
            run,
            graph_instance: Some(graph_instance),
            agent,
            frame,
            runtime_session_ref,
            assignment,
            gate_ref,
            subject_execution_ref,
        })
    }

    async fn dispatch_graphless(
        &self,
        plan: DispatchPlan,
    ) -> Result<DispatchFacts, WorkflowApplicationError> {
        let run = self.resolve_or_create_graphless_run(&plan).await?;
        let agent = self.resolve_or_create_agent(&run, &plan).await?;
        let association = if let Some(subject_ref) = &plan.subject_ref {
            Some(
                self.create_subject_association(run.id, agent.id, subject_ref, &plan.source)
                    .await?,
            )
        } else {
            None
        };
        let runtime_session_ref = self
            .resolve_or_create_runtime_session(&plan, &run, &agent)
            .await?;
        let frame = self
            .create_graphless_initial_frame(&agent, runtime_session_ref)
            .await?;
        let mut agent = agent;
        agent.set_current_frame(frame.id);
        self.agent_repo.update(&agent).await?;

        if let Some(parent_agent_id) = plan.parent_agent_id {
            let lineage = AgentLineage::new(
                run.id,
                Some(parent_agent_id),
                agent.id,
                lineage_relation_kind(&plan.agent_policy),
                Some(frame.id),
                None,
            );
            self.lineage_repo.create(&lineage).await?;
        }

        let gate_ref = if let Some(gate_policy) = &plan.gate_policy {
            Some(self.create_gate(&run, &agent, &frame, gate_policy).await?)
        } else {
            None
        };

        if let (Some(anchor_repo), Some(session_id)) = (self.anchor_repo, runtime_session_ref) {
            let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                session_id.to_string(),
                run.id,
                frame.id,
                agent.id,
                None,
                None,
            );
            anchor_repo.upsert(&anchor).await?;
        }

        let subject_execution_ref = association.as_ref().map(|assoc| SubjectExecutionRef {
            subject_ref: plan
                .subject_ref
                .clone()
                .expect("association requires subject"),
            association_id: assoc.id,
        });

        Ok(DispatchFacts {
            run,
            graph_instance: None,
            agent,
            frame,
            runtime_session_ref,
            assignment: None,
            gate_ref,
            subject_execution_ref,
        })
    }

    // ─── Run Resolution ──────────────────────────────────────────────────

    async fn resolve_or_create_run(
        &self,
        plan: &DispatchPlan,
        workflow_graph: &WorkflowGraph,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        match (&plan.run_policy, plan.parent_run_id) {
            // same-run: 复用现有 run 或追加 graph
            (RunPolicy::ReuseExisting | RunPolicy::AppendGraph, Some(run_id)) => {
                let run = self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
                    WorkflowApplicationError::BadRequest(format!("parent_run_id {run_id} 不存在"))
                })?;
                Ok(run)
            }
            (RunPolicy::ReuseExisting, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::ReuseExisting 需要 parent_run_id".to_string(),
            )),
            (RunPolicy::AppendGraph, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::AppendGraph 需要 parent_run_id".to_string(),
            )),
            // 创建新 run
            _ => {
                let run = create_lifecycle_run(plan.project_id, workflow_graph.id);
                self.run_repo.create(&run).await?;
                Ok(run)
            }
        }
    }

    async fn resolve_or_create_graphless_run(
        &self,
        plan: &DispatchPlan,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        match (&plan.run_policy, plan.parent_run_id) {
            (RunPolicy::ReuseExisting | RunPolicy::AppendGraph, Some(run_id)) => {
                let run = self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
                    WorkflowApplicationError::BadRequest(format!("parent_run_id {run_id} 不存在"))
                })?;
                Ok(run)
            }
            (RunPolicy::ReuseExisting, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::ReuseExisting 需要 parent_run_id".to_string(),
            )),
            (RunPolicy::AppendGraph, None) => Err(WorkflowApplicationError::BadRequest(
                "RunPolicy::AppendGraph 需要 parent_run_id".to_string(),
            )),
            _ => {
                let run = LifecycleRun::new_graphless(plan.project_id);
                self.run_repo.create(&run).await?;
                Ok(run)
            }
        }
    }

    // ─── Graph Instance Resolution ───────────────────────────────────────

    async fn resolve_or_create_graph_instance(
        &self,
        run: &LifecycleRun,
        workflow_graph: &WorkflowGraph,
        plan: &DispatchPlan,
    ) -> Result<WorkflowGraphInstance, WorkflowApplicationError> {
        match plan.run_policy {
            RunPolicy::ReuseExisting => {
                // 尝试复用现有 root graph instance
                let instances = self.graph_instance_repo.list_by_run(run.id).await?;
                if let Some(existing) = instances.into_iter().find(|gi| gi.is_root()) {
                    return Ok(existing);
                }
                let instance = WorkflowGraphInstance::new_root(run.id, workflow_graph.id);
                self.graph_instance_repo.create(&instance).await?;
                Ok(instance)
            }
            RunPolicy::AppendGraph => {
                let role = graph_instance_role_from_source(&plan.source);
                let instance = WorkflowGraphInstance::new(run.id, workflow_graph.id, role);
                self.graph_instance_repo.create(&instance).await?;
                Ok(instance)
            }
            RunPolicy::CreateLinkedRun => {
                // 新 run 创建 root graph instance
                let instance = WorkflowGraphInstance::new_root(run.id, workflow_graph.id);
                self.graph_instance_repo.create(&instance).await?;
                Ok(instance)
            }
        }
    }

    async fn ensure_graph_instance_activity_state(
        &self,
        run: &mut LifecycleRun,
        mut graph_instance: WorkflowGraphInstance,
        workflow_graph: &WorkflowGraph,
    ) -> Result<WorkflowGraphInstance, WorkflowApplicationError> {
        if graph_instance.activity_state.is_none() {
            let state = LifecycleEngine::initialize(workflow_graph, graph_instance.id)
                .map_err(|error| WorkflowApplicationError::BadRequest(error.to_string()))?;
            graph_instance
                .replace_activity_state(state)
                .map_err(WorkflowApplicationError::BadRequest)?;
            self.graph_instance_repo.update(&graph_instance).await?;
        }

        let graph_instances = self.graph_instance_repo.list_by_run(run.id).await?;
        run.sync_graph_instance_activity_projections(graph_instances.iter().filter_map(
            |instance| {
                instance
                    .activity_state
                    .as_ref()
                    .map(|state| (instance.id, state))
            },
        ));
        self.run_repo.update(run).await?;
        Ok(graph_instance)
    }

    // ─── Subject Association ─────────────────────────────────────────────

    async fn create_subject_association(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        subject_ref: &SubjectRef,
        source: &ExecutionSource,
    ) -> Result<LifecycleSubjectAssociation, WorkflowApplicationError> {
        let role = association_role_from_source(source);
        let assoc = if matches!(subject_ref.kind.as_str(), "task" | "story") {
            LifecycleSubjectAssociation::new_agent_scoped(run_id, agent_id, subject_ref, role, None)
        } else {
            LifecycleSubjectAssociation::new_run_scoped(run_id, subject_ref, role, None)
        };
        self.association_repo.create(&assoc).await?;
        Ok(assoc)
    }

    // ─── Agent Resolution ────────────────────────────────────────────────

    async fn resolve_or_create_agent(
        &self,
        run: &LifecycleRun,
        plan: &DispatchPlan,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        match plan.agent_policy {
            AgentPolicy::Reuse | AgentPolicy::Resume => {
                if let Some(agent_id) = plan.parent_agent_id {
                    return self.resolve_explicit_reuse_agent(run, plan, agent_id).await;
                }
                let agents = self.agent_repo.list_by_run(run.id).await?;
                if let Some(existing) = agents.into_iter().find(|a| a.status == "active") {
                    return Ok(existing);
                }
                Ok(self.create_agent(run, plan).await?)
            }
            AgentPolicy::Create | AgentPolicy::SpawnChild => {
                Ok(self.create_agent(run, plan).await?)
            }
        }
    }

    async fn resolve_explicit_reuse_agent(
        &self,
        run: &LifecycleRun,
        plan: &DispatchPlan,
        agent_id: Uuid,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        let agent = self.agent_repo.get(agent_id).await?.ok_or_else(|| {
            WorkflowApplicationError::BadRequest(format!("parent_agent_id {agent_id} 不存在"))
        })?;
        if agent.run_id != run.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "parent_agent_id {} 属于 run {}，不能复用到 run {}",
                agent.id, agent.run_id, run.id
            )));
        }
        if agent.project_id != plan.project_id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "parent_agent_id {} 属于 project {}，不能复用到 project {}",
                agent.id, agent.project_id, plan.project_id
            )));
        }
        if agent.status != "active" {
            return Err(WorkflowApplicationError::Conflict(format!(
                "parent_agent_id {} 当前不是 active",
                agent.id
            )));
        }
        Ok(agent)
    }

    async fn create_agent(
        &self,
        run: &LifecycleRun,
        plan: &DispatchPlan,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        let agent_kind = agent_kind_from_source(&plan.source);
        let agent = LifecycleAgent::new_root(run.id, plan.project_id, agent_kind);
        self.agent_repo.create(&agent).await?;
        Ok(agent)
    }

    // ─── Frame Creation ──────────────────────────────────────────────────

    async fn create_initial_frame(
        &self,
        agent: &LifecycleAgent,
        graph_instance: &WorkflowGraphInstance,
        workflow_graph: &WorkflowGraph,
        runtime_session_ref: Option<Uuid>,
        bind_entry_activity: bool,
    ) -> Result<AgentFrame, WorkflowApplicationError> {
        let mut builder = AgentFrameBuilder::new(agent.id).with_created_by("dispatch", None);
        if bind_entry_activity {
            builder = builder
                .with_graph_instance(graph_instance.id, workflow_graph.entry_activity_key.clone());
        }
        if let Some(session_id) = runtime_session_ref {
            builder = builder.with_runtime_session(session_id.to_string());
        }
        let frame = builder.build(self.frame_repo).await?;
        Ok(frame)
    }

    async fn create_graphless_initial_frame(
        &self,
        agent: &LifecycleAgent,
        runtime_session_ref: Option<Uuid>,
    ) -> Result<AgentFrame, WorkflowApplicationError> {
        let mut builder = AgentFrameBuilder::new(agent.id).with_created_by("dispatch", None);
        if let Some(session_id) = runtime_session_ref {
            builder = builder.with_runtime_session(session_id.to_string());
        }
        let frame = builder.build(self.frame_repo).await?;
        Ok(frame)
    }

    async fn resolve_or_create_entry_assignment(
        &self,
        run: &LifecycleRun,
        graph_instance: &WorkflowGraphInstance,
        workflow_graph: &WorkflowGraph,
        agent: &LifecycleAgent,
        frame: &AgentFrame,
    ) -> Result<AgentAssignment, WorkflowApplicationError> {
        let activity_key = workflow_graph.entry_activity_key.as_str();
        if !workflow_graph
            .activities
            .iter()
            .any(|activity| activity.key == activity_key)
        {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "workflow_graph {} 缺少 entry activity `{activity_key}`",
                workflow_graph.id
            )));
        }

        if let Some(existing) = self
            .assignment_repo
            .find_for_attempt(graph_instance.id, activity_key, 1)
            .await?
        {
            if existing.lease_status == "active" {
                if existing.agent_id != agent.id {
                    return Err(WorkflowApplicationError::Conflict(format!(
                        "graph entry assignment {} 已绑定 agent {}，不能绑定 agent {}",
                        existing.id, existing.agent_id, agent.id
                    )));
                }
                return Ok(existing);
            }
        }

        let next_attempt = self
            .assignment_repo
            .list_by_run(run.id)
            .await?
            .into_iter()
            .filter(|assignment| {
                assignment.graph_instance_id == graph_instance.id
                    && assignment.activity_key == activity_key
            })
            .map(|assignment| assignment.attempt)
            .max()
            .unwrap_or(0)
            + 1;

        let assignment = AgentAssignment::new(
            run.id,
            graph_instance.id,
            activity_key.to_string(),
            next_attempt,
            agent.id,
            frame.id,
        );
        self.assignment_repo.create(&assignment).await?;
        Ok(assignment)
    }

    async fn align_workflow_graph_with_instance(
        &self,
        plan: &DispatchPlan,
        requested_graph: WorkflowGraph,
        graph_instance: &WorkflowGraphInstance,
    ) -> Result<WorkflowGraph, WorkflowApplicationError> {
        if graph_instance.graph_id == requested_graph.id {
            return Ok(requested_graph);
        }

        return Err(WorkflowApplicationError::Conflict(format!(
            "workflow_graph_ref {:?} 与复用的 graph_instance {} graph_id {} 不一致",
            plan.workflow_graph_ref, graph_instance.id, graph_instance.graph_id
        )));
    }

    async fn resolve_or_create_runtime_session(
        &self,
        plan: &DispatchPlan,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
    ) -> Result<Option<Uuid>, WorkflowApplicationError> {
        match plan.runtime_policy {
            RuntimePolicy::AttachExisting(id) | RuntimePolicy::ContinueCurrent(id) => Ok(Some(id)),
            RuntimePolicy::CreateRuntimeSession => {
                let creator = self.runtime_session_creator.ok_or_else(|| {
                    WorkflowApplicationError::Internal(
                        "RuntimePolicy::CreateRuntimeSession 缺少 RuntimeSessionCreator"
                            .to_string(),
                    )
                })?;
                let request = RuntimeSessionCreationRequest {
                    project_id: plan.project_id,
                    run_id: run.id,
                    agent_id: agent.id,
                    source: plan.source.clone(),
                };
                Ok(Some(creator.create_runtime_session(request).await?))
            }
        }
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
        self.gate_repo.create(&gate).await?;
        Ok(gate_id)
    }
}

// ─── Helper Functions ────────────────────────────────────────────────────────

fn create_lifecycle_run(project_id: Uuid, root_graph_id: Uuid) -> LifecycleRun {
    LifecycleRun::new_control(project_id, root_graph_id)
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
        ExecutionSource::Routine => "source",
        ExecutionSource::Migration => "lineage",
        _ => "subject",
    }
}

fn runtime_session_title(request: &RuntimeSessionCreationRequest) -> String {
    let kind_label = match &request.source {
        ExecutionSource::User | ExecutionSource::ProjectAgent | ExecutionSource::Api => "新会话",
        ExecutionSource::Routine => "定时任务",
        ExecutionSource::ParentAgent => "子任务",
        ExecutionSource::Migration => "迁移",
    };
    let now = chrono::Local::now().format("%m/%d %H:%M");
    format!("{kind_label} · {now}")
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
        async fn list_by_root_graph(
            &self,
            _root_graph_id: Uuid,
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
        async fn get_by_run_and_id(
            &self,
            run_id: Uuid,
            id: Uuid,
        ) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|instance| instance.run_id == run_id && instance.id == id)
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
        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAssignmentRepo {
        items: Mutex<Vec<AgentAssignment>>,
    }
    #[async_trait::async_trait]
    impl AgentAssignmentRepository for InMemoryAssignmentRepo {
        async fn create(&self, assignment: &AgentAssignment) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(assignment.clone());
            Ok(())
        }
        async fn get(&self, assignment_id: Uuid) -> Result<Option<AgentAssignment>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|assignment| assignment.id == assignment_id)
                .cloned())
        }
        async fn find_for_attempt(
            &self,
            graph_instance_id: Uuid,
            activity_key: &str,
            attempt: i32,
        ) -> Result<Option<AgentAssignment>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|assignment| {
                    assignment.graph_instance_id == graph_instance_id
                        && assignment.activity_key == activity_key
                        && assignment.attempt == attempt
                })
                .cloned())
        }
        async fn find_active_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<AgentAssignment>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.agent_id == agent_id && a.lease_status == "active")
                .cloned()
                .collect())
        }
        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentAssignment>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|assignment| assignment.run_id == run_id)
                .cloned()
                .collect())
        }
        async fn update(&self, assignment: &AgentAssignment) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|item| item.id == assignment.id) {
                *existing = assignment.clone();
            }
            Ok(())
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

    #[derive(Default)]
    struct InMemoryRuntimeSessionCreator {
        items: Mutex<Vec<Uuid>>,
    }

    #[async_trait::async_trait]
    impl RuntimeSessionCreator for InMemoryRuntimeSessionCreator {
        async fn create_runtime_session(
            &self,
            _request: RuntimeSessionCreationRequest,
        ) -> Result<Uuid, WorkflowApplicationError> {
            let session_id = Uuid::new_v4();
            self.items.lock().unwrap().push(session_id);
            Ok(session_id)
        }
    }

    #[derive(Default)]
    struct InMemoryExecutionAnchorRepo {
        items: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }
    #[async_trait::async_trait]
    impl RuntimeSessionExecutionAnchorRepository for InMemoryExecutionAnchorRepo {
        async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items
                .iter_mut()
                .find(|item| item.runtime_session_id == anchor.runtime_session_id)
            {
                *existing = anchor.clone();
            } else {
                items.push(anchor.clone());
            }
            Ok(())
        }

        async fn update_assignment(
            &self,
            runtime_session_id: &str,
            assignment_id: Uuid,
            attempt: i32,
        ) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            let anchor = items
                .iter_mut()
                .find(|item| item.runtime_session_id == runtime_session_id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "runtime_session_execution_anchor",
                    id: runtime_session_id.to_string(),
                })?;
            anchor.fill_assignment(assignment_id, attempt);
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

        async fn latest_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.agent_id == agent_id)
                .max_by_key(|item| item.updated_at)
                .cloned())
        }
    }

    // ─── Helper ──────────────────────────────────────────────────────────

    fn make_service<'a>(
        run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        assignment_repo: &'a dyn AgentAssignmentRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
        runtime_session_creator: &'a dyn RuntimeSessionCreator,
    ) -> LifecycleDispatchService<'a> {
        LifecycleDispatchService::new(
            run_repo,
            workflow_graph_repo,
            graph_instance_repo,
            agent_repo,
            frame_repo,
            assignment_repo,
            association_repo,
            gate_repo,
            lineage_repo,
        )
        .with_runtime_session_creator(runtime_session_creator)
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
        }
    }

    fn new_story_root_intent(project_id: Uuid, story_id: Uuid) -> AgentLaunchIntent {
        AgentLaunchIntent {
            project_id,
            source: ExecutionSource::User,
            subject_ref: Some(SubjectRef::new("story", story_id)),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            agent_procedure_ref: None,
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
            subject_ref: SubjectRef::new("task", task_id),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            agent_procedure_ref: None,
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

    fn seed_custom_graph(
        repo: &InMemoryWorkflowGraphRepo,
        project_id: Uuid,
        key: &str,
        activity_key: &str,
    ) -> WorkflowGraph {
        let graph = build_test_workflow_graph(project_id, key, activity_key);
        repo.items.lock().unwrap().push(graph.clone());
        graph
    }

    fn build_test_workflow_graph(project_id: Uuid, key: &str, activity_key: &str) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            key,
            key,
            "test workflow graph",
            DefinitionSource::UserAuthored,
            activity_key,
            vec![ActivityDefinition {
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
            Vec::new(),
        )
        .expect("test workflow graph")
    }

    // ─── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn agent_launch_creates_surface_without_activity_assignment() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let intent = new_project_agent_intent(project_id);
        let result = service.launch_agent(&intent).await.expect("dispatch");

        let runs = run_repo.items.lock().unwrap().clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].topology, LifecycleRunTopology::Graphless);
        assert_eq!(runs[0].root_graph_id, None);
        let instances = gi_repo.items.lock().unwrap().clone();
        assert!(instances.is_empty());
        assert_eq!(result.runtime_refs.graph_instance_ref(), None);
        assert_eq!(agent_repo.items.lock().unwrap().len(), 1);
        let frames = frame_repo.items.lock().unwrap().clone();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].graph_instance_id, None);
        assert_eq!(frames[0].activity_key, None);
        let assignments = assignment_repo.items.lock().unwrap().clone();
        assert!(assignments.is_empty());
        assert_eq!(runtime_session_creator.items.lock().unwrap().len(), 1);
        assert_eq!(assoc_repo.items.lock().unwrap().len(), 1);
        assert!(result.delivery_runtime_ref.is_some());
    }

    #[tokio::test]
    async fn story_root_launch_creates_agent_scoped_story_association() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
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
        assert!(assignment_repo.items.lock().unwrap().is_empty());
        assert!(result.delivery_runtime_ref.is_some());
    }

    #[tokio::test]
    async fn subject_execution_initializes_activity_state_and_entry_assignment() {
        let project_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let mut intent = new_task_execution_intent(project_id, task_id);
        intent.workflow_graph_ref = Some(test_workflow_graph_ref(project_id));
        let result = service.execute_subject(&intent).await.expect("dispatch");

        let instances = gi_repo.items.lock().unwrap().clone();
        assert_eq!(instances.len(), 1);
        let state = instances[0]
            .activity_state
            .as_ref()
            .expect("activity state");
        let entry_attempt = state
            .attempts
            .iter()
            .find(|attempt| attempt.activity_key == workflow_graph.entry_activity_key)
            .expect("entry attempt");
        assert_eq!(entry_attempt.attempt, 1);
        let frames = frame_repo.items.lock().unwrap().clone();
        assert_eq!(
            frames[0].graph_instance_id,
            result.runtime_refs.graph_instance_ref()
        );
        assert_eq!(
            frames[0].activity_key.as_deref(),
            Some(workflow_graph.entry_activity_key.as_str())
        );
        let assignments = assignment_repo.items.lock().unwrap().clone();
        assert_eq!(assignments.len(), 1);
        assert_eq!(
            result.runtime_refs.assignment_ref(),
            Some(assignments[0].id)
        );
        assert_eq!(assignments[0].frame_id, result.runtime_refs.frame_ref);
        assert_eq!(
            Some(assignments[0].graph_instance_id),
            result.runtime_refs.graph_instance_ref()
        );
        assert_eq!(
            assignments[0].activity_key,
            workflow_graph.entry_activity_key
        );
        assert_eq!(assignments[0].attempt, entry_attempt.attempt as i32);
        assert_eq!(result.subject_execution_ref.subject_ref.kind, "task");
        assert_eq!(result.subject_execution_ref.subject_ref.id, task_id);
        assert_eq!(runtime_session_creator.items.lock().unwrap().len(), 1);
        assert_eq!(assoc_repo.items.lock().unwrap().len(), 1);
        assert!(result.delivery_runtime_ref.is_some());
    }

    #[tokio::test]
    async fn dispatch_resolves_workflow_graph_by_key_inside_service() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
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
        let instances = gi_repo.items.lock().unwrap().clone();
        assert_eq!(runs[0].root_graph_id, Some(workflow_graph.id));
        assert_eq!(instances[0].graph_id, workflow_graph.id);
        assert_eq!(
            instances[0]
                .activity_state
                .as_ref()
                .expect("activity state")
                .graph_instance_id,
            result
                .runtime_refs
                .graph_instance_ref()
                .expect("graph instance ref")
        );
        assert!(assignment_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn lifecycle_run_start_intent_initializes_root_graph_instance_state() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
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
        let instances = gi_repo.items.lock().unwrap().clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(instances.len(), 1);
        assert_eq!(runs[0].id, result.run_ref);
        assert_eq!(instances[0].id, result.graph_instance_ref);
        assert_eq!(instances[0].run_id, result.run_ref);
        assert_eq!(instances[0].graph_id, workflow_graph.id);
        assert_eq!(
            instances[0]
                .activity_state
                .as_ref()
                .expect("activity state")
                .graph_instance_id,
            result.graph_instance_ref
        );
        let active_attempts = instances[0]
            .activity_state
            .as_ref()
            .expect("activity state")
            .attempts
            .iter()
            .filter(|attempt| attempt.status == ActivityAttemptStatus::Ready)
            .collect::<Vec<_>>();
        assert_eq!(active_attempts.len(), 1);
        assert_eq!(
            active_attempts[0].activity_key,
            workflow_graph.entry_activity_key
        );
        assert!(agent_repo.items.lock().unwrap().is_empty());
        assert!(assignment_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_rejects_unknown_workflow_graph_key_without_creating_run() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let mut intent = new_project_agent_intent(project_id);
        intent.workflow_graph_ref = Some(WorkflowGraphRef::ByKey {
            project_id,
            key: "missing.lifecycle".to_string(),
        });

        let err = service
            .launch_agent(&intent)
            .await
            .expect_err("unknown graph key should fail");

        assert!(matches!(err, WorkflowApplicationError::NotFound(_)));
        assert!(run_repo.items.lock().unwrap().is_empty());
        assert!(gi_repo.items.lock().unwrap().is_empty());
        assert!(assignment_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reuse_existing_with_matching_graph_ref_uses_existing_graph_instance_definition() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let anchor_repo = InMemoryExecutionAnchorRepo::default();
        seed_test_workflow_graph(&workflow_repo, project_id);
        let custom_graph = seed_custom_graph(
            &workflow_repo,
            project_id,
            "custom.lifecycle",
            "custom_main",
        );
        let existing_run = create_lifecycle_run(project_id, custom_graph.id);
        let existing_instance = WorkflowGraphInstance::new_root(existing_run.id, custom_graph.id);
        run_repo.items.lock().unwrap().push(existing_run.clone());
        gi_repo
            .items
            .lock()
            .unwrap()
            .push(existing_instance.clone());
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        )
        .with_anchor_repo(&anchor_repo);

        let mut intent = new_task_execution_intent(project_id, Uuid::new_v4());
        intent.parent_run_id = Some(existing_run.id);
        intent.workflow_graph_ref = Some(WorkflowGraphRef::ByKey {
            project_id,
            key: "custom.lifecycle".to_string(),
        });
        intent.run_policy = RunPolicy::ReuseExisting;
        intent.agent_policy = AgentPolicy::Create;

        let result = service.execute_subject(&intent).await.expect("dispatch");

        assert_eq!(
            result.runtime_refs.graph_instance_ref(),
            Some(existing_instance.id)
        );
        let frames = frame_repo.items.lock().unwrap().clone();
        let assignments = assignment_repo.items.lock().unwrap().clone();
        assert_eq!(frames[0].activity_key.as_deref(), Some("custom_main"));
        assert_eq!(assignments[0].activity_key, "custom_main");
        assert_eq!(assignments[0].graph_instance_id, existing_instance.id);
        let anchors = anchor_repo.items.lock().unwrap().clone();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].activity_key.as_deref(), Some("custom_main"));
        assert_eq!(anchors[0].assignment_id, Some(assignments[0].id));
    }

    #[tokio::test]
    async fn reuse_existing_with_parent_agent_id_resumes_explicit_agent() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let existing_run = create_lifecycle_run(project_id, workflow_graph.id);
        let existing_instance = WorkflowGraphInstance::new_root(existing_run.id, workflow_graph.id);
        let first_agent = LifecycleAgent::new_root(existing_run.id, project_id, "routine");
        let target_agent = LifecycleAgent::new_root(existing_run.id, project_id, "routine");
        run_repo.items.lock().unwrap().push(existing_run.clone());
        gi_repo.items.lock().unwrap().push(existing_instance);
        agent_repo.items.lock().unwrap().push(first_agent.clone());
        agent_repo.items.lock().unwrap().push(target_agent.clone());
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
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
        let agents = agent_repo.items.lock().unwrap().clone();
        let updated_target = agents
            .iter()
            .find(|agent| agent.id == target_agent.id)
            .expect("target agent");
        assert_eq!(
            updated_target.current_frame_id,
            Some(result.runtime_refs.frame_ref)
        );
        let first = agents
            .iter()
            .find(|agent| agent.id == first_agent.id)
            .expect("first agent");
        assert_eq!(first.current_frame_id, None);
    }

    #[tokio::test]
    async fn reuse_existing_without_parent_run_id_is_rejected() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let mut intent = new_task_execution_intent(project_id, Uuid::new_v4());
        intent.run_policy = RunPolicy::ReuseExisting;
        intent.agent_policy = AgentPolicy::Resume;

        let err = service
            .execute_subject(&intent)
            .await
            .expect_err("reuse requires parent run");

        assert!(matches!(err, WorkflowApplicationError::BadRequest(_)));
        assert!(run_repo.items.lock().unwrap().is_empty());
        assert!(gi_repo.items.lock().unwrap().is_empty());
        assert!(agent_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reuse_existing_rejects_explicit_graph_ref_mismatch() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        seed_test_workflow_graph(&workflow_repo, project_id);
        let custom_graph = seed_custom_graph(
            &workflow_repo,
            project_id,
            "custom.lifecycle",
            "custom_main",
        );
        let existing_run = create_lifecycle_run(project_id, custom_graph.id);
        let existing_instance = WorkflowGraphInstance::new_root(existing_run.id, custom_graph.id);
        run_repo.items.lock().unwrap().push(existing_run.clone());
        gi_repo.items.lock().unwrap().push(existing_instance);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let mut intent = new_project_agent_intent(project_id);
        intent.parent_run_id = Some(existing_run.id);
        intent.workflow_graph_ref = Some(WorkflowGraphRef::ByKey {
            project_id,
            key: TEST_WORKFLOW_GRAPH_KEY.to_string(),
        });
        intent.run_policy = RunPolicy::ReuseExisting;

        let err = service
            .launch_agent(&intent)
            .await
            .expect_err("graph mismatch should fail");

        assert!(matches!(err, WorkflowApplicationError::Conflict(_)));
        assert!(assignment_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn agent_launch_append_graph_keeps_activity_assignment_out_of_launch_result() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let existing_run = create_lifecycle_run(project_id, workflow_graph.id);
        run_repo.items.lock().unwrap().push(existing_run.clone());

        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let intent = AgentLaunchIntent {
            project_id,
            source: ExecutionSource::ParentAgent,
            subject_ref: None,
            parent_run_id: Some(existing_run.id),
            parent_agent_id: None,
            workflow_graph_ref: Some(test_workflow_graph_ref(project_id)),
            agent_procedure_ref: None,
            run_policy: RunPolicy::AppendGraph,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Inherit,
            capability_policy: CapabilityPolicy::InheritedSlice,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
        };

        let result = service.launch_agent(&intent).await.expect("dispatch");

        // 没有新建 run
        assert_eq!(run_repo.items.lock().unwrap().len(), 1);
        assert_eq!(result.runtime_refs.run_ref, existing_run.id);
        // 新建了一个 graph instance（role=task_execution for ParentAgent）
        let instances = gi_repo.items.lock().unwrap().clone();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].role, "task_execution");
        assert_eq!(instances[0].graph_id, workflow_graph.id);
        assert!(
            instances[0].activity_state.is_some(),
            "dispatch 创建的 graph instance 必须可调度"
        );
        let frames = frame_repo.items.lock().unwrap().clone();
        assert_eq!(frames[0].graph_instance_id, None);
        assert_eq!(frames[0].activity_key, None);
        assert!(assignment_repo.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_with_gate_policy_creates_gate() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let existing_run = create_lifecycle_run(project_id, workflow_graph.id);
        run_repo.items.lock().unwrap().push(existing_run.clone());
        let intent = InteractionDispatchIntent {
            project_id,
            source: ExecutionSource::ParentAgent,
            parent_run_id: existing_run.id,
            parent_agent_id: Uuid::new_v4(),
            workflow_graph_ref: Some(test_workflow_graph_ref(project_id)),
            agent_procedure_ref: None,
            context_policy: ContextPolicy::Slice,
            capability_policy: CapabilityPolicy::InheritedSlice,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
            gate_policy: GatePolicy {
                gate_kind: "human_review".to_string(),
                correlation_id: Some("test-corr".to_string()),
                payload: None,
            },
        };

        let result = service
            .open_interaction_gate(&intent)
            .await
            .expect("dispatch");

        let gates = gate_repo.items.lock().unwrap().clone();
        assert_eq!(gates.len(), 1);
        assert_eq!(result.gate_ref, gates[0].id);
        assert_eq!(gates[0].gate_kind, "human_review");
        assert_eq!(gates[0].correlation_id, "test-corr");
        let instances = gi_repo.items.lock().unwrap().clone();
        assert!(
            instances[0].activity_state.is_some(),
            "interaction child graph instance 必须拥有 Activity state"
        );
        let assignments = assignment_repo.items.lock().unwrap().clone();
        assert_eq!(assignments.len(), 1);
        assert_eq!(
            Some(assignments[0].id),
            result.runtime_refs.assignment_ref()
        );
        assert_eq!(
            Some(assignments[0].graph_instance_id),
            result.runtime_refs.graph_instance_ref()
        );
    }

    #[tokio::test]
    async fn dispatch_with_parent_agent_creates_lineage() {
        let project_id = Uuid::new_v4();
        let run_repo = InMemoryRunRepo::default();
        let workflow_repo = InMemoryWorkflowGraphRepo::default();
        let gi_repo = InMemoryGraphInstanceRepo::default();
        let agent_repo = InMemoryAgentRepo::default();
        let frame_repo = InMemoryFrameRepo::default();
        let assignment_repo = InMemoryAssignmentRepo::default();
        let assoc_repo = InMemoryAssociationRepo::default();
        let gate_repo = InMemoryGateRepo::default();
        let lineage_repo = InMemoryLineageRepo::default();
        let runtime_session_creator = InMemoryRuntimeSessionCreator::default();
        seed_test_workflow_graph(&workflow_repo, project_id);
        let service = make_service(
            &run_repo,
            &workflow_repo,
            &gi_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
            &assoc_repo,
            &gate_repo,
            &lineage_repo,
            &runtime_session_creator,
        );

        let parent_agent_id = Uuid::new_v4();
        let mut intent = new_project_agent_intent(project_id);
        intent.parent_agent_id = Some(parent_agent_id);
        intent.agent_policy = AgentPolicy::SpawnChild;

        let result = service.launch_agent(&intent).await.expect("dispatch");

        let lineages = lineage_repo.items.lock().unwrap().clone();
        assert_eq!(lineages.len(), 1);
        assert_eq!(lineages[0].parent_agent_id, Some(parent_agent_id));
        assert_eq!(lineages[0].child_agent_id, result.runtime_refs.agent_ref);
        assert_eq!(lineages[0].relation_kind, "spawn");
    }
}
