//! Application-owned Lifecycle read model.
//!
//! Product repositories identify the LifecycleRun and AgentRun target. Runtime
//! thread/source coordinates come exclusively from the committed Product
//! binding, while execution availability comes exclusively from the canonical
//! Managed Runtime snapshot.

use std::cmp::Reverse;
use std::sync::Arc;

use agentdash_agent_runtime_contract::ManagedRuntimeSnapshot;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductProjectionError, AgentRunProductProjectionQueryPort,
    AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingRepository,
};
use agentdash_domain::DomainError;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleAgent, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository, OrchestrationInstance,
    RuntimeNodeState, RuntimeNodeStatus,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LifecycleRunView {
    pub run: LifecycleRun,
    pub agents: Vec<LifecycleAgentExecutionView>,
    pub subject_associations: Vec<LifecycleSubjectAssociation>,
}

#[derive(Debug, Clone)]
pub struct LifecycleAgentExecutionView {
    pub agent: LifecycleAgent,
    pub runtime: RuntimeExecutionTraceView,
    pub current_attempt: Option<LifecycleExecutionAttemptView>,
    pub attempts: Vec<LifecycleExecutionAttemptView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleExecutionAttemptView {
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub status: RuntimeNodeStatus,
    pub observed_at: DateTime<Utc>,
    pub artifacts: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTraceAbsenceReason {
    ProductBindingMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTraceStaleReason {
    ProductBindingTargetMismatch,
    ProjectionBindingMissing,
    ProjectionTargetMismatch,
    RuntimeThreadMismatch,
    RuntimeSourceBindingMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeExecutionTraceView {
    Absent {
        target: AgentRunTarget,
        reason: RuntimeTraceAbsenceReason,
    },
    Current {
        binding: AgentRunProductRuntimeBinding,
        snapshot: ManagedRuntimeSnapshot,
    },
    Stale {
        binding: AgentRunProductRuntimeBinding,
        reason: RuntimeTraceStaleReason,
    },
}

#[derive(Clone)]
pub struct LifecycleRunViewQueryDeps {
    pub lifecycle_runs: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
    pub subject_associations: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub product_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    pub product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
}

#[derive(Clone)]
pub struct LifecycleRunViewQueryService {
    deps: LifecycleRunViewQueryDeps,
}

impl LifecycleRunViewQueryService {
    pub fn new(deps: LifecycleRunViewQueryDeps) -> Self {
        Self { deps }
    }

    async fn runtime_trace(
        &self,
        target: &AgentRunTarget,
    ) -> Result<RuntimeExecutionTraceView, LifecycleRunViewQueryError> {
        let Some(binding) = self
            .deps
            .product_bindings
            .load_product_binding(target)
            .await
            .map_err(|message| LifecycleRunViewQueryError::ProductBinding {
                target: target.clone(),
                message,
            })?
        else {
            return Ok(RuntimeExecutionTraceView::Absent {
                target: target.clone(),
                reason: RuntimeTraceAbsenceReason::ProductBindingMissing,
            });
        };
        if binding.target != *target {
            return Ok(RuntimeExecutionTraceView::Stale {
                binding,
                reason: RuntimeTraceStaleReason::ProductBindingTargetMismatch,
            });
        }

        let snapshot = match self.deps.product_projection.runtime_snapshot(target).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let stale_reason = match error {
                    AgentRunProductProjectionError::TargetNotBound => {
                        Some(RuntimeTraceStaleReason::ProjectionBindingMissing)
                    }
                    AgentRunProductProjectionError::TargetMismatch => {
                        Some(RuntimeTraceStaleReason::ProjectionTargetMismatch)
                    }
                    AgentRunProductProjectionError::RuntimeThreadMismatch => {
                        Some(RuntimeTraceStaleReason::RuntimeThreadMismatch)
                    }
                    AgentRunProductProjectionError::RuntimeSourceBindingMismatch => {
                        Some(RuntimeTraceStaleReason::RuntimeSourceBindingMismatch)
                    }
                    AgentRunProductProjectionError::Binding(_)
                    | AgentRunProductProjectionError::Runtime(_)
                    | AgentRunProductProjectionError::Workspace(_)
                    | AgentRunProductProjectionError::Terminal(_) => None,
                };
                return match stale_reason {
                    Some(reason) => Ok(RuntimeExecutionTraceView::Stale { binding, reason }),
                    None => Err(LifecycleRunViewQueryError::RuntimeProjection {
                        target: target.clone(),
                        message: error.to_string(),
                    }),
                };
            }
        };
        if snapshot.thread_id != binding.runtime_thread_id {
            return Ok(RuntimeExecutionTraceView::Stale {
                binding,
                reason: RuntimeTraceStaleReason::RuntimeThreadMismatch,
            });
        }
        if snapshot.source_binding.as_ref() != Some(&binding.source_binding) {
            return Ok(RuntimeExecutionTraceView::Stale {
                binding,
                reason: RuntimeTraceStaleReason::RuntimeSourceBindingMismatch,
            });
        }
        Ok(RuntimeExecutionTraceView::Current { binding, snapshot })
    }
}

#[async_trait]
pub trait LifecycleRunViewQueryPort: Send + Sync {
    async fn lifecycle_run_view(
        &self,
        run_id: Uuid,
    ) -> Result<LifecycleRunView, LifecycleRunViewQueryError>;
}

#[async_trait]
impl LifecycleRunViewQueryPort for LifecycleRunViewQueryService {
    async fn lifecycle_run_view(
        &self,
        run_id: Uuid,
    ) -> Result<LifecycleRunView, LifecycleRunViewQueryError> {
        let run = self
            .deps
            .lifecycle_runs
            .get_by_id(run_id)
            .await?
            .ok_or(LifecycleRunViewQueryError::RunNotFound { run_id })?;
        let agents = self.deps.lifecycle_agents.list_by_run(run_id).await?;
        let mut subject_associations = self
            .deps
            .subject_associations
            .list_by_anchor(run_id, None)
            .await?;
        for agent in &agents {
            subject_associations.extend(
                self.deps
                    .subject_associations
                    .list_by_anchor(run_id, Some(agent.id))
                    .await?,
            );
        }
        subject_associations.sort_by_key(|association| association.created_at);
        subject_associations.dedup_by_key(|association| association.id);

        let mut agent_views = Vec::with_capacity(agents.len());
        for agent in agents {
            let target = AgentRunTarget {
                run_id,
                agent_id: agent.id,
            };
            let mut attempts = execution_attempts(&run, &target);
            attempts.sort_by_key(attempt_sort_key);
            let current_attempt = attempts.first().cloned();
            agent_views.push(LifecycleAgentExecutionView {
                agent,
                runtime: self.runtime_trace(&target).await?,
                current_attempt,
                attempts,
            });
        }
        agent_views.sort_by_key(|view| (view.agent.created_at, view.agent.id));

        Ok(LifecycleRunView {
            run,
            agents: agent_views,
            subject_associations,
        })
    }
}

#[derive(Debug, Error)]
pub enum LifecycleRunViewQueryError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("LifecycleRun not found: {run_id}")]
    RunNotFound { run_id: Uuid },
    #[error("AgentRun Product binding query failed for {target:?}: {message}")]
    ProductBinding {
        target: AgentRunTarget,
        message: String,
    },
    #[error("AgentRun Product projection query failed for {target:?}: {message}")]
    RuntimeProjection {
        target: AgentRunTarget,
        message: String,
    },
}

fn execution_attempts(
    run: &LifecycleRun,
    target: &AgentRunTarget,
) -> Vec<LifecycleExecutionAttemptView> {
    let mut attempts = Vec::new();
    for orchestration in &run.orchestrations {
        collect_execution_attempts(
            orchestration,
            &orchestration.node_tree,
            target,
            &mut attempts,
        );
    }
    attempts
}

fn collect_execution_attempts(
    orchestration: &OrchestrationInstance,
    nodes: &[RuntimeNodeState],
    target: &AgentRunTarget,
    attempts: &mut Vec<LifecycleExecutionAttemptView>,
) {
    for node in nodes {
        if node_targets_agent_run(node, target) {
            attempts.push(LifecycleExecutionAttemptView {
                orchestration_id: orchestration.orchestration_id,
                node_path: node.node_path.clone(),
                attempt: node.attempt.max(1),
                status: node.status,
                observed_at: node
                    .completed_at
                    .or(node.started_at)
                    .unwrap_or(orchestration.updated_at),
                artifacts: runtime_node_artifacts(orchestration, node),
            });
        }
        collect_execution_attempts(orchestration, &node.children, target, attempts);
    }
}

fn node_targets_agent_run(node: &RuntimeNodeState, target: &AgentRunTarget) -> bool {
    node.agent_call
        .as_ref()
        .is_some_and(|state| state.target == *target)
        || matches!(
            node.executor_run_ref.as_ref(),
            Some(ExecutorRunRef::AgentRun { run_id, agent_id })
                if *run_id == target.run_id && *agent_id == target.agent_id
        )
}

fn attempt_sort_key(
    attempt: &LifecycleExecutionAttemptView,
) -> (Reverse<bool>, Reverse<DateTime<Utc>>, Reverse<u32>, String) {
    (
        Reverse(matches!(
            attempt.status,
            RuntimeNodeStatus::Ready
                | RuntimeNodeStatus::Claiming
                | RuntimeNodeStatus::Running
                | RuntimeNodeStatus::Blocked
        )),
        Reverse(attempt.observed_at),
        Reverse(attempt.attempt),
        attempt.node_path.clone(),
    )
}

fn runtime_node_artifacts(orchestration: &OrchestrationInstance, node: &RuntimeNodeState) -> Value {
    let node_outputs = orchestration
        .state_snapshot
        .node_outputs
        .get(&node.node_id)
        .cloned()
        .unwrap_or(Value::Null);
    let artifact_refs = orchestration
        .state_snapshot
        .artifacts
        .iter()
        .filter(|artifact| artifact.node_path.as_deref() == Some(node.node_path.as_str()))
        .collect::<Vec<_>>();
    serde_json::json!({
        "node_outputs": node_outputs,
        "artifact_refs": artifact_refs,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeAvailabilityEvidence, ManagedRuntimeChangePage,
        ManagedRuntimeCommandAvailability, ManagedRuntimeCommandKind,
        ManagedRuntimeLifecycleStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSourceBindingEvidence,
        RuntimeChangeSequence, RuntimeProjectionRevision, RuntimeSourceRef, RuntimeThreadId,
        SurfaceRevision,
    };
    use agentdash_application_agentrun::agent_run::{
        AgentRunTerminalChangePage, AgentRunTerminalChangeSequence, AgentRunTerminalSnapshot,
    };
    use agentdash_domain::workflow::{
        AgentSource, OrchestrationLimits, OrchestrationPlanSnapshot, OrchestrationSourceRef,
        PlanNodeKind, SubjectRef,
    };
    use agentdash_test_support::workflow::{
        MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository,
        MemoryLifecycleSubjectAssociationRepository,
    };
    use agentdash_workspace_module::workspace_module::presentation_protocol::{
        WorkspaceModulePresentationAcknowledgeRequest, WorkspaceModulePresentationChange,
        WorkspaceModulePresentationChangePage, WorkspaceModulePresentationChangeSequence,
        WorkspaceModulePresentationSnapshot,
    };
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct BindingRepo {
        bindings: Mutex<BTreeMap<AgentRunTarget, AgentRunProductRuntimeBinding>>,
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingRepository for BindingRepo {
        async fn load_product_binding(
            &self,
            target: &AgentRunTarget,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok(self.bindings.lock().await.get(target).cloned())
        }
    }

    #[derive(Default)]
    struct ProjectionRepo {
        snapshots: Mutex<BTreeMap<AgentRunTarget, ManagedRuntimeSnapshot>>,
        missing: Mutex<BTreeSet<AgentRunTarget>>,
    }

    #[async_trait]
    impl AgentRunProductProjectionQueryPort for ProjectionRepo {
        async fn runtime_snapshot(
            &self,
            target: &AgentRunTarget,
        ) -> Result<ManagedRuntimeSnapshot, AgentRunProductProjectionError> {
            if self.missing.lock().await.contains(target) {
                return Err(AgentRunProductProjectionError::TargetNotBound);
            }
            self.snapshots
                .lock()
                .await
                .get(target)
                .cloned()
                .ok_or_else(|| AgentRunProductProjectionError::Runtime("snapshot missing".into()))
        }

        async fn runtime_changes(
            &self,
            _target: &AgentRunTarget,
            _after: Option<RuntimeChangeSequence>,
        ) -> Result<ManagedRuntimeChangePage, AgentRunProductProjectionError> {
            Err(AgentRunProductProjectionError::Runtime("unused".into()))
        }

        async fn workspace_presentation_snapshot(
            &self,
            _target: &AgentRunTarget,
        ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError> {
            Err(AgentRunProductProjectionError::Workspace("unused".into()))
        }

        async fn workspace_presentation_changes(
            &self,
            _target: &AgentRunTarget,
            _after: Option<WorkspaceModulePresentationChangeSequence>,
            _limit: usize,
        ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError> {
            Err(AgentRunProductProjectionError::Workspace("unused".into()))
        }

        async fn acknowledge_workspace_presentation(
            &self,
            _request: WorkspaceModulePresentationAcknowledgeRequest,
        ) -> Result<WorkspaceModulePresentationChange, AgentRunProductProjectionError> {
            Err(AgentRunProductProjectionError::Workspace("unused".into()))
        }

        async fn terminal_snapshot(
            &self,
            _target: &AgentRunTarget,
        ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError> {
            Err(AgentRunProductProjectionError::Terminal("unused".into()))
        }

        async fn terminal_changes(
            &self,
            _target: &AgentRunTarget,
            _after: Option<AgentRunTerminalChangeSequence>,
            _limit: usize,
        ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError> {
            Err(AgentRunProductProjectionError::Terminal("unused".into()))
        }
    }

    struct Fixture {
        runs: Arc<MemoryLifecycleRunRepository>,
        agents: Arc<MemoryLifecycleAgentRepository>,
        associations: Arc<MemoryLifecycleSubjectAssociationRepository>,
        bindings: Arc<BindingRepo>,
        projections: Arc<ProjectionRepo>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                runs: Arc::new(MemoryLifecycleRunRepository::default()),
                agents: Arc::new(MemoryLifecycleAgentRepository::default()),
                associations: Arc::new(MemoryLifecycleSubjectAssociationRepository::default()),
                bindings: Arc::new(BindingRepo::default()),
                projections: Arc::new(ProjectionRepo::default()),
            }
        }

        fn service(&self) -> LifecycleRunViewQueryService {
            LifecycleRunViewQueryService::new(LifecycleRunViewQueryDeps {
                lifecycle_runs: self.runs.clone(),
                lifecycle_agents: self.agents.clone(),
                subject_associations: self.associations.clone(),
                product_bindings: self.bindings.clone(),
                product_projection: self.projections.clone(),
            })
        }

        async fn insert_binding_and_snapshot(
            &self,
            target: AgentRunTarget,
            thread: &str,
            source: ManagedRuntimeSourceBindingEvidence,
        ) {
            let binding = AgentRunProductRuntimeBinding {
                target: target.clone(),
                runtime_thread_id: RuntimeThreadId::new(thread).unwrap(),
                source_binding: source.clone(),
            };
            self.bindings
                .bindings
                .lock()
                .await
                .insert(target.clone(), binding);
            self.projections
                .snapshots
                .lock()
                .await
                .insert(target, snapshot(thread, source));
        }
    }

    fn source(name: &str) -> ManagedRuntimeSourceBindingEvidence {
        ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new(name).unwrap(),
            committed_at_revision: RuntimeProjectionRevision(2),
            applied_surface_revision: SurfaceRevision(3),
            activated_at_revision: Some(RuntimeProjectionRevision(4)),
        }
    }

    fn snapshot(
        thread: &str,
        source_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> ManagedRuntimeSnapshot {
        let evidence = ManagedRuntimeAvailabilityEvidence {
            decided_at_revision: RuntimeProjectionRevision(7),
            blocking_operation_id: None,
            bound_surface_revision: Some(SurfaceRevision(3)),
            applied_surface_revision: Some(SurfaceRevision(3)),
        };
        ManagedRuntimeSnapshot {
            thread_id: RuntimeThreadId::new(thread).unwrap(),
            revision: RuntimeProjectionRevision(7),
            latest_change_sequence: RuntimeChangeSequence(5),
            captured_at_ms: 10,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            operations: Vec::new(),
            source_binding: Some(source_binding),
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability: BTreeMap::from([(
                ManagedRuntimeCommandKind::SubmitInput,
                ManagedRuntimeCommandAvailability::Available { evidence },
            )]),
        }
    }

    fn target(run: &LifecycleRun, agent: &LifecycleAgent) -> AgentRunTarget {
        AgentRunTarget {
            run_id: run.id,
            agent_id: agent.id,
        }
    }

    fn nested_attempt(run: &LifecycleRun, target: AgentRunTarget) -> OrchestrationInstance {
        let source_ref = OrchestrationSourceRef::Inline {
            source_digest: "sha256:nested".to_owned(),
        };
        let plan = OrchestrationPlanSnapshot {
            plan_digest: "sha256:plan".to_owned(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: Vec::new(),
            entry_node_ids: Vec::new(),
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        let mut orchestration = OrchestrationInstance::new("root", source_ref, plan);
        let now = Utc::now();
        orchestration.node_tree = vec![RuntimeNodeState {
            node_id: "phase".into(),
            node_path: "phase".into(),
            kind: PlanNodeKind::Phase,
            status: RuntimeNodeStatus::Running,
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: None,
            agent_call: None,
            children: vec![RuntimeNodeState {
                node_id: "agent".into(),
                node_path: "phase/agent".into(),
                kind: PlanNodeKind::AgentCall,
                status: RuntimeNodeStatus::Running,
                attempt: 2,
                inputs: Vec::new(),
                outputs: Vec::new(),
                executor_run_ref: Some(ExecutorRunRef::AgentRun {
                    run_id: target.run_id,
                    agent_id: target.agent_id,
                }),
                agent_call: None,
                children: Vec::new(),
                phase_path: vec!["phase".into()],
                started_at: Some(now),
                completed_at: None,
                error: None,
                trace_refs: Vec::new(),
                cache: None,
            }],
            phase_path: Vec::new(),
            started_at: Some(run.created_at),
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        }];
        orchestration
    }

    #[tokio::test]
    async fn direct_run_reads_current_thread_and_availability_from_final_projection() {
        let fixture = Fixture::new();
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let target = target(&run, &agent);
        fixture.runs.create(&run).await.unwrap();
        fixture.agents.create(&agent).await.unwrap();
        fixture
            .insert_binding_and_snapshot(target, "thread-direct", source("source-direct"))
            .await;

        let view = fixture.service().lifecycle_run_view(run.id).await.unwrap();

        let RuntimeExecutionTraceView::Current { binding, snapshot } = &view.agents[0].runtime
        else {
            panic!("expected current Runtime thread");
        };
        assert_eq!(binding.runtime_thread_id.as_str(), "thread-direct");
        assert!(
            snapshot
                .command_availability
                .contains_key(&ManagedRuntimeCommandKind::SubmitInput)
        );
        assert!(view.agents[0].current_attempt.is_none());
    }

    #[tokio::test]
    async fn multiple_agents_keep_missing_and_stale_runtime_traces_typed() {
        let fixture = Fixture::new();
        let run = LifecycleRun::new_control(Uuid::new_v4());
        let current = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::WorkflowAgent);
        let missing = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::Subagent);
        let stale = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::Subagent);
        fixture.runs.create(&run).await.unwrap();
        for agent in [&current, &missing, &stale] {
            fixture.agents.create(agent).await.unwrap();
        }
        fixture
            .insert_binding_and_snapshot(target(&run, &current), "thread-current", source("a"))
            .await;
        let stale_target = target(&run, &stale);
        fixture
            .insert_binding_and_snapshot(stale_target.clone(), "thread-stale", source("b"))
            .await;
        fixture.projections.snapshots.lock().await.insert(
            stale_target,
            snapshot("thread-stale", source("different-source")),
        );

        let view = fixture.service().lifecycle_run_view(run.id).await.unwrap();

        assert!(view.agents.iter().any(|view| matches!(
            view.runtime,
            RuntimeExecutionTraceView::Absent {
                reason: RuntimeTraceAbsenceReason::ProductBindingMissing,
                ..
            }
        )));
        assert!(view.agents.iter().any(|view| matches!(
            view.runtime,
            RuntimeExecutionTraceView::Stale {
                reason: RuntimeTraceStaleReason::RuntimeSourceBindingMismatch,
                ..
            }
        )));
        assert_eq!(view.agents.len(), 3);
    }

    #[tokio::test]
    async fn nested_orchestration_selects_current_attempt_without_using_node_thread_fields() {
        let fixture = Fixture::new();
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::WorkflowAgent);
        let target = target(&run, &agent);
        run.add_orchestration(nested_attempt(&run, target.clone()));
        fixture.runs.create(&run).await.unwrap();
        fixture.agents.create(&agent).await.unwrap();
        fixture
            .insert_binding_and_snapshot(target, "thread-final-binding", source("source-final"))
            .await;
        let subject = SubjectRef::new("task", Uuid::new_v4());
        fixture
            .associations
            .create(&LifecycleSubjectAssociation::new_agent_scoped(
                run.id,
                agent.id,
                &subject,
                "task_execution",
                None,
            ))
            .await
            .unwrap();

        let view = fixture.service().lifecycle_run_view(run.id).await.unwrap();

        let current = view.agents[0].current_attempt.as_ref().unwrap();
        assert_eq!(current.node_path, "phase/agent");
        assert_eq!(current.attempt, 2);
        assert_eq!(view.subject_associations.len(), 1);
        let RuntimeExecutionTraceView::Current { binding, .. } = &view.agents[0].runtime else {
            panic!("expected current binding");
        };
        assert_eq!(binding.runtime_thread_id.as_str(), "thread-final-binding");
    }
}
