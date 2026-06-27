use std::fmt;

use serde::{Deserialize, Serialize};

use super::MountDirective;

/// 顶层能力模型的声明式配置。
///
/// 工具能力、资源挂载、上下文和策略都属于顶层 Capability Model 的不同维度。
/// 当前已经落地 tool 与 mount 两个维度，后续 context overlay、permission policy、
/// resource budget 等能力维度继续扩展在这里。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilityConfig {
    /// 工具能力维度的声明式变更。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_directives: Vec<ToolCapabilityDirective>,
    /// VFS/mount 维度的声明式变更。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mount_directives: Vec<MountDirective>,
}
// ── ToolCapability 路径 ──

/// ToolCapability 路径 — 统一表达「能力级」和「工具级」两种寻址。
///
/// `capability` 是 tool capability key（如 `"file_read"` 或 `"mcp:code_analyzer"`）；
/// `tool` 为 `None` 表示短 path（整个能力），`Some(name)` 表示长 path（能力下的某个工具）。
///
/// 分隔符统一为 `::`（与 Rust 模块路径同构），与 `mcp:<server>` 的单冒号前缀不冲突。
/// MCP server name 禁止含 `::`，由 preset 校验层强制。
///
/// JSON 形式序列化为 qualified string：`"file_read"` / `"file_read::fs_grep"`
/// / `"workflow_management::upsert_workflow_tool"` / `"mcp:code_analyzer::scan"`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToolCapabilityPath {
    pub capability: String,
    pub tool: Option<String>,
}

const TOOL_CAPABILITY_PATH_SEPARATOR: &str = "::";
const MCP_CAPABILITY_PREFIX: &str = "mcp:";

fn normalize_mcp_capability_segment(field: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} 不能为空"));
    }
    if trimmed.contains(TOOL_CAPABILITY_PATH_SEPARATOR) {
        return Err(format!(
            "{field} `{value}` 不能包含 `{TOOL_CAPABILITY_PATH_SEPARATOR}`"
        ));
    }
    Ok(trimmed.to_string())
}

/// 将 Project MCP preset key 转为唯一的 MCP capability key。
pub fn mcp_capability_key(preset_key: &str) -> Result<String, String> {
    let preset_key = normalize_mcp_capability_segment("MCP preset key", preset_key)?;
    Ok(format!("{MCP_CAPABILITY_PREFIX}{preset_key}"))
}

/// 构造 MCP 工具级 capability path。
pub fn mcp_tool_capability_path(
    preset_key: &str,
    tool_name: &str,
) -> Result<ToolCapabilityPath, String> {
    let capability = mcp_capability_key(preset_key)?;
    let tool_name = normalize_mcp_capability_segment("MCP tool name", tool_name)?;
    Ok(ToolCapabilityPath::of_tool(capability, tool_name))
}

impl ToolCapabilityPath {
    /// 构造能力级短 path。
    pub fn of_capability(key: impl Into<String>) -> Self {
        Self {
            capability: key.into(),
            tool: None,
        }
    }

    /// 构造工具级长 path。
    pub fn of_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self {
            capability: cap.into(),
            tool: Some(tool.into()),
        }
    }

    /// 是否为工具级 path。
    pub fn is_tool_level(&self) -> bool {
        self.tool.is_some()
    }

    /// 序列化为 qualified string —— `"cap"` 或 `"cap::tool"`。
    pub fn to_qualified_string(&self) -> String {
        match &self.tool {
            None => self.capability.clone(),
            Some(tool) => format!("{}{TOOL_CAPABILITY_PATH_SEPARATOR}{tool}", self.capability),
        }
    }

    /// 解析 qualified string —— 反向对应 `to_qualified_string`。
    ///
    /// 规则：
    /// - 空字符串 → Err
    /// - 恰好一个 `::` → Ok(long path)；两边均不得为空
    /// - 多于一个 `::` → Err（不允许多级嵌套）
    /// - 无 `::` → Ok(short path)
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("ToolCapabilityPath 不能为空".to_string());
        }

        // 统计 `::` 出现次数（按字符位置扫描，避免误处理单 `:` 前缀）
        let parts: Vec<&str> = trimmed.split(TOOL_CAPABILITY_PATH_SEPARATOR).collect();
        match parts.len() {
            1 => Ok(Self {
                capability: parts[0].to_string(),
                tool: None,
            }),
            2 => {
                let cap = parts[0];
                let tool = parts[1];
                if cap.is_empty() {
                    return Err(format!("ToolCapabilityPath `{s}` 缺少 capability 段"));
                }
                if tool.is_empty() {
                    return Err(format!("ToolCapabilityPath `{s}` 缺少 tool 段"));
                }
                Ok(Self {
                    capability: cap.to_string(),
                    tool: Some(tool.to_string()),
                })
            }
            _ => Err(format!(
                "ToolCapabilityPath `{s}` 包含多个 `::` 分隔符，仅允许一级工具寻址"
            )),
        }
    }
}

impl fmt::Display for ToolCapabilityPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_qualified_string())
    }
}

impl Serialize for ToolCapabilityPath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_qualified_string())
    }
}

impl<'de> Deserialize<'de> for ToolCapabilityPath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// 工具能力指令 —— 在 agent baseline 上执行 Add/Remove。
///
/// `Add(path)` 追加能力或启用工具，`Remove(path)` 屏蔽能力或屏蔽工具。
/// `path` 为短 path 表示能力级操作；长 path 表示工具级操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapabilityDirective {
    Add(ToolCapabilityPath),
    Remove(ToolCapabilityPath),
}

impl ToolCapabilityDirective {
    /// 返回指令操作的 capability key（无论工具级还是能力级）。
    pub fn key(&self) -> &str {
        match self {
            Self::Add(path) | Self::Remove(path) => &path.capability,
        }
    }

    /// 返回指令携带的 path 引用。
    pub fn path(&self) -> &ToolCapabilityPath {
        match self {
            Self::Add(p) | Self::Remove(p) => p,
        }
    }

    pub fn is_add(&self) -> bool {
        matches!(self, Self::Add(_))
    }

    pub fn is_remove(&self) -> bool {
        matches!(self, Self::Remove(_))
    }

    /// 快捷构造：能力级 Add 指令（短 path）。
    pub fn add_simple(key: impl Into<String>) -> Self {
        Self::Add(ToolCapabilityPath::of_capability(key))
    }

    /// 快捷构造：能力级 Remove 指令（短 path）。
    pub fn remove_simple(key: impl Into<String>) -> Self {
        Self::Remove(ToolCapabilityPath::of_capability(key))
    }

    /// 快捷构造：工具级 Add 指令（长 path）。
    pub fn add_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self::Add(ToolCapabilityPath::of_tool(cap, tool))
    }

    /// 快捷构造：工具级 Remove 指令（长 path）。
    pub fn remove_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self::Remove(ToolCapabilityPath::of_tool(cap, tool))
    }
}

/// 工具能力归约状态机 slot —— 对单个 tool capability key 在一串 directive 后的最终状态。
///
/// 状态转移表：见 `compute_capabilities` 实现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCapabilitySlotState {
    /// 未被任何 directive 命中（对 auto_granted 能力仍可见）。
    NotDeclared,
    /// 命中过 `Add(cap, None)` —— 启用该 capability 全集。
    FullCapability,
    /// 仅命中过工具级 Add —— 只启用白名单内的工具。
    ToolWhitelist(std::collections::BTreeSet<String>),
    /// 最近命中 `Remove(cap, None)` —— 屏蔽整个能力。
    Blocked,
}

/// 归约结果：按 capability key 汇总 slot 状态 + 工具级排除集合。
#[derive(Debug, Clone, Default)]
pub struct ToolCapabilityReduction {
    pub slots: std::collections::BTreeMap<String, ToolCapabilitySlotState>,
    pub excluded_tools: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

/// 在一串 directive 上执行 slot 规则归约，产出 `ToolCapabilityReduction`。
///
/// 规则详见 `.trellis/spec/backend/capability/tool-capability-pipeline.md`
/// 「Slot 归约规则」章节。核心要点：
///
/// - `Add(cap, None)` → `FullCapability`（并清空 whitelist，若原先是 whitelist 状态则同义升级）
/// - `Add(cap, Some(t))` → `ToolWhitelist{t+}`（对 `FullCapability` 是 no-op）
/// - `Remove(cap, None)` → `Blocked`
/// - `Remove(cap, Some(t))` → 写入 `excluded_tools[cap] += t`，并在白名单中移除 t
///
/// 后来者胜 —— 每条指令按序执行。
pub fn reduce_tool_capability_directives(
    directives: &[ToolCapabilityDirective],
) -> ToolCapabilityReduction {
    use std::collections::{BTreeMap, BTreeSet};

    let mut slots: BTreeMap<String, ToolCapabilitySlotState> = BTreeMap::new();
    let mut excluded_tools: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for directive in directives {
        match directive {
            ToolCapabilityDirective::Add(path) => {
                let key = path.capability.clone();
                match &path.tool {
                    None => {
                        slots.insert(key, ToolCapabilitySlotState::FullCapability);
                    }
                    Some(tool) => {
                        let entry = slots
                            .entry(key.clone())
                            .or_insert(ToolCapabilitySlotState::NotDeclared);
                        match entry {
                            ToolCapabilitySlotState::FullCapability => {
                                // no-op：全集已启用
                            }
                            ToolCapabilitySlotState::ToolWhitelist(set) => {
                                set.insert(tool.clone());
                            }
                            ToolCapabilitySlotState::NotDeclared
                            | ToolCapabilitySlotState::Blocked => {
                                let mut set = BTreeSet::new();
                                set.insert(tool.clone());
                                *entry = ToolCapabilitySlotState::ToolWhitelist(set);
                            }
                        }
                    }
                }
            }
            ToolCapabilityDirective::Remove(path) => {
                let key = path.capability.clone();
                match &path.tool {
                    None => {
                        slots.insert(key, ToolCapabilitySlotState::Blocked);
                    }
                    Some(tool) => {
                        // 从白名单中移除（若存在），同时写入 excluded_tools
                        if let Some(ToolCapabilitySlotState::ToolWhitelist(set)) =
                            slots.get_mut(&key)
                        {
                            set.remove(tool);
                        }
                        excluded_tools.entry(key).or_default().insert(tool.clone());
                    }
                }
            }
        }
    }

    ToolCapabilityReduction {
        slots,
        excluded_tools,
    }
}
impl CapabilityConfig {
    pub fn is_empty(&self) -> bool {
        self.tool_directives.is_empty() && self.mount_directives.is_empty()
    }
}
