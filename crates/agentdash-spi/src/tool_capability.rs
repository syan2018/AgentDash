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

    /// 是否为平台 well-known key。
    pub fn is_well_known(&self) -> bool {
        WELL_KNOWN_KEYS.contains(&self.0.as_str())
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

pub const CAP_FILE_SYSTEM: &str = "file_system";
pub const CAP_CANVAS: &str = "canvas";
pub const CAP_WORKFLOW: &str = "workflow";
pub const CAP_COLLABORATION: &str = "collaboration";
pub const CAP_STORY_MANAGEMENT: &str = "story_management";
pub const CAP_TASK_MANAGEMENT: &str = "task_management";
pub const CAP_RELAY_MANAGEMENT: &str = "relay_management";
pub const CAP_WORKFLOW_MANAGEMENT: &str = "workflow_management";

const MCP_KEY_PREFIX: &str = "mcp:";

/// 所有平台 well-known key 列表（用于校验）。
pub const WELL_KNOWN_KEYS: &[&str] = &[
    CAP_FILE_SYSTEM,
    CAP_CANVAS,
    CAP_WORKFLOW,
    CAP_COLLABORATION,
    CAP_STORY_MANAGEMENT,
    CAP_TASK_MANAGEMENT,
    CAP_RELAY_MANAGEMENT,
    CAP_WORKFLOW_MANAGEMENT,
];

// ── well-known key → ToolCluster 映射 ──

/// 返回 well-known key 对应的 `ToolCluster` 集合。
/// 非 well-known key 或无 ToolCluster 映射的 key 返回空 vec。
pub fn capability_to_tool_clusters(cap: &ToolCapability) -> Vec<ToolCluster> {
    match cap.key() {
        CAP_FILE_SYSTEM => vec![ToolCluster::Read, ToolCluster::Write, ToolCluster::Execute],
        CAP_CANVAS => vec![ToolCluster::Canvas],
        CAP_WORKFLOW => vec![ToolCluster::Workflow],
        CAP_COLLABORATION => vec![ToolCluster::Collaboration],
        // MCP-only 能力不映射 ToolCluster
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
            key: CAP_FILE_SYSTEM,
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
            // 保留 agent config 声明路径（向后兼容）
            agent_can_grant: true,
            // 新增：workflow 绑定即可授予，配合 builtin_workflow_admin 等内建工作流
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
    // 自定义 MCP 不受 visibility rule 限制
    if cap.is_custom_mcp() {
        return true;
    }

    let rules = default_visibility_rules();
    let rule = match rules.iter().find(|r| r.key == cap.key()) {
        Some(r) => r,
        // 未知 well-known key 默认不可见
        None => return false,
    };

    // 屏蔽（AND）：owner 硬边界
    if !rule.allowed_owner_types.contains(&owner_type) {
        return false;
    }

    // 授予（OR）：任一来源命中即可见
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
    fn file_system_maps_to_read_write_execute() {
        let cap = ToolCapability::new(CAP_FILE_SYSTEM);
        let clusters = capability_to_tool_clusters(&cap);
        assert_eq!(
            clusters,
            vec![ToolCluster::Read, ToolCluster::Write, ToolCluster::Execute]
        );
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
    fn file_system_has_no_mcp_scope() {
        let cap = ToolCapability::new(CAP_FILE_SYSTEM);
        assert_eq!(capability_to_platform_mcp_scope(&cap), None);
    }

    #[test]
    fn visibility_project_session_gets_file_system() {
        let cap = ToolCapability::new(CAP_FILE_SYSTEM);
        assert!(is_capability_visible(
            &cap,
            SessionOwnerType::Project,
            false,
            false
        ));
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
