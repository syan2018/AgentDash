use std::collections::HashMap;
use std::sync::Arc;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::operation::OperationProviderRef;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::{
    ActorOperationSurface, OperationAuditEvent, OperationAuditSink, OperationAuthorizationScope,
    OperationCatalog, OperationDescriptor, OperationDispatcher, OperationExecutionCore,
    OperationExecutionError, OperationExecutionRequest, OperationExecutionResult,
    OperationInvocationCommand, OperationInvocationEnvelope, OperationOriginRef,
    OperationPlacement, OperationPlacementResolver, OperationPrincipal, OperationProvider,
    OperationResultRef, OperationResultStore, OperationSurfaceResolver, ScopedOperationResult,
    result_access_matches,
};
use crate::runtime_gateway::OperationAuthorityResolver;

pub struct OperationGateway {
    core: OperationExecutionCore,
    runtime: Arc<OperationProviderRuntime>,
    result_store: Arc<dyn OperationResultStore>,
}

impl OperationGateway {
    pub fn try_new(
        authority_resolver: Arc<dyn OperationAuthorityResolver>,
        providers: impl IntoIterator<Item = Arc<dyn OperationProvider>>,
        result_store: Arc<dyn OperationResultStore>,
        audit_sink: Arc<dyn OperationAuditSink>,
    ) -> Result<Self, OperationExecutionError> {
        let runtime = Arc::new(OperationProviderRuntime::try_new(
            authority_resolver,
            providers,
        )?);
        let core = OperationExecutionCore::new(
            runtime.clone(),
            runtime.clone(),
            runtime.clone(),
            result_store.clone(),
            audit_sink,
        );
        Ok(Self {
            core,
            runtime,
            result_store,
        })
    }

    pub async fn surface(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<ActorOperationSurface, OperationExecutionError> {
        self.runtime
            .resolve_surface(principal, scope, origin, cancel)
            .await
    }

    /// Resolve the current surface from stable server-owned context references.
    /// Authority revisions never cross the host boundary.
    pub async fn surface_current(
        &self,
        principal: &OperationPrincipal,
        scope_ref: &super::OperationScopeRef,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<ActorOperationSurface, OperationExecutionError> {
        self.runtime
            .resolve_surface(
                principal,
                &OperationAuthorizationScope {
                    scope_ref: scope_ref.clone(),
                    authority_revision: String::new(),
                },
                origin,
                cancel,
            )
            .await
    }

    pub async fn invoke(
        &self,
        command: OperationInvocationCommand,
        cancel: CancellationToken,
    ) -> Result<OperationExecutionResult, OperationExecutionError> {
        command
            .principal
            .principal_ref()
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        command
            .scope_ref
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        command
            .origin
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        let unresolved_scope = OperationAuthorizationScope {
            scope_ref: command.scope_ref.clone(),
            authority_revision: String::new(),
        };
        let surface = self
            .runtime
            .resolve_surface(
                &command.principal,
                &unresolved_scope,
                &command.origin,
                cancel.clone(),
            )
            .await?;
        let request = OperationExecutionRequest {
            operation_ref: command.operation_ref,
            input: command.input,
            principal: command.principal,
            scope: OperationAuthorizationScope {
                scope_ref: command.scope_ref,
                authority_revision: surface.authority_revision,
            },
            origin: command.origin,
            trace: command.trace,
            deadline: command.deadline,
            idempotency_key: command.idempotency_key,
            attachment_ref: command.attachment_ref,
        };
        self.core.execute(request, cancel).await
    }

    pub async fn resolve_result(
        &self,
        result_ref: &OperationResultRef,
        principal: &OperationPrincipal,
        scope_ref: &super::OperationScopeRef,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<Option<Value>, OperationExecutionError> {
        let unresolved_scope = OperationAuthorizationScope {
            scope_ref: scope_ref.clone(),
            authority_revision: String::new(),
        };
        let surface = self
            .runtime
            .resolve_surface(principal, &unresolved_scope, origin, cancel)
            .await?;
        let resolved_scope = OperationAuthorizationScope {
            scope_ref: scope_ref.clone(),
            authority_revision: surface.authority_revision,
        };
        self.result_store
            .get_authorized(
                result_ref,
                principal,
                &resolved_scope,
                &surface.granted_capabilities,
            )
            .await
    }
}

struct OperationProviderRuntime {
    authority_resolver: Arc<dyn OperationAuthorityResolver>,
    providers: HashMap<OperationProviderRef, Arc<dyn OperationProvider>>,
}

impl OperationProviderRuntime {
    fn try_new(
        authority_resolver: Arc<dyn OperationAuthorityResolver>,
        providers: impl IntoIterator<Item = Arc<dyn OperationProvider>>,
    ) -> Result<Self, OperationExecutionError> {
        let mut by_ref = HashMap::new();
        for provider in providers {
            let provider_ref = provider.provider_ref().clone();
            if by_ref.insert(provider_ref.clone(), provider).is_some() {
                return Err(OperationExecutionError::invalid_request(format!(
                    "Operation provider 重复注册: {}:{}",
                    provider_ref.namespace, provider_ref.provider_key
                )));
            }
        }
        Ok(Self {
            authority_resolver,
            providers: by_ref,
        })
    }

    fn provider_for(
        &self,
        descriptor: &OperationDescriptor,
    ) -> Result<Arc<dyn OperationProvider>, OperationExecutionError> {
        self.providers
            .get(&descriptor.operation_ref.provider)
            .cloned()
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: descriptor.operation_ref.clone(),
            })
    }
}

#[async_trait]
impl OperationSurfaceResolver for OperationProviderRuntime {
    async fn resolve_surface(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<ActorOperationSurface, OperationExecutionError> {
        let grant = self
            .authority_resolver
            .resolve(principal, scope, origin, cancel.clone())
            .await?;
        let mut descriptors = Vec::new();
        for provider in self.providers.values() {
            descriptors.extend(
                provider
                    .discover(principal, scope, origin, cancel.clone())
                    .await?,
            );
        }
        Ok(ActorOperationSurface {
            authority_revision: grant.authority_revision,
            granted_capabilities: grant.capabilities,
            catalog: OperationCatalog::try_new(descriptors)?,
        })
    }
}

#[async_trait]
impl OperationPlacementResolver for OperationProviderRuntime {
    async fn resolve_placement(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError> {
        self.provider_for(descriptor)?
            .resolve_placement(descriptor, principal, scope, cancel)
            .await
    }
}

#[async_trait]
impl OperationDispatcher for OperationProviderRuntime {
    async fn dispatch(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        self.provider_for(descriptor)?
            .invoke(descriptor, envelope, cancel)
            .await
    }
}

#[derive(Default)]
pub struct InMemoryOperationResultStore {
    results: RwLock<HashMap<uuid::Uuid, ScopedOperationResult>>,
}

#[async_trait]
impl OperationResultStore for InMemoryOperationResultStore {
    async fn put(&self, result: ScopedOperationResult) -> Result<(), OperationExecutionError> {
        let mut results = self.results.write().await;
        prune_expired(&mut results);
        results.insert(result.result_ref.result_id, result);
        Ok(())
    }

    async fn get_authorized(
        &self,
        result_ref: &OperationResultRef,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        current_capabilities: &std::collections::BTreeSet<String>,
    ) -> Result<Option<Value>, OperationExecutionError> {
        let mut results = self.results.write().await;
        prune_expired(&mut results);
        Ok(results.get(&result_ref.result_id).and_then(|result| {
            result_access_matches(&result.access, principal, scope, current_capabilities)
                .then(|| result.value.clone())
        }))
    }
}

fn prune_expired(results: &mut HashMap<uuid::Uuid, ScopedOperationResult>) {
    let now = chrono::Utc::now();
    results.retain(|_, result| result.access.expires_at > now);
}

pub struct TracingOperationAuditSink;

#[async_trait]
impl OperationAuditSink for TracingOperationAuditSink {
    async fn record(&self, event: OperationAuditEvent) {
        diag!(
            Info,
            Subsystem::Infra,
            operation_namespace = event.operation_ref.provider.namespace.as_str(),
            operation_provider = event.operation_ref.provider.provider_key.as_str(),
            operation_key = event.operation_ref.operation_key.as_str(),
            operation_version = event.operation_ref.contract_version,
            trace_id = event.trace.trace_id.as_str(),
            invocation_id = event.trace.invocation_id.as_str(),
            stage = format!("{:?}", event.stage),
            outcome_code = event.outcome_code.as_deref().unwrap_or(""),
            "Operation execution audit"
        );
    }
}
