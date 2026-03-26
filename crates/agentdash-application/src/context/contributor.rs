use agentdash_domain::{project::Project, story::Story, task::Task, workspace::Workspace};
use agentdash_injection::ContextFragment;
use serde_json::Value;

use crate::runtime::{RuntimeAddressSpace, RuntimeMcpServer};

/// Contributor 的结构化产出 — 同时包含上下文片段和 ACP MCP Server 声明
pub struct Contribution {
    pub context_fragments: Vec<ContextFragment>,
    /// application 层 MCP server 抽象，边界层再转换为具体协议类型
    pub mcp_servers: Vec<RuntimeMcpServer>,
}

impl Contribution {
    pub fn fragments_only(fragments: Vec<ContextFragment>) -> Self {
        Self {
            context_fragments: fragments,
            mcp_servers: vec![],
        }
    }
}

/// 上下文贡献者 — 所有上下文来源实现此 trait
///
/// 通过 Contributor 模式，新的上下文来源只需实现此 trait 并注册到构建流程，
/// 无需修改核心构建逻辑。
pub trait ContextContributor: Send + Sync {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution;
}

/// 贡献者输入 — 传递给每个 Contributor 的共享上下文
pub struct ContributorInput<'a> {
    pub task: &'a Task,
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub phase: TaskExecutionPhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionPhase {
    Start,
    Continue,
}

pub struct BuiltTaskAgentContext {
    pub prompt_blocks: Vec<Value>,
    pub working_dir: Option<String>,
    pub source_summary: Vec<String>,
    /// application 层 MCP server 抽象 — 由边界层转换后传递给 Agent
    pub mcp_servers: Vec<RuntimeMcpServer>,
}

/// 上下文贡献者注册表 — 持有"常驻"贡献者，避免在构建函数中硬编码
///
/// 存放在 `AppState` 中，所有 Task 构建共享同一注册表实例。
/// 动态/per-request 贡献者（如 MCP 注入）通过 `extra_contributors` 传入。
pub struct ContextContributorRegistry {
    pub(crate) contributors: Vec<Box<dyn ContextContributor>>,
}

impl ContextContributorRegistry {
    /// 创建包含内置贡献者的注册表
    pub fn with_builtins() -> Self {
        use super::builtins::*;
        Self {
            contributors: vec![
                Box::new(CoreContextContributor),
                Box::new(BindingContextContributor),
                Box::new(DeclaredSourcesContributor),
                Box::new(InstructionContributor),
            ],
        }
    }

    /// 注册新的常驻贡献者
    pub fn register(&mut self, contributor: Box<dyn ContextContributor>) {
        self.contributors.push(contributor);
    }

    pub fn len(&self) -> usize {
        self.contributors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contributors.is_empty()
    }
}

pub struct TaskAgentBuildInput<'a> {
    pub task: &'a Task,
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub address_space: Option<&'a RuntimeAddressSpace>,
    pub effective_agent_type: Option<&'a str>,
    pub phase: TaskExecutionPhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
    /// per-request 动态贡献者（如 MCP 注入，每次构建内容不同）
    pub extra_contributors: Vec<Box<dyn ContextContributor>>,
}
