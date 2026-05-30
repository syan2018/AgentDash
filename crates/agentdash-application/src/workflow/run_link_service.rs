use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleRun, LifecycleRunLink, LifecycleRunLinkRepository, LifecycleRunRepository,
    LifecycleRunStatus, RunLinkRole, RunLinkSubjectKind,
};

use super::error::WorkflowApplicationError;

/// LifecycleRun 与业务对象关联的查询/写入服务。
///
/// 替代通过 session_id -> SessionBinding 的隐式反查路径，
/// 提供 Story / RoutineExecution / Task 等对象到 run 的显式查询能力。
pub struct LifecycleRunLinkService {
    link_repo: Arc<dyn LifecycleRunLinkRepository>,
    run_repo: Arc<dyn LifecycleRunRepository>,
}

impl LifecycleRunLinkService {
    pub fn new(
        link_repo: Arc<dyn LifecycleRunLinkRepository>,
        run_repo: Arc<dyn LifecycleRunRepository>,
    ) -> Self {
        Self {
            link_repo,
            run_repo,
        }
    }

    /// 为 run 关联一个业务对象。
    pub async fn attach_subject(
        &self,
        run_id: Uuid,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
        role: RunLinkRole,
    ) -> Result<LifecycleRunLink, WorkflowApplicationError> {
        let link = LifecycleRunLink::new(run_id, subject_kind, subject_id, role);
        self.link_repo.create(&link).await?;
        Ok(link)
    }

    /// 查询 Story 关联的所有 LifecycleRun（通过 RunLink）。
    pub async fn list_runs_for_story(
        &self,
        story_id: Uuid,
    ) -> Result<Vec<LifecycleRun>, WorkflowApplicationError> {
        self.list_runs_for_subject(RunLinkSubjectKind::Story, story_id)
            .await
    }

    /// 查询 Story 当前活跃的 LifecycleRun（Running/Ready/Blocked）。
    pub async fn active_run_for_story(
        &self,
        story_id: Uuid,
    ) -> Result<Option<LifecycleRun>, WorkflowApplicationError> {
        let runs = self.list_runs_for_story(story_id).await?;
        Ok(runs.into_iter().find(|run| is_run_active(run.status)))
    }

    /// 查询某业务对象关联的所有 LifecycleRun。
    pub async fn list_runs_for_subject(
        &self,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
    ) -> Result<Vec<LifecycleRun>, WorkflowApplicationError> {
        let links = self
            .link_repo
            .list_by_subject(subject_kind, subject_id)
            .await?;
        if links.is_empty() {
            return Ok(Vec::new());
        }
        let run_ids: Vec<Uuid> = links.iter().map(|l| l.run_id).collect();
        let runs = self.run_repo.list_by_ids(&run_ids).await?;
        Ok(runs)
    }

    /// 查询某 run 的所有关联 subjects。
    pub async fn list_subjects_for_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<LifecycleRunLink>, WorkflowApplicationError> {
        Ok(self.link_repo.list_by_run(run_id).await?)
    }

    /// 查询某业务对象以特定 role 关联的所有 links。
    pub async fn list_links_by_subject_and_role(
        &self,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
        role: RunLinkRole,
    ) -> Result<Vec<LifecycleRunLink>, WorkflowApplicationError> {
        Ok(self
            .link_repo
            .list_by_subject_and_role(subject_kind, subject_id, role)
            .await?)
    }
}

fn is_run_active(status: LifecycleRunStatus) -> bool {
    matches!(
        status,
        LifecycleRunStatus::Ready | LifecycleRunStatus::Running | LifecycleRunStatus::Blocked
    )
}
