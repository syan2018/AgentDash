use serde::{Deserialize, Serialize};

/// 会话级 baseline capability 数据契约。
///
/// 统一承载"稳定能力描述"——companion agents 与 skills，
/// 同时提供结构化（API/前端）和 Connector system prompt 两种输出形态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionBaselineCapabilities {
    pub companion_agents: Vec<CompanionAgentEntry>,
    pub skills: Vec<SkillEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        self.companion_agents.is_empty() && self.skills.is_empty()
    }

    pub fn visible_skills(&self) -> Vec<&SkillEntry> {
        self.skills
            .iter()
            .filter(|s| !s.disable_model_invocation)
            .collect()
    }
}
