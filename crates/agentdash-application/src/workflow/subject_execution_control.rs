use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityExecutionClaimRepository, ActivityExecutionClaimStatus, AgentAssignment,
    AgentAssignmentRepository, AgentFrame, AgentFrameRepository, LifecycleAgent,
    LifecycleAgentRepository, LifecycleRunRepository, LifecycleSubjectAssociation,
    LifecycleSubjectAssociationRepository, RuntimeSessionSelectionPolicy, SubjectRef,
    WorkflowGraphInstanceRepository, WorkflowGraphRepository,
};

use super::{ActivityEvent, ActivityLifecycleRunService, WorkflowApplicationError};

#[derive(Debug, Clone)]
pub struct CancelSubjectExecutionCommand {
    pub subject_ref: SubjectRef,
    pub runtime_selection_policy: RuntimeSessionSelectionPolicy,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeCancelDeliveryCommand {
    pub runtime_session_id: String,
    pub run_ref: Uuid,
    pub graph_instance_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub assignment_ref: Uuid,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubjectExecutionCancelResult {
    pub subject_ref: SubjectRef,
    pub association_ref: Uuid,
    pub run_ref: Uuid,
    pub graph_instance_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub assignment_ref: Uuid,
    pub activity_key: String,
    pub attempt: i32,
    pub runtime_delivery: Option<RuntimeCancelDeliveryCommand>,
}

struct SubjectExecutionCancelTarget {
    association: LifecycleSubjectAssociation,
    agent: LifecycleAgent,
    assignment: AgentAssignment,
    delivery_frame: AgentFrame,
}

pub struct SubjectExecutionControlService<'a> {
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    workflow_graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
    claim_repo: &'a dyn ActivityExecutionClaimRepository,
    lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    agent_assignment_repo: &'a dyn AgentAssignmentRepository,
}

impl<'a> SubjectExecutionControlService<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_instance_repo: &'a dyn WorkflowGraphInstanceRepository,
        claim_repo: &'a dyn ActivityExecutionClaimRepository,
        lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        agent_assignment_repo: &'a dyn AgentAssignmentRepository,
    ) -> Self {
        Self {
            workflow_graph_repo,
            lifecycle_run_repo,
            workflow_graph_instance_repo,
            claim_repo,
            lifecycle_subject_association_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            agent_assignment_repo,
        }
    }

    pub async fn cancel_subject_execution(
        &self,
        command: CancelSubjectExecutionCommand,
    ) -> Result<SubjectExecutionCancelResult, WorkflowApplicationError> {
        let target = self.resolve_cancel_target(&command.subject_ref).await?;

        self.apply_cancel_event(&target, command.reason.clone())
            .await?;
        self.abandon_claim(&target).await?;
        self.release_assignment(&target).await?;

        let runtime_delivery = self.runtime_delivery_command(
            &target,
            command.runtime_selection_policy,
            command.reason.clone(),
        );

        Ok(SubjectExecutionCancelResult {
            subject_ref: command.subject_ref,
            association_ref: target.association.id,
            run_ref: target.assignment.run_id,
            graph_instance_ref: target.assignment.graph_instance_id,
            agent_ref: target.agent.id,
            frame_ref: target.delivery_frame.id,
            assignment_ref: target.assignment.id,
            activity_key: target.assignment.activity_key,
            attempt: target.assignment.attempt,
            runtime_delivery,
        })
    }

    pub async fn prepare_runtime_cancel_delivery(
        &self,
        subject_ref: &SubjectRef,
        policy: RuntimeSessionSelectionPolicy,
        reason: Option<String>,
    ) -> Result<Option<RuntimeCancelDeliveryCommand>, WorkflowApplicationError> {
        let target = self.resolve_cancel_target(subject_ref).await?;
        Ok(self.runtime_delivery_command(&target, policy, reason))
    }

    async fn resolve_cancel_target(
        &self,
        subject_ref: &SubjectRef,
    ) -> Result<SubjectExecutionCancelTarget, WorkflowApplicationError> {
        let associations = self
            .lifecycle_subject_association_repo
            .list_by_subject(subject_ref)
            .await?;
        let association = select_subject_association(&associations).ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!(
                "subject execution 不存在: {}:{}",
                subject_ref.kind, subject_ref.id
            ))
        })?;

        let agent = self.resolve_associated_agent(&association).await?;
        let assignment = self
            .resolve_active_assignment(&association, agent.id)
            .await?;
        let delivery_frame = self.resolve_delivery_frame(&agent, &assignment).await?;

        Ok(SubjectExecutionCancelTarget {
            association,
            agent,
            assignment,
            delivery_frame,
        })
    }

    async fn resolve_associated_agent(
        &self,
        association: &LifecycleSubjectAssociation,
    ) -> Result<LifecycleAgent, WorkflowApplicationError> {
        if let Some(agent_id) = association.anchor_agent_id {
            let agent = self
                .lifecycle_agent_repo
                .get(agent_id)
                .await?
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "subject execution anchor agent 不存在: {agent_id}"
                    ))
                })?;
            if agent.run_id != association.anchor_run_id {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "subject execution anchor agent {} 不属于 run {}",
                    agent.id, association.anchor_run_id
                )));
            }
            if agent.status != "active" {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "subject execution anchor agent {} 不是 active 状态",
                    agent.id
                )));
            }
            return Ok(agent);
        }

        self.lifecycle_agent_repo
            .list_by_run(association.anchor_run_id)
            .await?
            .into_iter()
            .filter(|agent| agent.status == "active")
            .max_by_key(|agent| agent.updated_at)
            .ok_or_else(|| {
                WorkflowApplicationError::Conflict(format!(
                    "subject execution run {} 没有 active lifecycle agent",
                    association.anchor_run_id
                ))
            })
    }

    async fn resolve_active_assignment(
        &self,
        association: &LifecycleSubjectAssociation,
        agent_id: Uuid,
    ) -> Result<AgentAssignment, WorkflowApplicationError> {
        self.agent_assignment_repo
            .list_by_run(association.anchor_run_id)
            .await?
            .into_iter()
            .filter(|assignment| assignment.agent_id == agent_id)
            .filter(|assignment| assignment.lease_status == "active")
            .max_by_key(|assignment| assignment.assigned_at)
            .ok_or_else(|| {
                WorkflowApplicationError::Conflict(format!(
                    "subject execution {}:{} 没有 active assignment",
                    association.subject_kind, association.subject_id
                ))
            })
    }

    async fn resolve_delivery_frame(
        &self,
        agent: &LifecycleAgent,
        assignment: &AgentAssignment,
    ) -> Result<AgentFrame, WorkflowApplicationError> {
        let frame_id = agent.current_frame_id.ok_or_else(|| {
            WorkflowApplicationError::Conflict(format!(
                "lifecycle agent {} 缺少 current AgentFrame",
                agent.id
            ))
        })?;
        let frame = self.agent_frame_repo.get(frame_id).await?.ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!("agent frame 不存在: {frame_id}"))
        })?;
        if frame.agent_id != agent.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "agent frame {} 不属于 lifecycle agent {}",
                frame.id, agent.id
            )));
        }
        if frame.graph_instance_id != Some(assignment.graph_instance_id)
            || frame.activity_key.as_deref() != Some(assignment.activity_key.as_str())
        {
            return Err(WorkflowApplicationError::Conflict(format!(
                "agent frame {} 与 assignment {} 的 activity scope 不一致",
                frame.id, assignment.id
            )));
        }
        Ok(frame)
    }

    async fn apply_cancel_event(
        &self,
        target: &SubjectExecutionCancelTarget,
        reason: Option<String>,
    ) -> Result<(), WorkflowApplicationError> {
        let attempt = u32::try_from(target.assignment.attempt).map_err(|_| {
            WorkflowApplicationError::Conflict(format!(
                "assignment {} 的 attempt 非法: {}",
                target.assignment.id, target.assignment.attempt
            ))
        })?;
        let activity_service = ActivityLifecycleRunService::new(
            self.workflow_graph_repo,
            self.lifecycle_run_repo,
            self.workflow_graph_instance_repo,
            self.claim_repo,
        );
        activity_service
            .apply_event(
                target.assignment.graph_instance_id,
                ActivityEvent::ActivityCancelled {
                    activity_key: target.assignment.activity_key.clone(),
                    attempt,
                    reason,
                },
            )
            .await?;
        Ok(())
    }

    async fn abandon_claim(
        &self,
        target: &SubjectExecutionCancelTarget,
    ) -> Result<(), WorkflowApplicationError> {
        let idempotency_key = format!(
            "{}:{}:{}:{}",
            target.assignment.run_id,
            target.assignment.graph_instance_id,
            target.assignment.activity_key,
            target.assignment.attempt
        );
        let Some(mut claim) = self
            .claim_repo
            .get_by_idempotency_key(&idempotency_key)
            .await?
        else {
            return Ok(());
        };
        if !claim.status.is_active() {
            return Ok(());
        }
        claim.status = ActivityExecutionClaimStatus::Abandoned;
        claim.updated_at = chrono::Utc::now();
        self.claim_repo.update(&claim).await?;
        Ok(())
    }

    async fn release_assignment(
        &self,
        target: &SubjectExecutionCancelTarget,
    ) -> Result<(), WorkflowApplicationError> {
        let mut assignment = target.assignment.clone();
        assignment.release();
        self.agent_assignment_repo.update(&assignment).await?;
        Ok(())
    }

    fn runtime_delivery_command(
        &self,
        target: &SubjectExecutionCancelTarget,
        policy: RuntimeSessionSelectionPolicy,
        reason: Option<String>,
    ) -> Option<RuntimeCancelDeliveryCommand> {
        target
            .delivery_frame
            .select_runtime_session_id(policy)
            .map(|runtime_session_id| RuntimeCancelDeliveryCommand {
                runtime_session_id,
                run_ref: target.assignment.run_id,
                graph_instance_ref: target.assignment.graph_instance_id,
                agent_ref: target.agent.id,
                frame_ref: target.delivery_frame.id,
                assignment_ref: target.assignment.id,
                reason,
            })
    }
}

fn select_subject_association(
    associations: &[LifecycleSubjectAssociation],
) -> Option<LifecycleSubjectAssociation> {
    associations
        .iter()
        .find(|association| association.anchor_agent_id.is_some())
        .or_else(|| associations.first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivityAttemptStatus, ActivityCompletionPolicy, ActivityDefinition,
        ActivityExecutionClaim, ActivityExecutorSpec, ActivityLifecycleRunState,
        AgentActivityExecutorSpec, DefinitionSource, ExecutorRunRef, LifecycleRun, WorkflowGraph,
        WorkflowGraphInstance,
    };

    use super::*;
    use crate::workflow::LifecycleEngine;

    struct GraphRepo {
        graph: WorkflowGraph,
    }

    #[async_trait::async_trait]
    impl WorkflowGraphRepository for GraphRepo {
        async fn create(&self, _lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok((self.graph.id == id).then(|| self.graph.clone()))
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(
                (self.graph.project_id == project_id && self.graph.key == key)
                    .then(|| self.graph.clone()),
            )
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<WorkflowGraph>, DomainError> {
            Ok((self.graph.project_id == project_id)
                .then(|| vec![self.graph.clone()])
                .unwrap_or_default())
        }

        async fn update(&self, _lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for RunRepo {
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

        async fn list_by_lifecycle(
            &self,
            lifecycle_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.lifecycle_id == lifecycle_id)
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

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct GraphInstanceRepo {
        instances: Mutex<Vec<WorkflowGraphInstance>>,
    }

    #[async_trait::async_trait]
    impl WorkflowGraphInstanceRepository for GraphInstanceRepo {
        async fn create(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            self.instances.lock().unwrap().push(instance.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .instances
                .lock()
                .unwrap()
                .iter()
                .find(|instance| instance.id == id)
                .cloned())
        }

        async fn get_by_run_and_id(
            &self,
            run_id: Uuid,
            id: Uuid,
        ) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .instances
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
                .instances
                .lock()
                .unwrap()
                .iter()
                .filter(|instance| instance.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            let mut instances = self.instances.lock().unwrap();
            if let Some(existing) = instances
                .iter_mut()
                .find(|existing| existing.id == instance.id)
            {
                *existing = instance.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct ClaimRepo {
        claims: Mutex<Vec<ActivityExecutionClaim>>,
    }

    #[async_trait::async_trait]
    impl ActivityExecutionClaimRepository for ClaimRepo {
        async fn create_or_get(
            &self,
            claim: &ActivityExecutionClaim,
        ) -> Result<ActivityExecutionClaim, DomainError> {
            self.claims.lock().unwrap().push(claim.clone());
            Ok(claim.clone())
        }

        async fn get_by_idempotency_key(
            &self,
            idempotency_key: &str,
        ) -> Result<Option<ActivityExecutionClaim>, DomainError> {
            Ok(self
                .claims
                .lock()
                .unwrap()
                .iter()
                .find(|claim| claim.idempotency_key == idempotency_key)
                .cloned())
        }

        async fn list_active_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
            Ok(self
                .claims
                .lock()
                .unwrap()
                .iter()
                .filter(|claim| claim.run_id == run_id && claim.status.is_active())
                .cloned()
                .collect())
        }

        async fn update(&self, claim: &ActivityExecutionClaim) -> Result<(), DomainError> {
            let mut claims = self.claims.lock().unwrap();
            if let Some(existing) = claims
                .iter_mut()
                .find(|existing| existing.idempotency_key == claim.idempotency_key)
            {
                *existing = claim.clone();
            }
            Ok(())
        }

        async fn abandon_claiming_before(
            &self,
            _cutoff: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct SubjectAssociationRepo {
        associations: Mutex<Vec<LifecycleSubjectAssociation>>,
    }

    #[async_trait::async_trait]
    impl LifecycleSubjectAssociationRepository for SubjectAssociationRepo {
        async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
            self.associations.lock().unwrap().push(assoc.clone());
            Ok(())
        }

        async fn list_by_subject(
            &self,
            subject: &SubjectRef,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .associations
                .lock()
                .unwrap()
                .iter()
                .filter(|assoc| {
                    assoc.subject_kind == subject.kind && assoc.subject_id == subject.id
                })
                .cloned()
                .collect())
        }

        async fn list_by_anchor(
            &self,
            run_id: Uuid,
            agent_id: Option<Uuid>,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .associations
                .lock()
                .unwrap()
                .iter()
                .filter(|assoc| assoc.anchor_run_id == run_id && assoc.anchor_agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.associations
                .lock()
                .unwrap()
                .retain(|assoc| assoc.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct AgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait::async_trait]
    impl LifecycleAgentRepository for AgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().unwrap().push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut agents = self.agents.lock().unwrap();
            if let Some(existing) = agents.iter_mut().find(|existing| existing.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct FrameRepo {
        frames: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait::async_trait]
    impl AgentFrameRepository for FrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.frames.lock().unwrap().push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn attach_runtime_session_ref(
            &self,
            frame_id: Uuid,
            runtime_session_id: &str,
        ) -> Result<(), DomainError> {
            let mut frames = self.frames.lock().unwrap();
            if let Some(frame) = frames.iter_mut().find(|frame| frame.id == frame_id) {
                frame.attach_runtime_session_ref(runtime_session_id);
            }
            Ok(())
        }

        async fn find_by_runtime_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .iter()
                .find(|frame| {
                    frame
                        .runtime_session_ids()
                        .iter()
                        .any(|session_id| session_id == runtime_session_id)
                })
                .cloned())
        }
    }

    #[derive(Default)]
    struct AssignmentRepo {
        assignments: Mutex<Vec<AgentAssignment>>,
    }

    #[async_trait::async_trait]
    impl AgentAssignmentRepository for AssignmentRepo {
        async fn create(&self, assignment: &AgentAssignment) -> Result<(), DomainError> {
            self.assignments.lock().unwrap().push(assignment.clone());
            Ok(())
        }

        async fn find_for_attempt(
            &self,
            graph_instance_id: Uuid,
            activity_key: &str,
            attempt: i32,
        ) -> Result<Option<AgentAssignment>, DomainError> {
            Ok(self
                .assignments
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

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentAssignment>, DomainError> {
            Ok(self
                .assignments
                .lock()
                .unwrap()
                .iter()
                .filter(|assignment| assignment.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, assignment: &AgentAssignment) -> Result<(), DomainError> {
            let mut assignments = self.assignments.lock().unwrap();
            if let Some(existing) = assignments
                .iter_mut()
                .find(|existing| existing.id == assignment.id)
            {
                *existing = assignment.clone();
            }
            Ok(())
        }
    }

    fn graph(project_id: Uuid) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            "task.freeform",
            "Task Freeform",
            "",
            DefinitionSource::UserAuthored,
            "main",
            vec![ActivityDefinition {
                key: "main".to_string(),
                description: "main".to_string(),
                executor: ActivityExecutorSpec::Agent(
                    AgentActivityExecutorSpec::create_activity_agent("task_agent"),
                ),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            }],
            Vec::new(),
        )
        .expect("graph")
    }

    fn running_state(graph: &WorkflowGraph, graph_instance_id: Uuid) -> ActivityLifecycleRunState {
        let mut state = LifecycleEngine::initialize(graph, graph_instance_id).expect("init");
        LifecycleEngine::apply_event(
            graph,
            &mut state,
            ActivityEvent::SchedulerClaimAccepted {
                activity_key: "main".to_string(),
                attempt: 1,
            },
        )
        .expect("claim");
        LifecycleEngine::apply_event(
            graph,
            &mut state,
            ActivityEvent::ExecutorStarted {
                activity_key: "main".to_string(),
                attempt: 1,
                executor_run: ExecutorRunRef::RuntimeSession {
                    session_id: "runtime-1".to_string(),
                },
            },
        )
        .expect("started");
        state
    }

    #[tokio::test]
    async fn cancel_subject_execution_targets_assignment_and_delivers_latest_runtime_ref() {
        let project_id = Uuid::new_v4();
        let subject = SubjectRef::new("task", Uuid::new_v4());
        let graph = graph(project_id);
        let graph_repo = GraphRepo {
            graph: graph.clone(),
        };
        let run_repo = RunRepo::default();
        let graph_instance_repo = GraphInstanceRepo::default();
        let claim_repo = ClaimRepo::default();
        let association_repo = SubjectAssociationRepo::default();
        let agent_repo = AgentRepo::default();
        let frame_repo = FrameRepo::default();
        let assignment_repo = AssignmentRepo::default();

        let mut run = LifecycleRun::new_control(project_id, graph.id);
        let mut graph_instance = WorkflowGraphInstance::new_root(run.id, graph.id);
        graph_instance
            .replace_activity_state(running_state(&graph, graph_instance.id))
            .expect("state");
        run.sync_graph_instance_activity_projections(
            graph_instance
                .activity_state
                .as_ref()
                .map(|state| (graph_instance.id, state))
                .into_iter(),
        );
        run_repo.create(&run).await.expect("run");
        graph_instance_repo
            .create(&graph_instance)
            .await
            .expect("instance");

        let mut agent = LifecycleAgent::new_root(run.id, project_id, "task_agent");
        let mut frame = AgentFrame::new_revision(agent.id, 1, "test");
        frame.graph_instance_id = Some(graph_instance.id);
        frame.activity_key = Some("main".to_string());
        frame.runtime_session_refs_json =
            AgentFrame::runtime_session_refs_json(["runtime-1", "runtime-2"]);
        agent.set_current_frame(frame.id);
        agent_repo.create(&agent).await.expect("agent");
        frame_repo.create(&frame).await.expect("frame");

        let assignment =
            AgentAssignment::new(run.id, graph_instance.id, "main", 1, agent.id, frame.id);
        assignment_repo
            .create(&assignment)
            .await
            .expect("assignment");
        association_repo
            .create(&LifecycleSubjectAssociation::new_agent_scoped(
                run.id,
                agent.id,
                &subject,
                "task_execution",
                None,
            ))
            .await
            .expect("association");

        let mut claim = ActivityExecutionClaim::new(run.id, graph_instance.id, "main", 1, "agent");
        claim.status = ActivityExecutionClaimStatus::Running;
        claim.executor_run_ref = Some(ExecutorRunRef::RuntimeSession {
            session_id: "runtime-2".to_string(),
        });
        claim_repo.create_or_get(&claim).await.expect("claim");

        let service = SubjectExecutionControlService::new(
            &graph_repo,
            &run_repo,
            &graph_instance_repo,
            &claim_repo,
            &association_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
        );
        let result = service
            .cancel_subject_execution(CancelSubjectExecutionCommand {
                subject_ref: subject.clone(),
                runtime_selection_policy: RuntimeSessionSelectionPolicy::LatestAttached,
                reason: Some("user cancel".to_string()),
            })
            .await
            .expect("cancel");

        assert_eq!(result.subject_ref, subject);
        assert_eq!(result.assignment_ref, assignment.id);
        assert_eq!(
            result
                .runtime_delivery
                .as_ref()
                .map(|command| command.runtime_session_id.as_str()),
            Some("runtime-2")
        );

        let persisted_instance = graph_instance_repo
            .get(graph_instance.id)
            .await
            .expect("query instance")
            .expect("instance");
        let state = persisted_instance.activity_state.expect("state");
        assert_eq!(
            state.status,
            agentdash_domain::workflow::ActivityRunStatus::Cancelled
        );
        assert_eq!(state.attempts[0].status, ActivityAttemptStatus::Cancelled);

        let claim = claim_repo
            .get_by_idempotency_key(&claim.idempotency_key)
            .await
            .expect("query claim")
            .expect("claim");
        assert_eq!(claim.status, ActivityExecutionClaimStatus::Abandoned);

        let assignment = assignment_repo
            .find_for_attempt(graph_instance.id, "main", 1)
            .await
            .expect("query assignment")
            .expect("assignment");
        assert_eq!(assignment.lease_status, "released");
        assert!(assignment.released_at.is_some());
    }

    #[tokio::test]
    async fn cancel_target_rejects_inactive_anchor_agent() {
        let project_id = Uuid::new_v4();
        let subject = SubjectRef::new("task", Uuid::new_v4());
        let graph = graph(project_id);
        let graph_repo = GraphRepo {
            graph: graph.clone(),
        };
        let run_repo = RunRepo::default();
        let graph_instance_repo = GraphInstanceRepo::default();
        let claim_repo = ClaimRepo::default();
        let association_repo = SubjectAssociationRepo::default();
        let agent_repo = AgentRepo::default();
        let frame_repo = FrameRepo::default();
        let assignment_repo = AssignmentRepo::default();

        let run = LifecycleRun::new_control(project_id, graph.id);
        let mut agent = LifecycleAgent::new_root(run.id, project_id, "task_agent");
        agent.status = "completed".to_string();
        agent_repo.create(&agent).await.expect("agent");
        let association = LifecycleSubjectAssociation::new_agent_scoped(
            run.id,
            agent.id,
            &subject,
            "task_execution",
            None,
        );

        let service = SubjectExecutionControlService::new(
            &graph_repo,
            &run_repo,
            &graph_instance_repo,
            &claim_repo,
            &association_repo,
            &agent_repo,
            &frame_repo,
            &assignment_repo,
        );
        let err = service
            .resolve_associated_agent(&association)
            .await
            .expect_err("inactive anchor agent should be rejected");

        assert!(matches!(err, WorkflowApplicationError::Conflict(_)));
    }
}
