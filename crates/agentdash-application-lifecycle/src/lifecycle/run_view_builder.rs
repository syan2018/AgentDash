//! Application-owned Lifecycle read model.
//!
//! Product repositories identify the LifecycleRun and AgentRun target. Runtime
//! thread/source coordinates come exclusively from the committed Product
//! binding, while execution availability comes exclusively from the canonical
//! Managed Runtime snapshot.

use std::cmp::Reverse;
use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeSnapshot, ManagedRuntimeSourceBindingEvidence, RuntimeThreadId,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductProjectionQueryPort, AgentRunProductRuntimeBinding,
    AgentRunProductRuntimeBindingRepository, AgentRunProductRuntimeSnapshotObservation,
    AgentRunProductRuntimeSnapshotStaleReason,
};
use agentdash_domain::DomainError;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleAgent, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleRunStatus, LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
    NodePortValue, OrchestrationInstance, PlanNodeKind, RuntimeNodeError, RuntimeNodeState,
    RuntimeNodeStatus, RuntimeTraceRef, SubjectRef,
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
pub struct SubjectExecutionView {
    pub subject_ref: SubjectRef,
    pub associations: Vec<LifecycleSubjectAssociation>,
    pub runs: Vec<LifecycleRunView>,
    pub current_agent: Option<LifecycleAgentExecutionView>,
    pub attempts: Vec<SubjectExecutionAttemptView>,
    pub current_attempt: Option<SubjectExecutionAttemptView>,
    pub artifacts: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectExecutionAttemptView {
    pub target: AgentRunTarget,
    pub runtime: RuntimeExecutionTraceView,
    pub attempt: LifecycleExecutionAttemptView,
}

#[derive(Debug, Clone)]
pub struct ProjectActiveAgentsView {
    pub project_id: Uuid,
    pub runs: Vec<LifecycleRunView>,
    pub agents: Vec<LifecycleAgentExecutionView>,
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
    pub runtime_node: LifecycleRuntimeNodeView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleRuntimeNodeView {
    pub node_id: String,
    pub node_path: String,
    pub kind: PlanNodeKind,
    pub status: RuntimeNodeStatus,
    pub attempt: u32,
    pub inputs: Vec<NodePortValue>,
    pub outputs: Vec<NodePortValue>,
    pub executor_run_ref: Option<ExecutorRunRef>,
    pub agent_call_target: Option<AgentRunTarget>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<RuntimeNodeError>,
    pub trace_refs: Vec<RuntimeTraceRef>,
    pub artifacts: Value,
    pub children: Vec<LifecycleRuntimeNodeView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTraceAbsenceReason {
    ProductBindingMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTraceStaleReason {
    ProductBindingTargetMismatch,
    ProjectionBindingMissing,
    ProductBindingChanged,
    RuntimeThreadMismatch,
    RuntimeAppliedSurfaceMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTraceFenceEvidence {
    pub expected_target: AgentRunTarget,
    pub observed_target: Option<AgentRunTarget>,
    pub expected_runtime_thread_id: Option<RuntimeThreadId>,
    pub observed_runtime_thread_id: Option<RuntimeThreadId>,
    pub observed_source_binding: Option<ManagedRuntimeSourceBindingEvidence>,
    pub observed_snapshot: Option<ManagedRuntimeSnapshot>,
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
        reason: RuntimeTraceStaleReason,
        evidence: RuntimeTraceFenceEvidence,
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
                reason: RuntimeTraceStaleReason::ProductBindingTargetMismatch,
                evidence: stale_fence_evidence(target, None, Some(&binding), None),
            });
        }

        let observation = self
            .deps
            .product_projection
            .runtime_snapshot_observation(target)
            .await
            .map_err(|error| LifecycleRunViewQueryError::RuntimeProjection {
                target: target.clone(),
                message: error.to_string(),
            })?;
        match observation {
            AgentRunProductRuntimeSnapshotObservation::Absent { .. } => {
                Ok(RuntimeExecutionTraceView::Stale {
                    reason: RuntimeTraceStaleReason::ProjectionBindingMissing,
                    evidence: stale_fence_evidence(target, Some(&binding), None, None),
                })
            }
            AgentRunProductRuntimeSnapshotObservation::Current {
                product_binding,
                snapshot,
            } if product_binding != binding => Ok(RuntimeExecutionTraceView::Stale {
                reason: RuntimeTraceStaleReason::ProductBindingChanged,
                evidence: stale_fence_evidence(
                    target,
                    Some(&binding),
                    Some(&product_binding),
                    Some(&snapshot),
                ),
            }),
            AgentRunProductRuntimeSnapshotObservation::Current {
                product_binding,
                snapshot,
            } => Ok(RuntimeExecutionTraceView::Current {
                binding: product_binding,
                snapshot,
            }),
            AgentRunProductRuntimeSnapshotObservation::Stale(stale) => {
                let reason = match stale.reason {
                    AgentRunProductRuntimeSnapshotStaleReason::ProductBindingTargetMismatch => {
                        RuntimeTraceStaleReason::ProductBindingTargetMismatch
                    }
                    AgentRunProductRuntimeSnapshotStaleReason::RuntimeThreadMismatch => {
                        RuntimeTraceStaleReason::RuntimeThreadMismatch
                    }
                    AgentRunProductRuntimeSnapshotStaleReason::RuntimeAppliedSurfaceMismatch => {
                        RuntimeTraceStaleReason::RuntimeAppliedSurfaceMismatch
                    }
                };
                Ok(RuntimeExecutionTraceView::Stale {
                    reason,
                    evidence: stale_fence_evidence(
                        target,
                        Some(&binding),
                        Some(&stale.product_binding),
                        stale.observed_snapshot.as_ref(),
                    ),
                })
            }
        }
    }

    async fn build_run_view(
        &self,
        run: LifecycleRun,
    ) -> Result<LifecycleRunView, LifecycleRunViewQueryError> {
        let agents = self.deps.lifecycle_agents.list_by_run(run.id).await?;
        let mut subject_associations = self
            .deps
            .subject_associations
            .list_by_anchor(run.id, None)
            .await?;
        for agent in &agents {
            subject_associations.extend(
                self.deps
                    .subject_associations
                    .list_by_anchor(run.id, Some(agent.id))
                    .await?,
            );
        }
        subject_associations.sort_by_key(|association| (association.created_at, association.id));
        subject_associations.dedup_by_key(|association| association.id);

        let mut agent_views = Vec::with_capacity(agents.len());
        for agent in agents {
            let target = AgentRunTarget {
                run_id: run.id,
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

#[async_trait]
pub trait LifecycleRunViewQueryPort: Send + Sync {
    async fn lifecycle_run_view(
        &self,
        run_id: Uuid,
    ) -> Result<LifecycleRunView, LifecycleRunViewQueryError>;

    async fn subject_execution_view(
        &self,
        subject: SubjectRef,
    ) -> Result<SubjectExecutionView, LifecycleRunViewQueryError>;

    async fn project_active_agents_view(
        &self,
        project_id: Uuid,
    ) -> Result<ProjectActiveAgentsView, LifecycleRunViewQueryError>;
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
        self.build_run_view(run).await
    }

    async fn subject_execution_view(
        &self,
        subject: SubjectRef,
    ) -> Result<SubjectExecutionView, LifecycleRunViewQueryError> {
        let mut associations = self
            .deps
            .subject_associations
            .list_by_subject(&subject)
            .await?;
        associations.sort_by_key(|association| Reverse((association.created_at, association.id)));
        let run_ids = unique_run_ids(&associations);
        let mut runs = self.deps.lifecycle_runs.list_by_ids(&run_ids).await?;
        runs.sort_by_key(|run| Reverse((run.last_activity_at, run.id)));

        let mut run_views = Vec::with_capacity(runs.len());
        for run in runs {
            run_views.push(self.build_run_view(run).await?);
        }
        let associated_targets = associated_targets(&associations, &run_views);
        let current_agent = select_subject_current_agent(&associations, &run_views);
        let mut attempts = subject_attempts(&associated_targets, &run_views);
        attempts.sort_by_key(subject_attempt_sort_key);
        let current_attempt = attempts.first().cloned();
        let artifacts = current_attempt
            .as_ref()
            .map(|current| current.attempt.artifacts.clone())
            .unwrap_or(Value::Null);

        Ok(SubjectExecutionView {
            subject_ref: subject,
            associations,
            runs: run_views,
            current_agent,
            attempts,
            current_attempt,
            artifacts,
        })
    }

    async fn project_active_agents_view(
        &self,
        project_id: Uuid,
    ) -> Result<ProjectActiveAgentsView, LifecycleRunViewQueryError> {
        let mut runs = self
            .deps
            .lifecycle_runs
            .list_by_project(project_id)
            .await?
            .into_iter()
            .filter(|run| is_project_active_run(run.status))
            .collect::<Vec<_>>();
        runs.sort_by_key(|run| Reverse((run.last_activity_at, run.id)));
        let mut run_views = Vec::with_capacity(runs.len());
        for run in runs {
            run_views.push(self.build_run_view(run).await?);
        }
        let agents = run_views
            .iter()
            .flat_map(|run| run.agents.iter().cloned())
            .collect();
        Ok(ProjectActiveAgentsView {
            project_id,
            runs: run_views,
            agents,
        })
    }
}

fn stale_fence_evidence(
    target: &AgentRunTarget,
    expected_binding: Option<&AgentRunProductRuntimeBinding>,
    observed_binding: Option<&AgentRunProductRuntimeBinding>,
    observed_snapshot: Option<&ManagedRuntimeSnapshot>,
) -> RuntimeTraceFenceEvidence {
    RuntimeTraceFenceEvidence {
        expected_target: target.clone(),
        observed_target: observed_binding.map(|binding| binding.target.clone()),
        expected_runtime_thread_id: expected_binding
            .map(|binding| binding.runtime_thread_id.clone()),
        observed_runtime_thread_id: match observed_snapshot {
            Some(snapshot) => Some(snapshot.thread_id.clone()),
            None => observed_binding.map(|binding| binding.runtime_thread_id.clone()),
        },
        observed_source_binding: match observed_snapshot {
            Some(snapshot) => snapshot.source_binding.clone(),
            None => None,
        },
        observed_snapshot: observed_snapshot.cloned(),
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

fn unique_run_ids(associations: &[LifecycleSubjectAssociation]) -> Vec<Uuid> {
    let mut run_ids = associations
        .iter()
        .map(|association| association.anchor_run_id)
        .collect::<Vec<_>>();
    run_ids.sort_unstable();
    run_ids.dedup();
    run_ids
}

fn associated_targets(
    associations: &[LifecycleSubjectAssociation],
    runs: &[LifecycleRunView],
) -> std::collections::BTreeSet<AgentRunTarget> {
    let mut targets = std::collections::BTreeSet::new();
    for association in associations {
        let Some(run) = runs
            .iter()
            .find(|run| run.run.id == association.anchor_run_id)
        else {
            continue;
        };
        if let Some(agent_id) = association.anchor_agent_id {
            if run.agents.iter().any(|view| view.agent.id == agent_id) {
                targets.insert(AgentRunTarget {
                    run_id: run.run.id,
                    agent_id,
                });
            }
        } else {
            targets.extend(run.agents.iter().map(|view| AgentRunTarget {
                run_id: run.run.id,
                agent_id: view.agent.id,
            }));
        }
    }
    targets
}

fn select_subject_current_agent(
    associations: &[LifecycleSubjectAssociation],
    runs: &[LifecycleRunView],
) -> Option<LifecycleAgentExecutionView> {
    for association in associations {
        let Some(agent_id) = association.anchor_agent_id else {
            continue;
        };
        if let Some(agent) = runs
            .iter()
            .find(|run| run.run.id == association.anchor_run_id)
            .and_then(|run| run.agents.iter().find(|view| view.agent.id == agent_id))
        {
            return Some(agent.clone());
        }
    }
    let targets = associated_targets(associations, runs);
    runs.iter()
        .flat_map(|run| run.agents.iter())
        .filter(|view| {
            targets.contains(&AgentRunTarget {
                run_id: view.agent.run_id,
                agent_id: view.agent.id,
            })
        })
        .max_by_key(|view| {
            (
                view.current_attempt.as_ref().is_some_and(|attempt| {
                    matches!(
                        attempt.status,
                        RuntimeNodeStatus::Ready
                            | RuntimeNodeStatus::Claiming
                            | RuntimeNodeStatus::Running
                            | RuntimeNodeStatus::Blocked
                    )
                }),
                view.current_attempt
                    .as_ref()
                    .map(|attempt| attempt.observed_at),
                view.agent.status == "active",
                view.agent.updated_at,
                view.agent.id,
            )
        })
        .cloned()
}

fn subject_attempts(
    targets: &std::collections::BTreeSet<AgentRunTarget>,
    runs: &[LifecycleRunView],
) -> Vec<SubjectExecutionAttemptView> {
    runs.iter()
        .flat_map(|run| run.agents.iter())
        .filter_map(|agent| {
            let target = AgentRunTarget {
                run_id: agent.agent.run_id,
                agent_id: agent.agent.id,
            };
            targets
                .contains(&target)
                .then_some((target, agent.runtime.clone(), &agent.attempts))
        })
        .flat_map(|(target, runtime, attempts)| {
            attempts
                .iter()
                .cloned()
                .map(move |attempt| SubjectExecutionAttemptView {
                    target: target.clone(),
                    runtime: runtime.clone(),
                    attempt,
                })
        })
        .collect()
}

fn subject_attempt_sort_key(
    item: &SubjectExecutionAttemptView,
) -> (
    Reverse<bool>,
    Reverse<DateTime<Utc>>,
    Reverse<u32>,
    AgentRunTarget,
    String,
    Uuid,
) {
    (
        Reverse(matches!(
            item.attempt.status,
            RuntimeNodeStatus::Ready
                | RuntimeNodeStatus::Claiming
                | RuntimeNodeStatus::Running
                | RuntimeNodeStatus::Blocked
        )),
        Reverse(item.attempt.observed_at),
        Reverse(item.attempt.attempt),
        item.target.clone(),
        item.attempt.node_path.clone(),
        item.attempt.orchestration_id,
    )
}

fn is_project_active_run(status: LifecycleRunStatus) -> bool {
    matches!(
        status,
        LifecycleRunStatus::Draft
            | LifecycleRunStatus::Ready
            | LifecycleRunStatus::Running
            | LifecycleRunStatus::Blocked
    )
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
                runtime_node: runtime_node_to_view(orchestration, node),
            });
        }
        collect_execution_attempts(orchestration, &node.children, target, attempts);
    }
}

fn runtime_node_to_view(
    orchestration: &OrchestrationInstance,
    node: &RuntimeNodeState,
) -> LifecycleRuntimeNodeView {
    LifecycleRuntimeNodeView {
        node_id: node.node_id.clone(),
        node_path: node.node_path.clone(),
        kind: node.kind,
        status: node.status,
        attempt: node.attempt,
        inputs: node.inputs.clone(),
        outputs: node.outputs.clone(),
        executor_run_ref: node.executor_run_ref.clone(),
        agent_call_target: node.agent_call.as_ref().map(|state| state.target.clone()),
        started_at: node.started_at,
        completed_at: node.completed_at,
        error: node.error.clone(),
        trace_refs: node.trace_refs.clone(),
        artifacts: runtime_node_artifacts(orchestration, node),
        children: node
            .children
            .iter()
            .map(|child| runtime_node_to_view(orchestration, child))
            .collect(),
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
) -> (
    Reverse<bool>,
    Reverse<DateTime<Utc>>,
    Reverse<u32>,
    String,
    Uuid,
) {
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
        attempt.orchestration_id,
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
        AgentRunProductProjectionError, AgentRunTerminalChangePage, AgentRunTerminalChangeSequence,
        AgentRunTerminalSnapshot, ProductAgentFrameRef, ProductExecutionProfileRef,
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
        bindings: Mutex<BTreeMap<AgentRunTarget, AgentRunProductRuntimeBinding>>,
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

        async fn runtime_snapshot_observation(
            &self,
            target: &AgentRunTarget,
        ) -> Result<AgentRunProductRuntimeSnapshotObservation, AgentRunProductProjectionError>
        {
            if self.missing.lock().await.contains(target) {
                return Ok(AgentRunProductRuntimeSnapshotObservation::Absent {
                    requested_target: target.clone(),
                });
            }
            let binding = self
                .bindings
                .lock()
                .await
                .get(target)
                .cloned()
                .ok_or(AgentRunProductProjectionError::TargetNotBound)?;
            let snapshot = self
                .snapshots
                .lock()
                .await
                .get(target)
                .cloned()
                .ok_or_else(|| {
                    AgentRunProductProjectionError::Runtime("snapshot missing".into())
                })?;
            let reason = if binding.target != *target {
                Some(AgentRunProductRuntimeSnapshotStaleReason::ProductBindingTargetMismatch)
            } else if binding.runtime_thread_id != snapshot.thread_id {
                Some(AgentRunProductRuntimeSnapshotStaleReason::RuntimeThreadMismatch)
            } else if snapshot.source_binding.as_ref().is_none_or(|source| {
                source.activated_at_revision.is_none()
                    || source.applied_surface_revision.0 != binding.launch_frame.revision
            }) {
                Some(AgentRunProductRuntimeSnapshotStaleReason::RuntimeAppliedSurfaceMismatch)
            } else {
                None
            };
            Ok(match reason {
                Some(reason) => AgentRunProductRuntimeSnapshotObservation::Stale(
                    agentdash_application_agentrun::agent_run::AgentRunProductRuntimeSnapshotStaleEvidence {
                        requested_target: target.clone(),
                        product_binding: binding,
                        observed_snapshot: Some(snapshot),
                        reason,
                    },
                ),
                None => AgentRunProductRuntimeSnapshotObservation::Current {
                    product_binding: binding,
                    snapshot,
                },
            })
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
            let execution_profile = execution_profile();
            let binding = AgentRunProductRuntimeBinding {
                target: target.clone(),
                runtime_thread_id: RuntimeThreadId::new(thread).unwrap(),
                launch_frame: ProductAgentFrameRef {
                    frame_id: Uuid::new_v4(),
                    agent_id: target.agent_id,
                    revision: source.applied_surface_revision.0,
                },
                execution_profile_digest: execution_profile.profile_digest.clone(),
                execution_profile,
            };
            self.bindings
                .bindings
                .lock()
                .await
                .insert(target.clone(), binding.clone());
            self.projections
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

    fn execution_profile() -> ProductExecutionProfileRef {
        let mut profile = ProductExecutionProfileRef {
            profile_key: "lifecycle-run-view-fixture".to_string(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({
                "provider": "fixture",
                "model": "lifecycle-run-view"
            }),
            credential_scope: None,
        };
        profile.refresh_digest();
        profile
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
            thread_name: None,
            thread_name_source: None,
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
            conversation_history: Vec::new(),
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
                reason: RuntimeTraceStaleReason::RuntimeAppliedSurfaceMismatch,
                ..
            }
        )));
        assert_eq!(view.agents.len(), 3);
    }

    #[tokio::test]
    async fn stale_source_binding_preserves_an_observed_missing_value() {
        let fixture = Fixture::new();
        let run = LifecycleRun::new_control(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::WorkflowAgent);
        let target = target(&run, &agent);
        let expected_source = source("expected-source");
        fixture.runs.create(&run).await.unwrap();
        fixture.agents.create(&agent).await.unwrap();
        fixture
            .insert_binding_and_snapshot(
                target.clone(),
                "thread-missing-source",
                expected_source.clone(),
            )
            .await;
        fixture
            .projections
            .snapshots
            .lock()
            .await
            .get_mut(&target)
            .unwrap()
            .source_binding = None;

        let view = fixture.service().lifecycle_run_view(run.id).await.unwrap();

        let RuntimeExecutionTraceView::Stale { reason, evidence } = &view.agents[0].runtime else {
            panic!("expected typed stale Runtime trace");
        };
        assert_eq!(
            *reason,
            RuntimeTraceStaleReason::RuntimeAppliedSurfaceMismatch
        );
        assert_eq!(
            evidence
                .expected_runtime_thread_id
                .as_ref()
                .unwrap()
                .as_str(),
            "thread-missing-source"
        );
        assert_eq!(
            evidence
                .observed_runtime_thread_id
                .as_ref()
                .unwrap()
                .as_str(),
            "thread-missing-source"
        );
        assert_eq!(evidence.observed_source_binding, None);
        assert_eq!(
            evidence
                .observed_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.source_binding.as_ref()),
            None
        );
    }

    #[tokio::test]
    async fn binding_change_between_reads_is_typed_stale_with_both_fences() {
        let fixture = Fixture::new();
        let run = LifecycleRun::new_control(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::WorkflowAgent);
        let target = target(&run, &agent);
        let expected_source = source("expected-source");
        let observed_source = source("observed-source");
        fixture.runs.create(&run).await.unwrap();
        fixture.agents.create(&agent).await.unwrap();
        fixture
            .insert_binding_and_snapshot(target.clone(), "thread-before", expected_source.clone())
            .await;
        fixture
            .projections
            .bindings
            .lock()
            .await
            .insert(target.clone(), {
                let execution_profile = execution_profile();
                AgentRunProductRuntimeBinding {
                    target: target.clone(),
                    runtime_thread_id: RuntimeThreadId::new("thread-after").unwrap(),
                    launch_frame: ProductAgentFrameRef {
                        frame_id: Uuid::new_v4(),
                        agent_id: target.agent_id,
                        revision: 1,
                    },
                    execution_profile_digest: execution_profile.profile_digest.clone(),
                    execution_profile,
                }
            });
        fixture.projections.snapshots.lock().await.insert(
            target.clone(),
            snapshot("thread-after", observed_source.clone()),
        );

        let view = fixture.service().lifecycle_run_view(run.id).await.unwrap();

        let RuntimeExecutionTraceView::Stale { reason, evidence } = &view.agents[0].runtime else {
            panic!("expected typed stale Runtime trace");
        };
        assert_eq!(*reason, RuntimeTraceStaleReason::ProductBindingChanged);
        assert_eq!(evidence.expected_target, target);
        assert_eq!(evidence.observed_target.as_ref(), Some(&target));
        assert_eq!(
            evidence
                .expected_runtime_thread_id
                .as_ref()
                .unwrap()
                .as_str(),
            "thread-before"
        );
        assert_eq!(
            evidence
                .observed_runtime_thread_id
                .as_ref()
                .unwrap()
                .as_str(),
            "thread-after"
        );
        assert_eq!(
            evidence.observed_source_binding.as_ref(),
            Some(&observed_source)
        );
        assert_eq!(
            evidence
                .observed_snapshot
                .as_ref()
                .map(|snapshot| snapshot.thread_id.as_str()),
            Some("thread-after")
        );
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
        assert_eq!(current.runtime_node.kind, PlanNodeKind::AgentCall);
        assert_eq!(
            current.runtime_node.executor_run_ref,
            Some(ExecutorRunRef::AgentRun {
                run_id: run.id,
                agent_id: agent.id,
            })
        );
    }

    #[tokio::test]
    async fn subject_execution_keeps_same_path_attempts_distinct_across_runs_and_agents() {
        let fixture = Fixture::new();
        let project_id = Uuid::new_v4();
        let subject = SubjectRef::new("task", Uuid::new_v4());
        let now = Utc::now();

        let mut first_run = LifecycleRun::new_control(project_id);
        let first_agent =
            LifecycleAgent::new_root(first_run.id, project_id, AgentSource::WorkflowAgent);
        let first_target = target(&first_run, &first_agent);
        let mut first_orchestration = nested_attempt(&first_run, first_target.clone());
        first_orchestration.node_tree[0].children[0].started_at =
            Some(now - chrono::Duration::seconds(10));
        first_orchestration
            .state_snapshot
            .node_outputs
            .insert("agent".into(), serde_json::json!({"result": "first"}));
        first_run.add_orchestration(first_orchestration);

        let mut second_run = LifecycleRun::new_control(project_id);
        let second_agent =
            LifecycleAgent::new_root(second_run.id, project_id, AgentSource::WorkflowAgent);
        let second_target = target(&second_run, &second_agent);
        let mut second_orchestration = nested_attempt(&second_run, second_target.clone());
        second_orchestration.node_tree[0].children[0].started_at = Some(now);
        second_orchestration
            .state_snapshot
            .node_outputs
            .insert("agent".into(), serde_json::json!({"result": "second"}));
        second_run.add_orchestration(second_orchestration);

        for run in [&first_run, &second_run] {
            fixture.runs.create(run).await.unwrap();
        }
        for agent in [&first_agent, &second_agent] {
            fixture.agents.create(agent).await.unwrap();
        }
        fixture
            .insert_binding_and_snapshot(first_target, "thread-first", source("first"))
            .await;
        fixture
            .insert_binding_and_snapshot(second_target.clone(), "thread-second", source("second"))
            .await;
        let mut first_association = LifecycleSubjectAssociation::new_agent_scoped(
            first_run.id,
            first_agent.id,
            &subject,
            "task_execution",
            None,
        );
        first_association.created_at = now - chrono::Duration::seconds(10);
        let mut second_association = LifecycleSubjectAssociation::new_agent_scoped(
            second_run.id,
            second_agent.id,
            &subject,
            "task_execution",
            None,
        );
        second_association.created_at = now;
        fixture
            .associations
            .create(&first_association)
            .await
            .unwrap();
        fixture
            .associations
            .create(&second_association)
            .await
            .unwrap();

        let view = fixture
            .service()
            .subject_execution_view(subject)
            .await
            .unwrap();

        assert_eq!(view.runs.len(), 2);
        assert_eq!(view.attempts.len(), 2);
        assert!(
            view.attempts
                .iter()
                .all(|attempt| attempt.attempt.node_path == "phase/agent")
        );
        assert_ne!(view.attempts[0].target, view.attempts[1].target);
        assert_eq!(
            view.current_agent.as_ref().unwrap().agent.id,
            second_agent.id
        );
        assert_eq!(view.current_attempt.as_ref().unwrap().target, second_target);
        assert_eq!(
            view.artifacts.pointer("/node_outputs/result"),
            Some(&serde_json::json!("second"))
        );
        assert_eq!(
            view.current_attempt
                .as_ref()
                .unwrap()
                .attempt
                .runtime_node
                .node_path,
            "phase/agent"
        );
    }

    #[tokio::test]
    async fn project_active_agents_excludes_terminal_runs_and_keeps_typed_absence() {
        let fixture = Fixture::new();
        let project_id = Uuid::new_v4();
        let mut active_run = LifecycleRun::new_control(project_id);
        active_run.status = LifecycleRunStatus::Blocked;
        let active_agent =
            LifecycleAgent::new_root(active_run.id, project_id, AgentSource::WorkflowAgent);
        let mut completed_run = LifecycleRun::new_control(project_id);
        completed_run.status = LifecycleRunStatus::Completed;
        let completed_agent =
            LifecycleAgent::new_root(completed_run.id, project_id, AgentSource::WorkflowAgent);
        fixture.runs.create(&active_run).await.unwrap();
        fixture.runs.create(&completed_run).await.unwrap();
        fixture.agents.create(&active_agent).await.unwrap();
        fixture.agents.create(&completed_agent).await.unwrap();

        let view = fixture
            .service()
            .project_active_agents_view(project_id)
            .await
            .unwrap();

        assert_eq!(view.runs.len(), 1);
        assert_eq!(view.runs[0].run.id, active_run.id);
        assert_eq!(view.agents.len(), 1);
        assert!(matches!(
            view.agents[0].runtime,
            RuntimeExecutionTraceView::Absent {
                reason: RuntimeTraceAbsenceReason::ProductBindingMissing,
                ..
            }
        ));
    }
}
