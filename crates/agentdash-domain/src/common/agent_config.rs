use serde::{Deserialize, Serialize};

use crate::common::MountCapability;
use crate::common::error::DomainError;
use crate::workflow::ToolCapabilityDirective;

/// Agent 级 System Prompt 注入模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPromptMode {
    /// 在全局 system prompt 之后追加（默认）。
    #[default]
    Append,
    /// 完全替换全局 system prompt。
    Override,
}

/// 思考/推理级别 — 跨层通用值对象。
///
/// 在 Domain 层定义，避免各层重复声明或依赖具体 Agent 运行时。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

// ── AgentPresetConfig ─────────────────────────────────────────────────

/// Agent 配置存储层的权威类型。
///
/// 统一 ProjectAgent / AgentPreset / AgentTemplate 的配置 JSON blob。
/// 所有字段均为 Option，方便模板安装、项目实例编辑和运行态解析共享同一结构。
///
/// 消费方通过 `to_agent_config()` 提取运行态执行器配置 [`AgentConfig`]。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentPresetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_mode: Option<SystemPromptMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Agent 级能力指令。替代旧 `tool_clusters: Option<Vec<String>>`。
    /// 前端 → API → 存储 → Resolver 全链路使用相同的 `ToolCapabilityDirective` 表示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_directives: Option<Vec<ToolCapabilityDirective>>,
    /// MCP Preset key 引用列表（如 `["github", "jira"]`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_preset_keys: Option<Vec<String>>,
    /// Agent 可访问的 Project VFS mount 及其权限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vfs_access_grants: Option<Vec<AgentVfsAccessGrant>>,
    /// Project SkillAsset key 引用列表（如 `["research", "writer"]`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_asset_keys: Option<Vec<String>>,
    /// 此 Agent 是否默认进入同项目其它 Agent 的 companion roster。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_companion_enabled: Option<bool>,
    /// 调用侧额外加入的非默认 companion agent 名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_companions: Option<Vec<String>>,
    /// 此 Agent 可见的 Workspace Module ref 白名单（形如 `ext:{key}` / `canvas:{mount_id}`）。
    ///
    /// 事实源为 ProjectAgent 定义，frame construction 据此填充
    /// `AgentFrame.visible_workspace_module_refs_json`。`None`/空 → 全集可见；非空 → 仅白名单。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_workspace_module_refs: Option<Vec<String>>,
}

/// 用于字段级合并的 helper macro — 消除逐字段重复代码。
macro_rules! merge_field {
    ($over:expr, $base:expr, $($field:ident),+ $(,)?) => {
        Self {
            $( $field: $over.$field.clone().or_else(|| $base.$field.clone()), )+
        }
    };
}

impl AgentPresetConfig {
    /// 字段级合并：`self`（override）非 None 的字段优先于 `base`。
    pub fn merge_over(&self, base: &AgentPresetConfig) -> AgentPresetConfig {
        merge_field!(
            self,
            base,
            executor,
            provider_id,
            model_id,
            agent_id,
            thinking_level,
            permission_policy,
            system_prompt,
            system_prompt_mode,
            display_name,
            description,
            capability_directives,
            mcp_preset_keys,
            vfs_access_grants,
            skill_asset_keys,
            default_companion_enabled,
            extra_companions,
            visible_workspace_module_refs,
        )
    }

    /// 提取运行态执行器配置 [`AgentConfig`]。
    ///
    /// `fallback_executor` 在 `self.executor` 为 None 时使用（通常来自 `Agent.agent_type`）。
    pub fn to_agent_config(&self, fallback_executor: &str) -> AgentConfig {
        AgentConfig {
            executor: self
                .executor
                .clone()
                .unwrap_or_else(|| fallback_executor.to_string()),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            agent_id: self.agent_id.clone(),
            thinking_level: self.thinking_level,
            permission_policy: self.permission_policy.clone(),
            system_prompt: self.system_prompt.clone(),
            system_prompt_mode: self.system_prompt_mode,
        }
    }

    /// 从 DB JSON 反序列化为权威配置结构。
    pub fn from_json(value: &serde_json::Value) -> Result<Self, DomainError> {
        serde_json::from_value(value.clone()).map_err(DomainError::Serialization)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentVfsAccessGrant {
    pub mount_id: String,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
}

// ── AgentConfig（运行态执行器配置）──────────────────────────────────────

/// AgentDash 统一执行器配置 — connector 层的运行态接口类型。
///
/// 只包含执行器运行所需的参数（executor / model / prompt 等），
/// 不包含 capability / companion / MCP 等配置（由 `AgentPresetConfig` 承载）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub executor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_mode: Option<SystemPromptMode>,
}

const CLOUD_NATIVE_EXECUTORS: &[&str] = &["PI_AGENT"];

impl AgentConfig {
    pub fn new(executor: impl Into<String>) -> Self {
        Self {
            executor: executor.into(),
            provider_id: None,
            model_id: None,
            agent_id: None,
            thinking_level: None,
            permission_policy: None,
            system_prompt: None,
            system_prompt_mode: None,
        }
    }

    /// 判断此配置是否指向云端原生 Agent（在云端进程内执行，不经由本机后端中继）。
    pub fn is_cloud_native(&self) -> bool {
        CLOUD_NATIVE_EXECUTORS
            .iter()
            .any(|id| self.executor.eq_ignore_ascii_case(id))
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self::new("CLAUDE_CODE")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_config_roundtrips_description_and_capability_directives() {
        let config = AgentPresetConfig::from_json(&serde_json::json!({
            "display_name": "Reviewer",
            "description": "检查代码结构",
            "skill_asset_keys": ["research", "review"],
            "vfs_access_grants": [{ "mount_id": "brief", "capabilities": ["read", "list"] }],
            "default_companion_enabled": true,
            "extra_companions": ["deep-reviewer"],
            "capability_directives": [{ "add": "workflow_management" }]
        }))
        .expect("valid preset config");

        assert_eq!(config.display_name.as_deref(), Some("Reviewer"));
        assert_eq!(config.description.as_deref(), Some("检查代码结构"));
        assert_eq!(
            config.skill_asset_keys.as_deref(),
            Some(["research".to_string(), "review".to_string()].as_slice())
        );
        assert_eq!(config.capability_directives.as_ref().map(Vec::len), Some(1));
        assert_eq!(config.vfs_access_grants.as_ref().map(Vec::len), Some(1));
        assert_eq!(config.default_companion_enabled, Some(true));
        assert_eq!(
            config.extra_companions.as_deref(),
            Some(["deep-reviewer".to_string()].as_slice())
        );
    }

    #[test]
    fn preset_config_merge_over_replaces_skill_asset_keys_when_present() {
        let base = AgentPresetConfig {
            skill_asset_keys: Some(vec!["base".to_string()]),
            mcp_preset_keys: Some(vec!["mcp-base".to_string()]),
            vfs_access_grants: Some(vec![AgentVfsAccessGrant {
                mount_id: "base".to_string(),
                capabilities: vec![MountCapability::Read],
            }]),
            ..Default::default()
        };
        let over = AgentPresetConfig {
            skill_asset_keys: Some(vec!["override".to_string()]),
            vfs_access_grants: Some(vec![AgentVfsAccessGrant {
                mount_id: "override".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
            }]),
            ..Default::default()
        };

        let merged = over.merge_over(&base);

        assert_eq!(merged.skill_asset_keys, Some(vec!["override".to_string()]));
        assert_eq!(merged.mcp_preset_keys, Some(vec!["mcp-base".to_string()]));
        assert_eq!(
            merged
                .vfs_access_grants
                .as_ref()
                .map(|items| items[0].mount_id.as_str()),
            Some("override")
        );
    }

    #[test]
    fn preset_config_roundtrips_and_merges_visible_workspace_module_refs() {
        let config = AgentPresetConfig::from_json(&serde_json::json!({
            "visible_workspace_module_refs": ["ext:demo", "canvas:dashboard-a"]
        }))
        .expect("valid preset config");
        assert_eq!(
            config.visible_workspace_module_refs.as_deref(),
            Some(["ext:demo".to_string(), "canvas:dashboard-a".to_string()].as_slice())
        );

        let base = AgentPresetConfig {
            visible_workspace_module_refs: Some(vec!["ext:base".to_string()]),
            ..Default::default()
        };
        let over = AgentPresetConfig {
            visible_workspace_module_refs: Some(vec!["ext:override".to_string()]),
            ..Default::default()
        };
        assert_eq!(
            over.merge_over(&base).visible_workspace_module_refs,
            Some(vec!["ext:override".to_string()])
        );
        // 未配置（None）保留 base，回归"未配置=全集"不被破坏。
        assert_eq!(
            AgentPresetConfig::default()
                .merge_over(&base)
                .visible_workspace_module_refs,
            Some(vec!["ext:base".to_string()])
        );
    }

    #[test]
    fn preset_config_rejects_invalid_typed_payload() {
        let result = AgentPresetConfig::from_json(&serde_json::json!({
            "thinking_level": "not_a_level"
        }));

        assert!(result.is_err());
    }
}
