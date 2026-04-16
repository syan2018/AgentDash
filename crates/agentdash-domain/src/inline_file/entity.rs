use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 内联文件 — inline_fs 的独立存储实体
///
/// 统一存储所有「文件内容嵌套在父实体」的场景：
/// - Context Container inline files（owner_kind = "project" / "story"）
/// - Lifecycle VFS port outputs（owner_kind = "lifecycle_run"）
/// - Lifecycle record artifact content（owner_kind = "lifecycle_run"）
/// - Agent Knowledge files（owner_kind = "project_agent_link"）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineFile {
    pub id: Uuid,
    /// 归属实体类型
    pub owner_kind: InlineFileOwnerKind,
    /// 归属实体 ID
    pub owner_id: Uuid,
    /// 容器标识（对应 ContextContainerDefinition.id，或 "port_outputs" / "record_artifacts"）
    pub container_id: String,
    /// 归一化文件路径
    pub path: String,
    /// 文件内容
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

impl InlineFile {
    pub fn new(
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: impl Into<String>,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            owner_kind,
            owner_id,
            container_id: container_id.into(),
            path: path.into(),
            content: content.into(),
            updated_at: Utc::now(),
        }
    }
}

/// 内联文件归属类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InlineFileOwnerKind {
    /// Project 级 context container
    Project,
    /// Story 级 context container
    Story,
    /// Lifecycle run 的 port outputs / record artifacts
    LifecycleRun,
    /// ProjectAgentLink 级 knowledge container
    ProjectAgentLink,
}

impl InlineFileOwnerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Story => "story",
            Self::LifecycleRun => "lifecycle_run",
            Self::ProjectAgentLink => "project_agent_link",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "project" => Some(Self::Project),
            "story" => Some(Self::Story),
            "lifecycle_run" => Some(Self::LifecycleRun),
            "project_agent_link" => Some(Self::ProjectAgentLink),
            _ => None,
        }
    }
}

impl std::fmt::Display for InlineFileOwnerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
