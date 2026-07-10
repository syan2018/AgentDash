use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::operation::{
    OperationEffect, OperationProviderRef, OperationRef, OperationReplayPolicy,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    DynamicOperationProvider, OperationActorKind, OperationAuthorizationScope, OperationDescriptor,
    OperationDispatch, OperationExecutionError, OperationExecutionPolicy,
    OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
    OperationProvenance, OperationReadiness,
};

pub const MCP_OPERATION_NAMESPACE: &str = "mcp";

#[derive(Debug, Clone, PartialEq)]
pub struct OperationMcpTool {
    pub server_name: String,
    pub tool_name: String,
    pub description: String,
    pub input_schema: Value,
    pub backend_id: String,
}

#[async_trait]
pub trait OperationMcpAccess: Send + Sync {
    async fn discover_tools(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationMcpTool>, OperationExecutionError>;

    async fn invoke_tool(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError>;
}

pub struct McpOperationProvider {
    access: Arc<dyn OperationMcpAccess>,
}

impl McpOperationProvider {
    pub fn new(access: Arc<dyn OperationMcpAccess>) -> Self {
        Self { access }
    }

    async fn resolve_tool(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<OperationMcpTool, OperationExecutionError> {
        self.access
            .discover_tools(principal, scope, cancel)
            .await?
            .into_iter()
            .find(|tool| {
                tool.server_name == descriptor.operation_ref.provider.provider_key
                    && tool.tool_name == descriptor.operation_ref.operation_key
            })
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: descriptor.operation_ref.clone(),
            })
    }
}

#[async_trait]
impl DynamicOperationProvider for McpOperationProvider {
    fn owns_provider(&self, provider: &OperationProviderRef) -> bool {
        provider.namespace == MCP_OPERATION_NAMESPACE
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
        cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError> {
        let tool = self
            .resolve_tool(descriptor, principal, scope, cancel)
            .await?;
        Ok(OperationPlacement::LocalBackend {
            backend_id: tool.backend_id,
        })
    }

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        self.access
            .invoke_tool(
                &envelope.principal,
                &envelope.scope,
                &descriptor.operation_ref.provider.provider_key,
                &descriptor.operation_ref.operation_key,
                envelope.input,
                cancel,
            )
            .await
    }
}

fn descriptor_from_tool(
    tool: OperationMcpTool,
) -> Result<OperationDescriptor, OperationExecutionError> {
    let operation_ref =
        OperationRef::new(MCP_OPERATION_NAMESPACE, tool.server_name, tool.tool_name, 1)
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
    Ok(OperationDescriptor {
        title: operation_ref.operation_key.clone(),
        description: Some(tool.description),
        input_schema: tool.input_schema,
        output_schema: json!(true),
        effect: OperationEffect::ExternalSideEffect,
        replay_policy: OperationReplayPolicy::NonReplayable,
        required_capabilities: BTreeSet::from([format!(
            "mcp:{}",
            operation_ref.provider.provider_key
        )]),
        actor_visibility: BTreeSet::from([OperationActorKind::User, OperationActorKind::Agent]),
        execution_policy: OperationExecutionPolicy::default(),
        readiness: OperationReadiness::Ready,
        provenance: OperationProvenance {
            source: "agent_frame.mcp_surface".to_string(),
            artifact_digest: None,
        },
        dispatch: OperationDispatch {
            provider: operation_ref.provider.clone(),
            route: format!(
                "{}/{}",
                operation_ref.provider.provider_key, operation_ref.operation_key
            ),
        },
        operation_ref,
    })
}

#[cfg(test)]
mod tests {
    use agentdash_domain::operation::OperationScopeRef;
    use agentdash_spi::{AuthIdentity, AuthMode};
    use uuid::Uuid;

    use super::*;

    struct FixtureAccess;

    #[async_trait]
    impl OperationMcpAccess for FixtureAccess {
        async fn discover_tools(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: CancellationToken,
        ) -> Result<Vec<OperationMcpTool>, OperationExecutionError> {
            Ok(vec![OperationMcpTool {
                server_name: "code-analyzer".to_string(),
                tool_name: "find_symbols".to_string(),
                description: "Find symbols".to_string(),
                input_schema: json!({ "type": "object" }),
                backend_id: "backend-1".to_string(),
            }])
        }

        async fn invoke_tool(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            server_name: &str,
            tool_name: &str,
            arguments: Value,
            _: CancellationToken,
        ) -> Result<Value, OperationExecutionError> {
            Ok(json!({
                "server": server_name,
                "tool": tool_name,
                "arguments": arguments,
            }))
        }
    }

    fn principal() -> OperationPrincipal {
        OperationPrincipal::authenticated_user(AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: "user-1".to_string(),
            subject: "user-1".to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: None,
            extra: Value::Null,
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
    async fn mcp_tool_maps_to_exact_provider_qualified_descriptor_and_backend() {
        let provider = McpOperationProvider::new(Arc::new(FixtureAccess));
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
        assert_eq!(descriptor.operation_ref.provider.namespace, "mcp");
        assert_eq!(
            descriptor.operation_ref.provider.provider_key,
            "code-analyzer"
        );
        assert_eq!(descriptor.operation_ref.operation_key, "find_symbols");
        assert_eq!(
            descriptor.required_capabilities,
            BTreeSet::from(["mcp:code-analyzer".to_string()])
        );
        assert_eq!(
            provider
                .resolve_placement(descriptor, &principal(), &scope(), CancellationToken::new(),)
                .await
                .expect("placement"),
            OperationPlacement::LocalBackend {
                backend_id: "backend-1".to_string()
            }
        );
    }

    #[test]
    fn invalid_mcp_identity_is_rejected_instead_of_weakly_normalized() {
        let error = descriptor_from_tool(OperationMcpTool {
            server_name: "bad server".to_string(),
            tool_name: "find_symbols".to_string(),
            description: String::new(),
            input_schema: json!(true),
            backend_id: "backend-1".to_string(),
        })
        .expect_err("invalid identity");
        assert_eq!(
            error.kind(),
            super::super::OperationExecutionErrorKind::InvalidRequest
        );
    }
}
