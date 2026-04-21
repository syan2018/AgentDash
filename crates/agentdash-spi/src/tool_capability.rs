//! 工具能力声明协议
//!
//! `ToolCapability` 是开放的 string key，分为两类：
//! - **平台 well-known key**：映射到 `ToolCluster` 和/或平台 MCP scope
//! - **用户自定义 MCP key**：`mcp:<server_name>` 格式，引用已注册的外部 MCP server
//!
//! 本模块仅定义协议类型和映射规则，不包含具体的 Resolver 实现
//! （Resolver 在 `agentdash-application` 中实现，因其依赖 MCP injection 等外部类型）。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::connector::ToolCluster;

/// 工具能力标识 — 开放 string key（非封闭枚举）。
///
/// 平台 well-known key 使用 `snake_case`（如 `file_system`）；
/// 用户自定义 MCP 使用 `mcp:<server_name>` 前缀。
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

    /// 提取 `mcp:<name>` 中的 server_name 部分；非 mcp key 返回 None。
    pub fn custom_mcp_server_name(&self) -> Option<&str> {
        self.0.strip_prefix(MCP_KEY_PREFIX)
    }

    /// 是否为平台 well-known key（含别名）。
    pub fn is_well_known(&self) -> bool {
        is_known_key(&self.0)
    }

    /// 从 server_name 构造 `mcp:<server_name>` key。
    pub fn custom_mcp(server_name: impl AsRef<str>) -> Self {
        Self(format!("{MCP_KEY_PREFIX}{}", server_name.as_ref()))
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

/// 别名：展开为 `file_read + file_write + shell_execute`，保持向后兼容。
pub const CAP_FILE_SYSTEM: &str = "file_system";
/// Read-only 文件访问：mounts_list, fs_read, fs_glob, fs_grep
pub const CAP_FILE_READ: &str = "file_read";
/// 文件写入：fs_apply_patch
pub const CAP_FILE_WRITE: &str = "file_write";
/// 命令执行：shell_exec
pub const CAP_SHELL_EXECUTE: &str = "shell_execute";
pub const CAP_CANVAS: &str = "canvas";
pub const CAP_WORKFLOW: &str = "workflow";
pub const CAP_COLLABORATION: &str = "collaboration";
pub const CAP_STORY_MANAGEMENT: &str = "story_management";
pub const CAP_TASK_MANAGEMENT: &str = "task_management";
pub const CAP_RELAY_MANAGEMENT: &str = "relay_management";
pub const CAP_WORKFLOW_MANAGEMENT: &str = "workflow_management";

const MCP_KEY_PREFIX: &str = "mcp:";

/// 所有平台 well-known key 列表（含新拆分 key，不含别名）。
pub const WELL_KNOWN_KEYS: &[&str] = &[
    CAP_FILE_READ,
    CAP_FILE_WRITE,
    CAP_SHELL_EXECUTE,
    CAP_CANVAS,
    CAP_WORKFLOW,
    CAP_COLLABORATION,
    CAP_STORY_MANAGEMENT,
    CAP_TASK_MANAGEMENT,
    CAP_RELAY_MANAGEMENT,
    CAP_WORKFLOW_MANAGEMENT,
];

/// 平台别名 → 展开目标。当前仅 `file_system` 一个别名。
pub const CAPABILITY_ALIASES: &[(&str, &[&str])] = &[
    (CAP_FILE_SYSTEM, &[CAP_FILE_READ, CAP_FILE_WRITE, CAP_SHELL_EXECUTE]),
];

/// 如果 key 是别名，返回展开后的 well-known key 列表；否则返回 None。
pub fn expand_alias(key: &str) -> Option<&'static [&'static str]> {
    CAPABILITY_ALIASES
        .iter()
        .find(|(alias, _)| *alias == key)
        .map(|(_, targets)| *targets)
}

/// 判断 key 是否为 well-known key 或别名。
pub fn is_known_key(key: &str) -> bool {
    WELL_KNOWN_KEYS.contains(&key) || CAPABILITY_ALIASES.iter().any(|(alias, _)| *alias == key)
}

// ── 工具注册表：每个 ToolCluster 下属的工具名 ──

pub const CLUSTER_READ_TOOLS: &[&str] = &["mounts_list", "fs_read", "fs_glob", "fs_grep"];
pub const CLUSTER_WRITE_TOOLS: &[&str] = &["fs_apply_patch"];
pub const CLUSTER_EXECUTE_TOOLS: &[&str] = &["shell_exec"];
pub const CLUSTER_WORKFLOW_TOOLS: &[&str] = &["report_workflow_artifact"];
pub const CLUSTER_COLLABORATION_TOOLS: &[&str] = &["companion_request", "companion_respond"];
pub const CLUSTER_CANVAS_TOOLS: &[&str] = &["canvases_list", "canvas_start", "bind_canvas_data", "present_canvas"];

/// 返回 ToolCluster 下属的全部工具名。
pub fn cluster_tools(cluster: ToolCluster) -> &'static [&'static str] {
    match cluster {
        ToolCluster::Read => CLUSTER_READ_TOOLS,
        ToolCluster::Write => CLUSTER_WRITE_TOOLS,
        ToolCluster::Execute => CLUSTER_EXECUTE_TOOLS,
        ToolCluster::Workflow => CLUSTER_WORKFLOW_TOOLS,
        ToolCluster::Collaboration => CLUSTER_COLLABORATION_TOOLS,
        ToolCluster::Canvas => CLUSTER_CANVAS_TOOLS,
    }
}

/// 返回 well-known capability key 下属的全部工具名（跨 cluster 合并）。
pub fn capability_tools(key: &str) -> Vec<&'static str> {
    let clusters = capability_to_tool_clusters_by_key(key);
    clusters.iter().flat_map(|c| cluster_tools(*c).iter().copied()).collect()
}

// ── 统一工具描述 ──

/// 工具来源 — 区分平台内嵌工具和 MCP 外部工具。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolSource {
    Platform { cluster: ToolCluster },
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
        matches!(self.source, ToolSource::Platform { .. })
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
        ToolSource::Platform { .. } => desc.capability_key.clone(),
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
pub fn platform_tool_descriptors() -> Vec<ToolDescriptor> {
    vec![
        // ── Read cluster (file_read) ──
        ToolDescriptor::platform("mounts_list", "List Mounts", "列出当前会话可用的文件系统挂载点", ToolCluster::Read, CAP_FILE_READ),
        ToolDescriptor::platform("fs_read", "Read File", "读取 mount 内指定文件的内容", ToolCluster::Read, CAP_FILE_READ),
        ToolDescriptor::platform("fs_glob", "Glob Search", "在 mount 内按 glob 模式搜索文件路径", ToolCluster::Read, CAP_FILE_READ),
        ToolDescriptor::platform("fs_grep", "Grep Search", "在 mount 内按正则表达式搜索文件内容", ToolCluster::Read, CAP_FILE_READ),
        // ── Write cluster (file_write) ──
        ToolDescriptor::platform("fs_apply_patch", "Apply Patch", "对 mount 内文件执行补丁操作（创建/更新/删除/重命名）", ToolCluster::Write, CAP_FILE_WRITE),
        // ── Execute cluster (shell_execute) ──
        ToolDescriptor::platform("shell_exec", "Shell Execute", "在工作空间内执行 shell 命令", ToolCluster::Execute, CAP_SHELL_EXECUTE),
        // ── Workflow cluster ──
        ToolDescriptor::platform("complete_lifecycle_node", "Complete Node", "声明当前 lifecycle node 完成或失败", ToolCluster::Workflow, CAP_WORKFLOW),
        // ── Collaboration cluster ──
        ToolDescriptor::platform("companion_request", "Companion Request", "向关联 agent 发起协作请求", ToolCluster::Collaboration, CAP_COLLABORATION),
        ToolDescriptor::platform("companion_respond", "Companion Respond", "回复协作 agent 的请求", ToolCluster::Collaboration, CAP_COLLABORATION),
        // ── Canvas cluster ──
        ToolDescriptor::platform("canvases_list", "List Canvases", "列出当前 project 的画布资产", ToolCluster::Canvas, CAP_CANVAS),
        ToolDescriptor::platform("canvas_start", "Start Canvas", "创建新画布资产", ToolCluster::Canvas, CAP_CANVAS),
        ToolDescriptor::platform("bind_canvas_data", "Bind Canvas Data", "绑定数据到画布", ToolCluster::Canvas, CAP_CANVAS),
        ToolDescriptor::platform("present_canvas", "Present Canvas", "向用户展示画布", ToolCluster::Canvas, CAP_CANVAS),
    ]
}

/// 按 capability key 过滤平台工具描述。
pub fn platform_tools_for_capability(key: &str) -> Vec<ToolDescriptor> {
    if let Some(expanded) = expand_alias(key) {
        expanded
            .iter()
            .flat_map(|k| platform_tools_for_capability(k))
            .collect()
    } else {
        platform_tool_descriptors()
            .into_iter()
            .filter(|d| d.capability_key == key)
            .collect()
    }
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
        // 别名展开
        CAP_FILE_SYSTEM => vec![ToolCluster::Read, ToolCluster::Write, ToolCluster::Execute],
        CAP_CANVAS => vec![ToolCluster::Canvas],
        CAP_WORKFLOW => vec![ToolCluster::Workflow],
        CAP_COLLABORATION => vec![ToolCluster::Collaboration],
        _ => vec![],
    }
}

// ── well-known key → 平台 MCP scope 标识 ──

/// 平台 MCP scope 标识（与 `agentdash-mcp::scope::ToolScope` 对应，
/// 但 SPI 层不直接依赖 MCP crate，用字符串表示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

// ── Visibility Rule（仅适用于平台 well-known 能力） ──

use agentdash_domain::session_binding::SessionOwnerType;

/// 平台 well-known 能力的可见性规则。
///
/// 语义分两层：
/// - **屏蔽（AND）**：`allowed_owner_types` 是硬边界，不在列表的 owner 一定不可见。
/// - **授予（OR）**：`auto_granted` / `agent_can_grant` / `workflow_can_grant`
///   至少一个来源命中即可见；三者同时为 false 代表该能力当前无任何授予源。
#[derive(Debug, Clone)]
pub struct CapabilityVisibilityRule {
    pub key: &'static str,
    /// 允许该能力生效的 session owner 类型（硬边界，AND 语义）
    pub allowed_owner_types: &'static [SessionOwnerType],
    /// 只要 owner 匹配就默认授予（用于基础能力，如 file_system）
    pub auto_granted: bool,
    /// agent config 显式声明即授予
    pub agent_can_grant: bool,
    /// 当前 session 绑定的 workflow 声明即授予
    pub workflow_can_grant: bool,
}

/// 返回所有平台 well-known 能力的默认可见性规则。
pub fn default_visibility_rules() -> &'static [CapabilityVisibilityRule] {
    use SessionOwnerType::*;

    static RULES: &[CapabilityVisibilityRule] = &[
        CapabilityVisibilityRule {
            key: CAP_FILE_READ,
            allowed_owner_types: &[Project, Story, Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_FILE_WRITE,
            allowed_owner_types: &[Project, Story, Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_SHELL_EXECUTE,
            allowed_owner_types: &[Project, Story, Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_CANVAS,
            allowed_owner_types: &[Project],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_WORKFLOW,
            allowed_owner_types: &[Project, Story, Task],
            auto_granted: false,
            agent_can_grant: false,
            workflow_can_grant: true,
        },
        CapabilityVisibilityRule {
            key: CAP_COLLABORATION,
            allowed_owner_types: &[Project],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_STORY_MANAGEMENT,
            allowed_owner_types: &[Story],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_TASK_MANAGEMENT,
            allowed_owner_types: &[Task],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_RELAY_MANAGEMENT,
            allowed_owner_types: &[Project],
            auto_granted: true,
            agent_can_grant: false,
            workflow_can_grant: false,
        },
        CapabilityVisibilityRule {
            key: CAP_WORKFLOW_MANAGEMENT,
            allowed_owner_types: &[Project],
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
/// 3. 屏蔽（AND）：`owner_type` 不在 `allowed_owner_types` 列表内 → 不可见。
/// 4. 授予（OR）：`auto_granted` 或（`agent_can_grant && agent_declares`）或
///    （`workflow_can_grant && has_active_workflow`）任一为真即可见。
pub fn is_capability_visible(
    cap: &ToolCapability,
    owner_type: SessionOwnerType,
    agent_declares: bool,
    has_active_workflow: bool,
) -> bool {
    if cap.is_custom_mcp() {
        return true;
    }

    // 别名不出现在 visibility rules 中，由展开后的 key 各自判断
    if expand_alias(cap.key()).is_some() {
        return false;
    }

    let rules = default_visibility_rules();
    let rule = match rules.iter().find(|r| r.key == cap.key()) {
        Some(r) => r,
        None => return false,
    };

    if !rule.allowed_owner_types.contains(&owner_type) {
        return false;
    }

    rule.auto_granted
        || (rule.agent_can_grant && agent_declares)
        || (rule.workflow_can_grant && has_active_workflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_key_classification() {
        let fs = ToolCapability::new(CAP_FILE_SYSTEM);
        assert!(fs.is_well_known());
        assert!(!fs.is_custom_mcp());
        assert_eq!(fs.custom_mcp_server_name(), None);
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
        assert_eq!(capability_to_tool_clusters(&cap), vec![ToolCluster::Execute]);
    }

    #[test]
    fn file_system_alias_expands_to_three_clusters() {
        let cap = ToolCapability::new(CAP_FILE_SYSTEM);
        let clusters = capability_to_tool_clusters(&cap);
        assert_eq!(clusters, vec![ToolCluster::Read, ToolCluster::Write, ToolCluster::Execute]);
    }

    #[test]
    fn expand_alias_file_system() {
        let expanded = expand_alias("file_system").unwrap();
        assert_eq!(expanded, &["file_read", "file_write", "shell_execute"]);
        assert!(expand_alias("file_read").is_none());
    }

    #[test]
    fn cluster_tools_returns_correct_tool_names() {
        assert_eq!(cluster_tools(ToolCluster::Read), &["mounts_list", "fs_read", "fs_glob", "fs_grep"]);
        assert_eq!(cluster_tools(ToolCluster::Write), &["fs_apply_patch"]);
        assert_eq!(cluster_tools(ToolCluster::Execute), &["shell_exec"]);
    }

    #[test]
    fn capability_tools_returns_all_tools() {
        let tools = capability_tools("file_system");
        assert!(tools.contains(&"fs_read"));
        assert!(tools.contains(&"fs_apply_patch"));
        assert!(tools.contains(&"shell_exec"));
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
        assert!(is_capability_visible(&cap, SessionOwnerType::Project, false, false));
    }

    #[test]
    fn visibility_project_session_gets_file_write() {
        let cap = ToolCapability::new(CAP_FILE_WRITE);
        assert!(is_capability_visible(&cap, SessionOwnerType::Project, false, false));
    }

    #[test]
    fn visibility_file_system_alias_not_directly_visible() {
        let cap = ToolCapability::new(CAP_FILE_SYSTEM);
        assert!(!is_capability_visible(&cap, SessionOwnerType::Project, false, false));
    }

    #[test]
    fn visibility_task_session_no_story_management() {
        let cap = ToolCapability::new(CAP_STORY_MANAGEMENT);
        assert!(!is_capability_visible(
            &cap,
            SessionOwnerType::Task,
            false,
            false
        ));
    }

    #[test]
    fn visibility_workflow_requires_active_workflow() {
        let cap = ToolCapability::new(CAP_WORKFLOW);
        assert!(!is_capability_visible(
            &cap,
            SessionOwnerType::Project,
            false,
            false,
        ));
        assert!(is_capability_visible(
            &cap,
            SessionOwnerType::Project,
            false,
            true,
        ));
    }

    #[test]
    fn visibility_workflow_management_requires_agent_declaration() {
        let cap = ToolCapability::new(CAP_WORKFLOW_MANAGEMENT);
        assert!(!is_capability_visible(
            &cap,
            SessionOwnerType::Project,
            false,
            false,
        ));
        assert!(is_capability_visible(
            &cap,
            SessionOwnerType::Project,
            true,
            false,
        ));
    }

    #[test]
    fn visibility_workflow_management_workflow_grant_path() {
        // 新 OR 语义：workflow 激活即可授予 workflow_management，
        // 无需 agent config 显式声明，匹配 builtin_workflow_admin 使用场景。
        let cap = ToolCapability::new(CAP_WORKFLOW_MANAGEMENT);
        assert!(is_capability_visible(
            &cap,
            SessionOwnerType::Project,
            false,
            true,
        ));
    }

    #[test]
    fn visibility_workflow_management_hard_boundary_still_blocks() {
        // 屏蔽 AND：allowed_owner_types 是硬边界，Task/Story owner
        // 即便同时命中所有授予源也不可见。
        let cap = ToolCapability::new(CAP_WORKFLOW_MANAGEMENT);
        assert!(!is_capability_visible(
            &cap,
            SessionOwnerType::Task,
            true,
            true,
        ));
        assert!(!is_capability_visible(
            &cap,
            SessionOwnerType::Story,
            true,
            true,
        ));
    }

    #[test]
    fn visibility_custom_mcp_always_visible() {
        let cap = ToolCapability::custom_mcp("anything");
        assert!(is_capability_visible(
            &cap,
            SessionOwnerType::Task,
            false,
            false,
        ));
    }

    #[test]
    fn serde_roundtrip() {
        let cap = ToolCapability::new("file_system");
        let json = serde_json::to_string(&cap).unwrap();
        assert_eq!(json, "\"file_system\"");
        let deserialized: ToolCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cap);
    }
}
