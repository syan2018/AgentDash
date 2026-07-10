use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::operation::OperationRef;

use super::{
    CommandActorPolicy, InteractionError, InteractionOwner, InteractionResult,
    PlatformCommandHandler,
};

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
#[serde(deny_unknown_fields)]
pub struct InteractionCommandRequest {
    pub instance_id: Uuid,
    pub command_id: Uuid,
    pub command_key: String,
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
                command_type: self.command_key.clone(),
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedInteractionCommand {
    pub request: InteractionCommandRequest,
    pub handler: PlatformCommandHandler,
    pub actor_policy: CommandActorPolicy,
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
    pub fn new(
        allowed_paths: Vec<String>,
        max_operations: usize,
        max_state_bytes: usize,
    ) -> InteractionResult<Self> {
        if max_operations == 0 || max_state_bytes == 0 {
            return Err(InteractionError::InvalidField {
                field: "state_patch_v1.limits",
                reason: "limits 必须大于 0",
            });
        }
        let mut allowed_paths = allowed_paths;
        for path in &allowed_paths {
            validate_json_pointer(path)?;
        }
        allowed_paths.sort();
        allowed_paths.dedup();
        if allowed_paths.is_empty() {
            return Err(InteractionError::InvalidField {
                field: "state_patch_v1.allowed_paths",
                reason: "至少声明一个合法 JSON Pointer",
            });
        }
        Ok(Self {
            allowed_paths,
            max_operations,
            max_state_bytes,
        })
    }

    pub fn validate_contract(&self) -> InteractionResult<()> {
        let canonical = Self::new(
            self.allowed_paths.clone(),
            self.max_operations,
            self.max_state_bytes,
        )?;
        if canonical.allowed_paths != self.allowed_paths {
            return Err(InteractionError::InvalidField {
                field: "state_patch_v1.allowed_paths",
                reason: "allowed paths 必须排序且去重",
            });
        }
        Ok(())
    }

    pub fn validate(&self, operations: &[StatePatchOperation]) -> InteractionResult<()> {
        self.validate_contract()?;
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
            if operation.op == StatePatchOperationKind::Remove && operation.value.is_some() {
                return Err(InteractionError::UnexpectedPatchValue {
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

impl OperationEffectIntent {
    pub fn validate(&self) -> InteractionResult<()> {
        self.operation_ref
            .validate()
            .map_err(|error| InteractionError::InvalidOperationRef {
                reason: error.to_string(),
            })?;
        if self.idempotency_key.trim().is_empty()
            || self
                .admission_audit
                .capability_revision_ref
                .trim()
                .is_empty()
        {
            return Err(InteractionError::InvalidField {
                field: "operation_effect_intent.identity",
                reason: "idempotency 与 capability revision 引用不能为空",
            });
        }
        self.admission_audit.scope.validate()
    }

    pub fn claim(
        &mut self,
        claim_token: Uuid,
        at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> InteractionResult<()> {
        let reclaimable = self.status == OperationEffectStatus::Claimed
            && self.claim_expires_at.is_some_and(|expires| expires <= at);
        if !matches!(
            self.status,
            OperationEffectStatus::Pending | OperationEffectStatus::RetryScheduled
        ) && !reclaimable
        {
            return Err(InteractionError::InvalidStatusTransition {
                from: effect_status_str(self.status),
                to: "claimed",
            });
        }
        self.status = OperationEffectStatus::Claimed;
        self.claim_token = Some(claim_token);
        self.claimed_at = Some(at);
        self.claim_expires_at = Some(expires_at);
        self.attempt = self.attempt.saturating_add(1);
        Ok(())
    }

    pub fn mark_succeeded(
        &mut self,
        claim_token: Uuid,
        at: DateTime<Utc>,
    ) -> InteractionResult<()> {
        self.require_claim(claim_token, "succeeded")?;
        self.status = OperationEffectStatus::Succeeded;
        self.completed_at = Some(at);
        self.claim_token = None;
        self.claim_expires_at = None;
        Ok(())
    }

    pub fn schedule_retry(
        &mut self,
        claim_token: Uuid,
        next_attempt_at: DateTime<Utc>,
        failure_code: impl Into<String>,
    ) -> InteractionResult<()> {
        self.require_claim(claim_token, "retry_scheduled")?;
        self.status = OperationEffectStatus::RetryScheduled;
        self.next_attempt_at = next_attempt_at;
        self.claim_token = None;
        self.claimed_at = None;
        self.claim_expires_at = None;
        self.last_failure_code = Some(failure_code.into());
        Ok(())
    }

    pub fn mark_terminal_failed(
        &mut self,
        claim_token: Uuid,
        at: DateTime<Utc>,
        failure_code: impl Into<String>,
    ) -> InteractionResult<()> {
        self.require_claim(claim_token, "terminal_failed")?;
        self.status = OperationEffectStatus::TerminalFailed;
        self.completed_at = Some(at);
        self.claim_token = None;
        self.claim_expires_at = None;
        self.last_failure_code = Some(failure_code.into());
        Ok(())
    }

    fn require_claim(&self, claim_token: Uuid, target: &'static str) -> InteractionResult<()> {
        if self.status != OperationEffectStatus::Claimed || self.claim_token != Some(claim_token) {
            return Err(InteractionError::InvalidStatusTransition {
                from: effect_status_str(self.status),
                to: target,
            });
        }
        Ok(())
    }
}

fn effect_status_str(status: OperationEffectStatus) -> &'static str {
    match status {
        OperationEffectStatus::Pending => "pending",
        OperationEffectStatus::Claimed => "claimed",
        OperationEffectStatus::Succeeded => "succeeded",
        OperationEffectStatus::RetryScheduled => "retry_scheduled",
        OperationEffectStatus::TerminalFailed => "terminal_failed",
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
    pub command_key: String,
    pub handler: PlatformCommandHandler,
    pub actor: InteractionActor,
    pub payload: Value,
    pub resulting_state_revision: u64,
    pub created_at: DateTime<Utc>,
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
    RetryScheduled,
    TerminalFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationEffectPrincipalRef {
    Human {
        user_id: String,
    },
    Agent {
        agent_id: Uuid,
        run_id: Option<Uuid>,
    },
    Workflow {
        run_id: Uuid,
    },
}

/// 仅用于审计首次 admission；replay 必须重新鉴权，不能把该快照当作授权。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEffectAdmissionAudit {
    pub principal: OperationEffectPrincipalRef,
    pub scope: InteractionOwner,
    pub capability_revision_ref: String,
    pub admitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationEffectIntent {
    pub effect_id: Uuid,
    pub instance_id: Uuid,
    pub source_event_id: Uuid,
    pub operation_ref: OperationRef,
    pub validated_input: Value,
    pub admission_audit: OperationEffectAdmissionAudit,
    pub idempotency_key: String,
    pub safety: OperationEffectSafety,
    pub status: OperationEffectStatus,
    pub attempt: u32,
    pub next_attempt_at: DateTime<Utc>,
    pub claim_token: Option<Uuid>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_failure_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn human_only_rejects_agent() {
        let request = InteractionCommandRequest {
            instance_id: Uuid::new_v4(),
            command_id: Uuid::new_v4(),
            command_key: "approve".into(),
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
        let contract = StatePatchV1Contract::new(vec!["/form".into()], 1, 32).expect("contract");
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

    #[test]
    fn remove_rejects_value_and_allowlist_must_be_canonical() {
        let contract = StatePatchV1Contract::new(vec!["/form".into()], 1, 32).expect("contract");
        assert!(matches!(
            contract.validate(&[StatePatchOperation {
                op: StatePatchOperationKind::Remove,
                path: "/form/name".into(),
                value: Some(Value::Null)
            }]),
            Err(InteractionError::UnexpectedPatchValue { .. })
        ));
        assert!(StatePatchV1Contract::new(vec!["invalid".into()], 1, 32).is_err());
    }

    #[test]
    fn external_command_shape_does_not_accept_handler() {
        let value = serde_json::json!({
            "instance_id": Uuid::new_v4(), "command_id": Uuid::new_v4(), "command_key": "set_value",
            "handler": { "handler": "instance_close_v1" }, "payload": {}, "expected_state_revision": 0,
            "actor": { "kind": "human", "user_id": "u" }, "origin": "user_workshop", "attachment_id": null
        });
        assert!(serde_json::from_value::<InteractionCommandRequest>(value).is_err());
    }

    #[test]
    fn expired_effect_claim_is_recoverable_and_terminal_clears_lease() {
        let now = Utc::now();
        let mut effect = OperationEffectIntent {
            effect_id: Uuid::new_v4(),
            instance_id: Uuid::new_v4(),
            source_event_id: Uuid::new_v4(),
            operation_ref: crate::operation::OperationRef::new("host", "core", "notify", 1)
                .expect("operation"),
            validated_input: serde_json::json!({}),
            admission_audit: OperationEffectAdmissionAudit {
                principal: OperationEffectPrincipalRef::Human {
                    user_id: "u".into(),
                },
                scope: InteractionOwner::User("u".into()),
                capability_revision_ref: "cap:1".into(),
                admitted_at: now,
            },
            idempotency_key: "effect-1".into(),
            safety: OperationEffectSafety::Idempotent,
            status: OperationEffectStatus::Pending,
            attempt: 0,
            next_attempt_at: now,
            claim_token: None,
            claimed_at: None,
            claim_expires_at: None,
            completed_at: None,
            last_failure_code: None,
        };
        let first = Uuid::new_v4();
        effect
            .claim(first, now, now + chrono::Duration::seconds(5))
            .expect("first claim");
        assert!(
            effect
                .claim(
                    Uuid::new_v4(),
                    now + chrono::Duration::seconds(1),
                    now + chrono::Duration::seconds(6)
                )
                .is_err()
        );
        let recovered = Uuid::new_v4();
        effect
            .claim(
                recovered,
                now + chrono::Duration::seconds(5),
                now + chrono::Duration::seconds(10),
            )
            .expect("expired claim is recoverable");
        effect
            .mark_terminal_failed(recovered, now + chrono::Duration::seconds(6), "terminal")
            .expect("terminal");
        assert!(effect.claim_token.is_none());
        assert!(effect.claim_expires_at.is_none());
    }
}
