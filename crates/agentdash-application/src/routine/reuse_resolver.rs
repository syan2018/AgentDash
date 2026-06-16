use uuid::Uuid;

use agentdash_domain::routine::{DispatchStrategy, Routine, RoutineDispatchRefs, RoutineExecution};
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, SubjectRef,
};

use crate::ApplicationError;
use crate::repository_set::RepositorySet;

const ROUTINE_REUSE_SCAN_LIMIT: u32 = 50;
const ROUTINE_EXECUTION_SUBJECT_KIND: &str = "routine_execution";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineDispatchReuseTarget {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineDispatchReuseResolution {
    pub target: Option<RoutineDispatchReuseTarget>,
    pub entity_key: Option<String>,
}

pub struct LifecycleAgentReuseResolver<'a> {
    routine_execution_repo: &'a dyn agentdash_domain::routine::RoutineExecutionRepository,
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    association_repo: &'a dyn LifecycleSubjectAssociationRepository,
}

impl<'a> LifecycleAgentReuseResolver<'a> {
    pub fn from_repositories(repos: &'a RepositorySet) -> Self {
        Self {
            routine_execution_repo: repos.routine_execution_repo.as_ref(),
            lifecycle_run_repo: repos.lifecycle_run_repo.as_ref(),
            lifecycle_agent_repo: repos.lifecycle_agent_repo.as_ref(),
            agent_frame_repo: repos.agent_frame_repo.as_ref(),
            association_repo: repos.lifecycle_subject_association_repo.as_ref(),
        }
    }

    pub fn new(
        routine_execution_repo: &'a dyn agentdash_domain::routine::RoutineExecutionRepository,
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    ) -> Self {
        Self {
            routine_execution_repo,
            lifecycle_run_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            association_repo,
        }
    }

    pub async fn resolve(
        &self,
        routine: &Routine,
        execution: &RoutineExecution,
    ) -> Result<RoutineDispatchReuseResolution, ApplicationError> {
        match &routine.dispatch_strategy {
            DispatchStrategy::Fresh => Ok(RoutineDispatchReuseResolution {
                target: None,
                entity_key: None,
            }),
            DispatchStrategy::Reuse => {
                let target = self.resolve_latest_target(routine, |_| true).await?;
                let target = target.ok_or_else(|| {
                    ApplicationError::Conflict(format!(
                        "Routine {} 使用 reuse 策略，但没有可复用的 active lifecycle agent anchor",
                        routine.id
                    ))
                })?;
                Ok(RoutineDispatchReuseResolution {
                    target: Some(target),
                    entity_key: None,
                })
            }
            DispatchStrategy::PerEntity { entity_key_path } => {
                let entity_key = resolve_entity_key(routine, execution, entity_key_path)?;
                let target = self
                    .resolve_latest_target(routine, |candidate| {
                        candidate.entity_key.as_deref() == Some(entity_key.as_str())
                    })
                    .await?;
                Ok(RoutineDispatchReuseResolution {
                    target,
                    entity_key: Some(entity_key),
                })
            }
        }
    }

    async fn resolve_latest_target<F>(
        &self,
        routine: &Routine,
        mut matches_candidate: F,
    ) -> Result<Option<RoutineDispatchReuseTarget>, ApplicationError>
    where
        F: FnMut(&RoutineExecution) -> bool,
    {
        let mut executions = self
            .routine_execution_repo
            .list_by_routine(routine.id, ROUTINE_REUSE_SCAN_LIMIT, 0)
            .await
            .map_err(ApplicationError::from)?;
        executions.sort_by_key(|execution| std::cmp::Reverse(execution.started_at));

        for candidate in executions {
            if !matches_candidate(&candidate) {
                continue;
            }
            let Some(refs) = candidate.dispatch_refs.as_ref() else {
                continue;
            };
            return self
                .validate_reuse_candidate(routine, &candidate, refs)
                .await
                .map(Some);
        }

        Ok(None)
    }

    async fn validate_reuse_candidate(
        &self,
        routine: &Routine,
        candidate: &RoutineExecution,
        refs: &RoutineDispatchRefs,
    ) -> Result<RoutineDispatchReuseTarget, ApplicationError> {
        let run_id = refs.run_id();
        let agent_id = refs.agent_id();
        let frame_id = refs.frame_id();
        let orchestration_id = refs.orchestration_id();
        let node_path = refs.node_path().map(str::to_string);

        let run = self
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "RoutineExecution {} 记录的 LifecycleRun {} 不存在",
                    candidate.id, run_id
                ))
            })?;
        if run.project_id != routine.project_id {
            return Err(ApplicationError::Conflict(format!(
                "RoutineExecution {} 记录的 LifecycleRun {} 不属于 Routine project {}",
                candidate.id, run_id, routine.project_id
            )));
        }

        let agent = self
            .lifecycle_agent_repo
            .get(agent_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "RoutineExecution {} 记录的 LifecycleAgent {} 不存在",
                    candidate.id, agent_id
                ))
            })?;
        if agent.run_id != run_id {
            return Err(ApplicationError::Conflict(format!(
                "RoutineExecution {} 记录的 LifecycleAgent {} 不属于 LifecycleRun {}",
                candidate.id, agent_id, run_id
            )));
        }
        if agent.project_id != routine.project_id {
            return Err(ApplicationError::Conflict(format!(
                "RoutineExecution {} 记录的 LifecycleAgent {} 不属于 Routine project {}",
                candidate.id, agent_id, routine.project_id
            )));
        }
        if agent.status != "active" {
            return Err(ApplicationError::Conflict(format!(
                "RoutineExecution {} 记录的 LifecycleAgent {} 当前不是 active",
                candidate.id, agent_id
            )));
        }

        let frame = self
            .agent_frame_repo
            .get(frame_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "RoutineExecution {} 记录的 AgentFrame {} 不存在",
                    candidate.id, frame_id
                ))
            })?;
        if frame.agent_id != agent_id {
            return Err(ApplicationError::Conflict(format!(
                "RoutineExecution {} 记录的 AgentFrame {} 不属于 LifecycleAgent {}",
                candidate.id, frame_id, agent_id
            )));
        }

        if let Some(orchestration_id) = orchestration_id {
            let orchestration = run.orchestration_by_id(orchestration_id).ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "RoutineExecution {} 记录的 OrchestrationInstance {} 不存在",
                    candidate.id, orchestration_id
                ))
            })?;
            let Some(node_path) = node_path.as_deref() else {
                return Err(ApplicationError::Conflict(format!(
                    "RoutineExecution {} 记录了 orchestration 但缺少 node_path",
                    candidate.id
                )));
            };
            if !orchestration
                .node_tree
                .iter()
                .any(|node| node.node_path == node_path)
            {
                return Err(ApplicationError::Conflict(format!(
                    "RoutineExecution {} 记录的 orchestration node {} 不存在",
                    candidate.id, node_path
                )));
            }
        }

        let subject = SubjectRef::new(ROUTINE_EXECUTION_SUBJECT_KIND, candidate.id);
        let has_subject_association = self
            .association_repo
            .list_by_subject(&subject)
            .await
            .map_err(ApplicationError::from)?
            .into_iter()
            .any(|association| {
                association.anchor_run_id == run_id
                    && match association.anchor_agent_id {
                        Some(anchor_agent_id) => anchor_agent_id == agent_id,
                        None => true,
                    }
            });
        if !has_subject_association {
            return Err(ApplicationError::Conflict(format!(
                "RoutineExecution {} 缺少指向 LifecycleRun {} 的 subject association",
                candidate.id, run_id
            )));
        }

        Ok(RoutineDispatchReuseTarget {
            run_id,
            agent_id,
            frame_id,
            orchestration_id,
            node_path,
        })
    }
}

fn resolve_entity_key(
    routine: &Routine,
    execution: &RoutineExecution,
    entity_key_path: &str,
) -> Result<String, ApplicationError> {
    let payload = execution.trigger_payload.as_ref().ok_or_else(|| {
        ApplicationError::BadRequest(format!(
            "Routine {} 使用 per_entity 策略，但当前触发没有 payload",
            routine.id
        ))
    })?;
    let value = resolve_json_path(payload, entity_key_path).ok_or_else(|| {
        ApplicationError::BadRequest(format!(
            "Routine {} 无法从 payload 路径 `{}` 解析 entity_key",
            routine.id, entity_key_path
        ))
    })?;
    let key = json_value_to_key_string(value);
    if key.is_empty() {
        return Err(ApplicationError::BadRequest(format!(
            "Routine {} 的 entity_key 路径 `{}` 解析为空",
            routine.id, entity_key_path
        )));
    }
    Ok(key)
}

fn json_value_to_key_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.trim().to_string(),
        _ => value.to_string(),
    }
}

fn resolve_json_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::routine::{
        RoutineExecutionRepository, RoutineExecutionStatus, RoutineTriggerConfig,
    };
    use agentdash_domain::workflow::{
        AgentFrame, AgentSource, LifecycleAgent, LifecycleRun, LifecycleSubjectAssociation,
        OrchestrationInstance, OrchestrationPlanSnapshot, OrchestrationSourceRef, PlanNode,
        PlanNodeKind, RuntimeNodeState, RuntimeNodeStatus,
    };
    use chrono::Utc;

    #[derive(Default)]
    struct InMemoryRoutineExecutionRepo {
        items: Mutex<Vec<RoutineExecution>>,
    }

    #[async_trait]
    impl RoutineExecutionRepository for InMemoryRoutineExecutionRepo {
        async fn create(&self, execution: &RoutineExecution) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(execution.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<RoutineExecution>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|execution| execution.id == id)
                .cloned())
        }

        async fn update(&self, execution: &RoutineExecution) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|item| item.id == execution.id) {
                *existing = execution.clone();
            }
            Ok(())
        }

        async fn list_by_routine(
            &self,
            routine_id: Uuid,
            limit: u32,
            offset: u32,
        ) -> Result<Vec<RoutineExecution>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|execution| execution.routine_id == routine_id)
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn find_latest_by_entity_key(
            &self,
            routine_id: Uuid,
            entity_key: &str,
        ) -> Result<Option<RoutineExecution>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|execution| {
                    execution.routine_id == routine_id
                        && execution.entity_key.as_deref() == Some(entity_key)
                })
                .max_by_key(|execution| execution.started_at)
                .cloned())
        }
    }

    #[derive(Default)]
    struct InMemoryRunRepo {
        items: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
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
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
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
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|item| item.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAgentRepo {
        items: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
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
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|item| item.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryFrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait]
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
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .items
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
    struct InMemoryAssociationRepo {
        items: Mutex<Vec<LifecycleSubjectAssociation>>,
    }

    #[async_trait]
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
                .filter(|association| {
                    association.subject_kind == subject.kind && association.subject_id == subject.id
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
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|association| {
                    association.anchor_run_id == run_id && association.anchor_agent_id == agent_id
                })
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items
                .lock()
                .unwrap()
                .retain(|association| association.id != id);
            Ok(())
        }
    }

    struct ResolverFixture {
        routine_execution_repo: InMemoryRoutineExecutionRepo,
        run_repo: InMemoryRunRepo,
        agent_repo: InMemoryAgentRepo,
        frame_repo: InMemoryFrameRepo,
        association_repo: InMemoryAssociationRepo,
    }

    impl ResolverFixture {
        fn new() -> Self {
            Self {
                routine_execution_repo: InMemoryRoutineExecutionRepo::default(),
                run_repo: InMemoryRunRepo::default(),
                agent_repo: InMemoryAgentRepo::default(),
                frame_repo: InMemoryFrameRepo::default(),
                association_repo: InMemoryAssociationRepo::default(),
            }
        }

        fn resolver(&self) -> LifecycleAgentReuseResolver<'_> {
            LifecycleAgentReuseResolver::new(
                &self.routine_execution_repo,
                &self.run_repo,
                &self.agent_repo,
                &self.frame_repo,
                &self.association_repo,
            )
        }

        fn seed_dispatch_anchor(
            &self,
            routine: &Routine,
            entity_key: Option<&str>,
        ) -> RoutineDispatchReuseTarget {
            let mut run = test_run(routine.project_id);
            let mut agent =
                LifecycleAgent::new_root(run.id, routine.project_id, AgentSource::Routine);
            let frame = AgentFrame::new_revision(agent.id, 1, "test");
            agent.set_current_frame(frame.id);
            let mut orchestration = test_orchestration("routine.main");
            let orchestration_id = orchestration.orchestration_id;
            orchestration.node_tree.push(RuntimeNodeState {
                node_id: "routine_main".to_string(),
                node_path: "routine.main".to_string(),
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
            });
            assert!(run.add_orchestration(orchestration));
            let mut execution = RoutineExecution::new(routine.id, "webhook");
            execution.status = RoutineExecutionStatus::Dispatched;
            execution.entity_key = entity_key.map(str::to_string);
            execution.dispatch_refs = Some(RoutineDispatchRefs::new(
                agentdash_domain::workflow::AgentRuntimeRefs::new(
                    run.id,
                    agent.id,
                    frame.id,
                    Some(agentdash_domain::workflow::OrchestrationBindingRefs::new(
                        orchestration_id,
                        "routine.main",
                        1,
                    )),
                ),
            ));
            let association = LifecycleSubjectAssociation::new_run_scoped(
                run.id,
                &SubjectRef::new(ROUTINE_EXECUTION_SUBJECT_KIND, execution.id),
                "source",
                None,
            );

            self.run_repo.items.lock().unwrap().push(run.clone());
            self.agent_repo.items.lock().unwrap().push(agent.clone());
            self.frame_repo.items.lock().unwrap().push(frame.clone());
            self.association_repo
                .items
                .lock()
                .unwrap()
                .push(association);
            self.routine_execution_repo
                .items
                .lock()
                .unwrap()
                .push(execution);

            RoutineDispatchReuseTarget {
                run_id: run.id,
                agent_id: agent.id,
                frame_id: frame.id,
                orchestration_id: Some(orchestration_id),
                node_path: Some("routine.main".to_string()),
            }
        }
    }

    fn test_routine(strategy: DispatchStrategy) -> Routine {
        Routine::new(
            Uuid::new_v4(),
            "test-routine",
            "test prompt",
            Uuid::new_v4(),
            RoutineTriggerConfig::Scheduled {
                cron_expression: "0 * * * *".to_string(),
                timezone: None,
            },
            strategy,
        )
    }

    fn test_run(project_id: Uuid) -> LifecycleRun {
        LifecycleRun::new_control(project_id)
    }

    fn test_orchestration(node_path: &str) -> OrchestrationInstance {
        let source_ref = OrchestrationSourceRef::Inline {
            source_digest: "sha256:test".to_string(),
        };
        let now = Utc::now();
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_digest: "sha256:test-plan".to_string(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: "routine_main".to_string(),
                node_path: node_path.to_string(),
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
            entry_node_ids: vec!["routine_main".to_string()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: now,
        };
        OrchestrationInstance::new("routine", source_ref, plan_snapshot)
    }

    #[test]
    fn resolve_json_path_reads_dot_separated_payload() {
        let data = json!({"a": {"b": {"c": 42}}});
        assert_eq!(resolve_json_path(&data, "a.b.c"), Some(&json!(42)));
        assert_eq!(resolve_json_path(&data, "a.b"), Some(&json!({"c": 42})));
        assert_eq!(resolve_json_path(&data, "x.y"), None);
    }

    #[test]
    fn json_value_to_key_string_prefers_raw_string() {
        assert_eq!(json_value_to_key_string(&json!(" PR-123 ")), "PR-123");
        assert_eq!(json_value_to_key_string(&json!(42)), "42");
    }

    #[tokio::test]
    async fn reuse_strategy_requires_existing_valid_agent_anchor() {
        let fixture = ResolverFixture::new();
        let routine = test_routine(DispatchStrategy::Reuse);
        let execution = RoutineExecution::new(routine.id, "scheduled");

        let err = fixture
            .resolver()
            .resolve(&routine, &execution)
            .await
            .expect_err("reuse without anchor must fail");

        assert!(matches!(err, ApplicationError::Conflict(_)));
    }

    #[tokio::test]
    async fn reuse_strategy_resolves_valid_run_agent_anchor() {
        let fixture = ResolverFixture::new();
        let routine = test_routine(DispatchStrategy::Reuse);
        let expected = fixture.seed_dispatch_anchor(&routine, None);
        let execution = RoutineExecution::new(routine.id, "scheduled");

        let resolution = fixture
            .resolver()
            .resolve(&routine, &execution)
            .await
            .expect("reuse anchor");

        assert_eq!(resolution.target, Some(expected));
        assert_eq!(resolution.entity_key, None);
    }

    #[tokio::test]
    async fn per_entity_resolves_entity_key_and_matching_anchor() {
        let fixture = ResolverFixture::new();
        let routine = test_routine(DispatchStrategy::PerEntity {
            entity_key_path: "issue.id".to_string(),
        });
        let expected = fixture.seed_dispatch_anchor(&routine, Some("42"));
        fixture.seed_dispatch_anchor(&routine, Some("84"));
        let mut execution = RoutineExecution::new(routine.id, "github:issues.opened");
        execution.trigger_payload = Some(json!({"issue": {"id": 42}}));

        let resolution = fixture
            .resolver()
            .resolve(&routine, &execution)
            .await
            .expect("per entity anchor");

        assert_eq!(resolution.entity_key.as_deref(), Some("42"));
        assert_eq!(resolution.target, Some(expected));
    }

    #[tokio::test]
    async fn per_entity_without_existing_anchor_creates_new_target_later() {
        let fixture = ResolverFixture::new();
        let routine = test_routine(DispatchStrategy::PerEntity {
            entity_key_path: "issue.id".to_string(),
        });
        let mut execution = RoutineExecution::new(routine.id, "github:issues.opened");
        execution.trigger_payload = Some(json!({"issue": {"id": 42}}));

        let resolution = fixture
            .resolver()
            .resolve(&routine, &execution)
            .await
            .expect("first entity trigger");

        assert_eq!(resolution.entity_key.as_deref(), Some("42"));
        assert_eq!(resolution.target, None);
    }

    #[tokio::test]
    async fn per_entity_missing_entity_key_is_bad_request() {
        let fixture = ResolverFixture::new();
        let routine = test_routine(DispatchStrategy::PerEntity {
            entity_key_path: "issue.id".to_string(),
        });
        let mut execution = RoutineExecution::new(routine.id, "github:issues.opened");
        execution.trigger_payload = Some(json!({"pull_request": {"id": 42}}));

        let err = fixture
            .resolver()
            .resolve(&routine, &execution)
            .await
            .expect_err("missing entity key should fail");

        assert!(matches!(err, ApplicationError::BadRequest(_)));
    }

    #[tokio::test]
    async fn reuse_candidate_without_subject_association_is_rejected() {
        let fixture = ResolverFixture::new();
        let routine = test_routine(DispatchStrategy::Reuse);
        fixture.seed_dispatch_anchor(&routine, None);
        fixture.association_repo.items.lock().unwrap().clear();
        let execution = RoutineExecution::new(routine.id, "scheduled");

        let err = fixture
            .resolver()
            .resolve(&routine, &execution)
            .await
            .expect_err("missing subject association should fail");

        assert!(matches!(err, ApplicationError::Conflict(_)));
    }
}
