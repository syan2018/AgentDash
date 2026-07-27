use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::operation::{
    OperationEffect, OperationProviderRef, OperationRef, OperationReplayPolicy,
};
use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{
    DynamicOperationProvider, OperationActorKind, OperationAuthorizationScope, OperationDescriptor,
    OperationDispatch, OperationExecutionError, OperationExecutionPolicy,
    OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
    OperationProvenance, OperationReadiness,
};

pub const PLATFORM_TOOL_OPERATION_NAMESPACE: &str = "platform";

/// Explicit Operation exposure for one native platform tool.
///
/// Runtime tool registration alone never makes a tool composable. The composition root must
/// deliberately supply this metadata, including replay semantics and actor visibility.
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformToolOperation {
    pub provider_key: String,
    pub tool_name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub effect: OperationEffect,
    pub replay_policy: OperationReplayPolicy,
    pub required_capability: String,
    pub provenance: String,
}

#[async_trait]
pub trait PlatformToolOperationAccess: Send + Sync {
    async fn discover_tools(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<Vec<PlatformToolOperation>, OperationExecutionError>;

    async fn invoke_tool(
        &self,
        envelope: OperationInvocationEnvelope,
        tool_name: &str,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError>;
}

pub struct PlatformToolOperationProvider {
    access: Arc<dyn PlatformToolOperationAccess>,
}

impl PlatformToolOperationProvider {
    pub fn new(access: Arc<dyn PlatformToolOperationAccess>) -> Self {
        Self { access }
    }

    async fn resolve_tool(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<PlatformToolOperation, OperationExecutionError> {
        self.access
            .discover_tools(principal, scope, cancel)
            .await?
            .into_iter()
            .find(|tool| {
                tool.provider_key == descriptor.operation_ref.provider.provider_key
                    && tool.tool_name == descriptor.operation_ref.operation_key
            })
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: descriptor.operation_ref.clone(),
            })
    }
}

#[async_trait]
impl DynamicOperationProvider for PlatformToolOperationProvider {
    fn owns_provider(&self, provider: &OperationProviderRef) -> bool {
        provider.namespace == PLATFORM_TOOL_OPERATION_NAMESPACE
    }

    async fn discover(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        _: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
        self.access
            .discover_tools(principal, scope, cancel)
            .await?
            .into_iter()
            .map(descriptor_from_tool)
            .collect()
    }

    async fn resolve_placement(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        _: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError> {
        self.resolve_tool(descriptor, principal, scope, cancel)
            .await?;
        Ok(OperationPlacement::Cloud)
    }

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        self.resolve_tool(
            descriptor,
            &envelope.principal,
            &envelope.scope,
            cancel.clone(),
        )
        .await?;
        self.access
            .invoke_tool(envelope, &descriptor.operation_ref.operation_key, cancel)
            .await
    }
}

fn descriptor_from_tool(
    tool: PlatformToolOperation,
) -> Result<OperationDescriptor, OperationExecutionError> {
    let operation_ref = OperationRef::new(
        PLATFORM_TOOL_OPERATION_NAMESPACE,
        tool.provider_key,
        tool.tool_name,
        1,
    )
    .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
    Ok(OperationDescriptor {
        title: operation_ref.operation_key.clone(),
        description: Some(tool.description),
        input_schema: tool.input_schema,
        output_schema: tool.output_schema,
        effect: tool.effect,
        replay_policy: tool.replay_policy,
        required_capabilities: BTreeSet::from([tool.required_capability]),
        actor_visibility: BTreeSet::from([OperationActorKind::Agent]),
        execution_policy: OperationExecutionPolicy::default(),
        readiness: OperationReadiness::Ready,
        provenance: OperationProvenance {
            source: tool.provenance,
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
    use agentdash_domain::operation::{OperationPrincipalRef, OperationScopeRef};
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    struct FixtureAccess;

    #[async_trait]
    impl PlatformToolOperationAccess for FixtureAccess {
        async fn discover_tools(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: CancellationToken,
        ) -> Result<Vec<PlatformToolOperation>, OperationExecutionError> {
            Ok(vec![PlatformToolOperation {
                provider_key: "vfs".to_string(),
                tool_name: "fs_read".to_string(),
                description: "Read a file".to_string(),
                input_schema: json!({"type": "object"}),
                output_schema: json!(true),
                effect: OperationEffect::Read,
                replay_policy: OperationReplayPolicy::ReplaySafe,
                required_capability: "file_read".to_string(),
                provenance: "platform_tool_broker".to_string(),
            }])
        }

        async fn invoke_tool(
            &self,
            envelope: OperationInvocationEnvelope,
            tool_name: &str,
            _: CancellationToken,
        ) -> Result<Value, OperationExecutionError> {
            Ok(json!({"tool": tool_name, "input": envelope.input}))
        }
    }

    fn principal() -> OperationPrincipal {
        OperationPrincipal::server_resolved(OperationPrincipalRef::AgentRunAgent {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        })
    }

    fn scope() -> OperationAuthorizationScope {
        OperationAuthorizationScope {
            scope_ref: OperationScopeRef::Project {
                project_id: Uuid::new_v4(),
            },
            authority_revision: "rev-1".to_string(),
        }
    }

    #[tokio::test]
    async fn native_tool_requires_explicit_operation_metadata() {
        let provider = PlatformToolOperationProvider::new(Arc::new(FixtureAccess));
        let descriptors = provider
            .discover(
                &principal(),
                &scope(),
                &OperationOriginRef::AgentTool,
                CancellationToken::new(),
            )
            .await
            .expect("discover");

        assert_eq!(descriptors.len(), 1);
        let descriptor = &descriptors[0];
        assert_eq!(descriptor.operation_ref.provider.namespace, "platform");
        assert_eq!(descriptor.operation_ref.provider.provider_key, "vfs");
        assert_eq!(descriptor.operation_ref.operation_key, "fs_read");
        assert_eq!(descriptor.effect, OperationEffect::Read);
        assert_eq!(descriptor.replay_policy, OperationReplayPolicy::ReplaySafe);
        assert_eq!(
            descriptor.required_capabilities,
            BTreeSet::from(["file_read".to_string()])
        );
        assert_eq!(
            descriptor.actor_visibility,
            BTreeSet::from([OperationActorKind::Agent])
        );
    }

    #[test]
    fn runtime_registration_does_not_implicitly_expose_control_tools() {
        let exposed = [
            "mounts_list",
            "fs_read",
            "fs_glob",
            "fs_grep",
            "fs_apply_patch",
            "shell_exec",
            "task_read",
            "task_write",
        ];
        assert!(!exposed.contains(&"workspace_module_invoke"));
        assert!(!exposed.contains(&"operation_script_run"));
        assert!(!exposed.contains(&"complete_lifecycle_node"));
    }
}
