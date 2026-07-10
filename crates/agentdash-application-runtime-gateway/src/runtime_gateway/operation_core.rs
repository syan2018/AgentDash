use std::sync::Arc;

use agentdash_domain::operation::{OperationOriginRef, OperationScopeRef};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::operation_types::{
    ActorOperationSurface, OperationActorKind, OperationAuditEvent, OperationAuditStage,
    OperationDescriptor, OperationExecutionRequest, OperationExecutionResult,
    OperationInvocationEnvelope, OperationPlacement, OperationPrincipal, OperationReadiness,
    OperationResultAccess, OperationResultRef, OperationResultValue, ScopedOperationResult,
};
use super::{
    OperationExecutionError, validate_json_schema_definition, validate_json_schema_subset,
};

#[async_trait]
pub trait OperationSurfaceResolver: Send + Sync {
    async fn resolve_surface(
        &self,
        principal: &OperationPrincipal,
        scope: &super::OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<ActorOperationSurface, OperationExecutionError>;
}

#[async_trait]
pub trait OperationPlacementResolver: Send + Sync {
    async fn resolve_placement(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &super::OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError>;
}

#[async_trait]
pub trait OperationDispatcher: Send + Sync {
    async fn dispatch(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError>;
}

#[async_trait]
pub trait OperationResultStore: Send + Sync {
    async fn put(&self, result: ScopedOperationResult) -> Result<(), OperationExecutionError>;

    async fn get_authorized(
        &self,
        result_ref: &OperationResultRef,
        principal: &OperationPrincipal,
        scope: &super::OperationAuthorizationScope,
        current_capabilities: &std::collections::BTreeSet<String>,
    ) -> Result<Option<Value>, OperationExecutionError>;
}

#[async_trait]
pub trait OperationAuditSink: Send + Sync {
    async fn record(&self, event: OperationAuditEvent);
}

pub struct OperationExecutionCore {
    surface_resolver: Arc<dyn OperationSurfaceResolver>,
    placement_resolver: Arc<dyn OperationPlacementResolver>,
    dispatcher: Arc<dyn OperationDispatcher>,
    result_store: Arc<dyn OperationResultStore>,
    audit_sink: Arc<dyn OperationAuditSink>,
}

impl OperationExecutionCore {
    pub fn new(
        surface_resolver: Arc<dyn OperationSurfaceResolver>,
        placement_resolver: Arc<dyn OperationPlacementResolver>,
        dispatcher: Arc<dyn OperationDispatcher>,
        result_store: Arc<dyn OperationResultStore>,
        audit_sink: Arc<dyn OperationAuditSink>,
    ) -> Self {
        Self {
            surface_resolver,
            placement_resolver,
            dispatcher,
            result_store,
            audit_sink,
        }
    }

    pub async fn execute(
        &self,
        request: OperationExecutionRequest,
        cancel: CancellationToken,
    ) -> Result<OperationExecutionResult, OperationExecutionError> {
        self.audit(&request, OperationAuditStage::Started, None)
            .await;
        let result = self.execute_inner(&request, cancel).await;
        match &result {
            Ok(_) => {
                self.audit(&request, OperationAuditStage::Completed, None)
                    .await;
            }
            Err(error) => {
                self.audit(
                    &request,
                    OperationAuditStage::Failed,
                    Some(error.code().to_string()),
                )
                .await;
            }
        }
        result
    }

    async fn execute_inner(
        &self,
        request: &OperationExecutionRequest,
        cancel: CancellationToken,
    ) -> Result<OperationExecutionResult, OperationExecutionError> {
        request
            .operation_ref
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        request
            .principal
            .principal_ref()
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        request
            .scope
            .scope_ref
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        request
            .origin
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        if request.deadline <= Utc::now() {
            return Err(OperationExecutionError::DeadlineExceeded);
        }

        let child_cancel = cancel.child_token();
        let surface = run_phase(
            request.deadline,
            &cancel,
            child_cancel.clone(),
            self.surface_resolver.resolve_surface(
                &request.principal,
                &request.scope,
                &request.origin,
                child_cancel.clone(),
            ),
        )
        .await?;
        if surface.authority_revision != request.scope.authority_revision {
            return Err(OperationExecutionError::AuthorityChanged {
                expected: request.scope.authority_revision.clone(),
                current: surface.authority_revision,
            });
        }
        let descriptor = surface
            .catalog
            .get(&request.operation_ref)
            .cloned()
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: request.operation_ref.clone(),
            })?;
        validate_descriptor_contract(&descriptor)?;
        if let OperationReadiness::Unavailable { code, message } = &descriptor.readiness {
            return Err(OperationExecutionError::NotReady {
                code: code.clone(),
                message: message.clone(),
            });
        }
        validate_actor_visibility(&descriptor, &request.principal)?;
        validate_capabilities(&descriptor, &surface.granted_capabilities)?;
        validate_replay_admission(&descriptor, request)?;
        validate_json_schema_subset(&descriptor.input_schema, &request.input)
            .map_err(|message| OperationExecutionError::InputSchemaViolation { message })?;

        let placement = run_phase(
            request.deadline,
            &cancel,
            child_cancel.clone(),
            self.placement_resolver.resolve_placement(
                &descriptor,
                &request.principal,
                &request.scope,
                &request.origin,
                child_cancel.clone(),
            ),
        )
        .await?;

        // Placement resolution can race with capability revocation or installation changes.
        // Re-resolve the actor surface immediately before dispatch and require the exact
        // descriptor and authority revision to remain stable.
        let current_surface = run_phase(
            request.deadline,
            &cancel,
            child_cancel.clone(),
            self.surface_resolver.resolve_surface(
                &request.principal,
                &request.scope,
                &request.origin,
                child_cancel.clone(),
            ),
        )
        .await?;
        if current_surface.authority_revision != surface.authority_revision {
            return Err(OperationExecutionError::AuthorityChanged {
                expected: surface.authority_revision,
                current: current_surface.authority_revision,
            });
        }
        let current_descriptor = current_surface
            .catalog
            .get(&request.operation_ref)
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: request.operation_ref.clone(),
            })?;
        if current_descriptor != &descriptor {
            return Err(OperationExecutionError::DescriptorChanged {
                operation_ref: request.operation_ref.clone(),
            });
        }
        validate_actor_visibility(current_descriptor, &request.principal)?;
        validate_capabilities(current_descriptor, &current_surface.granted_capabilities)?;
        self.audit(request, OperationAuditStage::Admitted, None)
            .await;

        let now = Utc::now();
        let until_deadline = (request.deadline - now)
            .to_std()
            .map_err(|_| OperationExecutionError::DeadlineExceeded)?;
        let timeout = descriptor.execution_policy.timeout().min(until_deadline);
        let envelope = OperationInvocationEnvelope {
            operation_ref: request.operation_ref.clone(),
            input: request.input.clone(),
            principal: request.principal.clone(),
            scope: request.scope.clone(),
            origin: request.origin.clone(),
            placement,
            trace: request.trace.clone(),
            deadline: request.deadline,
            idempotency_key: request.idempotency_key.clone(),
            attachment_ref: request.attachment_ref.clone(),
        };

        self.audit(request, OperationAuditStage::Dispatched, None)
            .await;
        let dispatch = self
            .dispatcher
            .dispatch(&descriptor, envelope, child_cancel.clone());
        let output = tokio::select! {
            _ = cancel.cancelled() => {
                child_cancel.cancel();
                return Err(OperationExecutionError::Cancelled);
            }
            timed = tokio::time::timeout(timeout, dispatch) => {
                match timed {
                    Ok(result) => result?,
                    Err(_) => {
                        child_cancel.cancel();
                        return Err(OperationExecutionError::DeadlineExceeded);
                    }
                }
            }
        };
        validate_json_schema_subset(&descriptor.output_schema, &output)
            .map_err(|message| OperationExecutionError::OutputSchemaViolation { message })?;
        let output_bytes = serde_json::to_vec(&output)
            .map_err(|error| OperationExecutionError::OutputEncoding {
                message: error.to_string(),
            })?
            .len();
        if output_bytes > descriptor.execution_policy.max_output_bytes {
            return Err(OperationExecutionError::OutputTooLarge {
                actual: output_bytes,
                limit: descriptor.execution_policy.max_output_bytes,
            });
        }

        let value = if output_bytes <= descriptor.execution_policy.max_inline_output_bytes {
            OperationResultValue::Inline { value: output }
        } else {
            let result_ref = OperationResultRef {
                result_id: Uuid::new_v4(),
            };
            let ttl_seconds = i64::try_from(descriptor.execution_policy.result_ttl_seconds)
                .map_err(|_| {
                    OperationExecutionError::invalid_request("Operation result TTL 超出可表示范围")
                })?;
            let expires_at = Utc::now() + chrono::Duration::seconds(ttl_seconds);
            self.result_store
                .put(ScopedOperationResult {
                    result_ref: result_ref.clone(),
                    operation_ref: request.operation_ref.clone(),
                    value: output,
                    access: OperationResultAccess {
                        principal_ref: request.principal.principal_ref().clone(),
                        scope: request.scope.clone(),
                        required_capabilities: descriptor.required_capabilities.clone(),
                        expires_at,
                    },
                })
                .await?;
            OperationResultValue::Ref { result_ref }
        };

        Ok(OperationExecutionResult {
            operation_ref: request.operation_ref.clone(),
            trace: request.trace.clone(),
            value,
            output_bytes,
        })
    }

    async fn audit(
        &self,
        request: &OperationExecutionRequest,
        stage: OperationAuditStage,
        outcome_code: Option<String>,
    ) {
        self.audit_sink
            .record(OperationAuditEvent {
                operation_ref: request.operation_ref.clone(),
                principal_ref: request.principal.principal_ref().clone(),
                scope: request.scope.clone(),
                origin: request.origin.clone(),
                trace: request.trace.clone(),
                stage,
                outcome_code,
                occurred_at: Utc::now(),
            })
            .await;
    }
}

async fn run_phase<T>(
    deadline: chrono::DateTime<Utc>,
    parent_cancel: &CancellationToken,
    child_cancel: CancellationToken,
    future: impl std::future::Future<Output = Result<T, OperationExecutionError>>,
) -> Result<T, OperationExecutionError> {
    let remaining = (deadline - Utc::now())
        .to_std()
        .map_err(|_| OperationExecutionError::DeadlineExceeded)?;
    tokio::select! {
        _ = parent_cancel.cancelled() => {
            child_cancel.cancel();
            Err(OperationExecutionError::Cancelled)
        }
        result = tokio::time::timeout(remaining, future) => {
            match result {
                Ok(result) => result,
                Err(_) => {
                    child_cancel.cancel();
                    Err(OperationExecutionError::DeadlineExceeded)
                }
            }
        }
    }
}

fn validate_descriptor_contract(
    descriptor: &OperationDescriptor,
) -> Result<(), OperationExecutionError> {
    descriptor.validate_identity()?;
    validate_json_schema_definition(&descriptor.input_schema).map_err(|message| {
        OperationExecutionError::InvalidDescriptor {
            field: "input_schema",
            message,
        }
    })?;
    validate_json_schema_definition(&descriptor.output_schema).map_err(|message| {
        OperationExecutionError::InvalidDescriptor {
            field: "output_schema",
            message,
        }
    })
}

fn validate_actor_visibility(
    descriptor: &OperationDescriptor,
    principal: &OperationPrincipal,
) -> Result<(), OperationExecutionError> {
    let actor_kind: OperationActorKind = principal.actor_kind();
    if descriptor.actor_visibility.contains(&actor_kind) {
        Ok(())
    } else {
        Err(OperationExecutionError::ActorDenied { actor_kind })
    }
}

fn validate_capabilities(
    descriptor: &OperationDescriptor,
    granted: &std::collections::BTreeSet<String>,
) -> Result<(), OperationExecutionError> {
    let missing = descriptor
        .required_capabilities
        .difference(granted)
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(OperationExecutionError::CapabilitiesDenied { missing })
    }
}

fn validate_replay_admission(
    descriptor: &OperationDescriptor,
    request: &OperationExecutionRequest,
) -> Result<(), OperationExecutionError> {
    if !matches!(request.origin, OperationOriginRef::EffectReplay { .. }) {
        return Ok(());
    }
    if descriptor.replay_policy == super::OperationReplayPolicy::NonReplayable {
        return Err(OperationExecutionError::ReplayDenied);
    }
    if request.idempotency_key.as_deref().is_none_or(str::is_empty) {
        return Err(OperationExecutionError::invalid_request(
            "Effect replay 必须携带稳定 idempotency_key",
        ));
    }
    Ok(())
}

pub fn result_access_matches(
    access: &OperationResultAccess,
    principal: &OperationPrincipal,
    scope: &super::OperationAuthorizationScope,
    current_capabilities: &std::collections::BTreeSet<String>,
) -> bool {
    access.expires_at > Utc::now()
        && access.principal_ref == *principal.principal_ref()
        && access.scope.scope_ref == scope.scope_ref
        && access.required_capabilities.is_subset(current_capabilities)
}

pub fn scope_project_id(scope: &OperationScopeRef) -> Option<Uuid> {
    match scope {
        OperationScopeRef::EnvironmentSetup { project_id, .. } => *project_id,
        OperationScopeRef::Project { project_id }
        | OperationScopeRef::WorkspaceBinding { project_id, .. } => Some(*project_id),
        OperationScopeRef::InteractionInstance { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap, VecDeque};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use agentdash_domain::operation::OperationRef;
    use agentdash_spi::{AuthIdentity, AuthMode};
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use serde_json::{Value, json};
    use tokio::sync::Mutex;

    use super::*;
    use crate::runtime_gateway::{
        ActorOperationSurface, OperationAuthorizationScope, OperationCatalog, OperationDescriptor,
        OperationDispatch, OperationEffect, OperationExecutionPolicy, OperationOriginRef,
        OperationProvenance, OperationReadiness, OperationReplayPolicy, OperationResultValue,
        OperationScopeRef, OperationTraceContext,
    };

    struct SequenceSurfaceResolver {
        surfaces: Mutex<VecDeque<ActorOperationSurface>>,
        last: Mutex<Option<ActorOperationSurface>>,
        hang: bool,
    }

    impl SequenceSurfaceResolver {
        fn new(surfaces: Vec<ActorOperationSurface>) -> Self {
            Self {
                surfaces: Mutex::new(surfaces.into()),
                last: Mutex::new(None),
                hang: false,
            }
        }

        fn hanging() -> Self {
            Self {
                surfaces: Mutex::new(VecDeque::new()),
                last: Mutex::new(None),
                hang: true,
            }
        }
    }

    #[async_trait]
    impl OperationSurfaceResolver for SequenceSurfaceResolver {
        async fn resolve_surface(
            &self,
            _principal: &OperationPrincipal,
            _scope: &OperationAuthorizationScope,
            _origin: &OperationOriginRef,
            cancel: CancellationToken,
        ) -> Result<ActorOperationSurface, OperationExecutionError> {
            if self.hang {
                cancel.cancelled().await;
                return Err(OperationExecutionError::Cancelled);
            }
            if let Some(surface) = self.surfaces.lock().await.pop_front() {
                *self.last.lock().await = Some(surface.clone());
                return Ok(surface);
            }
            self.last
                .lock()
                .await
                .clone()
                .ok_or_else(|| OperationExecutionError::provider_failed("missing test surface"))
        }
    }

    struct StaticPlacementResolver;

    #[async_trait]
    impl OperationPlacementResolver for StaticPlacementResolver {
        async fn resolve_placement(
            &self,
            _descriptor: &OperationDescriptor,
            _principal: &OperationPrincipal,
            _scope: &OperationAuthorizationScope,
            _origin: &OperationOriginRef,
            _cancel: CancellationToken,
        ) -> Result<OperationPlacement, OperationExecutionError> {
            Ok(OperationPlacement::Cloud)
        }
    }

    struct HangingPlacementResolver;

    #[async_trait]
    impl OperationPlacementResolver for HangingPlacementResolver {
        async fn resolve_placement(
            &self,
            _descriptor: &OperationDescriptor,
            _principal: &OperationPrincipal,
            _scope: &OperationAuthorizationScope,
            _origin: &OperationOriginRef,
            cancel: CancellationToken,
        ) -> Result<OperationPlacement, OperationExecutionError> {
            cancel.cancelled().await;
            Err(OperationExecutionError::Cancelled)
        }
    }

    struct RecordingDispatcher {
        calls: AtomicUsize,
        routes: Mutex<Vec<String>>,
        origins: Mutex<Vec<OperationOriginRef>>,
        output: Value,
    }

    impl RecordingDispatcher {
        fn new(output: Value) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                routes: Mutex::new(Vec::new()),
                origins: Mutex::new(Vec::new()),
                output,
            }
        }
    }

    #[async_trait]
    impl OperationDispatcher for RecordingDispatcher {
        async fn dispatch(
            &self,
            descriptor: &OperationDescriptor,
            envelope: OperationInvocationEnvelope,
            _cancel: CancellationToken,
        ) -> Result<Value, OperationExecutionError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.routes
                .lock()
                .await
                .push(descriptor.dispatch.route.clone());
            self.origins.lock().await.push(envelope.origin);
            Ok(self.output.clone())
        }
    }

    #[derive(Default)]
    struct MemoryResultStore {
        results: Mutex<HashMap<Uuid, ScopedOperationResult>>,
    }

    #[async_trait]
    impl OperationResultStore for MemoryResultStore {
        async fn put(&self, result: ScopedOperationResult) -> Result<(), OperationExecutionError> {
            self.results
                .lock()
                .await
                .insert(result.result_ref.result_id, result);
            Ok(())
        }

        async fn get_authorized(
            &self,
            result_ref: &OperationResultRef,
            principal: &OperationPrincipal,
            scope: &OperationAuthorizationScope,
            current_capabilities: &BTreeSet<String>,
        ) -> Result<Option<Value>, OperationExecutionError> {
            let results = self.results.lock().await;
            Ok(results.get(&result_ref.result_id).and_then(|result| {
                result_access_matches(&result.access, principal, scope, current_capabilities)
                    .then(|| result.value.clone())
            }))
        }
    }

    #[derive(Default)]
    struct RecordingAuditSink {
        stages: Mutex<Vec<OperationAuditStage>>,
    }

    #[async_trait]
    impl OperationAuditSink for RecordingAuditSink {
        async fn record(&self, event: OperationAuditEvent) {
            self.stages.lock().await.push(event.stage);
        }
    }

    fn operation_ref(provider_key: &str) -> OperationRef {
        OperationRef::new("extension", provider_key, "lookup", 1).expect("valid operation ref")
    }

    fn test_identity(user_id: &str) -> AuthIdentity {
        AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: user_id.to_string(),
            subject: user_id.to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: None,
            extra: Value::Null,
        }
    }

    fn descriptor(provider_key: &str, route: &str) -> OperationDescriptor {
        let operation_ref = operation_ref(provider_key);
        OperationDescriptor {
            operation_ref: operation_ref.clone(),
            title: format!("{provider_key} lookup"),
            description: None,
            input_schema: json!({
                "type": "object",
                "required": ["query"],
                "properties": { "query": { "type": "string" } },
                "additionalProperties": false
            }),
            output_schema: json!({
                "type": "object",
                "required": ["ok"],
                "properties": { "ok": { "type": "boolean" } }
            }),
            effect: OperationEffect::Read,
            replay_policy: OperationReplayPolicy::ReplaySafe,
            required_capabilities: BTreeSet::from(["extension.invoke".to_string()]),
            actor_visibility: BTreeSet::from([OperationActorKind::User, OperationActorKind::Agent]),
            execution_policy: OperationExecutionPolicy::default(),
            readiness: OperationReadiness::Ready,
            provenance: OperationProvenance {
                source: "test".to_string(),
                artifact_digest: None,
            },
            dispatch: OperationDispatch {
                provider: operation_ref.provider,
                route: route.to_string(),
            },
        }
    }

    fn surface(
        revision: &str,
        capabilities: BTreeSet<String>,
        descriptors: Vec<OperationDescriptor>,
    ) -> ActorOperationSurface {
        ActorOperationSurface {
            authority_revision: revision.to_string(),
            granted_capabilities: capabilities,
            catalog: OperationCatalog::try_new(descriptors).expect("valid catalog"),
        }
    }

    fn request(
        operation_ref: OperationRef,
        origin: OperationOriginRef,
    ) -> OperationExecutionRequest {
        OperationExecutionRequest {
            operation_ref,
            input: json!({ "query": "demo" }),
            principal: OperationPrincipal::authenticated_user(test_identity("user-1")),
            scope: OperationAuthorizationScope {
                scope_ref: OperationScopeRef::Project {
                    project_id: Uuid::new_v4(),
                },
                authority_revision: "rev-1".to_string(),
            },
            origin,
            trace: OperationTraceContext::root(),
            deadline: Utc::now() + Duration::seconds(5),
            idempotency_key: None,
            attachment_ref: None,
        }
    }

    fn core(
        surfaces: Arc<dyn OperationSurfaceResolver>,
        dispatcher: Arc<RecordingDispatcher>,
        result_store: Arc<MemoryResultStore>,
        audit: Arc<RecordingAuditSink>,
    ) -> OperationExecutionCore {
        OperationExecutionCore::new(
            surfaces,
            Arc::new(StaticPlacementResolver),
            dispatcher,
            result_store,
            audit,
        )
    }

    #[tokio::test]
    async fn exact_provider_identity_and_direct_nested_calls_share_the_core() {
        let alpha = descriptor("alpha.weather", "alpha-route");
        let beta = descriptor("beta.weather", "beta-route");
        let actor_surface = surface(
            "rev-1",
            BTreeSet::from(["extension.invoke".to_string()]),
            vec![alpha, beta.clone()],
        );
        let resolver = Arc::new(SequenceSurfaceResolver::new(vec![actor_surface]));
        let dispatcher = Arc::new(RecordingDispatcher::new(json!({ "ok": true })));
        let result_store = Arc::new(MemoryResultStore::default());
        let audit = Arc::new(RecordingAuditSink::default());
        let core = core(resolver, dispatcher.clone(), result_store, audit);

        core.execute(
            request(operation_ref("beta.weather"), OperationOriginRef::AgentTool),
            CancellationToken::new(),
        )
        .await
        .expect("direct invocation should succeed");
        core.execute(
            request(
                operation_ref("beta.weather"),
                OperationOriginRef::OperationScriptNested {
                    script_invocation_id: "script-1".to_string(),
                },
            ),
            CancellationToken::new(),
        )
        .await
        .expect("nested invocation should use the same core");

        assert_eq!(dispatcher.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            dispatcher.routes.lock().await.as_slice(),
            ["beta-route", "beta-route"]
        );
    }

    #[tokio::test]
    async fn capability_revocation_between_placement_and_dispatch_is_denied() {
        let descriptor = descriptor("alpha.weather", "alpha-route");
        let resolver = Arc::new(SequenceSurfaceResolver::new(vec![
            surface(
                "rev-1",
                BTreeSet::from(["extension.invoke".to_string()]),
                vec![descriptor.clone()],
            ),
            surface("rev-1", BTreeSet::new(), vec![descriptor]),
        ]));
        let dispatcher = Arc::new(RecordingDispatcher::new(json!({ "ok": true })));
        let core = core(
            resolver,
            dispatcher.clone(),
            Arc::new(MemoryResultStore::default()),
            Arc::new(RecordingAuditSink::default()),
        );

        let error = core
            .execute(
                request(
                    operation_ref("alpha.weather"),
                    OperationOriginRef::AgentTool,
                ),
                CancellationToken::new(),
            )
            .await
            .expect_err("revoked capability must be re-admitted before dispatch");

        assert_eq!(
            error.kind(),
            super::super::OperationExecutionErrorKind::Denied
        );
        assert_eq!(dispatcher.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn authority_revision_change_during_admission_is_rejected() {
        let descriptor = descriptor("alpha.weather", "alpha-route");
        let capabilities = BTreeSet::from(["extension.invoke".to_string()]);
        let resolver = Arc::new(SequenceSurfaceResolver::new(vec![
            surface("rev-1", capabilities.clone(), vec![descriptor.clone()]),
            surface("rev-2", capabilities, vec![descriptor]),
        ]));
        let dispatcher = Arc::new(RecordingDispatcher::new(json!({ "ok": true })));
        let core = core(
            resolver,
            dispatcher.clone(),
            Arc::new(MemoryResultStore::default()),
            Arc::new(RecordingAuditSink::default()),
        );

        let error = core
            .execute(
                request(
                    operation_ref("alpha.weather"),
                    OperationOriginRef::AgentTool,
                ),
                CancellationToken::new(),
            )
            .await
            .expect_err("authority change must be rejected");

        assert_eq!(
            error.kind(),
            super::super::OperationExecutionErrorKind::AuthorityChanged
        );
        assert_eq!(dispatcher.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_hanging_surface_resolver() {
        let resolver = Arc::new(SequenceSurfaceResolver::hanging());
        let dispatcher = Arc::new(RecordingDispatcher::new(json!({ "ok": true })));
        let core = Arc::new(core(
            resolver,
            dispatcher,
            Arc::new(MemoryResultStore::default()),
            Arc::new(RecordingAuditSink::default()),
        ));
        let cancel = CancellationToken::new();
        let execution = tokio::spawn({
            let core = core.clone();
            let cancel = cancel.clone();
            async move {
                core.execute(
                    request(
                        operation_ref("alpha.weather"),
                        OperationOriginRef::AgentTool,
                    ),
                    cancel,
                )
                .await
            }
        });
        tokio::task::yield_now().await;
        cancel.cancel();

        let error = execution
            .await
            .expect("execution task should finish")
            .expect_err("cancelled resolution must fail");
        assert_eq!(
            error.kind(),
            super::super::OperationExecutionErrorKind::Cancelled
        );
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_hanging_placement_resolver() {
        let operation = descriptor("alpha.weather", "alpha-route");
        let actor_surface = surface(
            "rev-1",
            BTreeSet::from(["extension.invoke".to_string()]),
            vec![operation],
        );
        let resolver = Arc::new(SequenceSurfaceResolver::new(vec![actor_surface]));
        let dispatcher = Arc::new(RecordingDispatcher::new(json!({ "ok": true })));
        let core = Arc::new(OperationExecutionCore::new(
            resolver,
            Arc::new(HangingPlacementResolver),
            dispatcher.clone(),
            Arc::new(MemoryResultStore::default()),
            Arc::new(RecordingAuditSink::default()),
        ));
        let cancel = CancellationToken::new();
        let execution = tokio::spawn({
            let core = core.clone();
            let cancel = cancel.clone();
            async move {
                core.execute(
                    request(
                        operation_ref("alpha.weather"),
                        OperationOriginRef::AgentTool,
                    ),
                    cancel,
                )
                .await
            }
        });
        tokio::task::yield_now().await;
        cancel.cancel();

        let error = execution
            .await
            .expect("execution task should finish")
            .expect_err("cancelled placement must fail");
        assert_eq!(
            error.kind(),
            super::super::OperationExecutionErrorKind::Cancelled
        );
        assert_eq!(dispatcher.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn unavailable_descriptor_is_rejected_before_placement() {
        let mut operation = descriptor("alpha.weather", "alpha-route");
        operation.readiness = OperationReadiness::Unavailable {
            code: "provider_offline".to_string(),
            message: "provider is offline".to_string(),
        };
        let actor_surface = surface(
            "rev-1",
            BTreeSet::from(["extension.invoke".to_string()]),
            vec![operation],
        );
        let dispatcher = Arc::new(RecordingDispatcher::new(json!({ "ok": true })));
        let core = core(
            Arc::new(SequenceSurfaceResolver::new(vec![actor_surface])),
            dispatcher.clone(),
            Arc::new(MemoryResultStore::default()),
            Arc::new(RecordingAuditSink::default()),
        );

        let error = core
            .execute(
                request(
                    operation_ref("alpha.weather"),
                    OperationOriginRef::AgentTool,
                ),
                CancellationToken::new(),
            )
            .await
            .expect_err("unavailable descriptor must fail before dispatch");
        assert_eq!(
            error.kind(),
            super::super::OperationExecutionErrorKind::Unavailable
        );
        assert_eq!(dispatcher.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn large_result_ref_rechecks_owner_scope_and_capability() {
        let mut descriptor = descriptor("alpha.weather", "alpha-route");
        descriptor.output_schema = json!(true);
        descriptor.execution_policy.max_inline_output_bytes = 1;
        let actor_surface = surface(
            "rev-1",
            BTreeSet::from(["extension.invoke".to_string()]),
            vec![descriptor],
        );
        let resolver = Arc::new(SequenceSurfaceResolver::new(vec![actor_surface]));
        let dispatcher = Arc::new(RecordingDispatcher::new(json!({ "ok": true })));
        let result_store = Arc::new(MemoryResultStore::default());
        let core = core(
            resolver,
            dispatcher,
            result_store.clone(),
            Arc::new(RecordingAuditSink::default()),
        );
        let execution_request = request(
            operation_ref("alpha.weather"),
            OperationOriginRef::UserWorkshop,
        );
        let principal = execution_request.principal.clone();
        let scope = execution_request.scope.clone();
        let result = core
            .execute(execution_request, CancellationToken::new())
            .await
            .expect("large result should be stored");
        let OperationResultValue::Ref { result_ref } = result.value else {
            panic!("expected scoped result ref");
        };

        let denied = result_store
            .get_authorized(
                &result_ref,
                &OperationPrincipal::authenticated_user(test_identity("another-user")),
                &scope,
                &BTreeSet::from(["extension.invoke".to_string()]),
            )
            .await
            .expect("lookup should not fail");
        assert!(denied.is_none(), "result id must not be a bearer token");

        let allowed = result_store
            .get_authorized(
                &result_ref,
                &principal,
                &scope,
                &BTreeSet::from(["extension.invoke".to_string()]),
            )
            .await
            .expect("lookup should not fail");
        assert_eq!(allowed, Some(json!({ "ok": true })));
    }

    #[test]
    fn catalog_rejects_invalid_schema_and_orders_exact_identities() {
        let mut invalid = descriptor("invalid.weather", "invalid-route");
        invalid.input_schema = json!({ "oneOf": [] });
        assert!(OperationCatalog::try_new(vec![invalid]).is_err());

        let catalog = OperationCatalog::try_new(vec![
            descriptor("zeta.weather", "zeta-route"),
            descriptor("alpha.weather", "alpha-route"),
        ])
        .expect("catalog should be valid");
        let providers = catalog
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.operation_ref.provider.provider_key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(providers, vec!["alpha.weather", "zeta.weather"]);
    }
}
