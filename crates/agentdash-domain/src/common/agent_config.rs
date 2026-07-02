use serde::{Deserialize, Serialize};

use crate::common::MountCapability;
use crate::common::error::DomainError;
use crate::workflow::{ToolCapabilityDirective, mcp_capability_key};

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentBackendRequirement {
    #[default]
    Required,
    Optional,
}

// ── AgentPresetConfig ─────────────────────────────────────────────────

/// Agent 配置存储层的权威类型。
///
/// 统一 ProjectAgent / AgentPreset / AgentTemplate 的配置 JSON blob。
/// 所有字段均为 Option，方便模板安装、项目实例编辑和运行态解析共享同一结构。
///
/// 消费方通过 `to_agent_config()` 提取运行态执行器配置 [`AgentConfig`]。
#[derive(Debug, Clone, Default, Serialize)]
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
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_requirement: Option<AgentBackendRequirement>,

    /// Agent 级能力指令。前端、API、存储与 Resolver 使用同一套 directive 表示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_directives: Option<Vec<ToolCapabilityDirective>>,
    /// ProjectAgent preset 暴露的 Project VFS mount 及其 capability 裁剪。
    ///
    /// 该字段只描述 Project VFS mount exposure 输入；运行期 mount/path 准入由
    /// RuntimeVfsAccessPolicy 表达。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_vfs_mount_exposure_grants: Option<Vec<ProjectVfsMountExposureGrant>>,
    /// Project SkillAsset key 引用列表（如 `["research", "writer"]`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_asset_keys: Option<Vec<String>>,
    /// 此 Agent 是否默认进入同项目其它 Agent 的 companion roster。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_companion_enabled: Option<bool>,
    /// 调用侧额外加入的非默认 companion agent 名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_companions: Option<Vec<String>>,
    /// 此 Agent 可见的 Workspace Module ref 白名单（形如 `ext:{key}` / `canvas:{canvas_mount_id}`）。
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
            display_name,
            description,
            backend_requirement,
            capability_directives,
            project_vfs_mount_exposure_grants,
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
        }
    }

    pub fn backend_requirement_or_default(&self) -> AgentBackendRequirement {
        self.backend_requirement.unwrap_or_default()
    }

    /// 从 DB JSON 反序列化为权威配置结构。
    pub fn from_json(value: &serde_json::Value) -> Result<Self, DomainError> {
        if value
            .as_object()
            .is_some_and(|object| object.contains_key("vfs_access_grants"))
        {
            return Err(DomainError::InvalidConfig(
                "`vfs_access_grants` 已收束为 `project_vfs_mount_exposure_grants`".to_string(),
            ));
        }
        serde_json::from_value::<Self>(value.clone()).map_err(DomainError::Serialization)
    }

    /// 归一化 ProjectAgent config JSON，并保留未知字段。
    ///
    /// API 的 `config` 仍是 opaque JSON；这里集中处理 legacy `mcp_preset_keys`
    /// 迁移，避免 route 层手写一套路径语义。
    pub fn normalize_json_value(
        value: &serde_json::Value,
    ) -> Result<serde_json::Value, DomainError> {
        let normalized = Self::from_json(value)?;
        let mut object = match value {
            serde_json::Value::Object(map) => map.clone(),
            _ => {
                return Err(DomainError::InvalidConfig(
                    "AgentPresetConfig 必须是 JSON object".to_string(),
                ));
            }
        };

        object.remove("mcp_preset_keys");
        object.remove("system_prompt_mode");
        match normalized.capability_directives {
            Some(directives) => {
                object.insert(
                    "capability_directives".to_string(),
                    serde_json::to_value(directives)?,
                );
            }
            None => {
                object.remove("capability_directives");
            }
        }

        Ok(serde_json::Value::Object(object))
    }

    fn prepend_legacy_mcp_preset_keys(
        &mut self,
        legacy_keys: Option<Vec<String>>,
    ) -> Result<(), String> {
        let Some(legacy_keys) = legacy_keys else {
            return Ok(());
        };
        let mut legacy_directives = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for (index, key) in legacy_keys.into_iter().enumerate() {
            let capability_key = mcp_capability_key(&key)
                .map_err(|reason| format!("mcp_preset_keys[{index}] 非法: {reason}"))?;
            if seen.insert(capability_key.clone()) {
                legacy_directives.push(ToolCapabilityDirective::add_simple(capability_key));
            }
        }

        if legacy_directives.is_empty() {
            return Ok(());
        }

        let mut merged = legacy_directives;
        if let Some(explicit) = self.capability_directives.take() {
            merged.extend(explicit);
        }
        self.capability_directives = Some(merged);
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AgentPresetConfigWire {
    executor: Option<String>,
    provider_id: Option<String>,
    model_id: Option<String>,
    agent_id: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    permission_policy: Option<String>,
    system_prompt: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    backend_requirement: Option<AgentBackendRequirement>,
    capability_directives: Option<Vec<ToolCapabilityDirective>>,
    mcp_preset_keys: Option<Vec<String>>,
    project_vfs_mount_exposure_grants: Option<Vec<ProjectVfsMountExposureGrant>>,
    skill_asset_keys: Option<Vec<String>>,
    default_companion_enabled: Option<bool>,
    extra_companions: Option<Vec<String>>,
    visible_workspace_module_refs: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for AgentPresetConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = AgentPresetConfigWire::deserialize(deserializer)?;
        let mut config = Self {
            executor: wire.executor,
            provider_id: wire.provider_id,
            model_id: wire.model_id,
            agent_id: wire.agent_id,
            thinking_level: wire.thinking_level,
            permission_policy: wire.permission_policy,
            system_prompt: wire.system_prompt,
            display_name: wire.display_name,
            description: wire.description,
            backend_requirement: wire.backend_requirement,
            capability_directives: wire.capability_directives,
            project_vfs_mount_exposure_grants: wire.project_vfs_mount_exposure_grants,
            skill_asset_keys: wire.skill_asset_keys,
            default_companion_enabled: wire.default_companion_enabled,
            extra_companions: wire.extra_companions,
            visible_workspace_module_refs: wire.visible_workspace_module_refs,
        };
        config
            .prepend_legacy_mcp_preset_keys(wire.mcp_preset_keys)
            .map_err(serde::de::Error::custom)?;
        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectVfsMountExposureGrant {
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
            "project_vfs_mount_exposure_grants": [{ "mount_id": "brief", "capabilities": ["read", "list"] }],
            "default_companion_enabled": true,
            "extra_companions": ["deep-reviewer"],
            "backend_requirement": "optional",
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
        assert_eq!(
            config
                .project_vfs_mount_exposure_grants
                .as_ref()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(config.default_companion_enabled, Some(true));
        assert_eq!(
            config.backend_requirement,
            Some(AgentBackendRequirement::Optional)
        );
        assert_eq!(
            config.extra_companions.as_deref(),
            Some(["deep-reviewer".to_string()].as_slice())
        );

        let serialized = serde_json::to_value(&config).expect("serialize config");
        assert!(serialized.get("vfs_access_grants").is_none());
        assert_eq!(
            serialized["project_vfs_mount_exposure_grants"],
            serde_json::json!([{ "mount_id": "brief", "capabilities": ["read", "list"] }])
        );
    }

    #[test]
    fn preset_config_normalizes_legacy_mcp_preset_keys_before_explicit_directives() {
        let config = AgentPresetConfig::from_json(&serde_json::json!({
            "mcp_preset_keys": [" abc-config ", "docs", "abc-config"],
            "capability_directives": [
                { "remove": "mcp:abc-config::ABCConfigAnalyzer_get_file_content" }
            ]
        }))
        .expect("legacy mcp preset keys should normalize");

        let directives = config
            .capability_directives
            .as_ref()
            .expect("normalized capability directives");
        assert_eq!(directives.len(), 3);
        assert_eq!(
            serde_json::to_value(directives).expect("serialize directives"),
            serde_json::json!([
                { "add": "mcp:abc-config" },
                { "add": "mcp:docs" },
                { "remove": "mcp:abc-config::ABCConfigAnalyzer_get_file_content" }
            ])
        );

        let serialized = serde_json::to_value(&config).expect("serialize config");
        assert!(serialized.get("mcp_preset_keys").is_none());
    }

    #[test]
    fn preset_config_normalize_json_value_preserves_unknown_fields() {
        let normalized = AgentPresetConfig::normalize_json_value(&serde_json::json!({
            "display_name": "Analyzer",
            "unknown_future_field": { "keep": true },
            "mcp_preset_keys": ["abc-config"],
            "capability_directives": [{ "remove": "mcp:abc-config::hidden_tool" }]
        }))
        .expect("normalize config json");

        assert_eq!(normalized["display_name"], "Analyzer");
        assert_eq!(normalized["unknown_future_field"]["keep"], true);
        assert!(normalized.get("mcp_preset_keys").is_none());
        assert_eq!(
            normalized["capability_directives"],
            serde_json::json!([
                { "add": "mcp:abc-config" },
                { "remove": "mcp:abc-config::hidden_tool" }
            ])
        );
    }

    #[test]
    fn preset_config_rejects_invalid_legacy_mcp_preset_keys() {
        let result = AgentPresetConfig::from_json(&serde_json::json!({
            "mcp_preset_keys": ["abc::config"]
        }));

        assert!(result.is_err());
    }

    #[test]
    fn preset_config_rejects_old_vfs_access_grants_field() {
        let result = AgentPresetConfig::from_json(&serde_json::json!({
            "vfs_access_grants": [{ "mount_id": "legacy", "capabilities": ["read"] }]
        }));

        assert!(result.is_err());
    }

    #[test]
    fn preset_config_merge_over_replaces_skill_asset_keys_when_present() {
        let base = AgentPresetConfig {
            skill_asset_keys: Some(vec!["base".to_string()]),
            project_vfs_mount_exposure_grants: Some(vec![ProjectVfsMountExposureGrant {
                mount_id: "base".to_string(),
                capabilities: vec![MountCapability::Read],
            }]),
            ..Default::default()
        };
        let over = AgentPresetConfig {
            skill_asset_keys: Some(vec!["override".to_string()]),
            project_vfs_mount_exposure_grants: Some(vec![ProjectVfsMountExposureGrant {
                mount_id: "override".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
            }]),
            ..Default::default()
        };

        let merged = over.merge_over(&base);

        assert_eq!(merged.skill_asset_keys, Some(vec!["override".to_string()]));
        assert_eq!(
            merged
                .project_vfs_mount_exposure_grants
                .as_ref()
                .map(|items| items[0].mount_id.as_str()),
            Some("override")
        );
    }

    #[test]
    fn preset_config_roundtrips_and_merges_visible_workspace_module_refs() {
        let config = AgentPresetConfig::from_json(&serde_json::json!({
            "visible_workspace_module_refs": ["ext:demo", "canvas:cvs-dashboard-a"]
        }))
        .expect("valid preset config");
        assert_eq!(
            config.visible_workspace_module_refs.as_deref(),
            Some(["ext:demo".to_string(), "canvas:cvs-dashboard-a".to_string()].as_slice())
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

    #[test]
    fn preset_config_backend_requirement_defaults_to_required() {
        let config = AgentPresetConfig::from_json(&serde_json::json!({}))
            .expect("empty config should parse");

        assert_eq!(
            config.backend_requirement_or_default(),
            AgentBackendRequirement::Required
        );

        let optional = AgentPresetConfig::from_json(&serde_json::json!({
            "backend_requirement": "optional"
        }))
        .expect("optional requirement should parse");
        assert_eq!(
            optional.backend_requirement_or_default(),
            AgentBackendRequirement::Optional
        );
    }

    #[test]
    fn preset_config_normalization_drops_system_prompt_mode() {
        let normalized = AgentPresetConfig::normalize_json_value(&serde_json::json!({
            "system_prompt": "agent rules",
            "system_prompt_mode": "override"
        }))
        .expect("normalizes stale prompt mode");

        assert_eq!(normalized.get("system_prompt_mode"), None);
        assert_eq!(
            normalized
                .get("system_prompt")
                .and_then(serde_json::Value::as_str),
            Some("agent rules")
        );
    }
}
