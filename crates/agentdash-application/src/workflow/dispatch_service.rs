use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentFrame, AgentLaunchDispatchResult, AgentLaunchIntent, AgentLineage, AgentPolicy,
    AgentRuntimeRefs, ExecutionDispatchResult, ExecutionIntent, ExecutionSource, ExecutorRunRef,
    GatePolicy, InteractionDispatchIntent, InteractionGateOpenedDispatchResult, LifecycleAgent,
    LifecycleGate, LifecycleRun, LifecycleRunStartDispatchResult, LifecycleRunStartIntent,
    LifecycleSubjectAssociation, OrchestrationBindingRefs, OrchestrationInstance,
    OrchestrationPlanSnapshot, OrchestrationSourceRef, RunPolicy, RuntimePolicy,
    RuntimeSessionExecutionAnchor, SubjectExecutionDispatchResult, SubjectExecutionIntent,
    SubjectExecutionRef, SubjectRef, ValidationSeverity, WorkflowGraph, WorkflowGraphRef,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, LifecycleAgentRepository,
    LifecycleGateRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    RuntimeSessionExecutionAnchorRepository, WorkflowGraphRepository,
};

use super::WorkflowApplicationError;
use super::frame_builder::AgentFrameBuilder;
use super::graph_resolver::WorkflowGraphResolver;
use super::orchestration::{
    OrchestrationRuntimeEvent, ROOT_ORCHESTRATION_ROLE, WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION,
    WorkflowGraphCompileInput, WorkflowGraphCompileMode, WorkflowGraphCompileSourceMetadata,
    WorkflowGraphCompiler, activate_orchestration, apply_orchestration_event_to_run,
};
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
/// - 创建 / 复用 OrchestrationInstance
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
    agent_repo: &'a dyn LifecycleAgentRepository,
    frame_repo: &'a dyn AgentFrameRepository,
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
}

struct DispatchFacts {
    run: LifecycleRun,
    orchestration_binding: Option<OrchestrationBindingRefs>,
    agent: LifecycleAgent,
    frame: AgentFrame,
    runtime_session_ref: Option<Uuid>,
    gate_ref: Option<Uuid>,
    subject_execution_ref: Option<SubjectExecutionRef>,
}

impl DispatchFacts {
    fn runtime_refs(&self) -> AgentRuntimeRefs {
        AgentRuntimeRefs::new(
            self.run.id,
            self.agent.id,
            self.frame.id,
            self.orchestration_binding.clone(),
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
        }
    }
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
            frame_repo,
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
        let plan_snapshot = compile_static_graph_orchestration_plan(&workflow_graph)?;
        let mut run = create_lifecycle_run(intent.project_id, workflow_graph.id);
        let orchestration_binding = ensure_workflow_graph_orchestration(
            &mut run,
            &workflow_graph,
            ROOT_ORCHESTRATION_ROLE,
            plan_snapshot,
        )?;
        self.run_repo.create(&run).await?;

        Ok(LifecycleRunStartDispatchResult {
            run_ref: run.id,
            orchestration_ref: orchestration_binding.orchestration_ref,
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
        let plan_snapshot = compile_static_graph_orchestration_plan(&workflow_graph)?;
        let mut run = self.resolve_or_create_run(&plan, &workflow_graph).await?;
        let orchestration_binding = ensure_workflow_graph_orchestration(
            &mut run,
            &workflow_graph,
            orchestration_role_for_dispatch(&plan),
            plan_snapshot,
        )?;
        self.run_repo.update(&run).await?;
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
            .create_initial_frame(&agent, runtime_session_ref)
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
            let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
                session_id.to_string(),
                run.id,
                frame.id,
                agent.id,
                orchestration_binding.orchestration_ref,
                orchestration_binding.node_path.clone(),
                orchestration_binding.attempt,
            );
            anchor_repo.upsert(&anchor).await?;
        }
        let session_id = runtime_session_ref.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "Graph-backed dispatch 缺少 RuntimeSession，无法 materialize entry NodeStarted"
                    .to_string(),
            )
        })?;
        let (updated_run, _) = apply_orchestration_event_to_run(
            run,
            orchestration_binding.orchestration_ref,
            OrchestrationRuntimeEvent::NodeStarted {
                node_path: orchestration_binding.node_path.clone(),
                attempt: orchestration_binding.attempt,
                executor_run_ref: Some(ExecutorRunRef::RuntimeSession {
                    session_id: session_id.to_string(),
                }),
                timestamp: chrono::Utc::now(),
            },
        )
        .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        run = updated_run;
        self.run_repo.update(&run).await?;

        let subject_execution_ref = association.as_ref().map(|assoc| SubjectExecutionRef {
            subject_ref: plan
                .subject_ref
                .clone()
                .expect("association requires subject"),
            association_id: assoc.id,
        });

        Ok(DispatchFacts {
            run,
            orchestration_binding: Some(orchestration_binding),
            agent,
            frame,
            runtime_session_ref,
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
            orchestration_binding: None,
            agent,
            frame,
            runtime_session_ref,
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
        runtime_session_ref: Option<Uuid>,
    ) -> Result<AgentFrame, WorkflowApplicationError> {
        let mut builder = AgentFrameBuilder::new(agent.id).with_created_by("dispatch", None);
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

fn compile_static_graph_orchestration_plan(
    workflow_graph: &WorkflowGraph,
) -> Result<OrchestrationPlanSnapshot, WorkflowApplicationError> {
    let output = WorkflowGraphCompiler::compile(WorkflowGraphCompileInput {
        graph: workflow_graph,
        source_metadata: WorkflowGraphCompileSourceMetadata::from_graph(workflow_graph),
        compile_mode: WorkflowGraphCompileMode::Strict,
        target_schema_version: WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION,
    });

    if output.has_blocking_diagnostics() {
        return Err(WorkflowApplicationError::BadRequest(
            blocking_compile_diagnostics_message(workflow_graph, &output.diagnostics),
        ));
    }

    Ok(output.plan_snapshot)
}

fn blocking_compile_diagnostics_message(
    workflow_graph: &WorkflowGraph,
    diagnostics: &[super::WorkflowGraphCompileDiagnostic],
) -> String {
    let details = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == ValidationSeverity::Error)
        .map(|diagnostic| {
            format!(
                "{} at {}: {}",
                diagnostic.code, diagnostic.source_path, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "WorkflowGraph {} 无法编译为 OrchestrationPlanSnapshot: {}",
        workflow_graph.id, details
    )
}

fn ensure_workflow_graph_orchestration(
    run: &mut LifecycleRun,
    workflow_graph: &WorkflowGraph,
    role: &str,
    plan_snapshot: OrchestrationPlanSnapshot,
) -> Result<OrchestrationBindingRefs, WorkflowApplicationError> {
    if let Some(existing) = run.orchestrations.iter().find(|orchestration| {
        orchestration.role == role
            && orchestration.plan_snapshot.plan_digest == plan_snapshot.plan_digest
    }) {
        return orchestration_entry_binding(existing);
    }

    let source_ref = OrchestrationSourceRef::WorkflowGraph {
        graph_id: workflow_graph.id,
        graph_version: Some(workflow_graph.version),
    };
    let orchestration = activate_orchestration(role.to_string(), source_ref, plan_snapshot);
    let binding = orchestration_entry_binding(&orchestration)?;
    run.add_orchestration(orchestration);
    Ok(binding)
}

fn orchestration_entry_binding(
    orchestration: &OrchestrationInstance,
) -> Result<OrchestrationBindingRefs, WorkflowApplicationError> {
    let entry_node_id = orchestration
        .activation
        .ready_node_ids
        .first()
        .or_else(|| orchestration.plan_snapshot.entry_node_ids.first())
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "OrchestrationInstance {} 缺少 entry runtime node",
                orchestration.orchestration_id
            ))
        })?;
    let node = orchestration
        .node_tree
        .iter()
        .find(|node| node.node_id == *entry_node_id)
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "OrchestrationInstance {} entry node {} 尚未 materialize",
                orchestration.orchestration_id, entry_node_id
            ))
        })?;
    Ok(OrchestrationBindingRefs::new(
        orchestration.orchestration_id,
        node.node_path.clone(),
        node.attempt,
    ))
}

fn orchestration_role_for_dispatch(plan: &DispatchPlan) -> &str {
    if matches!(plan.run_policy, RunPolicy::AppendGraph) {
        append_orchestration_role_from_source(&plan.source)
    } else {
        ROOT_ORCHESTRATION_ROLE
    }
}

fn append_orchestration_role_from_source(source: &ExecutionSource) -> &'static str {
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
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
        runtime_session_creator: &'a dyn RuntimeSessionCreator,
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

    // Tests

    #[tokio::test]
    async fn agent_launch_creates_graphless_surface_without_orchestration_binding() {
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
        .with_anchor_repo(&anchor_repo);

        let result = service
            .launch_agent(&new_project_agent_intent(project_id))
            .await
            .expect("dispatch");

        let runs = run_repo.items.lock().unwrap().clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].topology, LifecycleRunTopology::Graphless);
        assert_eq!(runs[0].root_graph_id, None);
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
        )
        .with_anchor_repo(&anchor_repo);

        let mut intent = new_task_execution_intent(project_id, task_id);
        intent.workflow_graph_ref = Some(test_workflow_graph_ref(project_id));
        let result = service.execute_subject(&intent).await.expect("dispatch");

        let runs = run_repo.items.lock().unwrap().clone();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].root_graph_id, Some(workflow_graph.id));
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
            RuntimeNodeStatus::Running
        );
        assert_eq!(
            orchestration.node_tree[0].executor_run_ref,
            Some(ExecutorRunRef::RuntimeSession {
                session_id: session_id.to_string()
            })
        );
        assert_eq!(
            orchestration.node_tree[0].trace_refs,
            vec![RuntimeTraceRef::RuntimeSession {
                session_id: session_id.to_string()
            }]
        );
        assert!(orchestration.node_tree[0].started_at.is_some());

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
        assert_eq!(runs[0].root_graph_id, Some(workflow_graph.id));
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
        let session_id = result.delivery_runtime_ref.expect("runtime session");
        assert_eq!(
            orchestration.node_tree[0].status,
            RuntimeNodeStatus::Running
        );
        assert_eq!(
            orchestration.node_tree[0].executor_run_ref,
            Some(ExecutorRunRef::RuntimeSession {
                session_id: session_id.to_string()
            })
        );
        assert_eq!(
            orchestration.node_tree[0].trace_refs,
            vec![RuntimeTraceRef::RuntimeSession {
                session_id: session_id.to_string()
            }]
        );
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
        let workflow_graph = seed_test_workflow_graph(&workflow_repo, project_id);
        let existing_run = create_lifecycle_run(project_id, workflow_graph.id);
        let first_agent = LifecycleAgent::new_root(existing_run.id, project_id, "routine");
        let target_agent = LifecycleAgent::new_root(existing_run.id, project_id, "routine");
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
}
