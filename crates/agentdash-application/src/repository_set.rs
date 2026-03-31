use std::sync::Arc;

use agentdash_domain::agent::{AgentRepository, ProjectAgentLinkRepository};
use agentdash_domain::auth_session::AuthSessionRepository;
use agentdash_domain::backend::BackendRepository;
use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::identity::UserDirectoryRepository;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowAssignmentRepository,
    WorkflowDefinitionRepository,
};
use agentdash_domain::workspace::WorkspaceRepository;

/// 持久化层端口 — 所有 Repository trait 对象的集合
///
/// 在 application 层定义，使 gateway / service 可直接持有仓储引用，
/// 无需依赖 api 层的 `AppState`。
#[derive(Clone)]
pub struct RepositorySet {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub canvas_repo: Arc<dyn CanvasRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub session_binding_repo: Arc<dyn SessionBindingRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    pub auth_session_repo: Arc<dyn AuthSessionRepository>,
    pub user_directory_repo: Arc<dyn UserDirectoryRepository>,
    pub settings_repo: Arc<dyn SettingsRepository>,
    pub agent_repo: Arc<dyn AgentRepository>,
    pub agent_link_repo: Arc<dyn ProjectAgentLinkRepository>,
    pub workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    pub lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    pub workflow_assignment_repo: Arc<dyn WorkflowAssignmentRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}
