use std::path::PathBuf;

use agentdash_domain::context_source::ContextSourceRef;
use serde::Serialize;

/// Runtime agent 渲染 `SessionContextBundle` 时允许进入主 system prompt 的 slot。
///
/// 这是云端 Agent 上下文主数据面的唯一白名单；application 组装侧和 connector
/// 消费侧必须引用同一份定义，避免新增 slot 后出现“bundle 已产出但 PiAgent 看不到”
/// 的漂移。
pub const RUNTIME_AGENT_CONTEXT_SLOTS: &[&str] = &[
    "task",
    "story",
    "project",
    "workspace",
    "initial_context",
    "vfs",
    "tools",
    "persona",
    "required_context",
    "workflow",
    "workflow_context",
    "story_context",
    "runtime_policy",
    "mcp_config",
    "declared_source",
    "static_fragment",
    "requirements",
    "constraints",
    // constraint（单数）：hook provider / companion tools 产出的
    // per-item hook injection 使用该 slot 名；与 "constraints" 复数 slot 并存
    // 是 pre-existing 约定。两者都纳入 Bundle render 白名单避免 PR 4 后丢失。
    "constraint",
    "codebase",
    "references",
    "project_guidelines",
    "instruction",
    "instruction_append",
    // companion_agents: PR 4（04-30-session-pipeline-architecture-refactor）把
    // companion agents 渲染从 SP 独立 section 统一归入 Bundle 主数据面，白名单
    // 纳入 companion_agents slot 后由 fragment_bridge 接入的 hook snapshot
    // 产出的 companion agents 条目自动进入 `## Project Context`。
    "companion_agents",
];

#[derive(Debug, thiserror::Error)]
pub enum InjectionError {
    #[error("缺少工作区，无法解析来源: {0}")]
    MissingWorkspace(String),
    #[error("来源路径不存在: {0}")]
    PathNotFound(PathBuf),
    #[error("来源文件过大: {path} ({size} bytes)")]
    SourceTooLarge { path: PathBuf, size: u64 },
    #[error("不支持的文件类型: {0}")]
    UnsupportedFileType(PathBuf),
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML 解析失败: {0}")]
    Yaml(String),
    #[error("IO 失败: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    Append,
    Override,
}

/// Context fragment 的可见性 scope。
///
/// 一个 fragment 可以同时属于多个 scope（例如既参与 F1 system prompt，又记录到审计总线）。
/// scope 决定了下游消费者（PiAgent connector / title generator / summarizer / bridge replay /
/// audit bus）各自能看到哪些 fragment，从协议层加固 `bce0825` 之类的 scope 隔离问题。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FragmentScope {
    /// 进入 F1 system prompt 主通道（默认）
    RuntimeAgent,
    /// title generator 可见
    TitleGen,
    /// 压缩 / 摘要器可见
    Summarizer,
    /// Bridge replay（continuation 历史重放）可见
    BridgeReplay,
    /// 审计总线可见（默认会携带）
    Audit,
}

impl FragmentScope {
    #[inline]
    fn bit(self) -> u8 {
        match self {
            FragmentScope::RuntimeAgent => 1 << 0,
            FragmentScope::TitleGen => 1 << 1,
            FragmentScope::Summarizer => 1 << 2,
            FragmentScope::BridgeReplay => 1 << 3,
            FragmentScope::Audit => 1 << 4,
        }
    }
}

/// FragmentScope 的位集合。
///
/// 采用简单的 `u8` bitmask，避免引入额外依赖。提供 `|` / `|=` 等常用操作，以便
/// 调用方可以书写 `FragmentScope::RuntimeAgent | FragmentScope::Audit`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FragmentScopeSet(u8);

impl FragmentScopeSet {
    /// 空集合。
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 包含所有 scope。
    pub const fn all() -> Self {
        Self(0b0001_1111)
    }

    /// 单个 scope 组成的集合。
    pub fn only(scope: FragmentScope) -> Self {
        Self(scope.bit())
    }

    /// 是否包含指定 scope。
    pub fn contains(self, scope: FragmentScope) -> bool {
        (self.0 & scope.bit()) != 0
    }

    /// 插入一个 scope，返回新集合。
    pub fn with(mut self, scope: FragmentScope) -> Self {
        self.0 |= scope.bit();
        self
    }

    /// 是否为空。
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// 当前集合的原始 bit 表示，主要用于调试 / 序列化。
    pub fn bits(self) -> u8 {
        self.0
    }
}

impl std::ops::BitOr for FragmentScope {
    type Output = FragmentScopeSet;
    fn bitor(self, rhs: Self) -> Self::Output {
        FragmentScopeSet(self.bit() | rhs.bit())
    }
}

impl std::ops::BitOr<FragmentScope> for FragmentScopeSet {
    type Output = FragmentScopeSet;
    fn bitor(self, rhs: FragmentScope) -> Self::Output {
        FragmentScopeSet(self.0 | rhs.bit())
    }
}

impl std::ops::BitOr for FragmentScopeSet {
    type Output = FragmentScopeSet;
    fn bitor(self, rhs: Self) -> Self::Output {
        FragmentScopeSet(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign<FragmentScope> for FragmentScopeSet {
    fn bitor_assign(&mut self, rhs: FragmentScope) {
        self.0 |= rhs.bit();
    }
}

impl std::ops::BitOrAssign for FragmentScopeSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl From<FragmentScope> for FragmentScopeSet {
    fn from(scope: FragmentScope) -> Self {
        Self::only(scope)
    }
}

#[derive(Debug, Clone)]
pub struct ContextFragment {
    pub slot: String,
    pub label: String,
    pub order: i32,
    pub strategy: MergeStrategy,
    /// 可见性 scope 集合。未显式声明的 fragment 应使用 `ContextFragment::default_scope()`。
    pub scope: FragmentScopeSet,
    /// 产出来源标记（吸收原 `HookInjection.source`）。
    ///
    /// 约定前缀：
    /// - `legacy:session_plan` — `build_session_plan_fragments` 产出
    /// - `legacy:contributor:<name>` — 内置 Contributor 产出
    /// - `legacy:workspace_source` — 工作空间声明式来源
    /// - `legacy:source_resolver:<kind>` — 声明式来源解析器
    /// - `hook:<trigger>` — 后续 Hook 注入路径使用
    pub source: String,
    pub content: String,
}

impl ContextFragment {
    /// 默认 scope：同时进入 runtime agent 主通道与审计总线。
    pub fn default_scope() -> FragmentScopeSet {
        FragmentScope::RuntimeAgent | FragmentScope::Audit
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VfsDescriptor {
    pub id: String,
    pub label: String,
    pub kind: agentdash_domain::context_source::ContextSourceKind,
    pub provider: String,
    pub supports: Vec<String>,
    pub selector: Option<SelectorHint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectorHint {
    pub trigger: Option<String>,
    pub placeholder: String,
    pub result_item_type: String,
}

pub struct VfsContext {
    /// 是否存在可用 Workspace（仅用于能力发现的开关）。
    /// 业务编排层不应依赖或传播任何本机路径。
    pub workspace_available: bool,
    pub has_mcp: bool,
}

pub trait VfsDiscoveryProvider: Send + Sync {
    fn descriptor(&self, ctx: &VfsContext) -> Option<VfsDescriptor>;
}

pub struct ResolveSourcesRequest<'a> {
    pub sources: &'a [ContextSourceRef],
    pub base_order: i32,
}

pub struct ResolveSourcesOutput {
    pub fragments: Vec<ContextFragment>,
    pub warnings: Vec<String>,
}

pub trait SourceResolver: Send + Sync {
    fn resolve(
        &self,
        source: &ContextSourceRef,
        order: i32,
    ) -> Result<ContextFragment, InjectionError>;
}
