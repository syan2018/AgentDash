use std::sync::Arc;

use agentdash_domain::{
    project::ProjectRepository,
    story::StoryRepository,
    workflow::{LifecycleDefinitionRepository, WorkflowDefinitionRepository},
    workspace::WorkspaceRepository,
};

/// MCP 层服务依赖聚合
///
/// 封装 MCP 工具所需的全部 Repository 和 Application Service 引用。
/// 由 API 启动层（`agentdash-api`）从 `AppState` 构造并注入。
///
/// 设计原则：
/// - 仅依赖 Domain 层 trait（不依赖 Infrastructure 实现）
/// - 通过 `Arc` 共享，各 MCP Server 实例引用同一服务集合
/// - 后续可按需添加 Application Service（如 TaskExecutionGateway）
///
/// **M1-b 更新**：Task 合入 Story aggregate，不再注入 task_repo；所有 task CRUD 经 story_repo。
#[derive(Clone)]
pub struct McpServices {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    pub lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
}
