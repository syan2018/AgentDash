use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{INTERACTION_CONTRACT_V1, InteractionError, InteractionOwner, InteractionResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionInstanceStatus {
    Open,
    Closed,
}

impl InteractionInstanceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedArtifact {
    pub artifact_ref: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionRetention {
    pub retain_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionInstance {
    pub id: Uuid,
    pub owner: InteractionOwner,
    pub definition_id: Uuid,
    pub definition_revision_id: Uuid,
    pub interaction_contract_version: u16,
    pub state: Value,
    pub state_revision: u64,
    pub status: InteractionInstanceStatus,
    #[serde(default)]
    pub pinned_artifacts: Vec<PinnedArtifact>,
    pub retention: InteractionRetention,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

impl InteractionInstance {
    pub fn new_v1(
        owner: InteractionOwner,
        definition_id: Uuid,
        definition_revision_id: Uuid,
        initial_state: Value,
        retention: InteractionRetention,
    ) -> InteractionResult<Self> {
        owner.validate()?;
        if definition_id.is_nil() || definition_revision_id.is_nil() {
            return Err(InteractionError::InvalidField {
                field: "interaction_instance.definition",
                reason: "definition ids 不能为空",
            });
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            owner,
            definition_id,
            definition_revision_id,
            interaction_contract_version: INTERACTION_CONTRACT_V1,
            state: initial_state,
            state_revision: 0,
            status: InteractionInstanceStatus::Open,
            pinned_artifacts: vec![],
            retention,
            created_at: now,
            updated_at: now,
            closed_at: None,
        })
    }

    pub fn close(&mut self, at: DateTime<Utc>) -> InteractionResult<()> {
        if self.status != InteractionInstanceStatus::Open {
            return Err(InteractionError::InvalidStatusTransition {
                from: self.status.as_str(),
                to: "closed",
            });
        }
        self.status = InteractionInstanceStatus::Closed;
        self.closed_at = Some(at);
        self.updated_at = at;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttachmentSubject {
    AgentRun { run_id: Uuid },
    UserWorkshop { user_id: String },
    WorkflowRun { run_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionAttachment {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub subject: AttachmentSubject,
    pub created_at: DateTime<Utc>,
    pub detached_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reference", rename_all = "snake_case")]
pub enum RuntimeBindingTarget {
    Resource(String),
    Artifact(String),
    Provider(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionRuntimeBinding {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub attachment_id: Option<Uuid>,
    pub slot_key: String,
    pub target: RuntimeBindingTarget,
    pub authorization_ref: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_instance_does_not_change_definition_pin() {
        let revision = Uuid::new_v4();
        let mut instance = InteractionInstance::new_v1(
            InteractionOwner::User("u".into()),
            Uuid::new_v4(),
            revision,
            serde_json::json!({}),
            InteractionRetention { retain_until: None },
        )
        .expect("instance");
        instance.close(Utc::now()).expect("close");
        assert_eq!(instance.definition_revision_id, revision);
        assert_eq!(instance.status, InteractionInstanceStatus::Closed);
    }
}
