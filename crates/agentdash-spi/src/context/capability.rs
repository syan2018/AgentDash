use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionAgentEntry {
    pub name: String,
    pub executor: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub file_path: String,
    #[serde(default)]
    pub disable_model_invocation: bool,
}

impl SessionBaselineCapabilities {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn visible_skills(&self) -> Vec<&SkillEntry> {
        self.skills
            .iter()
            .filter(|s| !s.disable_model_invocation)
            .collect()
    }
}
