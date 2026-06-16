//! Task view 投影器。
//!
//! Task 的 durable 真相源已经收口到 `LifecycleRun.tasks`。运行态证据由
//! `SubjectExecutionView` 通过 LifecycleRun / LifecycleAgent / AgentFrame /
//! RuntimeSessionExecutionAnchor 读取。

use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_domain::workflow::{
    LifecycleAgentRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    RuntimeNodeStatus, RuntimeSessionExecutionAnchorRepository,
};

use crate::repository_set::RepositorySet;

#[derive(Debug, thiserror::Error)]
pub enum TaskViewProjectionError {
    #[error(transparent)]
    Domain(#[from] agentdash_domain::DomainError),
}

pub async fn project_task_view_from_runtime_node_status(
    _repos: &RepositorySet,
    task_id: Uuid,
    node_status: RuntimeNodeStatus,
    reason: &str,
    context: serde_json::Value,
) -> Result<(), TaskViewProjectionError> {
    tracing::debug!(
        task_id = %task_id,
        node_status = ?node_status,
        reason,
        context = %context,
        "Task runtime projection is read through SubjectExecutionView"
    );
    Ok(())
}

pub async fn project_task_views_on_boot(
    _project_repo: &Arc<dyn ProjectRepository>,
    _state_change_repo: &Arc<dyn StateChangeRepository>,
    _story_repo: &Arc<dyn StoryRepository>,
    _association_repo: &Arc<dyn LifecycleSubjectAssociationRepository>,
    _lifecycle_run_repo: &Arc<dyn LifecycleRunRepository>,
    _lifecycle_agent_repo: &Arc<dyn LifecycleAgentRepository>,
    _execution_anchor_repo: &Arc<dyn RuntimeSessionExecutionAnchorRepository>,
) -> Result<(), TaskViewProjectionError> {
    tracing::info!(
        "Task view boot projection skipped; SubjectExecutionView derives runtime state from lifecycle evidence"
    );
    Ok(())
}
