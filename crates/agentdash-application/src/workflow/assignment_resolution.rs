use uuid::Uuid;

use agentdash_domain::workflow::{
    WorkflowAgentRole, WorkflowAssignmentRepository, WorkflowDefinition,
    WorkflowDefinitionRepository, WorkflowRun, WorkflowRunRepository, WorkflowTargetKind,
};

use super::error::WorkflowApplicationError;
use super::run::select_active_run;

/// 从 assignment 解析出的可用 workflow + 已恢复或新建的 active run。
#[derive(Debug, Clone)]
pub struct ResolvedAssignment {
    pub definition: WorkflowDefinition,
    pub run: WorkflowRun,
    /// 本次调用是否创建了新 run（false 表示恢复了已有 active run）。
    pub newly_created: bool,
}

/// Assignment resolution 入参。
pub struct ResolveAssignmentInput {
    pub project_id: Uuid,
    pub role: WorkflowAgentRole,
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    /// activate_phase 需要 session_binding_id 的 phase，如果有的话。
    pub session_binding_id: Option<Uuid>,
}

/// 根据 project 的默认 assignment 自动解析 workflow 并创建 / 恢复 run。
///
/// 幂等：如果目标已存在 active run，直接返回它而不创建新 run。
/// 只在没有 active run 时才新建。
pub async fn resolve_assignment_and_ensure_run<D, A, R>(
    definition_repo: &D,
    assignment_repo: &A,
    run_repo: &R,
    input: ResolveAssignmentInput,
) -> Result<Option<ResolvedAssignment>, WorkflowApplicationError>
where
    D: WorkflowDefinitionRepository + ?Sized,
    A: WorkflowAssignmentRepository + ?Sized,
    R: WorkflowRunRepository + ?Sized,
{
    let assignments = assignment_repo
        .list_by_project_and_role(input.project_id, input.role)
        .await?;
    let default_assignment = assignments.into_iter().find(|a| a.enabled && a.is_default);

    let assignment = match default_assignment {
        Some(a) => a,
        None => return Ok(None),
    };

    let definition = definition_repo
        .get_by_id(assignment.workflow_id)
        .await?
        .ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!(
                "assignment 引用的 workflow_definition 不存在: {}",
                assignment.workflow_id
            ))
        })?;

    if !definition.enabled {
        return Ok(None);
    }

    if definition.target_kind != input.target_kind {
        return Err(WorkflowApplicationError::Conflict(format!(
            "assignment 引用的 workflow `{}` target_kind={:?} 与请求的 {:?} 不匹配",
            definition.key, definition.target_kind, input.target_kind
        )));
    }

    let existing_runs = run_repo
        .list_by_target(input.target_kind, input.target_id)
        .await?;

    if let Some(active_run) = select_active_run(existing_runs) {
        return Ok(Some(ResolvedAssignment {
            definition,
            run: active_run,
            newly_created: false,
        }));
    }

    let mut run = WorkflowRun::new(
        input.project_id,
        definition.id,
        input.target_kind,
        input.target_id,
        &definition.phases,
    );

    if let Some(first_phase) = definition.phases.first() {
        if first_phase.requires_session {
            if let Some(binding_id) = input.session_binding_id {
                let _ = run.attach_session_binding(&first_phase.key, binding_id);
            }
        }
        if !first_phase.requires_session || input.session_binding_id.is_some() {
            let _ = run.activate_phase(&first_phase.key);
        }
    }

    run_repo.create(&run).await?;

    Ok(Some(ResolvedAssignment {
        definition,
        run,
        newly_created: true,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use uuid::Uuid;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        WorkflowAgentRole, WorkflowAssignment, WorkflowAssignmentRepository, WorkflowDefinition,
        WorkflowDefinitionRepository, WorkflowRun, WorkflowRunRepository, WorkflowTargetKind,
    };

    use super::*;
    use crate::workflow::{TRELLIS_DEV_TASK_TEMPLATE_KEY, build_builtin_workflow_definition};

    #[derive(Default)]
    struct MemoryStore {
        definitions: Mutex<HashMap<Uuid, WorkflowDefinition>>,
        assignments: Mutex<HashMap<Uuid, WorkflowAssignment>>,
        runs: Mutex<HashMap<Uuid, WorkflowRun>>,
    }

    #[async_trait]
    impl WorkflowDefinitionRepository for MemoryStore {
        async fn create(&self, w: &WorkflowDefinition) -> Result<(), DomainError> {
            self.definitions.lock().unwrap().insert(w.id, w.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self.definitions.lock().unwrap().get(&id).cloned())
        }
        async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .values()
                .find(|w| w.key == key)
                .cloned())
        }
        async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self.definitions.lock().unwrap().values().cloned().collect())
        }
        async fn list_enabled(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .values()
                .filter(|w| w.enabled)
                .cloned()
                .collect())
        }
        async fn list_by_target_kind(
            &self,
            tk: WorkflowTargetKind,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .values()
                .filter(|w| w.target_kind == tk)
                .cloned()
                .collect())
        }
        async fn update(&self, w: &WorkflowDefinition) -> Result<(), DomainError> {
            self.definitions.lock().unwrap().insert(w.id, w.clone());
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.definitions.lock().unwrap().remove(&id);
            Ok(())
        }
    }

    #[async_trait]
    impl WorkflowAssignmentRepository for MemoryStore {
        async fn create(&self, a: &WorkflowAssignment) -> Result<(), DomainError> {
            self.assignments.lock().unwrap().insert(a.id, a.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowAssignment>, DomainError> {
            Ok(self.assignments.lock().unwrap().get(&id).cloned())
        }
        async fn list_by_project(&self, pid: Uuid) -> Result<Vec<WorkflowAssignment>, DomainError> {
            Ok(self
                .assignments
                .lock()
                .unwrap()
                .values()
                .filter(|a| a.project_id == pid)
                .cloned()
                .collect())
        }
        async fn list_by_project_and_role(
            &self,
            pid: Uuid,
            role: WorkflowAgentRole,
        ) -> Result<Vec<WorkflowAssignment>, DomainError> {
            Ok(self
                .assignments
                .lock()
                .unwrap()
                .values()
                .filter(|a| a.project_id == pid && a.role == role)
                .cloned()
                .collect())
        }
        async fn update(&self, a: &WorkflowAssignment) -> Result<(), DomainError> {
            self.assignments.lock().unwrap().insert(a.id, a.clone());
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.assignments.lock().unwrap().remove(&id);
            Ok(())
        }
    }

    #[async_trait]
    impl WorkflowRunRepository for MemoryStore {
        async fn create(&self, r: &WorkflowRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().insert(r.id, r.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowRun>, DomainError> {
            Ok(self.runs.lock().unwrap().get(&id).cloned())
        }
        async fn list_by_project(&self, pid: Uuid) -> Result<Vec<WorkflowRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .values()
                .filter(|r| r.project_id == pid)
                .cloned()
                .collect())
        }
        async fn list_by_workflow(&self, wid: Uuid) -> Result<Vec<WorkflowRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .values()
                .filter(|r| r.workflow_id == wid)
                .cloned()
                .collect())
        }
        async fn list_by_target(
            &self,
            tk: WorkflowTargetKind,
            tid: Uuid,
        ) -> Result<Vec<WorkflowRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .values()
                .filter(|r| r.target_kind == tk && r.target_id == tid)
                .cloned()
                .collect())
        }
        async fn update(&self, r: &WorkflowRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().insert(r.id, r.clone());
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().remove(&id);
            Ok(())
        }
    }

    async fn setup_store_with_assignment(
        store: &MemoryStore,
        project_id: Uuid,
    ) -> WorkflowDefinition {
        let definition =
            build_builtin_workflow_definition(TRELLIS_DEV_TASK_TEMPLATE_KEY).expect("definition");
        WorkflowDefinitionRepository::create(store, &definition)
            .await
            .unwrap();
        let mut assignment = WorkflowAssignment::new(
            project_id,
            definition.id,
            WorkflowAgentRole::TaskExecutionWorker,
        );
        assignment.enabled = true;
        assignment.is_default = true;
        WorkflowAssignmentRepository::create(store, &assignment)
            .await
            .unwrap();
        definition
    }

    #[tokio::test]
    async fn resolve_creates_run_when_no_active_run_exists() {
        let store = MemoryStore::default();
        let project_id = Uuid::new_v4();
        let definition = setup_store_with_assignment(&store, project_id).await;
        let target_id = Uuid::new_v4();

        let result = resolve_assignment_and_ensure_run(
            &store,
            &store,
            &store,
            ResolveAssignmentInput {
                project_id,
                role: WorkflowAgentRole::TaskExecutionWorker,
                target_kind: WorkflowTargetKind::Task,
                target_id,
                session_binding_id: None,
            },
        )
        .await
        .expect("resolve assignment");

        let resolved = result.expect("should have resolved");
        assert!(resolved.newly_created);
        assert_eq!(resolved.definition.id, definition.id);
        assert_eq!(resolved.run.target_id, target_id);
        assert_eq!(resolved.run.current_phase_key.as_deref(), Some("start"));
    }

    #[tokio::test]
    async fn resolve_reuses_existing_active_run() {
        let store = MemoryStore::default();
        let project_id = Uuid::new_v4();
        let _definition = setup_store_with_assignment(&store, project_id).await;
        let target_id = Uuid::new_v4();

        let first = resolve_assignment_and_ensure_run(
            &store,
            &store,
            &store,
            ResolveAssignmentInput {
                project_id,
                role: WorkflowAgentRole::TaskExecutionWorker,
                target_kind: WorkflowTargetKind::Task,
                target_id,
                session_binding_id: None,
            },
        )
        .await
        .unwrap()
        .unwrap();

        let second = resolve_assignment_and_ensure_run(
            &store,
            &store,
            &store,
            ResolveAssignmentInput {
                project_id,
                role: WorkflowAgentRole::TaskExecutionWorker,
                target_kind: WorkflowTargetKind::Task,
                target_id,
                session_binding_id: None,
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert!(!second.newly_created);
        assert_eq!(first.run.id, second.run.id);
    }

    #[tokio::test]
    async fn resolve_returns_none_without_default_assignment() {
        let store = MemoryStore::default();
        let project_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();

        let result = resolve_assignment_and_ensure_run(
            &store,
            &store,
            &store,
            ResolveAssignmentInput {
                project_id,
                role: WorkflowAgentRole::TaskExecutionWorker,
                target_kind: WorkflowTargetKind::Task,
                target_id,
                session_binding_id: None,
            },
        )
        .await
        .unwrap();

        assert!(result.is_none());
    }
}
