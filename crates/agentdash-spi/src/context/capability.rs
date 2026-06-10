use serde::{Deserialize, Serialize};

use crate::platform::skill_discovery::{
    SkillContextExposure, SkillDiscoveryDiagnostic, skill_capability_key,
};

/// Companion sub-session 的能力裁剪模式。
///
/// 控制 companion 继承父 session 能力时保留的范围。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionSliceMode {
    /// 完整继承父 session 能力。
    Full,
    /// 精简模式 — 保留 Read/List/Execute/Exec，移除 MCP。
    #[default]
    Compact,
    /// 仅保留 workflow 相关能力子集。
    WorkflowOnly,
    /// 仅保留约束相关能力子集。
    ConstraintsOnly,
}

/// 会话级 baseline capability 数据契约。
///
/// 承载"稳定能力描述"——skills 列表。
/// Companion agents 已迁移至 `CapabilityState.companion` 维度。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionBaselineCapabilities {
    pub skills: Vec<SkillEntry>,
    #[serde(default)]
    pub skill_clusters: Vec<SkillProviderCluster>,
    #[serde(default)]
    pub skill_diagnostics: Vec<SkillDiscoveryDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionAgentEntry {
    pub name: String,
    pub executor: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillEntry {
    /// 兼容旧消费者的展示名；新逻辑应优先使用 `capability_key` 做唯一标识。
    pub name: String,
    #[serde(default)]
    pub capability_key: String,
    #[serde(default)]
    pub provider_key: String,
    #[serde(default)]
    pub local_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub description: String,
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<String>,
    #[serde(default)]
    pub exposure: SkillContextExposure,
    #[serde(default)]
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCapabilityEntry {
    pub capability_key: String,
    pub provider_key: String,
    pub local_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub description: String,
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<String>,
    #[serde(default)]
    pub exposure: SkillContextExposure,
    #[serde(default)]
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillProviderCluster {
    pub provider_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_count: Option<usize>,
    #[serde(default)]
    pub default_exposed_skills: Vec<SkillCapabilityEntry>,
}

impl SkillEntry {
    pub fn capability_key_or_name(&self) -> &str {
        if self.capability_key.is_empty() {
            &self.name
        } else {
            &self.capability_key
        }
    }

    pub fn from_capability_entry(entry: &SkillCapabilityEntry) -> Self {
        Self {
            name: entry.local_name.clone(),
            capability_key: entry.capability_key.clone(),
            provider_key: entry.provider_key.clone(),
            local_name: entry.local_name.clone(),
            display_name: entry.display_name.clone(),
            description: entry.description.clone(),
            file_path: entry.file_path.clone(),
            base_dir: entry.base_dir.clone(),
            exposure: entry.exposure,
            disable_model_invocation: entry.disable_model_invocation,
        }
    }

    pub fn legacy(
        name: impl Into<String>,
        description: impl Into<String>,
        file_path: impl Into<String>,
        disable_model_invocation: bool,
    ) -> Self {
        let name = name.into();
        Self {
            capability_key: name.clone(),
            provider_key: String::new(),
            local_name: name.clone(),
            name,
            display_name: None,
            description: description.into(),
            file_path: file_path.into(),
            base_dir: None,
            exposure: SkillContextExposure::DefaultExposed,
            disable_model_invocation,
        }
    }
}

impl SkillCapabilityEntry {
    pub fn new(
        provider_key: impl Into<String>,
        local_name: impl Into<String>,
        description: impl Into<String>,
        file_path: impl Into<String>,
    ) -> Self {
        let provider_key = provider_key.into();
        let local_name = local_name.into();
        Self {
            capability_key: skill_capability_key(&provider_key, &local_name),
            provider_key,
            local_name,
            display_name: None,
            description: description.into(),
            file_path: file_path.into(),
            base_dir: None,
            exposure: SkillContextExposure::DefaultExposed,
            disable_model_invocation: false,
        }
    }
}

impl SessionBaselineCapabilities {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.skill_clusters.is_empty()
    }

    pub fn visible_skills(&self) -> Vec<&SkillEntry> {
        self.skills
            .iter()
            .filter(|s| s.exposure.is_default_exposed() && !s.disable_model_invocation)
            .collect()
    }
}
