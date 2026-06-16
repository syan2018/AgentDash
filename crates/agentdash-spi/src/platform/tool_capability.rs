//! 工具能力声明协议
//!
//! `ToolCapability` 是开放的 string key，分为两类：
//! - **平台 well-known key**：映射到 `ToolCluster` 和/或平台 MCP scope
//! - **用户自定义 MCP key**：`mcp:<preset_key>` 格式，引用已注册的 project MCP preset
//!
//! 本模块仅定义协议类型和映射规则，不包含具体的 Resolver 实现
//! （Resolver 在 `agentdash-application` 中实现，因其依赖 MCP injection 等外部类型）。

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connector::ToolCluster;

/// 工具能力标识 — 开放 string key（非封闭枚举）。
///
/// 平台 well-known key 使用 `snake_case`（如 `file_system`）；
/// 用户自定义 MCP 使用 `mcp:<preset_key>` 前缀。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolCapability(String);

impl ToolCapability {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn key(&self) -> &str {
        &self.0
    }

    /// 是否为 `mcp:<name>` 格式的用户自定义 MCP 能力。
    pub fn is_custom_mcp(&self) -> bool {
        self.0.starts_with(MCP_KEY_PREFIX)
    }

    /// 提取 `mcp:<name>` 中的 preset_key 部分；非 mcp key 返回 None。
    pub fn custom_mcp_server_name(&self) -> Option<&str> {
        self.0.strip_prefix(MCP_KEY_PREFIX)
    }

    /// 是否为平台 well-known key。
    pub fn is_well_known(&self) -> bool {
        is_known_key(&self.0)
    }

    /// 从 preset_key 构造 `mcp:<preset_key>` key。
    pub fn custom_mcp(preset_key: impl AsRef<str>) -> Self {
        Self(format!("{MCP_KEY_PREFIX}{}", preset_key.as_ref()))
    }
}

impl fmt::Display for ToolCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ToolCapability {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for ToolCapability {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ── 平台 well-known capability key 常量 ──

/// Read-only 文件访问：mounts_list, fs_read, fs_glob, fs_grep
pub const CAP_FILE_READ: &str = "file_read";
/// 文件写入：fs_apply_patch
pub const CAP_FILE_WRITE: &str = "file_write";
/// 命令执行：shell_exec
pub const CAP_SHELL_EXECUTE: &str = "shell_execute";
/// Workspace module：workspace_module_list, workspace_module_describe,
/// workspace_module_create, workspace_module_invoke, workspace_module_present
pub const CAP_WORKSPACE_MODULE: &str = "workspace_module";
pub const CAP_WORKFLOW: &str = "workflow";
pub const CAP_COLLABORATION: &str = "collaboration";
pub const CAP_STORY_MANAGEMENT: &str = "story_management";
pub const CAP_TASK_MANAGEMENT: &str = "task_management";
pub const CAP_RELAY_MANAGEMENT: &str = "relay_management";
pub const CAP_WORKFLOW_MANAGEMENT: &str = "workflow_management";

const MCP_KEY_PREFIX: &str = "mcp:";

/// 所有平台 well-known key 列表。
///
/// 别名机制已在 2026-04-22 的 capability_directives 重构中移除；
/// `file_system` 如今在迁移阶段被拆解为 `file_read + file_write + shell_execute`。
pub const WELL_KNOWN_KEYS: &[&str] = &[
    CAP_FILE_READ,
    CAP_FILE_WRITE,
    CAP_SHELL_EXECUTE,
    CAP_WORKSPACE_MODULE,
    CAP_WORKFLOW,
    CAP_COLLABORATION,
    CAP_STORY_MANAGEMENT,
    CAP_TASK_MANAGEMENT,
    CAP_RELAY_MANAGEMENT,
    CAP_WORKFLOW_MANAGEMENT,
];

/// 判断 key 是否为 well-known key。
pub fn is_known_key(key: &str) -> bool {
    WELL_KNOWN_KEYS.contains(&key)
}

// ── 工具注册表：每个 ToolCluster 下属的工具名 ──

pub const CLUSTER_READ_TOOLS: &[&str] = &["mounts_list", "fs_read", "fs_glob", "fs_grep"];
pub const CLUSTER_WRITE_TOOLS: &[&str] = &["fs_apply_patch"];
pub const CLUSTER_EXECUTE_TOOLS: &[&str] = &["shell_exec"];
pub const CLUSTER_WORKFLOW_TOOLS: &[&str] = &["complete_lifecycle_node"];
pub const CLUSTER_COLLABORATION_TOOLS: &[&str] = &["companion_request", "companion_respond"];
pub const CLUSTER_WORKSPACE_MODULE_TOOLS: &[&str] = &[
    "workspace_module_list",
    "workspace_module_describe",
    "workspace_module_create",
    "workspace_module_invoke",
    "workspace_module_present",
];

/// 返回 ToolCluster 下属的全部工具名。
pub fn cluster_tools(cluster: ToolCluster) -> &'static [&'static str] {
    match cluster {
        ToolCluster::Read => CLUSTER_READ_TOOLS,
        ToolCluster::Write => CLUSTER_WRITE_TOOLS,
        ToolCluster::Execute => CLUSTER_EXECUTE_TOOLS,
        ToolCluster::Workflow => CLUSTER_WORKFLOW_TOOLS,
        ToolCluster::Collaboration => CLUSTER_COLLABORATION_TOOLS,
        ToolCluster::WorkspaceModule => CLUSTER_WORKSPACE_MODULE_TOOLS,
    }
}

/// 返回 well-known capability key 下属的全部工具名（跨 cluster 合并）。
pub fn capability_tools(key: &str) -> Vec<&'static str> {
    let clusters = capability_to_tool_clusters_by_key(key);
    clusters
        .iter()
        .flat_map(|c| cluster_tools(*c).iter().copied())
        .collect()
}

// ── 统一工具描述 ──

/// 工具来源 — 区分平台内嵌工具（cluster 级）、平台 MCP scope 工具、以及用户自定义 MCP 工具。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolSource {
    /// 平台 cluster-based 工具（read/write/execute/workflow/collaboration/workspace_module）。
    Platform { cluster: ToolCluster },
    /// 平台 MCP scope 工具（relay/story/task/workflow 四大 scope 下静态注册的工具）。
    PlatformMcp { scope: PlatformMcpScope },
    /// 用户自定义 MCP server 的工具。
    Mcp { server_name: String },
}

/// 统一工具描述 — 平台内嵌工具和 MCP 工具的共用元数据。
///
/// 前端查询工具目录、connector 组装 system prompt、以及 capability editor
/// 展示工具列表时都消费此类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub source: ToolSource,
    /// 所属 capability key（平台工具由 cluster 反推，MCP 工具为 `mcp:<server>`）
    pub capability_key: String,
}

impl ToolDescriptor {
    pub fn platform(
        name: &str,
        display_name: &str,
        description: &str,
        cluster: ToolCluster,
        capability_key: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: description.to_string(),
            source: ToolSource::Platform { cluster },
            capability_key: capability_key.to_string(),
        }
    }

    pub fn platform_mcp(
        name: &str,
        display_name: &str,
        description: &str,
        scope: PlatformMcpScope,
        capability_key: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: description.to_string(),
            source: ToolSource::PlatformMcp { scope },
            capability_key: capability_key.to_string(),
        }
    }

    pub fn mcp(name: &str, description: &str, server_name: &str) -> Self {
        Self {
            name: name.to_string(),
            display_name: name.to_string(),
            description: description.to_string(),
            source: ToolSource::Mcp {
                server_name: server_name.to_string(),
            },
            capability_key: format!("mcp:{server_name}"),
        }
    }

    pub fn is_platform(&self) -> bool {
        matches!(
            self.source,
            ToolSource::Platform { .. } | ToolSource::PlatformMcp { .. }
        )
    }
}

/// 格式化工具描述为 system prompt 片段（platform + MCP 统一格式）。
///
/// 输出形如：
/// ```text
/// - **fs_read** (file_read): 读取 mount 内文件内容
/// ```
pub fn format_tool_for_prompt(desc: &ToolDescriptor) -> String {
    let source_tag = match &desc.source {
        ToolSource::Platform { .. } | ToolSource::PlatformMcp { .. } => desc.capability_key.clone(),
        ToolSource::Mcp { server_name } => format!("mcp:{server_name}"),
    };
    if desc.description.is_empty() {
        format!("- **{}** ({})", desc.name, source_tag)
    } else {
        format!("- **{}** ({}): {}", desc.name, source_tag, desc.description)
    }
}

// ── 平台工具注册表（编译期静态元数据）──

/// 返回所有平台内嵌工具的完整描述（编译期已知的静态元数据）。
///
/// 包含两类来源：
/// - `ToolSource::Platform { cluster }` — cluster-based 内嵌工具（read/write/execute/workflow/
///   collaboration/workspace_module）
/// - `ToolSource::PlatformMcp { scope }` — relay/story/task/workflow 四大 scope 下的 MCP 工具
///
/// 所有 `#[tool]` handler 名称与 `agentdash-mcp/src/servers/*.rs` 中注册的 Rust 函数名保持一致；
/// display_name 由函数名 + 简单大小写转换自动合成。
pub fn platform_tool_descriptors() -> Vec<ToolDescriptor> {
    vec![
        // ── Read cluster (file_read) ──
        ToolDescriptor::platform(
            "mounts_list",
            "List Mounts",
            "列出当前会话可用的文件系统挂载点",
            ToolCluster::Read,
            CAP_FILE_READ,
        ),
        ToolDescriptor::platform(
            "fs_read",
            "Read File",
            "读取 mount 内指定文件的内容",
            ToolCluster::Read,
            CAP_FILE_READ,
        ),
        ToolDescriptor::platform(
            "fs_glob",
            "Glob Search",
            "在 mount 内按 glob 模式搜索文件路径",
            ToolCluster::Read,
            CAP_FILE_READ,
        ),
        ToolDescriptor::platform(
            "fs_grep",
            "Grep Search",
            "在 mount 内按正则表达式搜索文件内容",
            ToolCluster::Read,
            CAP_FILE_READ,
        ),
        // ── Write cluster (file_write) ──
        ToolDescriptor::platform(
            "fs_apply_patch",
            "Apply Patch",
            "对 mount 内文件执行补丁操作（创建/更新/删除/重命名）",
            ToolCluster::Write,
            CAP_FILE_WRITE,
        ),
        // ── Execute cluster (shell_execute) ──
        ToolDescriptor::platform(
            "shell_exec",
            "Shell Execute",
            "在工作空间内执行 shell 命令",
            ToolCluster::Execute,
            CAP_SHELL_EXECUTE,
        ),
        // ── Workflow cluster ──
        ToolDescriptor::platform(
            "complete_lifecycle_node",
            "Complete Node",
            "声明当前 lifecycle node 完成或失败",
            ToolCluster::Workflow,
            CAP_WORKFLOW,
        ),
        // ── Collaboration cluster ──
        ToolDescriptor::platform(
            "companion_request",
            "Companion Request",
            "发起 human/platform/parent/sub 结构化协作请求",
            ToolCluster::Collaboration,
            CAP_COLLABORATION,
        ),
        ToolDescriptor::platform(
            "companion_respond",
            "Companion Respond",
            "回传 companion 协作请求结果",
            ToolCluster::Collaboration,
            CAP_COLLABORATION,
        ),
        // ── Workspace Module cluster ──
        ToolDescriptor::platform(
            "workspace_module_list",
            "List Workspace Modules",
            "列出当前 project 可见的 workspace module（enabled extension + visible canvas）摘要",
            ToolCluster::WorkspaceModule,
            CAP_WORKSPACE_MODULE,
        ),
        ToolDescriptor::platform(
            "workspace_module_describe",
            "Describe Workspace Module",
            "返回单个 workspace module 的 UI entries 与 operations（含 input/output schema）",
            ToolCluster::WorkspaceModule,
            CAP_WORKSPACE_MODULE,
        ),
        ToolDescriptor::platform(
            "workspace_module_create",
            "Create Workspace Module",
            "创建或接入 workspace module 实例；Canvas 使用 kind=canvas 创建后返回 canvas:{mount_id}",
            ToolCluster::WorkspaceModule,
            CAP_WORKSPACE_MODULE,
        ),
        ToolDescriptor::platform(
            "workspace_module_invoke",
            "Invoke Workspace Module",
            "按 module_id + operation_key + input 调用 workspace module 的 operation（宿主解析内部路由并分支派发）",
            ToolCluster::WorkspaceModule,
            CAP_WORKSPACE_MODULE,
        ),
        ToolDescriptor::platform(
            "workspace_module_present",
            "Present Workspace Module",
            "请求前端打开/激活 workspace module 的 UI view（extension webview 或 canvas panel）",
            ToolCluster::WorkspaceModule,
            CAP_WORKSPACE_MODULE,
        ),
        // ── Platform MCP: Relay scope (capability=relay_management) ──
        ToolDescriptor::platform_mcp(
            "list_projects",
            "List Projects",
            "列出所有项目，可按名称关键字过滤",
            PlatformMcpScope::Relay,
            CAP_RELAY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "get_project",
            "Get Project",
            "获取指定项目的完整信息，包括配置和关联的 Story 概况",
            PlatformMcpScope::Relay,
            CAP_RELAY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "create_story",
            "Create Story",
            "在指定项目中创建一个新的 Story（用户价值单元）",
            PlatformMcpScope::Relay,
            CAP_RELAY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "list_stories",
            "List Stories",
            "列出指定项目下的所有 Story",
            PlatformMcpScope::Relay,
            CAP_RELAY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "get_story_detail",
            "Get Story Detail",
            "获取 Story 的完整详情；Task 列表通过 Story Task projection 查询",
            PlatformMcpScope::Relay,
            CAP_RELAY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "update_story_status",
            "Update Story Status",
            "变更 Story 状态（如从 created 推进到 context_ready）",
            PlatformMcpScope::Relay,
            CAP_RELAY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "update_project_context_config",
            "Update Project Context Config",
            "更新 Project 的 VFS Mount 配置",
            PlatformMcpScope::Relay,
            CAP_RELAY_MANAGEMENT,
        ),
        // ── Platform MCP: Story scope (capability=story_management) ──
        ToolDescriptor::platform_mcp(
            "get_story_context",
            "Get Story Context",
            "获取当前 Story 的完整上下文信息（声明式来源与 VFS Mount）",
            PlatformMcpScope::Story,
            CAP_STORY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "update_story_context",
            "Update Story Context",
            "更新 Story 上下文：声明式 source_refs / VFS Mount / 会话编排",
            PlatformMcpScope::Story,
            CAP_STORY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "update_story_details",
            "Update Story Details",
            "更新 Story 基本信息（标题、描述、优先级、类型、标签）",
            PlatformMcpScope::Story,
            CAP_STORY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "create_task",
            "Create Task",
            "通过 Story-bound LifecycleRun 创建 run-scoped Task 计划项",
            PlatformMcpScope::Story,
            CAP_STORY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "batch_create_tasks",
            "Batch Create Tasks",
            "通过 Story-bound LifecycleRun 批量创建 run-scoped Task 计划项",
            PlatformMcpScope::Story,
            CAP_STORY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "list_tasks",
            "List Tasks",
            "查询当前 Story 的 Task projection",
            PlatformMcpScope::Story,
            CAP_STORY_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "advance_story_status",
            "Advance Story Status",
            "推进 Story 生命周期状态（如从 created 到 context_ready，或到 decomposed）",
            PlatformMcpScope::Story,
            CAP_STORY_MANAGEMENT,
        ),
        // ── Platform MCP: Task scope (capability=task_management) ──
        ToolDescriptor::platform_mcp(
            "get_task_info",
            "Get Task Info",
            "获取当前绑定 Task 的完整信息",
            PlatformMcpScope::Task,
            CAP_TASK_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "update_task_status",
            "Update Task Status",
            "推进当前 Task 的计划状态",
            PlatformMcpScope::Task,
            CAP_TASK_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "report_artifact",
            "Report Artifact",
            "记录 Task 关联的 SubjectExecution 产物路径或摘要",
            PlatformMcpScope::Task,
            CAP_TASK_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "get_sibling_tasks",
            "Get Sibling Tasks",
            "查看同一 LifecycleRun 内的其它 Task 计划状态（只读，用于协调）",
            PlatformMcpScope::Task,
            CAP_TASK_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "get_story_context",
            "Get Story Context",
            "获取 Task 关联 Story 的上下文信息；Task scope 的 Story 归属可为空",
            PlatformMcpScope::Task,
            CAP_TASK_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "append_task_description",
            "Append Task Description",
            "向 Task 描述中追加内容（记录执行过程发现的关键信息）",
            PlatformMcpScope::Task,
            CAP_TASK_MANAGEMENT,
        ),
        // ── Platform MCP: Workflow scope (capability=workflow_management) ──
        ToolDescriptor::platform_mcp(
            "list_workflows",
            "List Workflows",
            "列出当前项目下所有 Workflow 和 Lifecycle 定义",
            PlatformMcpScope::Workflow,
            CAP_WORKFLOW_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "get_workflow",
            "Get Workflow",
            "获取单个 Workflow 定义的完整详情（含 contract）",
            PlatformMcpScope::Workflow,
            CAP_WORKFLOW_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "get_lifecycle",
            "Get Lifecycle",
            "获取单个 Lifecycle 定义的完整详情（含 steps、edges）",
            PlatformMcpScope::Workflow,
            CAP_WORKFLOW_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "upsert_workflow_tool",
            "Upsert Workflow",
            "创建或更新 Workflow 定义（单步行为契约）。保存时自动校验，失败会返回详细错误信息。",
            PlatformMcpScope::Workflow,
            CAP_WORKFLOW_MANAGEMENT,
        ),
        ToolDescriptor::platform_mcp(
            "upsert_lifecycle_tool",
            "Upsert Lifecycle",
            "创建或更新 Lifecycle 定义（多步 DAG 编排）并自动绑定到当前 Project。保存时自动校验 DAG 拓扑、port 契约和 workflow 引用。step.procedure_key 引用的 Workflow 必须已存在。",
            PlatformMcpScope::Workflow,
            CAP_WORKFLOW_MANAGEMENT,
        ),
    ]
}

/// 按 capability key 过滤平台工具描述。
///
/// 支持 well-known key（包括 cluster-based 与 platform MCP scope 两类）。
/// 非 well-known key / 自定义 MCP key 返回空 vec。
pub fn platform_tools_for_capability(key: &str) -> Vec<ToolDescriptor> {
    platform_tool_descriptors()
        .into_iter()
        .filter(|d| d.capability_key == key)
        .collect()
}

// ── well-known key → ToolCluster 映射 ──

/// 返回 well-known key 对应的 `ToolCluster` 集合。
/// 别名自动展开。非 well-known key 返回空 vec。
pub fn capability_to_tool_clusters(cap: &ToolCapability) -> Vec<ToolCluster> {
    capability_to_tool_clusters_by_key(cap.key())
}

fn capability_to_tool_clusters_by_key(key: &str) -> Vec<ToolCluster> {
    match key {
        CAP_FILE_READ => vec![ToolCluster::Read],
        CAP_FILE_WRITE => vec![ToolCluster::Write],
        CAP_SHELL_EXECUTE => vec![ToolCluster::Execute],
        CAP_WORKSPACE_MODULE => vec![ToolCluster::WorkspaceModule],
        CAP_WORKFLOW => vec![ToolCluster::Workflow],
        CAP_COLLABORATION => vec![ToolCluster::Collaboration],
        _ => vec![],
    }
}

// ── well-known key → 平台 MCP scope 标识 ──

/// 平台 MCP scope 标识（与 SPI `ToolScope` 对应，
/// 但 SPI 层不直接依赖 MCP crate，用字符串表示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformMcpScope {
    Relay,
    Story,
    Task,
    Workflow,
}

/// 返回 well-known key 对应的平台 MCP scope。
/// 无平台 MCP 映射的 key 返回 None。
pub fn capability_to_platform_mcp_scope(cap: &ToolCapability) -> Option<PlatformMcpScope> {
    match cap.key() {
        CAP_RELAY_MANAGEMENT => Some(PlatformMcpScope::Relay),
        CAP_STORY_MANAGEMENT => Some(PlatformMcpScope::Story),
        CAP_TASK_MANAGEMENT => Some(PlatformMcpScope::Task),
        CAP_WORKFLOW_MANAGEMENT => Some(PlatformMcpScope::Workflow),
        _ => None,
    }
}

// ── Capability Scope ──

/// Session 的能力作用域。
///
/// 替代原 `SessionOwnerType` 在 capability visibility 中的角色。
/// 仅用于判断 "当前 session 处于什么级别的执行上下文"，不表达 ownership 关系。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityScope {
    #[default]
    Project,
    Story,
    Task,
}

impl fmt::Display for CapabilityScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project => write!(f, "project"),
            Self::Story => write!(f, "story"),
            Self::Task => write!(f, "task"),
        }
    }
}

/// 携带 ID 的 session 作用域上下文。
///
/// 用于 capability resolver 和 MCP injection 等需要具体 entity ID 的场景。
/// `owner_type()` 返回简单 `CapabilityScope` 以供 visibility 规则匹配。
#[derive(Debug, Clone)]
pub enum CapabilityScopeCtx {
    Project {
        project_id: Uuid,
    },
    Story {
        project_id: Uuid,
        story_id: Uuid,
    },
    Task {
        project_id: Uuid,
        story_id: Option<Uuid>,
        task_id: Uuid,
    },
}

impl CapabilityScopeCtx {
    pub fn owner_type(&self) -> CapabilityScope {
        match self {
            Self::Project { .. } => CapabilityScope::Project,
            Self::Story { .. } => CapabilityScope::Story,
            Self::Task { .. } => CapabilityScope::Task,
        }
    }

    pub fn project_id(&self) -> Uuid {
        match self {
            Self::Project { project_id }
            | Self::Story { project_id, .. }
            | Self::Task { project_id, .. } => *project_id,
        }
    }

    pub fn story_id(&self) -> Option<Uuid> {
        match self {
            Self::Story { story_id, .. } => Some(*story_id),
            Self::Task { story_id, .. } => *story_id,
            _ => None,
        }
    }

    pub fn task_id(&self) -> Option<Uuid> {
        match self {
            Self::Task { task_id, .. } => Some(*task_id),
            _ => None,
        }
    }
}

impl Default for CapabilityScopeCtx {
    fn default() -> Self {
        Self::Project {
            project_id: Uuid::nil(),
        }
    }
}

// ── Visibility Rule（仅适用于平台 well-known 能力） ──

/// 平台 well-known 能力的可见性规则。
///
/// 语义分两层：
/// - **屏蔽（AND）**：`allowed_scopes` 是硬边界，不在列表的 scope 一定不可见。
/// - **授予（OR）**：`auto_granted` / `agent_can_grant` / `workflow_can_grant`
///   至少一个来源命中即可见；三者同时为 false 代表该能力当前无任何授予源。
#[derive(Debug, Clone)]
pub struct CapabilityVisibilityRule {
    pub key: &'static str,
    /// 允许该能力生效的 session 作用域（硬边界，AND 语义）
    pub allowed_scopes: &'static [CapabilityScope],
    /// 只要 scope 匹配就默认授予（用于基础能力，如 file_system）
    pub auto_granted: bool,
    /// agent config 显式声明即授予
    pub agent_can_grant: bool,
    /// 当前 workflow 声明即授予
    pub workflow_can_grant: bool,
}

/// 返回所有平台 well-known 能力的默认可见性规则。
pub fn default_visibility_rules() -> &'static [CapabilityVisibilityRule] {
    use CapabilityScope::*;

    static RULES: &[CapabilityVisibilityRule] = &[
        CapabilityVisibilityRule {
            key: CAP_FILE_READ,
            allowed_scopes: &[Project, Story, Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_FILE_WRITE,
            allowed_scopes: &[Project, Story, Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_SHELL_EXECUTE,
            allowed_scopes: &[Project, Story, Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_WORKSPACE_MODULE,
            allowed_scopes: &[Project, Story, Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_WORKFLOW,
            allowed_scopes: &[Project, Story, Task],
            auto_granted: false,
            agent_can_grant: false,
            workflow_can_grant: true,
        },
        CapabilityVisibilityRule {
            key: CAP_COLLABORATION,
            allowed_scopes: &[Project],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_STORY_MANAGEMENT,
            allowed_scopes: &[Story],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_TASK_MANAGEMENT,
            allowed_scopes: &[Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_RELAY_MANAGEMENT,
            allowed_scopes: &[Project],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_WORKFLOW_MANAGEMENT,
            allowed_scopes: &[Project],
            auto_granted: false,
            agent_can_grant: true,
            workflow_can_grant: true,
        },
    ];
    RULES
}

/// 根据 visibility rule 判断某个 well-known capability 在给定上下文中是否生效。
///
/// 判定逻辑：
/// 1. 自定义 `mcp:*` 能力不受规则约束，始终可见。
/// 2. 未登记的 well-known key 不可见。
/// 3. 屏蔽（AND）：`scope` 不在 `allowed_scopes` 列表内 → 不可见。
/// 4. 授予（OR）：`auto_granted` 或（`agent_can_grant && agent_declares`）或
///    （`workflow_can_grant && workflow_declares`）任一为真即可见。
pub fn is_capability_visible(
    cap: &ToolCapability,
    scope: CapabilityScope,
    agent_declares: bool,
    workflow_declares: bool,
) -> bool {
    if cap.is_custom_mcp() {
        return true;
    }

    let rules = default_visibility_rules();
    let rule = match rules.iter().find(|r| r.key == cap.key()) {
        Some(r) => r,
        None => return false,
    };

    if !rule.allowed_scopes.contains(&scope) {
        return false;
    }

    rule.auto_granted
        || (rule.agent_can_grant && agent_declares)
        || (rule.workflow_can_grant && workflow_declares)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_key_classification() {
        let fr = ToolCapability::new(CAP_FILE_READ);
        assert!(fr.is_well_known());
        assert!(!fr.is_custom_mcp());
        assert_eq!(fr.custom_mcp_server_name(), None);
    }

    #[test]
    fn custom_mcp_key_parsing() {
        let cap = ToolCapability::custom_mcp("code_analyzer");
        assert!(!cap.is_well_known());
        assert!(cap.is_custom_mcp());
        assert_eq!(cap.custom_mcp_server_name(), Some("code_analyzer"));
        assert_eq!(cap.key(), "mcp:code_analyzer");
    }

    #[test]
    fn file_read_maps_to_read_cluster() {
        let cap = ToolCapability::new(CAP_FILE_READ);
        assert_eq!(capability_to_tool_clusters(&cap), vec![ToolCluster::Read]);
    }

    #[test]
    fn file_write_maps_to_write_cluster() {
        let cap = ToolCapability::new(CAP_FILE_WRITE);
        assert_eq!(capability_to_tool_clusters(&cap), vec![ToolCluster::Write]);
    }

    #[test]
    fn shell_execute_maps_to_execute_cluster() {
        let cap = ToolCapability::new(CAP_SHELL_EXECUTE);
        assert_eq!(
            capability_to_tool_clusters(&cap),
            vec![ToolCluster::Execute]
        );
    }

    #[test]
    fn file_system_alias_no_longer_recognized() {
        // file_system 别名已随 capability_directives 重构下线
        let cap = ToolCapability::new("file_system");
        assert!(!cap.is_well_known());
        assert!(capability_to_tool_clusters(&cap).is_empty());
    }

    #[test]
    fn cluster_tools_returns_correct_tool_names() {
        assert_eq!(
            cluster_tools(ToolCluster::Read),
            &["mounts_list", "fs_read", "fs_glob", "fs_grep"]
        );
        assert_eq!(cluster_tools(ToolCluster::Write), &["fs_apply_patch"]);
        assert_eq!(cluster_tools(ToolCluster::Execute), &["shell_exec"]);
    }

    #[test]
    fn capability_tools_returns_all_tools() {
        let tools = capability_tools(CAP_FILE_READ);
        assert!(tools.contains(&"fs_read"));
        assert!(tools.contains(&"fs_grep"));
    }

    #[test]
    fn mcp_only_capabilities_map_to_no_clusters() {
        let cap = ToolCapability::new(CAP_RELAY_MANAGEMENT);
        assert!(capability_to_tool_clusters(&cap).is_empty());
    }

    #[test]
    fn relay_management_maps_to_relay_scope() {
        let cap = ToolCapability::new(CAP_RELAY_MANAGEMENT);
        assert_eq!(
            capability_to_platform_mcp_scope(&cap),
            Some(PlatformMcpScope::Relay)
        );
    }

    #[test]
    fn file_read_has_no_mcp_scope() {
        let cap = ToolCapability::new(CAP_FILE_READ);
        assert_eq!(capability_to_platform_mcp_scope(&cap), None);
    }

    #[test]
    fn visibility_project_session_gets_file_read() {
        let cap = ToolCapability::new(CAP_FILE_READ);
        assert!(is_capability_visible(
            &cap,
            CapabilityScope::Project,
            false,
            false
        ));
    }

    #[test]
    fn visibility_project_session_gets_file_write() {
        let cap = ToolCapability::new(CAP_FILE_WRITE);
        assert!(is_capability_visible(
            &cap,
            CapabilityScope::Project,
            false,
            false
        ));
    }

    #[test]
    fn visibility_task_session_no_story_management() {
        let cap = ToolCapability::new(CAP_STORY_MANAGEMENT);
        assert!(!is_capability_visible(
            &cap,
            CapabilityScope::Task,
            false,
            false
        ));
    }

    #[test]
    fn visibility_workflow_requires_workflow_declaration() {
        let cap = ToolCapability::new(CAP_WORKFLOW);
        assert!(!is_capability_visible(
            &cap,
            CapabilityScope::Project,
            false,
            false,
        ));
        assert!(is_capability_visible(
            &cap,
            CapabilityScope::Project,
            false,
            true,
        ));
    }

    #[test]
    fn visibility_workflow_management_requires_agent_declaration() {
        let cap = ToolCapability::new(CAP_WORKFLOW_MANAGEMENT);
        assert!(!is_capability_visible(
            &cap,
            CapabilityScope::Project,
            false,
            false,
        ));
        assert!(is_capability_visible(
            &cap,
            CapabilityScope::Project,
            true,
            false,
        ));
    }

    #[test]
    fn visibility_workflow_management_workflow_grant_path() {
        let cap = ToolCapability::new(CAP_WORKFLOW_MANAGEMENT);
        assert!(is_capability_visible(
            &cap,
            CapabilityScope::Project,
            false,
            true,
        ));
    }

    #[test]
    fn visibility_workflow_management_hard_boundary_still_blocks() {
        let cap = ToolCapability::new(CAP_WORKFLOW_MANAGEMENT);
        assert!(!is_capability_visible(
            &cap,
            CapabilityScope::Task,
            true,
            true,
        ));
        assert!(!is_capability_visible(
            &cap,
            CapabilityScope::Story,
            true,
            true,
        ));
    }

    #[test]
    fn visibility_custom_mcp_always_visible() {
        let cap = ToolCapability::custom_mcp("anything");
        assert!(is_capability_visible(
            &cap,
            CapabilityScope::Task,
            false,
            false,
        ));
    }

    #[test]
    fn serde_roundtrip() {
        let cap = ToolCapability::new("file_read");
        let json = serde_json::to_string(&cap).unwrap();
        assert_eq!(json, "\"file_read\"");
        let deserialized: ToolCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cap);
    }

    #[test]
    fn platform_mcp_scope_tools_are_registered() {
        let workflow_tools = platform_tools_for_capability(CAP_WORKFLOW_MANAGEMENT);
        assert!(
            !workflow_tools.is_empty(),
            "workflow_management 应有静态工具"
        );
        let names: Vec<&str> = workflow_tools.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"list_workflows"));
        assert!(names.contains(&"upsert_workflow_tool"));

        let relay_tools = platform_tools_for_capability(CAP_RELAY_MANAGEMENT);
        let relay_names: Vec<&str> = relay_tools.iter().map(|d| d.name.as_str()).collect();
        assert!(relay_names.contains(&"list_projects"));
        assert!(relay_names.contains(&"create_story"));

        let story_tools = platform_tools_for_capability(CAP_STORY_MANAGEMENT);
        let story_names: Vec<&str> = story_tools.iter().map(|d| d.name.as_str()).collect();
        assert!(story_names.contains(&"get_story_context"));
        assert!(story_names.contains(&"create_task"));

        let task_tools = platform_tools_for_capability(CAP_TASK_MANAGEMENT);
        let task_names: Vec<&str> = task_tools.iter().map(|d| d.name.as_str()).collect();
        assert!(task_names.contains(&"get_task_info"));
        assert!(task_names.contains(&"report_artifact"));
    }

    #[test]
    fn tool_source_platform_mcp_serializes() {
        let desc = ToolDescriptor::platform_mcp(
            "list_workflows",
            "List Workflows",
            "",
            PlatformMcpScope::Workflow,
            CAP_WORKFLOW_MANAGEMENT,
        );
        let json = serde_json::to_string(&desc.source).unwrap();
        assert!(json.contains("platform_mcp"));
        assert!(json.contains("workflow"));
    }
}
