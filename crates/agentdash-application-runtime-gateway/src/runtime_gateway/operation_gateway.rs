use std::collections::HashMap;
use std::sync::Arc;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::operation::OperationProviderRef;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::{
    ActorOperationSurface, DynamicOperationProvider, OperationAuditEvent, OperationAuditSink,
    OperationAuthorizationScope, OperationCatalog, OperationDescriptor, OperationDispatcher,
    OperationExecutionCore, OperationExecutionError, OperationExecutionRequest,
    OperationExecutionResult, OperationInvocationCommand, OperationInvocationEnvelope,
    OperationOriginRef, OperationPlacement, OperationPlacementResolver, OperationPrincipal,
    OperationProvider, OperationResultRef, OperationResultStore, OperationSurfaceResolver,
    ScopedOperationResult, result_access_matches,
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
        dynamic_providers: impl IntoIterator<Item = Arc<dyn DynamicOperationProvider>>,
        result_store: Arc<dyn OperationResultStore>,
        audit_sink: Arc<dyn OperationAuditSink>,
    ) -> Result<Self, OperationExecutionError> {
        let runtime = Arc::new(OperationProviderRuntime::try_new(
            authority_resolver,
            providers,
            dynamic_providers,
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
    dynamic_providers: Vec<Arc<dyn DynamicOperationProvider>>,
}

impl OperationProviderRuntime {
    fn try_new(
        authority_resolver: Arc<dyn OperationAuthorityResolver>,
        providers: impl IntoIterator<Item = Arc<dyn OperationProvider>>,
        dynamic_providers: impl IntoIterator<Item = Arc<dyn DynamicOperationProvider>>,
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
            dynamic_providers: dynamic_providers.into_iter().collect(),
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

    fn dynamic_provider_for(
        &self,
        provider_ref: &OperationProviderRef,
    ) -> Result<Option<Arc<dyn DynamicOperationProvider>>, OperationExecutionError> {
        let mut matches = self
            .dynamic_providers
            .iter()
            .filter(|provider| provider.owns_provider(provider_ref));
        let first = matches.next().cloned();
        if matches.next().is_some() {
            return Err(OperationExecutionError::invalid_request(format!(
                "Operation provider ownership 重复: {}:{}",
                provider_ref.namespace, provider_ref.provider_key
            )));
        }
        Ok(first)
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
        for provider in &self.dynamic_providers {
            match provider
                .discover(principal, scope, origin, cancel.clone())
                .await
            {
                Ok(discovered) => match OperationCatalog::try_new(discovered.clone()) {
                    Ok(_) => descriptors.extend(discovered),
                    Err(error) => {
                        diag!(
                            Warn,
                            Subsystem::Infra,
                            error = error.to_string(),
                            "Dynamic Operation provider descriptor set isolated"
                        );
                    }
                },
                Err(OperationExecutionError::Cancelled) => {
                    return Err(OperationExecutionError::Cancelled);
                }
                Err(error) => {
                    diag!(
                        Warn,
                        Subsystem::Infra,
                        error = error.to_string(),
                        "Dynamic Operation provider discovery isolated"
                    );
                }
            }
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
        if let Ok(provider) = self.provider_for(descriptor) {
            return provider
                .resolve_placement(descriptor, principal, scope, cancel)
                .await;
        }
        self.dynamic_provider_for(&descriptor.operation_ref.provider)?
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: descriptor.operation_ref.clone(),
            })?
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
        if let Ok(provider) = self.provider_for(descriptor) {
            return provider.invoke(descriptor, envelope, cancel).await;
        }
        self.dynamic_provider_for(&descriptor.operation_ref.provider)?
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: descriptor.operation_ref.clone(),
            })?
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_domain::operation::{OperationEffect, OperationRef, OperationReplayPolicy};
    use agentdash_spi::{AuthIdentity, AuthMode};
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::runtime_gateway::{
        OperationActorKind, OperationAuthorityGrant, OperationDispatch, OperationExecutionPolicy,
        OperationOriginRef, OperationProvenance, OperationReadiness, OperationScopeRef,
    };

    struct AllowAuthority;

    #[async_trait]
    impl OperationAuthorityResolver for AllowAuthority {
        async fn resolve(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<OperationAuthorityGrant, OperationExecutionError> {
            Ok(OperationAuthorityGrant {
                authority_revision: "rev-1".to_string(),
                capabilities: BTreeSet::new(),
            })
        }
    }

    struct FixtureDynamicProvider {
        provider_key: &'static str,
        invalid: bool,
    }

    #[async_trait]
    impl DynamicOperationProvider for FixtureDynamicProvider {
        fn owns_provider(&self, provider: &OperationProviderRef) -> bool {
            provider.namespace == "dynamic" && provider.provider_key == self.provider_key
        }

        async fn discover(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
            Ok(vec![descriptor(self.provider_key, self.invalid)])
        }

        async fn resolve_placement(
            &self,
            _: &OperationDescriptor,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: CancellationToken,
        ) -> Result<OperationPlacement, OperationExecutionError> {
            Ok(OperationPlacement::Cloud)
        }

        async fn invoke(
            &self,
            _: &OperationDescriptor,
            _: OperationInvocationEnvelope,
            _: CancellationToken,
        ) -> Result<Value, OperationExecutionError> {
            Ok(json!({ "ok": true }))
        }
    }

    fn descriptor(provider_key: &str, invalid: bool) -> OperationDescriptor {
        let operation_ref = OperationRef::new("dynamic", provider_key, "echo", 1).expect("ref");
        OperationDescriptor {
            operation_ref: operation_ref.clone(),
            title: if invalid { "" } else { "Echo" }.to_string(),
            description: None,
            input_schema: json!({ "type": "object" }),
            output_schema: json!({ "type": "object" }),
            effect: OperationEffect::Read,
            replay_policy: OperationReplayPolicy::ReplaySafe,
            required_capabilities: BTreeSet::new(),
            actor_visibility: BTreeSet::from([OperationActorKind::User]),
            execution_policy: OperationExecutionPolicy::default(),
            readiness: OperationReadiness::Ready,
            provenance: OperationProvenance {
                source: "fixture".to_string(),
                artifact_digest: None,
            },
            dispatch: OperationDispatch {
                provider: operation_ref.provider,
                route: "echo".to_string(),
            },
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

    #[tokio::test]
    async fn invalid_dynamic_descriptor_is_isolated_from_valid_provider() {
        let gateway = OperationGateway::try_new(
            Arc::new(AllowAuthority),
            [],
            [
                Arc::new(FixtureDynamicProvider {
                    provider_key: "invalid",
                    invalid: true,
                }) as Arc<dyn DynamicOperationProvider>,
                Arc::new(FixtureDynamicProvider {
                    provider_key: "valid",
                    invalid: false,
                }),
            ],
            Arc::new(InMemoryOperationResultStore::default()),
            Arc::new(TracingOperationAuditSink),
        )
        .expect("gateway");

        let surface = gateway
            .surface_current(
                &principal(),
                &OperationScopeRef::Project {
                    project_id: Uuid::new_v4(),
                },
                &OperationOriginRef::UserWorkshop,
                CancellationToken::new(),
            )
            .await
            .expect("surface");

        let descriptors = surface.catalog.descriptors();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].operation_ref.provider.provider_key, "valid");
    }
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
