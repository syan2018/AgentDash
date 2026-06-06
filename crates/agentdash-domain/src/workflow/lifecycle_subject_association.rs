use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// SubjectRef → whole run 或 LifecycleAgent 的关系。
///
/// anchor 只能是 run 或 LifecycleAgent；runtime node 证据来自 RuntimeSessionExecutionAnchor。
/// 当 `anchor_agent_id` 非空时，表示 agent-scoped association，该 agent 必须属于 `anchor_run_id`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleSubjectAssociation {
    pub id: Uuid,
    pub anchor_run_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_agent_id: Option<Uuid>,
    pub subject_kind: String,
    pub subject_id: Uuid,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Subject 的稳定引用结构（kind + id）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SubjectRef {
    pub kind: String,
    pub id: Uuid,
}

impl SubjectRef {
    pub fn new(kind: impl Into<String>, id: Uuid) -> Self {
        Self {
            kind: kind.into(),
            id,
        }
    }
}

impl LifecycleSubjectAssociation {
    /// 创建 whole-run association（anchor 仅为 run）。
    pub fn new_run_scoped(
        anchor_run_id: Uuid,
        subject: &SubjectRef,
        role: impl Into<String>,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            anchor_run_id,
            anchor_agent_id: None,
            subject_kind: subject.kind.clone(),
            subject_id: subject.id,
            role: role.into(),
            metadata_json: metadata,
            created_at: Utc::now(),
        }
    }

    /// 创建 agent-scoped association。
    pub fn new_agent_scoped(
        anchor_run_id: Uuid,
        anchor_agent_id: Uuid,
        subject: &SubjectRef,
        role: impl Into<String>,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            anchor_run_id,
            anchor_agent_id: Some(anchor_agent_id),
            subject_kind: subject.kind.clone(),
            subject_id: subject.id,
            role: role.into(),
            metadata_json: metadata,
            created_at: Utc::now(),
        }
    }

    pub fn is_agent_scoped(&self) -> bool {
        self.anchor_agent_id.is_some()
    }
}
