use uuid::Uuid;

use agentdash_application_workflow::orchestration::{
    OrchestrationRuntimeEvent, apply_orchestration_event_to_run,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgent, LifecycleAgentRepository, LifecycleRunRepository,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository, OrchestrationBindingRefs,
    RuntimeControlRefs, RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    SubjectRef,
};

use super::WorkflowApplicationError;

#[derive(Debug, Clone)]
struct DeliveryRuntimeSelection {
    run_id: Uuid,
    agent_id: Uuid,
    current_frame_id: Uuid,
    runtime_session_id: String,
    anchor: RuntimeSessionExecutionAnchor,
}

#[derive(Debug, thiserror::Error)]
enum DeliveryRuntimeSelectionError {
    #[error("LifecycleRun {run_id} 不存在")]
    RunNotFound { run_id: Uuid },
    #[error("LifecycleAgent {agent_id} 不存在")]
    AgentNotFound { agent_id: Uuid },
    #[error("LifecycleAgent {agent_id} 属于 run {actual_run_id}，不匹配请求 run {run_id}")]
    AgentRunMismatch {
        run_id: Uuid,
        agent_id: Uuid,
        actual_run_id: Uuid,
    },
    #[error("LifecycleAgent {agent_id} 缺少 current delivery binding")]
    CurrentDeliveryMissing { run_id: Uuid, agent_id: Uuid },
    #[error("RuntimeSessionExecutionAnchor {runtime_session_id} 不存在")]
    AnchorMissing { runtime_session_id: String },
    #[error(
        "RuntimeSessionExecutionAnchor {runtime_session_id} 指向 run {actual_run_id}/agent {actual_agent_id}/launch frame {actual_launch_frame_id}，不匹配期望 run {expected_run_id}/agent {expected_agent_id}/launch frame {expected_launch_frame_id}"
    )]
    AnchorMismatch {
        runtime_session_id: String,
        expected_run_id: Uuid,
        expected_agent_id: Uuid,
        expected_launch_frame_id: Uuid,
        actual_run_id: Uuid,
        actual_agent_id: Uuid,
        actual_launch_frame_id: Uuid,
    },
    #[error("LifecycleAgent {agent_id} 缺少当前 AgentFrame revision")]
    CurrentFrameMissing { agent_id: Uuid },
    #[error("AgentFrame {frame_id} 不存在")]
    CurrentFrameNotFound { frame_id: Uuid },
    #[error(transparent)]
    Repository(#[from] agentdash_domain::DomainError),
}

#[derive(Debug, Clone)]
pub struct CancelSubjectExecutionCommand {
    pub subject_ref: SubjectRef,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeCancelDeliveryCommand {
    pub runtime_session_id: String,
    pub runtime_refs: RuntimeControlRefs,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubjectExecutionCancelResult {
    pub subject_ref: SubjectRef,
    pub association_ref: Uuid,
    pub runtime_refs: RuntimeControlRefs,
    pub activity_key: Option<String>,
    pub attempt: Option<i32>,
    pub runtime_delivery: Option<RuntimeCancelDeliveryCommand>,
}

struct SubjectExecutionCancelTarget {
    association: LifecycleSubjectAssociation,
    selection: DeliveryRuntimeSelection,
}

pub struct SubjectExecutionControlService<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
}

impl<'a> SubjectExecutionControlService<'a> {
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_subject_association_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
        }
    }

    pub async fn cancel_subject_execution(
        &self,
        command: CancelSubjectExecutionCommand,
    ) -> Result<SubjectExecutionCancelResult, WorkflowApplicationError> {
        let target = self.resolve_cancel_target(&command.subject_ref).await?;

        self.materialize_cancelled_node(&target, command.reason.clone())
            .await?;

        let runtime_delivery = self
            .runtime_delivery_command(&target, command.reason.clone())
            .await?;

        Ok(SubjectExecutionCancelResult {
            subject_ref: command.subject_ref,
            association_ref: target.association.id,
            runtime_refs: runtime_refs_for_target(&target),
            activity_key: None,
            attempt: None,
            runtime_delivery,
        })
    }

    pub async fn prepare_runtime_cancel_delivery(
        &self,
        subject_ref: &SubjectRef,
        reason: Option<String>,
    ) -> Result<Option<RuntimeCancelDeliveryCommand>, WorkflowApplicationError> {
        let target = self.resolve_cancel_target(subject_ref).await?;
        self.runtime_delivery_command(&target, reason).await
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
        let selection = select_current_delivery(
            self.lifecycle_run_repo,
            self.lifecycle_agent_repo,
            self.agent_frame_repo,
            self.execution_anchor_repo,
            association.anchor_run_id,
            agent.id,
        )
        .await
        .map_err(workflow_error_from_selection_error)?;

        Ok(SubjectExecutionCancelTarget {
            association,
            selection,
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

    async fn materialize_cancelled_node(
        &self,
        target: &SubjectExecutionCancelTarget,
        reason: Option<String>,
    ) -> Result<(), WorkflowApplicationError> {
        let anchor = &target.selection.anchor;
        let Some(orchestration_id) = anchor.orchestration_id else {
            return Ok(());
        };
        let Some(node_path) = anchor.node_path.as_deref() else {
            return Ok(());
        };
        let mut run = self
            .lifecycle_run_repo
            .get_by_id(target.association.anchor_run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle run 不存在: {}",
                    target.association.anchor_run_id
                ))
            })?;
        let (next_run, _) = apply_orchestration_event_to_run(
            run,
            orchestration_id,
            OrchestrationRuntimeEvent::NodeCancelled {
                node_path: node_path.to_string(),
                attempt: anchor.node_attempt.unwrap_or(1),
                reason,
                timestamp: chrono::Utc::now(),
            },
        )
        .map_err(|error| WorkflowApplicationError::Conflict(error.to_string()))?;
        run = next_run;
        self.lifecycle_run_repo.update(&run).await?;
        Ok(())
    }

    async fn runtime_delivery_command(
        &self,
        target: &SubjectExecutionCancelTarget,
        reason: Option<String>,
    ) -> Result<Option<RuntimeCancelDeliveryCommand>, WorkflowApplicationError> {
        Ok(Some(RuntimeCancelDeliveryCommand {
            runtime_session_id: target.selection.runtime_session_id.clone(),
            runtime_refs: runtime_refs_for_target(target),
            reason,
        }))
    }
}

fn runtime_refs_for_target(target: &SubjectExecutionCancelTarget) -> RuntimeControlRefs {
    let anchor = &target.selection.anchor;
    let orchestration_binding = match (anchor.orchestration_id, anchor.node_path.clone()) {
        (Some(orchestration_id), Some(node_path)) => Some(OrchestrationBindingRefs::new(
            orchestration_id,
            node_path,
            anchor.node_attempt.unwrap_or(1),
        )),
        _ => None,
    };
    RuntimeControlRefs::new(
        target.selection.run_id,
        target.selection.agent_id,
        target.selection.current_frame_id,
        orchestration_binding,
    )
}

async fn select_current_delivery(
    lifecycle_run_repo: &dyn LifecycleRunRepository,
    lifecycle_agent_repo: &dyn LifecycleAgentRepository,
    agent_frame_repo: &dyn AgentFrameRepository,
    execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<DeliveryRuntimeSelection, DeliveryRuntimeSelectionError> {
    lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or(DeliveryRuntimeSelectionError::RunNotFound { run_id })?;
    let agent = lifecycle_agent_repo
        .get(agent_id)
        .await?
        .ok_or(DeliveryRuntimeSelectionError::AgentNotFound { agent_id })?;
    if agent.run_id != run_id {
        return Err(DeliveryRuntimeSelectionError::AgentRunMismatch {
            run_id,
            agent_id,
            actual_run_id: agent.run_id,
        });
    }
    let binding = agent
        .current_delivery
        .clone()
        .ok_or(DeliveryRuntimeSelectionError::CurrentDeliveryMissing { run_id, agent_id })?;
    let anchor = execution_anchor_repo
        .find_by_session(&binding.runtime_session_id)
        .await?
        .ok_or_else(|| DeliveryRuntimeSelectionError::AnchorMissing {
            runtime_session_id: binding.runtime_session_id.clone(),
        })?;
    validate_anchor_matches(&anchor, run_id, agent_id, binding.launch_frame_id)?;
    let current_frame = agent_frame_repo
        .get_current(agent.id)
        .await?
        .ok_or(DeliveryRuntimeSelectionError::CurrentFrameMissing { agent_id })?;
    if current_frame.agent_id != agent_id {
        return Err(DeliveryRuntimeSelectionError::CurrentFrameNotFound {
            frame_id: current_frame.id,
        });
    }
    Ok(DeliveryRuntimeSelection {
        run_id: anchor.run_id,
        agent_id: anchor.agent_id,
        current_frame_id: current_frame.id,
        runtime_session_id: anchor.runtime_session_id.clone(),
        anchor,
    })
}

fn validate_anchor_matches(
    anchor: &RuntimeSessionExecutionAnchor,
    expected_run_id: Uuid,
    expected_agent_id: Uuid,
    expected_launch_frame_id: Uuid,
) -> Result<(), DeliveryRuntimeSelectionError> {
    if anchor.run_id == expected_run_id
        && anchor.agent_id == expected_agent_id
        && anchor.launch_frame_id == expected_launch_frame_id
    {
        return Ok(());
    }
    Err(DeliveryRuntimeSelectionError::AnchorMismatch {
        runtime_session_id: anchor.runtime_session_id.clone(),
        expected_run_id,
        expected_agent_id,
        expected_launch_frame_id,
        actual_run_id: anchor.run_id,
        actual_agent_id: anchor.agent_id,
        actual_launch_frame_id: anchor.launch_frame_id,
    })
}

fn workflow_error_from_selection_error(
    error: DeliveryRuntimeSelectionError,
) -> WorkflowApplicationError {
    match error {
        DeliveryRuntimeSelectionError::RunNotFound { .. }
        | DeliveryRuntimeSelectionError::AgentNotFound { .. }
        | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. } => {
            WorkflowApplicationError::NotFound(error.to_string())
        }
        DeliveryRuntimeSelectionError::Repository(source) => WorkflowApplicationError::from(source),
        other => WorkflowApplicationError::Conflict(other.to_string()),
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
        AgentFrame, AgentSource, DeliveryBindingStatus, LifecycleRun, LifecycleRunStatus,
        OrchestrationInstance, OrchestrationPlanSnapshot, OrchestrationSourceRef,
        OrchestrationStatus, PlanNodeKind, RuntimeNodeState, RuntimeNodeStatus,
        RuntimeSessionExecutionAnchor,
    };
    use chrono::Utc;

    use super::*;

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

        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct AnchorRepo {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    impl AnchorRepo {
        fn insert(&self, anchor: RuntimeSessionExecutionAnchor) {
            self.anchors.lock().unwrap().push(anchor);
        }
    }

    #[async_trait::async_trait]
    impl RuntimeSessionExecutionAnchorRepository for AnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut anchors = self.anchors.lock().unwrap();
            if let Some(existing) = anchors
                .iter()
                .find(|existing| existing.runtime_session_id == anchor.runtime_session_id)
            {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            anchors.push(anchor.clone());
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

    fn run_with_running_node(project_id: Uuid) -> (LifecycleRun, Uuid) {
        let graph_id = Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id);
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id,
            graph_version: Some(1),
        };
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_digest: "sha256:subject-cancel-test".to_string(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: Vec::new(),
            entry_node_ids: vec!["main".to_string()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        let mut orchestration = OrchestrationInstance::new("root", source_ref, plan_snapshot);
        orchestration.status = OrchestrationStatus::Running;
        orchestration.node_tree = vec![RuntimeNodeState {
            node_id: "main".to_string(),
            node_path: "main".to_string(),
            kind: PlanNodeKind::AgentCall,
            status: RuntimeNodeStatus::Running,
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: None,
            children: Vec::new(),
            phase_path: Vec::new(),
            started_at: Some(Utc::now()),
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        }];
        let orchestration_id = orchestration.orchestration_id;
        run.add_orchestration(orchestration);
        (run, orchestration_id)
    }

    #[tokio::test]
    async fn cancel_subject_execution_targets_orchestration_node_and_delivers_runtime_ref() {
        let project_id = Uuid::new_v4();
        let subject = SubjectRef::new("task", Uuid::new_v4());
        let run_repo = RunRepo::default();
        let association_repo = SubjectAssociationRepo::default();
        let agent_repo = AgentRepo::default();
        let frame_repo = FrameRepo::default();
        let anchor_repo = AnchorRepo::default();

        let (run, orchestration_id) = run_with_running_node(project_id);
        run_repo.create(&run).await.expect("run");

        let mut agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::Unknown);
        let frame = AgentFrame::new_revision(agent.id, 1, "test");
        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "runtime-2",
            run.id,
            frame.id,
            agent.id,
            orchestration_id,
            "main",
            1,
        );
        agent.bind_current_delivery_from_anchor(
            &anchor,
            DeliveryBindingStatus::Running,
            anchor.updated_at,
        );
        agent_repo.create(&agent).await.expect("agent");
        frame_repo.create(&frame).await.expect("frame");
        anchor_repo.insert(anchor);
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

        let service = SubjectExecutionControlService::new(
            &run_repo,
            &association_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
        );
        let result = service
            .cancel_subject_execution(CancelSubjectExecutionCommand {
                subject_ref: subject.clone(),
                reason: Some("user cancel".to_string()),
            })
            .await
            .expect("cancel");

        assert_eq!(result.subject_ref, subject);
        assert_eq!(
            result.runtime_refs.orchestration_ref(),
            Some(orchestration_id)
        );
        assert_eq!(result.runtime_refs.node_path(), Some("main"));
        assert_eq!(result.runtime_refs.node_attempt(), Some(1));
        assert_eq!(
            result
                .runtime_delivery
                .as_ref()
                .map(|command| command.runtime_session_id.as_str()),
            Some("runtime-2")
        );

        let persisted_run = run_repo
            .get_by_id(run.id)
            .await
            .expect("query run")
            .expect("run");
        assert_eq!(persisted_run.status, LifecycleRunStatus::Cancelled);
        let node = &persisted_run.orchestrations[0].node_tree[0];
        assert_eq!(node.status, RuntimeNodeStatus::Cancelled);
        assert_eq!(
            node.error.as_ref().map(|error| error.code.as_str()),
            Some("runtime_node_cancelled")
        );
    }

    #[tokio::test]
    async fn cancel_plain_subject_execution_delivers_runtime_without_orchestration_binding() {
        let project_id = Uuid::new_v4();
        let subject = SubjectRef::new("task", Uuid::new_v4());
        let run_repo = RunRepo::default();
        let association_repo = SubjectAssociationRepo::default();
        let agent_repo = AgentRepo::default();
        let frame_repo = FrameRepo::default();
        let anchor_repo = AnchorRepo::default();

        let run = LifecycleRun::new_plain(project_id);
        run_repo.create(&run).await.expect("run");
        let mut agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::Unknown);
        let frame = AgentFrame::new_revision(agent.id, 1, "test");
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-plain-1",
            run.id,
            frame.id,
            agent.id,
        );
        agent.bind_current_delivery_from_anchor(
            &anchor,
            DeliveryBindingStatus::Running,
            anchor.updated_at,
        );
        agent_repo.create(&agent).await.expect("agent");
        frame_repo.create(&frame).await.expect("frame");
        anchor_repo.insert(anchor);
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

        let service = SubjectExecutionControlService::new(
            &run_repo,
            &association_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
        );
        let result = service
            .cancel_subject_execution(CancelSubjectExecutionCommand {
                subject_ref: subject.clone(),
                reason: Some("user cancel".to_string()),
            })
            .await
            .expect("cancel");

        assert_eq!(result.subject_ref, subject);
        assert_eq!(result.runtime_refs.run_ref, run.id);
        assert_eq!(result.runtime_refs.agent_ref, agent.id);
        assert_eq!(result.runtime_refs.frame_ref, frame.id);
        assert_eq!(result.runtime_refs.orchestration_ref(), None);
        assert_eq!(result.activity_key, None);
        assert_eq!(result.attempt, None);
        let command = result.runtime_delivery.expect("runtime delivery");
        assert_eq!(command.runtime_session_id, "runtime-plain-1");
        assert_eq!(command.runtime_refs.orchestration_ref(), None);
    }

    #[tokio::test]
    async fn cancel_target_rejects_inactive_anchor_agent() {
        let project_id = Uuid::new_v4();
        let subject = SubjectRef::new("task", Uuid::new_v4());
        let run_repo = RunRepo::default();
        let association_repo = SubjectAssociationRepo::default();
        let agent_repo = AgentRepo::default();
        let frame_repo = FrameRepo::default();
        let anchor_repo = AnchorRepo::default();

        let run = LifecycleRun::new_control(project_id);
        let mut agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::Unknown);
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
            &run_repo,
            &association_repo,
            &agent_repo,
            &frame_repo,
            &anchor_repo,
        );
        let err = service
            .resolve_associated_agent(&association)
            .await
            .expect_err("inactive anchor agent should be rejected");

        assert!(matches!(err, WorkflowApplicationError::Conflict(_)));
    }
}
