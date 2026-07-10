use std::sync::Arc;

use agentdash_application_ports::operation_script::{
    OperationScriptAllowedOperation, OperationScriptEngine, OperationScriptError,
    OperationScriptLimits, OperationScriptPreflightRequest, OperationScriptPreflightResult,
    OperationScriptPreflightToken, OperationScriptProgram, OperationScriptRunOutcome,
    OperationScriptRunRequest,
};
use agentdash_domain::operation::OperationRef;
use agentdash_spi::AuthIdentity;
use chrono::{Duration, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    ActorOperationSurface, GatewayOperationScriptExecutor, OperationExecutionError,
    OperationExecutionResult, OperationGateway, OperationInvocationCommand, OperationOriginRef,
    OperationPrincipal, OperationPrincipalRef, OperationScopeRef, OperationTraceContext,
};

#[derive(Debug, Clone, PartialEq)]
pub struct HostOperationScriptProgram {
    pub language: String,
    pub host_api_version: u16,
    pub source: String,
    pub input: Value,
    pub requested_operations: Vec<OperationRef>,
    pub limits: OperationScriptLimits,
}

pub struct BoundOperationScriptHost {
    host: BoundOperationHost,
    engine: Arc<dyn OperationScriptEngine>,
    executor: Arc<GatewayOperationScriptExecutor>,
}

/// Untrusted caller-controlled portion of a host invocation.
///
/// Principal, scope, origin, authority revision and placement are deliberately absent. A trusted
/// host adapter binds the first three and the gateway resolves the latter two for every call.
#[derive(Debug, Clone)]
pub struct HostOperationInvocation {
    pub operation_ref: OperationRef,
    pub input: Value,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostInvocationOptions {
    trace: OperationTraceContext,
    timeout: Duration,
}

impl HostInvocationOptions {
    const MAX_TIMEOUT_SECONDS: i64 = 120;

    pub fn root(timeout: Duration) -> Self {
        Self {
            trace: OperationTraceContext::root(),
            timeout: clamp_timeout(timeout),
        }
    }

    pub fn child(parent: &OperationTraceContext, timeout: Duration) -> Self {
        Self {
            trace: OperationTraceContext::child_of(parent),
            timeout: clamp_timeout(timeout),
        }
    }
}

impl Default for HostInvocationOptions {
    fn default() -> Self {
        Self::root(Duration::seconds(30))
    }
}

fn clamp_timeout(timeout: Duration) -> Duration {
    timeout
        .max(Duration::milliseconds(1))
        .min(Duration::seconds(
            HostInvocationOptions::MAX_TIMEOUT_SECONDS,
        ))
}

#[derive(Clone)]
pub struct BoundOperationHost {
    gateway: Arc<OperationGateway>,
    principal: OperationPrincipal,
    scope_ref: OperationScopeRef,
    origin: OperationOriginRef,
    attachment_ref: Option<String>,
}

impl BoundOperationHost {
    fn new(
        gateway: Arc<OperationGateway>,
        principal: OperationPrincipal,
        scope_ref: OperationScopeRef,
        origin: OperationOriginRef,
        attachment_ref: Option<String>,
    ) -> Result<Self, OperationExecutionError> {
        principal
            .principal_ref()
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        scope_ref
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        origin
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        Ok(Self {
            gateway,
            principal,
            scope_ref,
            origin,
            attachment_ref,
        })
    }

    pub async fn discover(
        &self,
        cancel: CancellationToken,
    ) -> Result<ActorOperationSurface, OperationExecutionError> {
        self.gateway
            .surface_current(&self.principal, &self.scope_ref, &self.origin, cancel)
            .await
    }

    pub async fn invoke(
        &self,
        invocation: HostOperationInvocation,
        options: HostInvocationOptions,
        cancel: CancellationToken,
    ) -> Result<OperationExecutionResult, OperationExecutionError> {
        self.gateway
            .invoke(
                OperationInvocationCommand {
                    operation_ref: invocation.operation_ref,
                    input: invocation.input,
                    principal: self.principal.clone(),
                    scope_ref: self.scope_ref.clone(),
                    origin: self.origin.clone(),
                    trace: options.trace,
                    deadline: Utc::now() + options.timeout,
                    idempotency_key: invocation.idempotency_key,
                    attachment_ref: self.attachment_ref.clone(),
                },
                cancel,
            )
            .await
    }

    pub fn principal(&self) -> &OperationPrincipal {
        &self.principal
    }

    pub fn scope_ref(&self) -> &OperationScopeRef {
        &self.scope_ref
    }

    pub fn origin(&self) -> &OperationOriginRef {
        &self.origin
    }

    pub fn operation_script(
        self,
        engine: Arc<dyn OperationScriptEngine>,
    ) -> BoundOperationScriptHost {
        BoundOperationScriptHost {
            executor: Arc::new(GatewayOperationScriptExecutor::new(self.gateway.clone())),
            host: self,
            engine,
        }
    }
}

impl BoundOperationScriptHost {
    pub async fn preflight(
        &self,
        program: HostOperationScriptProgram,
        cancel: CancellationToken,
    ) -> Result<OperationScriptPreflightResult, OperationScriptError> {
        let (program, context) = self.resolve(program, cancel.clone()).await?;
        self.engine
            .preflight(OperationScriptPreflightRequest { program, context }, cancel)
            .await
    }

    pub async fn run(
        &self,
        program: HostOperationScriptProgram,
        token: OperationScriptPreflightToken,
        cancel: CancellationToken,
    ) -> Result<OperationScriptRunOutcome, OperationScriptError> {
        let (program, context) = self.resolve(program, cancel.clone()).await?;
        self.engine
            .run(
                OperationScriptRunRequest {
                    program,
                    context,
                    token,
                },
                self.executor.clone(),
                cancel,
            )
            .await
    }

    async fn resolve(
        &self,
        program: HostOperationScriptProgram,
        cancel: CancellationToken,
    ) -> Result<
        (
            OperationScriptProgram,
            agentdash_application_ports::operation_script::OperationScriptExecutionContext,
        ),
        OperationScriptError,
    > {
        let surface =
            self.host
                .discover(cancel)
                .await
                .map_err(|error| OperationScriptError::Internal {
                    code: gateway_script_error_code(&error),
                })?;
        let mut allowed_operations = Vec::with_capacity(program.requested_operations.len());
        for operation_ref in &program.requested_operations {
            let descriptor = surface.catalog.get(operation_ref).ok_or_else(|| {
                OperationScriptError::OperationDenied {
                    operation_key: operation_key(operation_ref),
                }
            })?;
            let encoded =
                serde_json::to_vec(descriptor).map_err(|_| OperationScriptError::Internal {
                    code: "descriptor_serialization_failed",
                })?;
            allowed_operations.push(OperationScriptAllowedOperation {
                operation_ref: descriptor.operation_ref.clone(),
                descriptor_digest: format!("sha256:{:x}", Sha256::digest(encoded)),
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
                principal: self.host.principal.principal_ref().clone(),
                scope: self.host.scope_ref.clone(),
                authority_revision: surface.authority_revision,
                granted_capabilities: surface.granted_capabilities,
                origin: self.host.origin.clone(),
                trace_id: OperationTraceContext::root().trace_id,
                attachment_ref: self.host.attachment_ref.clone(),
            },
        ))
    }
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

fn gateway_script_error_code(error: &OperationExecutionError) -> &'static str {
    match error.kind() {
        super::OperationExecutionErrorKind::Cancelled => "surface_cancelled",
        super::OperationExecutionErrorKind::DeadlineExceeded => "surface_deadline_exceeded",
        _ => "surface_unavailable",
    }
}

pub struct UserWorkshopOperationHost;

impl UserWorkshopOperationHost {
    pub fn project(
        gateway: Arc<OperationGateway>,
        identity: AuthIdentity,
        project_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::authenticated_user(identity),
            OperationScopeRef::Project { project_id },
            OperationOriginRef::UserWorkshop,
            None,
        )
    }

    pub fn canvas(
        gateway: Arc<OperationGateway>,
        identity: AuthIdentity,
        project_id: Uuid,
        definition_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::authenticated_user(identity),
            OperationScopeRef::Project { project_id },
            OperationOriginRef::Canvas { definition_id },
            None,
        )
    }

    pub fn interaction(
        gateway: Arc<OperationGateway>,
        identity: AuthIdentity,
        instance_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::authenticated_user(identity),
            OperationScopeRef::InteractionInstance { instance_id },
            OperationOriginRef::Interaction { instance_id },
            None,
        )
    }

    pub fn interaction_attachment(
        gateway: Arc<OperationGateway>,
        identity: AuthIdentity,
        instance_id: Uuid,
        attachment_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::authenticated_user(identity),
            OperationScopeRef::InteractionInstance { instance_id },
            OperationOriginRef::Interaction { instance_id },
            Some(format!("interaction-attachment:{attachment_id}")),
        )
    }

    pub fn extension_panel(
        gateway: Arc<OperationGateway>,
        identity: AuthIdentity,
        project_id: Uuid,
        installation_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::authenticated_user(identity),
            OperationScopeRef::Project { project_id },
            OperationOriginRef::ExtensionPanel { installation_id },
            None,
        )
    }
}

pub struct AgentRunOperationHost;

impl AgentRunOperationHost {
    pub fn project(
        gateway: Arc<OperationGateway>,
        run_id: Uuid,
        agent_id: Uuid,
        project_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::server_resolved(OperationPrincipalRef::AgentRunAgent {
                run_id,
                agent_id,
            }),
            OperationScopeRef::Project { project_id },
            OperationOriginRef::AgentTool,
            None,
        )
    }

    pub fn interaction(
        gateway: Arc<OperationGateway>,
        run_id: Uuid,
        agent_id: Uuid,
        instance_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::server_resolved(OperationPrincipalRef::AgentRunAgent {
                run_id,
                agent_id,
            }),
            OperationScopeRef::InteractionInstance { instance_id },
            OperationOriginRef::AgentTool,
            None,
        )
    }
}

pub struct ExtensionServiceOperationHost;

impl ExtensionServiceOperationHost {
    pub fn project(
        gateway: Arc<OperationGateway>,
        installation_id: Uuid,
        project_id: Uuid,
    ) -> Result<BoundOperationHost, OperationExecutionError> {
        BoundOperationHost::new(
            gateway,
            OperationPrincipal::server_resolved(OperationPrincipalRef::ExtensionInstallation {
                installation_id,
            }),
            OperationScopeRef::Project { project_id },
            OperationOriginRef::ExtensionService,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_spi::AuthMode;
    use async_trait::async_trait;

    use super::*;
    use crate::runtime_gateway::{
        EphemeralOperationResultStore, OperationAuthorityGrant, OperationAuthorityResolver,
        OperationAuthorizationScope, TracingOperationAuditSink,
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
                authority_revision: "test-revision".to_string(),
                capabilities: BTreeSet::new(),
            })
        }
    }

    fn gateway() -> Arc<OperationGateway> {
        Arc::new(
            OperationGateway::try_new(
                Arc::new(AllowAuthority),
                [],
                [],
                Arc::new(EphemeralOperationResultStore::default()),
                Arc::new(TracingOperationAuditSink),
            )
            .expect("gateway"),
        )
    }

    fn identity() -> AuthIdentity {
        AuthIdentity {
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
        }
    }

    #[test]
    fn canvas_host_binds_user_project_without_runtime_attachment() {
        let project_id = Uuid::new_v4();
        let definition_id = Uuid::new_v4();
        let host =
            UserWorkshopOperationHost::canvas(gateway(), identity(), project_id, definition_id)
                .expect("host");

        assert_eq!(
            host.principal().principal_ref(),
            &OperationPrincipalRef::User {
                user_id: "user-1".to_string()
            }
        );
        assert_eq!(host.scope_ref(), &OperationScopeRef::Project { project_id });
        assert_eq!(host.origin(), &OperationOriginRef::Canvas { definition_id });
        assert_eq!(host.attachment_ref, None);
    }

    #[test]
    fn interaction_host_uses_instance_as_scope_and_origin() {
        let instance_id = Uuid::new_v4();
        let host = UserWorkshopOperationHost::interaction(gateway(), identity(), instance_id)
            .expect("host");

        assert_eq!(
            host.scope_ref(),
            &OperationScopeRef::InteractionInstance { instance_id }
        );
        assert_eq!(
            host.origin(),
            &OperationOriginRef::Interaction { instance_id }
        );
        assert_eq!(host.attachment_ref, None);
    }

    #[test]
    fn interaction_attachment_is_explicit_and_distinct_from_instance_identity() {
        let instance_id = Uuid::new_v4();
        let attachment_id = Uuid::new_v4();
        let host = UserWorkshopOperationHost::interaction_attachment(
            gateway(),
            identity(),
            instance_id,
            attachment_id,
        )
        .expect("host");

        assert_eq!(
            host.attachment_ref.as_deref(),
            Some(format!("interaction-attachment:{attachment_id}").as_str())
        );
    }

    #[test]
    fn agent_host_binds_server_resolved_agent_identity() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let host =
            AgentRunOperationHost::project(gateway(), run_id, agent_id, project_id).expect("host");

        assert_eq!(
            host.principal().principal_ref(),
            &OperationPrincipalRef::AgentRunAgent { run_id, agent_id }
        );
        assert!(host.principal().user_identity().is_none());
        assert_eq!(host.origin(), &OperationOriginRef::AgentTool);
    }
}
