use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::interaction::CommandActorPolicy;
use agentdash_domain::operation::{
    OperationEffect, OperationProviderRef, OperationRef, OperationReplayPolicy,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    DynamicOperationProvider, OperationActorKind, OperationAuthorizationScope, OperationDescriptor,
    OperationDispatch, OperationExecutionError, OperationExecutionPolicy,
    OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
    OperationProvenance, OperationReadiness,
};

pub const INTERACTION_OPERATION_NAMESPACE: &str = "interaction";

#[derive(Debug, Clone)]
pub struct InteractionCommandOperation {
    pub definition_id: Uuid,
    pub definition_revision_id: Uuid,
    pub title: String,
    pub command_key: String,
    pub actor_policy: CommandActorPolicy,
    pub payload_schema: Value,
}

#[derive(Debug, Clone)]
pub struct InteractionCommandInvocation {
    pub principal: OperationPrincipal,
    pub scope: OperationAuthorizationScope,
    pub definition_id: Uuid,
    pub definition_revision_id: Uuid,
    pub command_key: String,
    pub input: Value,
}

#[async_trait]
pub trait InteractionOperationAccess: Send + Sync {
    async fn discover_commands(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<Vec<InteractionCommandOperation>, OperationExecutionError>;

    async fn invoke_command(
        &self,
        invocation: InteractionCommandInvocation,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError>;
}

pub struct InteractionOperationProvider {
    access: Arc<dyn InteractionOperationAccess>,
}

impl InteractionOperationProvider {
    pub fn new(access: Arc<dyn InteractionOperationAccess>) -> Self {
        Self { access }
    }
}

#[async_trait]
impl DynamicOperationProvider for InteractionOperationProvider {
    fn owns_provider(&self, provider: &OperationProviderRef) -> bool {
        provider.namespace == INTERACTION_OPERATION_NAMESPACE
    }

    async fn discover(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        _: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
        self.access
            .discover_commands(principal, scope, cancel)
            .await?
            .into_iter()
            .map(descriptor_from_command)
            .collect()
    }

    async fn resolve_placement(
        &self,
        _: &OperationDescriptor,
        _: &OperationPrincipal,
        _: &OperationAuthorizationScope,
        _: &OperationOriginRef,
        _: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError> {
        Ok(OperationPlacement::Cloud)
    }

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        let (definition_id, definition_revision_id) = descriptor
            .operation_ref
            .provider
            .provider_key
            .split_once('.')
            .ok_or_else(|| {
                OperationExecutionError::invalid_request(
                    "Interaction provider key 缺少 exact revision",
                )
            })?;
        let definition_id = Uuid::parse_str(definition_id).map_err(|_| {
            OperationExecutionError::invalid_request(
                "Interaction provider key 必须包含 definition UUID",
            )
        })?;
        let definition_revision_id = Uuid::parse_str(definition_revision_id).map_err(|_| {
            OperationExecutionError::invalid_request(
                "Interaction provider key 必须包含 revision UUID",
            )
        })?;
        self.access
            .invoke_command(
                InteractionCommandInvocation {
                    principal: envelope.principal,
                    scope: envelope.scope,
                    definition_id,
                    definition_revision_id,
                    command_key: descriptor.operation_ref.operation_key.clone(),
                    input: envelope.input,
                },
                cancel,
            )
            .await
    }
}

fn descriptor_from_command(
    command: InteractionCommandOperation,
) -> Result<OperationDescriptor, OperationExecutionError> {
    let operation_ref = OperationRef::new(
        INTERACTION_OPERATION_NAMESPACE,
        format!(
            "{}.{}",
            command.definition_id, command.definition_revision_id
        ),
        command.command_key,
        1,
    )
    .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
    let mut actor_visibility = BTreeSet::from([OperationActorKind::User]);
    if command.actor_policy == CommandActorPolicy::Direct {
        actor_visibility.insert(OperationActorKind::Agent);
    }
    Ok(OperationDescriptor {
        title: format!("{} · {}", command.title, operation_ref.operation_key),
        description: Some("Submit a typed command to a canonical Interaction instance.".into()),
        input_schema: json!({
            "type":"object",
            "properties":{
                "instance_id":{"type":"string","format":"uuid"},
                "command_id":{"type":"string","format":"uuid"},
                "payload":command.payload_schema,
                "expected_state_revision":{"type":"integer","minimum":0}
            },
            "required":["instance_id","command_id","payload","expected_state_revision"],
            "additionalProperties":false
        }),
        output_schema: json!({"type":"object"}),
        effect: OperationEffect::LocalMutation,
        replay_policy: OperationReplayPolicy::Idempotent,
        required_capabilities: BTreeSet::from(["operation.invoke".to_string()]),
        actor_visibility,
        execution_policy: OperationExecutionPolicy::default(),
        readiness: OperationReadiness::Ready,
        provenance: OperationProvenance {
            source: format!(
                "interaction_definition_revision:{}",
                command.definition_revision_id
            ),
            artifact_digest: None,
        },
        dispatch: OperationDispatch {
            provider: operation_ref.provider.clone(),
            route: operation_ref.operation_key.clone(),
        },
        operation_ref,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_command_descriptor_pins_revision_and_is_agent_visible() {
        let definition_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        let descriptor = descriptor_from_command(InteractionCommandOperation {
            definition_id,
            definition_revision_id: revision_id,
            title: "Board".into(),
            command_key: "board.patch".into(),
            actor_policy: CommandActorPolicy::Direct,
            payload_schema: json!({"type":"array"}),
        })
        .expect("descriptor");

        assert_eq!(
            descriptor.operation_ref.provider.provider_key,
            format!("{definition_id}.{revision_id}")
        );
        assert!(
            descriptor
                .actor_visibility
                .contains(&OperationActorKind::Agent)
        );
        assert_eq!(descriptor.replay_policy, OperationReplayPolicy::Idempotent);
    }

    #[test]
    fn human_only_command_is_not_agent_visible() {
        let descriptor = descriptor_from_command(InteractionCommandOperation {
            definition_id: Uuid::new_v4(),
            definition_revision_id: Uuid::new_v4(),
            title: "Approval".into(),
            command_key: "approval.confirm".into(),
            actor_policy: CommandActorPolicy::HumanOnly,
            payload_schema: json!({"type":"object"}),
        })
        .expect("descriptor");
        assert!(
            !descriptor
                .actor_visibility
                .contains(&OperationActorKind::Agent)
        );
    }
}
