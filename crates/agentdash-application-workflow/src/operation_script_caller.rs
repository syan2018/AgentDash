use std::sync::Arc;

use agentdash_application_ports::operation_script::{
    OperationScriptAllowedOperation, OperationScriptEngine, OperationScriptError,
    OperationScriptLimits, OperationScriptPreflightRequest, OperationScriptPreflightResult,
    OperationScriptPreflightToken, OperationScriptProgram, OperationScriptRunOutcome,
    OperationScriptRunRequest,
};
use agentdash_application_operation_gateway::{
    GatewayOperationScriptExecutor, OperationExecutionError, OperationGateway, OperationOriginRef,
    OperationPrincipal, OperationPrincipalRef, OperationScopeRef,
};
use agentdash_domain::operation::OperationRef;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

/// Trusted Workflow-owned coordinates. Authority and the executable manifest are deliberately
/// absent and are rebuilt from the canonical gateway surface for every preflight and run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowOperationScriptCallContext {
    pub principal: OperationPrincipalRef,
    pub scope: OperationScopeRef,
    pub origin: OperationOriginRef,
    pub trace_id: String,
    pub attachment_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowOperationScriptProgram {
    pub language: String,
    pub host_api_version: u16,
    pub source: String,
    pub input: Value,
    pub requested_operations: Vec<OperationRef>,
    pub limits: OperationScriptLimits,
}

#[derive(Debug, Error)]
pub enum WorkflowOperationScriptCallerError {
    #[error("Workflow OperationScript surface 解析失败: {0}")]
    Surface(#[source] OperationExecutionError),
    #[error("Workflow OperationScript 请求的 Operation 不在当前 surface: {operation_ref}")]
    OperationUnavailable { operation_ref: String },
    #[error("Workflow OperationScript descriptor 无法序列化")]
    DescriptorSerialization(#[source] serde_json::Error),
    #[error(transparent)]
    Script(#[from] OperationScriptError),
}

pub struct WorkflowOperationScriptCaller {
    engine: Arc<dyn OperationScriptEngine>,
    gateway: Arc<OperationGateway>,
    operation_executor: Arc<GatewayOperationScriptExecutor>,
}

impl WorkflowOperationScriptCaller {
    pub fn new(engine: Arc<dyn OperationScriptEngine>, gateway: Arc<OperationGateway>) -> Self {
        Self {
            engine,
            gateway: gateway.clone(),
            operation_executor: Arc::new(GatewayOperationScriptExecutor::new(gateway)),
        }
    }

    pub async fn preflight(
        &self,
        program: WorkflowOperationScriptProgram,
        context: WorkflowOperationScriptCallContext,
        cancel: CancellationToken,
    ) -> Result<OperationScriptPreflightResult, WorkflowOperationScriptCallerError> {
        let (program, context) = self
            .resolve_request(program, context, cancel.clone())
            .await?;
        self.engine
            .preflight(OperationScriptPreflightRequest { program, context }, cancel)
            .await
            .map_err(Into::into)
    }

    pub async fn run(
        &self,
        program: WorkflowOperationScriptProgram,
        context: WorkflowOperationScriptCallContext,
        token: OperationScriptPreflightToken,
        cancel: CancellationToken,
    ) -> Result<OperationScriptRunOutcome, WorkflowOperationScriptCallerError> {
        let (program, context) = self
            .resolve_request(program, context, cancel.clone())
            .await?;
        self.engine
            .run(
                OperationScriptRunRequest {
                    program,
                    context,
                    token,
                },
                self.operation_executor.clone(),
                cancel,
            )
            .await
            .map_err(Into::into)
    }

    async fn resolve_request(
        &self,
        program: WorkflowOperationScriptProgram,
        context: WorkflowOperationScriptCallContext,
        cancel: CancellationToken,
    ) -> Result<
        (
            OperationScriptProgram,
            agentdash_application_ports::operation_script::OperationScriptExecutionContext,
        ),
        WorkflowOperationScriptCallerError,
    > {
        let principal = OperationPrincipal::server_resolved(context.principal.clone());
        let surface = self
            .gateway
            .surface_current(&principal, &context.scope, &context.origin, cancel)
            .await
            .map_err(WorkflowOperationScriptCallerError::Surface)?;
        let mut allowed_operations = Vec::with_capacity(program.requested_operations.len());
        for operation_ref in &program.requested_operations {
            let descriptor = surface.catalog.get(operation_ref).ok_or_else(|| {
                WorkflowOperationScriptCallerError::OperationUnavailable {
                    operation_ref: operation_key(operation_ref),
                }
            })?;
            allowed_operations.push(OperationScriptAllowedOperation {
                operation_ref: descriptor.operation_ref.clone(),
                descriptor_digest: descriptor_digest(descriptor)?,
                effect: descriptor.effect.clone(),
                replay_policy: descriptor.replay_policy,
                recursive_operation_script: false,
            });
        }
        Ok((
            OperationScriptProgram {
                dialect: program.language,
                host_api_version: program.host_api_version,
                source: program.source,
                input: program.input,
                allowed_operations,
                limits: program.limits,
            },
            agentdash_application_ports::operation_script::OperationScriptExecutionContext {
                principal: context.principal,
                scope: context.scope,
                authority_revision: surface.authority_revision,
                granted_capabilities: surface.granted_capabilities,
                origin: context.origin,
                trace_id: context.trace_id,
                attachment_ref: context.attachment_ref,
            },
        ))
    }
}

fn descriptor_digest(
    descriptor: &agentdash_application_operation_gateway::OperationDescriptor,
) -> Result<String, WorkflowOperationScriptCallerError> {
    let encoded = serde_json::to_vec(descriptor)
        .map_err(WorkflowOperationScriptCallerError::DescriptorSerialization)?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

fn operation_key(operation_ref: &OperationRef) -> String {
    format!(
        "{}:{}:{}:v{}",
        operation_ref.provider.namespace,
        operation_ref.provider.provider_key,
        operation_ref.operation_key,
        operation_ref.contract_version
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use agentdash_application_ports::operation_script::{
        OPERATION_SCRIPT_HOST_API_V1, OperationScriptResultValue, RHAI_V1_DIALECT,
    };
    use agentdash_application_operation_gateway::{
        InMemoryOperationResultStore, OperationActorKind, OperationAuthorityGrant,
        OperationAuthorityResolver, OperationAuthorizationScope, OperationDescriptor,
        OperationDispatch, OperationExecutionPolicy, OperationInvocationEnvelope,
        OperationPlacement, OperationProvenance, OperationProvider, OperationReadiness,
        TracingOperationAuditSink,
    };
    use agentdash_domain::operation::{
        OperationEffect, OperationProviderRef, OperationReplayPolicy,
    };
    use agentdash_infrastructure::{RhaiOperationScriptConfig, RhaiOperationScriptEngine};
    use async_trait::async_trait;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    struct SequencedAuthority {
        allow_resolutions: usize,
        resolutions: AtomicUsize,
    }

    #[async_trait]
    impl OperationAuthorityResolver for SequencedAuthority {
        async fn resolve(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            cancel: CancellationToken,
        ) -> Result<OperationAuthorityGrant, OperationExecutionError> {
            if cancel.is_cancelled() {
                return Err(OperationExecutionError::Cancelled);
            }
            let resolution = self.resolutions.fetch_add(1, Ordering::SeqCst);
            let allowed = resolution < self.allow_resolutions;
            Ok(OperationAuthorityGrant {
                authority_revision: if allowed {
                    "workflow-rev-allowed".to_string()
                } else {
                    "workflow-rev-revoked".to_string()
                },
                capabilities: if allowed {
                    BTreeSet::from(["workflow.fixture.invoke".to_string()])
                } else {
                    BTreeSet::new()
                },
            })
        }
    }

    struct FixtureProvider {
        provider_ref: OperationProviderRef,
        operation_ref: OperationRef,
        invocations: AtomicUsize,
        hang: bool,
    }

    #[async_trait]
    impl OperationProvider for FixtureProvider {
        fn provider_ref(&self) -> &OperationProviderRef {
            &self.provider_ref
        }

        async fn discover(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
            Ok(vec![OperationDescriptor {
                operation_ref: self.operation_ref.clone(),
                title: "Workflow fixture".to_string(),
                description: None,
                input_schema: json!({"type":"object"}),
                output_schema: json!({"type":"object"}),
                effect: OperationEffect::Read,
                replay_policy: OperationReplayPolicy::ReplaySafe,
                required_capabilities: BTreeSet::from(["workflow.fixture.invoke".to_string()]),
                actor_visibility: BTreeSet::from([OperationActorKind::Workflow]),
                execution_policy: OperationExecutionPolicy::default(),
                readiness: OperationReadiness::Ready,
                provenance: OperationProvenance {
                    source: "workflow-test".to_string(),
                    artifact_digest: None,
                },
                dispatch: OperationDispatch {
                    provider: self.provider_ref.clone(),
                    route: "echo".to_string(),
                },
            }])
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
            _: &OperationDescriptor,
            envelope: OperationInvocationEnvelope,
            cancel: CancellationToken,
        ) -> Result<Value, OperationExecutionError> {
            self.invocations.fetch_add(1, Ordering::SeqCst);
            if self.hang {
                cancel.cancelled().await;
                return Err(OperationExecutionError::Cancelled);
            }
            Ok(json!({"echo": envelope.input}))
        }
    }

    fn operation_ref() -> OperationRef {
        OperationRef::new("workflow", "fixture", "echo", 1).expect("operation ref")
    }

    fn caller(
        allow_resolutions: usize,
        hang: bool,
    ) -> (WorkflowOperationScriptCaller, Arc<FixtureProvider>) {
        let operation_ref = operation_ref();
        let provider = Arc::new(FixtureProvider {
            provider_ref: operation_ref.provider.clone(),
            operation_ref,
            invocations: AtomicUsize::new(0),
            hang,
        });
        let gateway = Arc::new(
            OperationGateway::try_new(
                Arc::new(SequencedAuthority {
                    allow_resolutions,
                    resolutions: AtomicUsize::new(0),
                }),
                [provider.clone() as Arc<dyn OperationProvider>],
                [],
                Arc::new(InMemoryOperationResultStore::default()),
                Arc::new(TracingOperationAuditSink),
            )
            .expect("gateway"),
        );
        let engine = Arc::new(
            RhaiOperationScriptEngine::new(&[9; 32], RhaiOperationScriptConfig::default())
                .expect("engine"),
        );
        (
            WorkflowOperationScriptCaller::new(engine, gateway),
            provider,
        )
    }

    fn context() -> WorkflowOperationScriptCallContext {
        let run_id = Uuid::new_v4();
        WorkflowOperationScriptCallContext {
            principal: OperationPrincipalRef::WorkflowNode {
                run_id,
                node_key: "prepare/report".to_string(),
            },
            scope: OperationScopeRef::Project {
                project_id: Uuid::new_v4(),
            },
            origin: OperationOriginRef::Workflow,
            trace_id: format!("workflow:{run_id}:prepare/report:1"),
            attachment_ref: None,
        }
    }

    fn program() -> WorkflowOperationScriptProgram {
        WorkflowOperationScriptProgram {
            language: RHAI_V1_DIALECT.to_string(),
            host_api_version: OPERATION_SCRIPT_HOST_API_V1,
            source: r#"ops.invoke("workflow:fixture:echo:v1", input)"#.to_string(),
            input: json!({"value": 7}),
            requested_operations: vec![operation_ref()],
            limits: OperationScriptLimits::default(),
        }
    }

    #[tokio::test]
    async fn preflight_and_run_rebuild_trusted_surface_and_dispatch_nested_operation() {
        let (caller, provider) = caller(usize::MAX, false);
        let program = program();
        let context = context();
        let preflight = caller
            .preflight(program.clone(), context.clone(), CancellationToken::new())
            .await
            .expect("preflight");
        let outcome = caller
            .run(program, context, preflight.token, CancellationToken::new())
            .await
            .expect("run");

        let OperationScriptResultValue::Inline { value } = outcome.value else {
            panic!("expected inline result")
        };
        assert_eq!(value, json!({"echo":{"value":7}}));
        assert_eq!(provider.invocations.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn nested_operation_reenters_admission_after_run_context_resolution() {
        let (caller, provider) = caller(2, false);
        let program = program();
        let context = context();
        let preflight = caller
            .preflight(program.clone(), context.clone(), CancellationToken::new())
            .await
            .expect("preflight");
        let error = caller
            .run(program, context, preflight.token, CancellationToken::new())
            .await
            .expect_err("nested admission must observe revoked capability");

        assert!(matches!(
            error,
            WorkflowOperationScriptCallerError::Script(
                OperationScriptError::ExecutionFailed { ref calls, .. }
            ) if calls.len() == 1 && calls[0].error_code.as_deref() == Some("denied")
        ));
        assert_eq!(provider.invocations.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn caller_cancellation_reaches_nested_gateway_dispatch() {
        let (caller, provider) = caller(usize::MAX, true);
        let program = program();
        let context = context();
        let preflight = caller
            .preflight(program.clone(), context.clone(), CancellationToken::new())
            .await
            .expect("preflight");
        let cancel = CancellationToken::new();
        let run = caller.run(program, context, preflight.token, cancel.clone());
        tokio::pin!(run);
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => cancel.cancel(),
            result = &mut run => panic!("run completed before cancellation: {result:?}"),
        }
        let error = run.await.expect_err("cancelled run");

        assert!(matches!(
            error,
            WorkflowOperationScriptCallerError::Script(OperationScriptError::ExecutionFailed {
                outcome_unknown: true,
                ..
            })
        ));
        assert_eq!(provider.invocations.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancelled_preflight_does_not_enter_script_engine() {
        let (caller, _) = caller(usize::MAX, false);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = caller
            .preflight(program(), context(), cancel)
            .await
            .expect_err("cancelled surface resolution");
        assert!(matches!(
            error,
            WorkflowOperationScriptCallerError::Surface(OperationExecutionError::Cancelled)
        ));
    }

    #[tokio::test]
    async fn duplicate_requested_operation_is_rejected_by_shared_engine_validation() {
        let (caller, _) = caller(usize::MAX, false);
        let mut program = program();
        program.requested_operations.push(operation_ref());
        let error = caller
            .preflight(program, context(), CancellationToken::new())
            .await
            .expect_err("duplicate exact refs must be rejected");
        assert!(matches!(
            error,
            WorkflowOperationScriptCallerError::Script(OperationScriptError::InvalidRequest {
                field: "allowed_operations",
                ..
            })
        ));
    }

    #[test]
    fn workflow_context_carries_no_authority_or_manifest_fields() {
        let context = context();
        assert!(matches!(
            context.principal,
            OperationPrincipalRef::WorkflowNode { .. }
        ));
        assert_eq!(context.origin, OperationOriginRef::Workflow);
        assert!(context.trace_id.starts_with("workflow:"));
    }
}
