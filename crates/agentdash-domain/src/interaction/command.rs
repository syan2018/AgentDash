use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{CommandActorPolicy, InteractionError, InteractionResult, PlatformCommandHandler};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionActor {
    Human {
        user_id: String,
    },
    Agent {
        agent_id: Uuid,
        run_id: Option<Uuid>,
    },
}
impl InteractionActor {
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionCommandOrigin {
    UserWorkshop,
    AgentFrame,
    Workflow,
    ExtensionComponent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionCommandRequest {
    pub instance_id: Uuid,
    pub command_id: Uuid,
    pub command_type: String,
    pub handler: PlatformCommandHandler,
    pub payload: Value,
    pub expected_state_revision: u64,
    pub actor: InteractionActor,
    pub origin: InteractionCommandOrigin,
    pub attachment_id: Option<Uuid>,
}

impl InteractionCommandRequest {
    pub fn enforce_actor_policy(&self, policy: CommandActorPolicy) -> InteractionResult<()> {
        if self.actor.is_agent() && policy == CommandActorPolicy::HumanOnly {
            Err(InteractionError::HumanOnlyCommand {
                command_type: self.command_type.clone(),
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatePatchOperationKind {
    Add,
    Remove,
    Replace,
}
impl StatePatchOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Replace => "replace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatePatchOperation {
    pub op: StatePatchOperationKind,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatePatchV1Contract {
    pub allowed_paths: Vec<String>,
    pub max_operations: usize,
    pub max_state_bytes: usize,
}

impl StatePatchV1Contract {
    pub fn validate(&self, operations: &[StatePatchOperation]) -> InteractionResult<()> {
        if operations.len() > self.max_operations {
            return Err(InteractionError::PatchLimitExceeded {
                actual: operations.len(),
                maximum: self.max_operations,
            });
        }
        for operation in operations {
            validate_json_pointer(&operation.path)?;
            if !self
                .allowed_paths
                .iter()
                .any(|allowed| pointer_is_within(&operation.path, allowed))
            {
                return Err(InteractionError::PatchPathDenied {
                    path: operation.path.clone(),
                });
            }
            if matches!(
                operation.op,
                StatePatchOperationKind::Add | StatePatchOperationKind::Replace
            ) && operation.value.is_none()
            {
                return Err(InteractionError::MissingPatchValue {
                    operation: operation.op.as_str(),
                    path: operation.path.clone(),
                });
            }
        }
        Ok(())
    }
    pub fn validate_state_size(&self, state: &Value) -> InteractionResult<()> {
        let actual_bytes = serde_json::to_vec(state)
            .map_err(|e| InteractionError::Serialization {
                context: "interaction_state",
                message: e.to_string(),
            })?
            .len();
        if actual_bytes > self.max_state_bytes {
            Err(InteractionError::StateSizeExceeded {
                actual_bytes,
                maximum_bytes: self.max_state_bytes,
            })
        } else {
            Ok(())
        }
    }
}

fn validate_json_pointer(path: &str) -> InteractionResult<()> {
    if path.is_empty()
        || !path.starts_with('/')
        || path.split('/').skip(1).any(|segment| {
            let b = segment.as_bytes();
            (0..b.len())
                .any(|i| b[i] == b'~' && b.get(i + 1) != Some(&b'0') && b.get(i + 1) != Some(&b'1'))
        })
    {
        Err(InteractionError::InvalidField {
            field: "state_patch.path",
            reason: "必须是合法且非根 JSON Pointer",
        })
    } else {
        Ok(())
    }
}
fn pointer_is_within(path: &str, allowed: &str) -> bool {
    path == allowed
        || path
            .strip_prefix(allowed)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionEvent {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub sequence: u64,
    pub command_id: Uuid,
    pub command_type: String,
    pub actor: InteractionActor,
    pub payload: Value,
    pub resulting_state_revision: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalOperationRef {
    pub provider: String,
    pub operation: String,
    pub version: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationEffectSafety {
    ReplaySafe,
    Idempotent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationEffectStatus {
    Pending,
    Claimed,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationEffectIntent {
    pub effect_id: Uuid,
    pub instance_id: Uuid,
    pub source_event_id: Uuid,
    pub operation_ref: CanonicalOperationRef,
    pub validated_input: Value,
    pub principal_scope_snapshot: Value,
    pub idempotency_key: String,
    pub safety: OperationEffectSafety,
    pub status: OperationEffectStatus,
    pub attempt: u32,
    pub next_attempt_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn human_only_rejects_agent() {
        let request = InteractionCommandRequest {
            instance_id: Uuid::new_v4(),
            command_id: Uuid::new_v4(),
            command_type: "approve".into(),
            handler: PlatformCommandHandler::StatePatchV1,
            payload: Value::Null,
            expected_state_revision: 0,
            actor: InteractionActor::Agent {
                agent_id: Uuid::new_v4(),
                run_id: None,
            },
            origin: InteractionCommandOrigin::AgentFrame,
            attachment_id: None,
        };
        assert!(matches!(
            request.enforce_actor_policy(CommandActorPolicy::HumanOnly),
            Err(InteractionError::HumanOnlyCommand { .. })
        ));
    }
    #[test]
    fn patch_contract_enforces_boundaries() {
        let contract = StatePatchV1Contract {
            allowed_paths: vec!["/form".into()],
            max_operations: 1,
            max_state_bytes: 32,
        };
        contract
            .validate(&[StatePatchOperation {
                op: StatePatchOperationKind::Replace,
                path: "/form/name".into(),
                value: Some(Value::String("A".into())),
            }])
            .expect("allowed");
        assert!(matches!(
            contract.validate(&[StatePatchOperation {
                op: StatePatchOperationKind::Remove,
                path: "/secret".into(),
                value: None
            }]),
            Err(InteractionError::PatchPathDenied { .. })
        ));
    }
}
