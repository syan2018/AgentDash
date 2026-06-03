//! 通用启动对账管线
//!
//! 服务重启后按固定顺序执行：Session 恢复 → Task view 投影 → Infrastructure。
//! Phase 之间存在依赖：Task view 投影依赖 Session 先完成（否则会误判 session 仍在运行）。
//!
//! **定位说明**：本管线只覆盖 projection 方向（session/lifecycle 真相源 → Task view）。
//! 运行期反向（业务终态 → session cancel）的 command 通道见
//! [`crate::reconcile::terminal_cancel`]。

use std::sync::Arc;

use crate::session::SessionRuntimeService;
use crate::task::view_projector::project_task_views_on_boot;
use crate::workflow::FreeformLifecycleService;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_domain::workflow::{
    AgentProcedureRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    WorkflowGraphInstanceRepository, WorkflowGraphRepository,
};

/// 启动对账管线的依赖集合
///
/// M2-c：Task view 改为"从 LifecycleRun/step state 反投影"（Scheme A）。
/// projector 通过 `LifecycleSubjectAssociation(kind=Task)` 定位 Task。
pub struct BootReconcileDeps {
    pub session_runtime: SessionRuntimeService,
    pub project_repo: Arc<dyn ProjectRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    pub workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    pub workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

/// 单阶段对账结果
#[derive(Debug)]
pub struct PhaseReport {
    pub phase: &'static str,
    pub reconciled: usize,
    pub errors: Vec<String>,
}

/// 完整管线执行结果
#[derive(Debug)]
pub struct BootReconcileReport {
    pub phases: Vec<PhaseReport>,
}

impl BootReconcileReport {
    pub fn total_reconciled(&self) -> usize {
        self.phases.iter().map(|p| p.reconciled).sum()
    }

    pub fn has_errors(&self) -> bool {
        self.phases.iter().any(|p| !p.errors.is_empty())
    }
}

/// 执行完整的启动对账管线。
///
/// 阶段执行顺序固定且不可跳过：
/// 1. **Session 恢复** — 将残留 running 状态的 session 标记为 interrupted
/// 2. **Task view 投影** — 根据 LifecycleRun/step state 反投影 Task view
/// 3. **Infrastructure 恢复** — 预留（定时触发器重建等）
pub async fn run_boot_reconcile(deps: &BootReconcileDeps) -> BootReconcileReport {
    let mut phases = Vec::with_capacity(4);

    // ── Phase 1: Session Reconcile ──────────────────────────
    let session_report = run_session_reconcile(&deps.session_runtime).await;
    phases.push(session_report);

    // ── Phase 2: Freeform Lifecycle Ownership ───────────────
    let freeform_report = run_freeform_lifecycle_reconcile(
        deps.project_repo.as_ref(),
        deps.agent_procedure_repo.as_ref(),
        deps.workflow_graph_repo.as_ref(),
    )
    .await;
    phases.push(freeform_report);

    // ── Phase 3: Task View Projection ───────────────────────
    let task_report = run_task_view_projection(deps).await;
    phases.push(task_report);

    // ── Phase 4: Infrastructure Restore ─────────────────────
    // 目前仅占位，后续 tick-loop 触发器重建等逻辑在此扩展
    phases.push(PhaseReport {
        phase: "infrastructure_restore",
        reconciled: 0,
        errors: Vec::new(),
    });

    let report = BootReconcileReport { phases };

    tracing::info!(
        total_reconciled = report.total_reconciled(),
        has_errors = report.has_errors(),
        "启动对账管线执行完成"
    );

    report
}

/// Freeform lifecycle reconcile — ensure each project owns the builtin freeform definitions.
///
/// Dispatch paths stay strict and resolve `builtin.freeform_session` from the workflow graph
/// repository instead of synthesizing graph definitions on demand.
async fn run_freeform_lifecycle_reconcile(
    project_repo: &dyn ProjectRepository,
    agent_procedure_repo: &dyn AgentProcedureRepository,
    workflow_graph_repo: &dyn WorkflowGraphRepository,
) -> PhaseReport {
    let projects = match project_repo.list_all().await {
        Ok(projects) => projects,
        Err(err) => {
            return PhaseReport {
                phase: "freeform_lifecycle_ownership",
                reconciled: 0,
                errors: vec![err.to_string()],
            };
        }
    };
    let service = FreeformLifecycleService::new(agent_procedure_repo, workflow_graph_repo);
    let mut reconciled = 0;
    let mut errors = Vec::new();

    for project in projects {
        match service.ensure_definition(project.id).await {
            Ok(_) => {
                reconciled += 1;
            }
            Err(err) => {
                errors.push(format!("project {}: {err}", project.id));
            }
        }
    }

    PhaseReport {
        phase: "freeform_lifecycle_ownership",
        reconciled,
        errors,
    }
}

async fn run_session_reconcile(session_runtime: &SessionRuntimeService) -> PhaseReport {
    match session_runtime.recover_interrupted_sessions().await {
        Ok(()) => {
            tracing::info!("Phase 1 (Session Recovery) 完成");
            PhaseReport {
                phase: "session_recovery",
                reconciled: 0, // recover_interrupted_sessions 暂未返回计数
                errors: Vec::new(),
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "Phase 1 (Session Recovery) 出错（非致命）");
            PhaseReport {
                phase: "session_recovery",
                reconciled: 0,
                errors: vec![err.to_string()],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::project::{Project, ProjectSubjectGrant, ProjectSubjectType};
    use agentdash_domain::workflow::{AgentProcedure, WorkflowGraph};
    use tokio::sync::Mutex;

    use crate::workflow::{FREEFORM_AGENT_PROCEDURE_KEY, FREEFORM_LIFECYCLE_KEY};

    #[derive(Default)]
    struct InMemoryProjectRepo {
        items: Mutex<Vec<Project>>,
    }

    #[async_trait::async_trait]
    impl ProjectRepository for InMemoryProjectRepo {
        async fn create(&self, project: &Project) -> Result<(), DomainError> {
            self.items.lock().await.push(project.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Project>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|project| project.id == id)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
            Ok(self.items.lock().await.clone())
        }

        async fn update(&self, project: &Project) -> Result<(), DomainError> {
            let mut items = self.items.lock().await;
            if let Some(existing) = items.iter_mut().find(|item| item.id == project.id) {
                *existing = project.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
            self.items.lock().await.retain(|project| project.id != id);
            Ok(())
        }

        async fn list_subject_grants(
            &self,
            _project_id: uuid::Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(Vec::new())
        }

        async fn upsert_subject_grant(
            &self,
            _grant: &ProjectSubjectGrant,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_subject_grant(
            &self,
            _project_id: uuid::Uuid,
            _subject_type: ProjectSubjectType,
            _subject_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryAgentProcedureRepo {
        items: Mutex<Vec<AgentProcedure>>,
    }

    #[async_trait::async_trait]
    impl AgentProcedureRepository for InMemoryAgentProcedureRepo {
        async fn create(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
            self.items.lock().await.push(procedure.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|procedure| procedure.id == id)
                .cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|procedure| procedure.key == key)
                .cloned())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: uuid::Uuid,
            key: &str,
        ) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|procedure| procedure.project_id == project_id && procedure.key == key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self.items.lock().await.clone())
        }

        async fn list_by_project(
            &self,
            project_id: uuid::Uuid,
        ) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|procedure| procedure.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
            let mut items = self.items.lock().await;
            if let Some(existing) = items.iter_mut().find(|item| item.id == procedure.id) {
                *existing = procedure.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
            self.items
                .lock()
                .await
                .retain(|procedure| procedure.id != id);
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
            self.items.lock().await.push(lifecycle.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|lifecycle| lifecycle.id == id)
                .cloned())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: uuid::Uuid,
            key: &str,
        ) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|lifecycle| lifecycle.project_id == project_id && lifecycle.key == key)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: uuid::Uuid,
        ) -> Result<Vec<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|lifecycle| lifecycle.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            let mut items = self.items.lock().await;
            if let Some(existing) = items.iter_mut().find(|item| item.id == lifecycle.id) {
                *existing = lifecycle.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
            self.items
                .lock()
                .await
                .retain(|lifecycle| lifecycle.id != id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn freeform_lifecycle_reconcile_ensures_builtin_graph_for_projects() {
        let project = Project::new("test".to_string(), String::new());
        let project_id = project.id;
        let project_repo = InMemoryProjectRepo {
            items: Mutex::new(vec![project]),
        };
        let agent_procedure_repo = InMemoryAgentProcedureRepo::default();
        let workflow_graph_repo = InMemoryWorkflowGraphRepo::default();

        let report = run_freeform_lifecycle_reconcile(
            &project_repo,
            &agent_procedure_repo,
            &workflow_graph_repo,
        )
        .await;

        assert_eq!(report.phase, "freeform_lifecycle_ownership");
        assert_eq!(report.reconciled, 1);
        assert!(report.errors.is_empty());

        let procedures = agent_procedure_repo
            .list_by_project(project_id)
            .await
            .expect("procedures");
        assert_eq!(procedures.len(), 1);
        assert_eq!(procedures[0].key, FREEFORM_AGENT_PROCEDURE_KEY);

        let graphs = workflow_graph_repo
            .list_by_project(project_id)
            .await
            .expect("graphs");
        assert_eq!(graphs.len(), 1);
        assert_eq!(graphs[0].key, FREEFORM_LIFECYCLE_KEY);

        let second_report = run_freeform_lifecycle_reconcile(
            &project_repo,
            &agent_procedure_repo,
            &workflow_graph_repo,
        )
        .await;
        assert_eq!(second_report.reconciled, 1);
        assert!(second_report.errors.is_empty());
        assert_eq!(
            agent_procedure_repo
                .list_by_project(project_id)
                .await
                .expect("procedures after second reconcile")
                .len(),
            1
        );
        assert_eq!(
            workflow_graph_repo
                .list_by_project(project_id)
                .await
                .expect("graphs after second reconcile")
                .len(),
            1
        );
    }
}

async fn run_task_view_projection(deps: &BootReconcileDeps) -> PhaseReport {
    match project_task_views_on_boot(
        &deps.project_repo,
        &deps.state_change_repo,
        &deps.story_repo,
        &deps.lifecycle_subject_association_repo,
        &deps.lifecycle_run_repo,
        &deps.workflow_graph_instance_repo,
    )
    .await
    {
        Ok(()) => {
            tracing::info!("Phase 2 (Task View Projection) 完成");
            PhaseReport {
                phase: "task_view_projection",
                reconciled: 0,
                errors: Vec::new(),
            }
        }
        Err(err) => {
            tracing::error!(error = %err, "Phase 2 (Task View Projection) 失败");
            PhaseReport {
                phase: "task_view_projection",
                reconciled: 0,
                errors: vec![err.to_string()],
            }
        }
    }
}
