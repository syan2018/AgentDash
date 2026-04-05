use serde::{Deserialize, Serialize};

/// Per-agent 调度配置，从 Agent.base_config 或 ProjectAgentLink.config_override 中提取。
///
/// 字段约定存储在 JSON key `scheduling` 下，例如：
/// ```json
/// { "scheduling": { "cron_schedule": "*/10 * * * *", "cron_session_mode": "reuse" } }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentSchedulingConfig {
    /// 标准 cron 表达式（5 字段：minute hour day_of_month month day_of_week）。
    /// 为空时表示不启用定时调度。
    pub cron_schedule: Option<String>,
    /// Cron 触发时的 session 模式
    #[serde(default)]
    pub cron_session_mode: CronSessionMode,
}

/// Cron 触发时的 session 生命周期模式
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CronSessionMode {
    /// 复用已有 Project Agent session（follow-up prompt 追加触发）
    #[default]
    Reuse,
    /// 每次 tick 新建独立 session
    Fresh,
}

impl AgentSchedulingConfig {
    /// 从 agent merged config JSON 中提取调度配置。
    /// 如果 JSON 中不包含 `scheduling` 字段或解析失败，返回 None。
    pub fn from_merged_config(config: &serde_json::Value) -> Option<Self> {
        let scheduling_value = config.get("scheduling")?;
        serde_json::from_value::<Self>(scheduling_value.clone()).ok()
    }

    pub fn has_cron(&self) -> bool {
        self.cron_schedule
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
    }
}
