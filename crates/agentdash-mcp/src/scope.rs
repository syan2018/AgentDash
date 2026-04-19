use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// MCP 工具层级
///
/// 决定连接方可见的工具集，每个层级暴露不同粒度的操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolScope {
    /// 中继层：面向用户，支持跨 Project 的全局看板操作
    Relay,
    /// Story 层：面向编排 Agent（如 PlanAgent），支持 Story 上下文管理与 Task 拆解
    Story,
    /// Task 层：面向执行 Agent，支持 Task 状态更新与产物上报
    Task,
    /// Workflow 层：面向拥有 workflow_management 能力的 Agent，支持 Workflow/Lifecycle CRUD
    Workflow,
}

/// MCP 会话上下文
///
/// 携带当前 MCP 连接的层级信息和实体绑定。
/// 由传输层在连接建立时根据路径/参数构造。
#[derive(Debug, Clone)]
pub struct McpSessionContext {
    /// 当前会话的工具层级
    pub scope: ToolScope,
    /// 关联的 Project ID（Relay 层可选，Story/Task 层从实体反查）
    pub project_id: Option<Uuid>,
    /// 关联的 Story ID（Story 层必填，Task 层从实体反查）
    pub story_id: Option<Uuid>,
    /// 关联的 Task ID（仅 Task 层使用）
    pub task_id: Option<Uuid>,
    /// 可选的调用者标识（用于审计和隔离）
    pub caller_id: Option<String>,
}

impl McpSessionContext {
    /// 创建 Relay 层上下文
    pub fn relay(project_id: Option<Uuid>) -> Self {
        Self {
            scope: ToolScope::Relay,
            project_id,
            story_id: None,
            task_id: None,
            caller_id: None,
        }
    }

    /// 创建 Story 层上下文
    pub fn story(project_id: Uuid, story_id: Uuid) -> Self {
        Self {
            scope: ToolScope::Story,
            project_id: Some(project_id),
            story_id: Some(story_id),
            task_id: None,
            caller_id: None,
        }
    }

    /// 创建 Task 层上下文
    pub fn task(project_id: Uuid, story_id: Uuid, task_id: Uuid) -> Self {
        Self {
            scope: ToolScope::Task,
            project_id: Some(project_id),
            story_id: Some(story_id),
            task_id: Some(task_id),
            caller_id: None,
        }
    }

    pub fn with_caller(mut self, caller_id: impl Into<String>) -> Self {
        self.caller_id = Some(caller_id.into());
        self
    }
}
