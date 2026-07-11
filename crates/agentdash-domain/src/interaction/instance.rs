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
    AgentRun { run_id: Uuid, agent_id: Uuid },
    UserWorkshop { user_id: String },
    WorkflowRun { run_id: Uuid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionAttachmentRole {
    Editor,
    Observer,
    Renderer,
    Automation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentCapabilityProjection {
    pub can_read_state: bool,
    pub can_submit_commands: bool,
    pub can_bind_resources: bool,
    pub can_render: bool,
}

impl AttachmentCapabilityProjection {
    pub fn for_role(role: InteractionAttachmentRole) -> Self {
        match role {
            InteractionAttachmentRole::Editor => Self {
                can_read_state: true,
                can_submit_commands: true,
                can_bind_resources: true,
                can_render: true,
            },
            InteractionAttachmentRole::Observer => Self {
                can_read_state: true,
                can_submit_commands: false,
                can_bind_resources: false,
                can_render: false,
            },
            InteractionAttachmentRole::Renderer => Self {
                can_read_state: true,
                can_submit_commands: true,
                can_bind_resources: false,
                can_render: true,
            },
            InteractionAttachmentRole::Automation => Self {
                can_read_state: true,
                can_submit_commands: true,
                can_bind_resources: false,
                can_render: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionAttachment {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub subject: AttachmentSubject,
    pub role: InteractionAttachmentRole,
    pub capabilities: AttachmentCapabilityProjection,
    pub created_at: DateTime<Utc>,
    pub detached_at: Option<DateTime<Utc>>,
}

impl InteractionAttachment {
    pub fn validate(&self) -> InteractionResult<()> {
        if self.id.is_nil() || self.instance_id.is_nil() {
            return Err(InteractionError::InvalidField {
                field: "interaction_attachment.identity",
                reason: "attachment/instance id 不能为空",
            });
        }
        if self.capabilities != AttachmentCapabilityProjection::for_role(self.role) {
            return Err(InteractionError::InvalidField {
                field: "interaction_attachment.capabilities",
                reason: "capability projection 必须由 attachment role 派生",
            });
        }
        match &self.subject {
            AttachmentSubject::AgentRun { run_id, agent_id }
                if run_id.is_nil() || agent_id.is_nil() =>
            {
                Err(InteractionError::InvalidField {
                    field: "interaction_attachment.subject",
                    reason: "AgentRun subject run_id/agent_id 不能为空",
                })
            }
            AttachmentSubject::WorkflowRun { run_id } if run_id.is_nil() => {
                Err(InteractionError::InvalidField {
                    field: "interaction_attachment.subject",
                    reason: "run id 不能为空",
                })
            }
            AttachmentSubject::UserWorkshop { user_id } if user_id.trim().is_empty() => {
                Err(InteractionError::InvalidField {
                    field: "interaction_attachment.subject",
                    reason: "user id 不能为空",
                })
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeBindingTarget {
    Resource {
        resource_ref: String,
        version_ref: String,
    },
    Artifact {
        artifact_ref: String,
        digest: String,
    },
    Provider {
        provider_ref: String,
        contract_version: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBindingAuthorizationRef {
    pub grant_ref: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionRuntimeBinding {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub attachment_id: Option<Uuid>,
    pub slot_key: String,
    pub target: RuntimeBindingTarget,
    pub authorization: RuntimeBindingAuthorizationRef,
    pub created_at: DateTime<Utc>,
}

impl InteractionRuntimeBinding {
    pub fn validate(&self) -> InteractionResult<()> {
        if self.instance_id.is_nil()
            || self.slot_key.trim().is_empty()
            || self.authorization.grant_ref.trim().is_empty()
            || self.authorization.revision == 0
        {
            return Err(InteractionError::InvalidField {
                field: "interaction_runtime_binding",
                reason: "identity、slot 与 versioned authorization ref 必须有效",
            });
        }
        match &self.target {
            RuntimeBindingTarget::Resource {
                resource_ref,
                version_ref,
            } if resource_ref.trim().is_empty() || version_ref.trim().is_empty() => {
                Err(InteractionError::InvalidField {
                    field: "runtime_binding.resource",
                    reason: "resource/version ref 不能为空",
                })
            }
            RuntimeBindingTarget::Artifact {
                artifact_ref,
                digest,
            } if artifact_ref.trim().is_empty() || !valid_sha256(digest) => {
                Err(InteractionError::InvalidField {
                    field: "runtime_binding.artifact",
                    reason: "artifact ref 与 sha256 digest 必须有效",
                })
            }
            RuntimeBindingTarget::Provider {
                provider_ref,
                contract_version,
            } if provider_ref.trim().is_empty() || *contract_version == 0 => {
                Err(InteractionError::InvalidField {
                    field: "runtime_binding.provider",
                    reason: "provider ref 与 contract version 必须有效",
                })
            }
            _ => Ok(()),
        }
    }
}

fn valid_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.chars().all(|character| character.is_ascii_hexdigit())
    })
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

    #[test]
    fn attachment_role_has_explicit_capability_projection() {
        let projection =
            AttachmentCapabilityProjection::for_role(InteractionAttachmentRole::Observer);
        assert!(projection.can_read_state);
        assert!(!projection.can_submit_commands);
    }
}
