use uuid::Uuid;

pub use agentdash_platform_spi::ToolScope;

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
    /// 关联的 Task ID（由非 MCP runtime tools 使用时可为空）
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

    pub fn with_caller(mut self, caller_id: impl Into<String>) -> Self {
        self.caller_id = Some(caller_id.into());
        self
    }
}
