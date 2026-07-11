use std::collections::BTreeSet;

use agentdash_domain::operation::OperationProviderRef;
use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{
    OperationAuthorizationScope, OperationDescriptor, OperationExecutionError,
    OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
};

#[async_trait]
pub trait OperationProvider: Send + Sync {
    fn provider_ref(&self) -> &OperationProviderRef;

    async fn discover(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationDescriptor>, OperationExecutionError>;

    async fn resolve_placement(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError>;

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError>;
}

/// Catalog-backed provider whose concrete provider identities are resolved from current scope
/// facts (for example MCP servers and Project Extension installations).
#[async_trait]
pub trait DynamicOperationProvider: Send + Sync {
    fn owns_provider(&self, provider: &OperationProviderRef) -> bool;

    async fn discover(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationDescriptor>, OperationExecutionError>;

    async fn resolve_placement(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError>;

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationAuthorityGrant {
    pub authority_revision: String,
    pub capabilities: BTreeSet<String>,
}

#[async_trait]
pub trait OperationAuthorityResolver: Send + Sync {
    async fn resolve(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationAuthorityGrant, OperationExecutionError>;
}
