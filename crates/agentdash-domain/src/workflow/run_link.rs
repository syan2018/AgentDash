use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::error::DomainError;

/// LifecycleRun 与业务对象的显式关联。
///
/// 替代通过 `LifecycleRun.session_id -> SessionBinding -> Story` 的隐式反查路径，
/// 让 LifecycleRun 与 Story / RoutineExecution / Task / Project 等对象的关系
/// 通过 role 语义显式表达。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleRunLink {
    pub id: Uuid,
    pub run_id: Uuid,
    pub subject_kind: RunLinkSubjectKind,
    pub subject_id: Uuid,
    pub role: RunLinkRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl LifecycleRunLink {
    pub fn new(
        run_id: Uuid,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
        role: RunLinkRole,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            subject_kind,
            subject_id,
            role,
            metadata: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RunLinkSubjectKind {
    Story,
    Project,
    RoutineExecution,
    Task,
    LifecycleRun,
    External,
}

impl RunLinkSubjectKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Story => "story",
            Self::Project => "project",
            Self::RoutineExecution => "routine_execution",
            Self::Task => "task",
            Self::LifecycleRun => "lifecycle_run",
            Self::External => "external",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "story" => Some(Self::Story),
            "project" => Some(Self::Project),
            "routine_execution" => Some(Self::RoutineExecution),
            "task" => Some(Self::Task),
            "lifecycle_run" => Some(Self::LifecycleRun),
            "external" => Some(Self::External),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RunLinkRole {
    /// Run 的触发来源（如 RoutineExecution, manual command, 另一个 run）
    Source,
    /// Run 正在处理的对象（如 Story, Project, 外部实体）
    Subject,
    /// Run 输出投影到哪里（如 Story view, Task view）
    ProjectionTarget,
    /// Run 内 actor 可申请管理权限的 scope
    ControlScope,
    /// 父 run / activity lineage
    SpawnedBy,
}

impl RunLinkRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Subject => "subject",
            Self::ProjectionTarget => "projection_target",
            Self::ControlScope => "control_scope",
            Self::SpawnedBy => "spawned_by",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "source" => Some(Self::Source),
            "subject" => Some(Self::Subject),
            "projection_target" => Some(Self::ProjectionTarget),
            "control_scope" => Some(Self::ControlScope),
            "spawned_by" => Some(Self::SpawnedBy),
            _ => None,
        }
    }
}

// ─── Repository ──────────────────────────────────────────

#[async_trait::async_trait]
pub trait LifecycleRunLinkRepository: Send + Sync {
    async fn create(&self, link: &LifecycleRunLink) -> Result<(), DomainError>;

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleRunLink>, DomainError>;

    async fn list_by_subject(
        &self,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
    ) -> Result<Vec<LifecycleRunLink>, DomainError>;

    async fn list_by_subject_and_role(
        &self,
        subject_kind: RunLinkSubjectKind,
        subject_id: Uuid,
        role: RunLinkRole,
    ) -> Result<Vec<LifecycleRunLink>, DomainError>;

    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;

    async fn delete_by_run(&self, run_id: Uuid) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_link_new_sets_fields() {
        let run_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let link = LifecycleRunLink::new(
            run_id,
            RunLinkSubjectKind::Story,
            story_id,
            RunLinkRole::Subject,
        );

        assert_eq!(link.run_id, run_id);
        assert_eq!(link.subject_kind, RunLinkSubjectKind::Story);
        assert_eq!(link.subject_id, story_id);
        assert_eq!(link.role, RunLinkRole::Subject);
        assert!(link.metadata.is_none());
    }

    #[test]
    fn run_link_subject_kind_roundtrip() {
        for kind in [
            RunLinkSubjectKind::Story,
            RunLinkSubjectKind::Project,
            RunLinkSubjectKind::RoutineExecution,
            RunLinkSubjectKind::Task,
            RunLinkSubjectKind::LifecycleRun,
            RunLinkSubjectKind::External,
        ] {
            assert_eq!(RunLinkSubjectKind::from_str(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn run_link_role_roundtrip() {
        for role in [
            RunLinkRole::Source,
            RunLinkRole::Subject,
            RunLinkRole::ProjectionTarget,
            RunLinkRole::ControlScope,
            RunLinkRole::SpawnedBy,
        ] {
            assert_eq!(RunLinkRole::from_str(role.as_str()), Some(role));
        }
    }

    #[test]
    fn run_link_serde_roundtrip() {
        let link = LifecycleRunLink::new(
            Uuid::new_v4(),
            RunLinkSubjectKind::RoutineExecution,
            Uuid::new_v4(),
            RunLinkRole::Source,
        )
        .with_metadata(serde_json::json!({"trigger": "cron"}));

        let json = serde_json::to_string(&link).unwrap();
        let back: LifecycleRunLink = serde_json::from_str(&json).unwrap();
        assert_eq!(back, link);
    }
}
