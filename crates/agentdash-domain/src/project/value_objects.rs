use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_container::{ContextContainerDefinition, MountDerivationPolicy};

/// 项目级配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// 默认 Agent 类型（如 "claude-code"）
    pub default_agent_type: Option<String>,
    /// 默认 Workspace ID
    pub default_workspace_id: Option<Uuid>,
    /// Agent 预设列表
    #[serde(default)]
    pub agent_presets: Vec<AgentPreset>,
    /// 项目级上下文容器定义
    #[serde(default)]
    pub context_containers: Vec<ContextContainerDefinition>,
    /// 项目级挂载派生策略
    #[serde(default)]
    pub mount_policy: MountDerivationPolicy,
    /// 自主调度相关配置（stall 检测、turn 限制、定时唤醒等）
    #[serde(default)]
    pub scheduling: SchedulingConfig,
}

/// 自主调度与 session 安全网配置
///
/// 所有字段均为 Option，未设置时使用系统默认值。
/// 这些配置同时作为平台安全网参数（由平台强制执行）
/// 和 Agent 行为偏好（由 Project Agent 自行解释）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulingConfig {
    /// Session 无活动超时（毫秒）。超时后平台自动取消 session。
    /// 默认 300_000 (5 分钟)。设为 0 则禁用 stall 检测。
    pub stall_timeout_ms: Option<u64>,
    /// 单 Task 最大 turn 数。超限后平台拒绝继续执行（防失控）。
    pub max_turns_per_task: Option<u32>,
    /// Project Agent 被定时唤醒的间隔（毫秒）。仅对配置了自主调度的 Project 生效。
    pub poll_interval_ms: Option<u64>,
}

/// Agent 预设配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPreset {
    pub name: String,
    pub agent_type: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectVisibility {
    Private,
    TemplateVisible,
}

impl ProjectVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::TemplateVisible => "template_visible",
        }
    }
}

impl std::fmt::Display for ProjectVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRole {
    Owner,
    Editor,
    Viewer,
}

impl ProjectRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}

impl std::fmt::Display for ProjectRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSubjectType {
    User,
    Group,
}

impl ProjectSubjectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
        }
    }
}

impl std::fmt::Display for ProjectSubjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSubjectGrant {
    pub project_id: Uuid,
    pub subject_type: ProjectSubjectType,
    pub subject_id: String,
    pub role: ProjectRole,
    pub granted_by_user_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl ProjectSubjectGrant {
    pub fn new(
        project_id: Uuid,
        subject_type: ProjectSubjectType,
        subject_id: String,
        role: ProjectRole,
        granted_by_user_id: String,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            project_id,
            subject_type,
            subject_id,
            role,
            granted_by_user_id,
            created_at: now,
            updated_at: now,
        }
    }
}
