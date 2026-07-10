use std::sync::Arc;

use agentdash_application::backend::{
    McpProbeBackendTarget, McpProbeBackendTargetResolutionError, resolve_mcp_probe_backend_target,
};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::workspace::WorkspaceDetectionError;
use agentdash_application_ports::backend_transport::{BackendTransport, TransportError};
use agentdash_application_runtime_gateway::{
    CompositeOperationAuthorityResolver, ExtensionOperationProvider,
    ExtensionOperationRuntimeContext, InMemoryOperationResultStore, InteractionCommandOperation,
    InteractionOperationAccess, InteractionOperationProvider, McpOperationProvider,
    McpProbeSetupPort, McpProbeTarget, McpProbeToolOutput, McpProbeTransportInput,
    McpProbeTransportOutput, OperationGateway, RuntimeGatewaySetupError, SetupOperationAccessPort,
    SetupOperationAuthorityResolver, SetupOperationProvider, TracingOperationAuditSink,
    WorkspaceBrowseDirectoryEntry, WorkspaceBrowseDirectoryInput, WorkspaceBrowseDirectoryOutput,
    WorkspaceBrowseDirectorySetupPort, WorkspaceDetectGitInput, WorkspaceDetectGitOutput,
    WorkspaceDetectGitSetupPort, WorkspaceDetectInput, WorkspaceDetectOutput,
    WorkspaceDetectSetupPort, WorkspaceDiscoverByIdentityCandidateOutput,
    WorkspaceDiscoverByIdentityInput, WorkspaceDiscoverByIdentityOutput,
    WorkspaceDiscoverByIdentitySetupPort, WorkspaceDiscoverByIdentitySkippedOutput,
};
use agentdash_spi::AuthIdentity;
use agentdash_spi::platform::mcp_probe::McpProbeTransport;
use agentdash_spi::platform::mcp_relay::{McpRelayProvider, RelayProbeTarget};
use agentdash_workspace_module::runtime_tool_provider::SharedOperationGatewayHandle;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::relay::registry::BackendRegistry;

pub(crate) fn build_operation_gateway(
    mcp_probe_relay: Arc<dyn agentdash_spi::McpRelayProvider>,
    operation_mcp_access: Arc<dyn agentdash_application_runtime_gateway::OperationMcpAccess>,
    runtime_surface_query: Arc<
        dyn agentdash_application_agentrun::agent_run::AgentRunRuntimeSurfaceQueryPort,
    >,
    repos: RepositorySet,
    backend_registry: Arc<BackendRegistry>,
    setup_action_transport: Arc<
        dyn agentdash_application_ports::backend_transport::BackendTransport,
    >,
    gateway_handle: SharedOperationGatewayHandle,
) -> Result<Arc<OperationGateway>, agentdash_application_runtime_gateway::OperationExecutionError> {
    let mcp_probe_setup = Arc::new(ApplicationMcpProbeSetupPort::new(
        Some(mcp_probe_relay),
        McpProbeBackendTargetResolver::new(repos.clone(), backend_registry.clone()),
        Arc::new(agentdash_infrastructure::RmcpProbeTransport::new()),
    ));
    let workspace_setup = Arc::new(ApplicationWorkspaceSetupPort::new(setup_action_transport));

    let setup_provider = Arc::new(SetupOperationProvider::new(
        mcp_probe_setup,
        workspace_setup.clone(),
        workspace_setup.clone(),
        workspace_setup.clone(),
        workspace_setup,
    ));
    let setup_authority: Arc<
        dyn agentdash_application_runtime_gateway::OperationAuthorityResolver,
    > = Arc::new(SetupOperationAuthorityResolver::new(Arc::new(
        ApplicationSetupOperationAccess {
            repos: repos.clone(),
        },
    )));
    let surface_authority: Arc<
        dyn agentdash_application_runtime_gateway::OperationAuthorityResolver,
    > = Arc::new(ApplicationSurfaceOperationAuthority {
        repos: repos.clone(),
    });
    let extension_provider = Arc::new(ExtensionOperationProvider::new(
        repos.project_extension_installation_repo.clone(),
        Arc::new(ApplicationExtensionOperationContext {
            repos: repos.clone(),
            runtime_surface_query,
        }),
        backend_registry.clone(),
        backend_registry.clone(),
        backend_registry,
    ));
    let interaction_provider = Arc::new(InteractionOperationProvider::new(Arc::new(
        ApplicationInteractionOperationAccess {
            repos: repos.clone(),
            gateway_handle,
        },
    )));
    OperationGateway::try_new(
        Arc::new(CompositeOperationAuthorityResolver::new(
            setup_authority,
            surface_authority.clone(),
            surface_authority.clone(),
            surface_authority.clone(),
            surface_authority,
        )),
        [setup_provider as Arc<dyn agentdash_application_runtime_gateway::OperationProvider>],
        [
            Arc::new(McpOperationProvider::new(operation_mcp_access))
                as Arc<dyn agentdash_application_runtime_gateway::DynamicOperationProvider>,
            extension_provider,
            interaction_provider,
        ],
        Arc::new(InMemoryOperationResultStore::default()),
        Arc::new(TracingOperationAuditSink),
    )
    .map(Arc::new)
}

struct ApplicationInteractionOperationAccess {
    repos: RepositorySet,
    gateway_handle: SharedOperationGatewayHandle,
}

#[derive(serde::Deserialize)]
struct InteractionOperationInput {
    instance_id: uuid::Uuid,
    command_id: uuid::Uuid,
    #[serde(default)]
    payload: serde_json::Value,
    expected_state_revision: u64,
}

#[async_trait]
impl InteractionOperationAccess for ApplicationInteractionOperationAccess {
    async fn discover_commands(
        &self,
        principal: &agentdash_application_runtime_gateway::OperationPrincipal,
        scope: &agentdash_application_runtime_gateway::OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<
        Vec<InteractionCommandOperation>,
        agentdash_application_runtime_gateway::OperationExecutionError,
    > {
        use agentdash_application_runtime_gateway::{
            OperationExecutionError, OperationPrincipalRef, OperationScopeRef,
        };
        use agentdash_domain::interaction::{
            CommandActorPolicy, InteractionDefinitionStatus, InteractionOwner,
        };
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let project_id = match &scope.scope_ref {
            OperationScopeRef::Project { project_id }
            | OperationScopeRef::WorkspaceBinding { project_id, .. } => *project_id,
            OperationScopeRef::InteractionInstance { instance_id } => {
                let instance = self
                    .repos
                    .interaction_instance_repo
                    .get(*instance_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                        operation_ref: agentdash_domain::operation::OperationRef::new(
                            "interaction",
                            instance_id.to_string(),
                            "unknown",
                            1,
                        )
                        .expect("static operation ref"),
                    })?;
                match instance.owner {
                    InteractionOwner::Project(project_id) => project_id,
                    InteractionOwner::User(_) => return Ok(vec![]),
                }
            }
            OperationScopeRef::EnvironmentSetup { .. } => return Ok(vec![]),
        };
        let agent = matches!(
            principal.principal_ref(),
            OperationPrincipalRef::AgentRunAgent { .. }
        );
        let definitions = self
            .repos
            .interaction_definition_repo
            .list_canvas_by_project(project_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        let mut commands = Vec::new();
        for definition in definitions {
            if definition.status != InteractionDefinitionStatus::Active
                || !matches!(definition.owner, InteractionOwner::Project(owner) if owner == project_id)
            {
                continue;
            }
            let revision = self
                .repos
                .interaction_definition_repo
                .get_revision(definition.current_revision_id)
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                .ok_or_else(|| OperationExecutionError::NotReady {
                    code: "interaction_revision_missing".into(),
                    message: format!(
                        "Interaction revision 不存在: {}",
                        definition.current_revision_id
                    ),
                })?;
            commands.extend(
                revision
                    .command_definitions
                    .iter()
                    .filter(|command| !agent || command.actor_policy == CommandActorPolicy::Direct)
                    .map(|command| InteractionCommandOperation {
                        definition_id: revision.definition_id,
                        definition_revision_id: revision.revision_id,
                        title: revision.title.clone(),
                        command_key: command.command_key.clone(),
                        actor_policy: command.actor_policy,
                        payload_schema: command.payload_schema.clone(),
                    }),
            );
        }
        Ok(commands)
    }

    async fn invoke_command(
        &self,
        principal: &agentdash_application_runtime_gateway::OperationPrincipal,
        scope: &agentdash_application_runtime_gateway::OperationAuthorizationScope,
        definition_id: uuid::Uuid,
        definition_revision_id: uuid::Uuid,
        command_key: &str,
        input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, agentdash_application_runtime_gateway::OperationExecutionError>
    {
        use agentdash_application::interaction::{
            InteractionCommandCallerContext, InteractionCommandInput, InteractionCommandService,
        };
        use agentdash_application_runtime_gateway::OperationExecutionError;
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let input: InteractionOperationInput = serde_json::from_value(input)
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        let instance = self
            .repos
            .interaction_instance_repo
            .get(input.instance_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
            .ok_or_else(|| OperationExecutionError::NotReady {
                code: "interaction_not_found".into(),
                message: format!("Interaction instance 不存在: {}", input.instance_id),
            })?;
        if instance.definition_id != definition_id {
            return Err(OperationExecutionError::invalid_request(
                "Interaction operation 与 instance definition 不一致",
            ));
        }
        if instance.definition_revision_id != definition_revision_id {
            return Err(OperationExecutionError::invalid_request(
                "Interaction operation exact revision 与 instance pinned revision 不一致",
            ));
        }
        let caller = match principal.principal_ref() {
            agentdash_domain::operation::OperationPrincipalRef::AgentRunAgent {
                run_id,
                agent_id,
            } => InteractionCommandCallerContext::ResolvedAgentRun {
                run_id: *run_id,
                agent_id: *agent_id,
            },
            agentdash_domain::operation::OperationPrincipalRef::User { user_id } => {
                InteractionCommandCallerContext::AuthenticatedUser {
                    user_id: user_id.clone(),
                }
            }
            agentdash_domain::operation::OperationPrincipalRef::WorkflowNode { .. }
            | agentdash_domain::operation::OperationPrincipalRef::ExtensionInstallation {
                ..
            } => {
                return Err(OperationExecutionError::CapabilitiesDenied {
                    missing: vec!["interaction.command.actor".into()],
                });
            }
        };
        let service = InteractionCommandService::new(
            self.repos.interaction_definition_repo.clone(),
            self.repos.interaction_instance_repo.clone(),
            self.repos.interaction_command_transaction.clone(),
            self.repos.interaction_event_repo.clone(),
            Arc::new(InteractionOperationAdmission {
                repos: self.repos.clone(),
                scope: scope.clone(),
            }),
            Arc::new(InteractionOperationEffectAdmission {
                gateway_handle: self.gateway_handle.clone(),
                principal: principal.clone(),
                scope_ref: scope.scope_ref.clone(),
            }),
        );
        let commit = service
            .execute(
                InteractionCommandInput {
                    instance_id: input.instance_id,
                    command_id: input.command_id,
                    command_key: command_key.to_string(),
                    payload: input.payload,
                    expected_state_revision: input.expected_state_revision,
                },
                caller,
                chrono::Utc::now(),
            )
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        let (instance, event, duplicate) = match commit {
            agentdash_domain::interaction::InteractionCommandCommit::Committed {
                instance,
                event,
                ..
            } => (instance, event, false),
            agentdash_domain::interaction::InteractionCommandCommit::Duplicate {
                instance,
                event,
                ..
            } => (instance, event, true),
        };
        Ok(serde_json::json!({
            "instance_id": instance.id,
            "state": instance.state,
            "state_revision": instance.state_revision,
            "event_id": event.id,
            "event_sequence": event.sequence,
            "duplicate": duplicate,
        }))
    }
}

struct InteractionOperationAdmission {
    repos: RepositorySet,
    scope: agentdash_application_runtime_gateway::OperationAuthorizationScope,
}

#[async_trait]
impl agentdash_application::interaction::InteractionCommandAdmissionPort
    for InteractionOperationAdmission
{
    async fn admit(
        &self,
        instance: &agentdash_domain::interaction::InteractionInstance,
        _: &agentdash_application::interaction::InteractionCommandInput,
        caller: &agentdash_application::interaction::InteractionCommandCallerContext,
    ) -> agentdash_application::interaction::InteractionApplicationResult<
        agentdash_application::interaction::InteractionCommandAdmission,
    > {
        use agentdash_application::interaction::{
            InteractionApplicationError, InteractionCommandAdmission,
            InteractionCommandCallerContext,
        };
        use agentdash_domain::interaction::{
            AttachmentSubject, InteractionActor, InteractionCommandOrigin, InteractionOwner,
        };
        match caller {
            InteractionCommandCallerContext::ResolvedAgentRun { run_id, agent_id } => {
                let attachment = self.repos.interaction_instance_repo.list_attachments(instance.id).await?
                    .into_iter().find(|attachment| {
                        attachment.detached_at.is_none()
                            && attachment.capabilities.can_submit_commands
                            && matches!(attachment.subject, AttachmentSubject::AgentRun { run_id: attached } if attached == *run_id)
                    }).ok_or_else(|| InteractionApplicationError::AccessDenied {
                        reason: "AgentRun 没有可提交 command 的 active Interaction attachment".into(),
                    })?;
                Ok(InteractionCommandAdmission {
                    actor: InteractionActor::Agent {
                        agent_id: *agent_id,
                        run_id: Some(*run_id),
                    },
                    origin: InteractionCommandOrigin::AgentFrame,
                    attachment_id: Some(attachment.id),
                    capability_revision_ref: format!(
                        "{}:attachment:{}",
                        self.scope.authority_revision, attachment.id
                    ),
                })
            }
            InteractionCommandCallerContext::AuthenticatedUser { user_id } => {
                if matches!(&instance.owner, InteractionOwner::User(owner) if owner != user_id) {
                    return Err(InteractionApplicationError::AccessDenied {
                        reason: "Interaction 不属于当前用户".into(),
                    });
                }
                Ok(InteractionCommandAdmission {
                    actor: InteractionActor::Human {
                        user_id: user_id.clone(),
                    },
                    origin: InteractionCommandOrigin::UserWorkshop,
                    attachment_id: None,
                    capability_revision_ref: self.scope.authority_revision.clone(),
                })
            }
        }
    }

    async fn admit_close(
        &self,
        _: &agentdash_domain::interaction::InteractionInstance,
        _: &agentdash_application::interaction::InteractionCloseInput,
        _: &agentdash_application::interaction::InteractionCommandCallerContext,
    ) -> agentdash_application::interaction::InteractionApplicationResult<()> {
        Err(
            agentdash_application::interaction::InteractionApplicationError::AccessDenied {
                reason: "Interaction Operation 不提供 close".into(),
            },
        )
    }
}

struct InteractionOperationEffectAdmission {
    gateway_handle: SharedOperationGatewayHandle,
    principal: agentdash_application_runtime_gateway::OperationPrincipal,
    scope_ref: agentdash_domain::operation::OperationScopeRef,
}

#[async_trait]
impl agentdash_application::interaction::InteractionEffectDescriptorAdmissionPort
    for InteractionOperationEffectAdmission
{
    async fn admit_replay_safe(
        &self,
        operation_ref: &agentdash_domain::operation::OperationRef,
    ) -> agentdash_application::interaction::InteractionApplicationResult<
        agentdash_domain::interaction::OperationEffectSafety,
    > {
        use agentdash_application::interaction::InteractionApplicationError;
        use agentdash_application_runtime_gateway::{OperationReadiness, OperationReplayPolicy};
        let gateway = self.gateway_handle.get().await.ok_or_else(|| {
            InteractionApplicationError::ContractUnavailable {
                reason: "canonical OperationGateway 尚未装配".into(),
            }
        })?;
        let origin = match self.principal.principal_ref() {
            agentdash_domain::operation::OperationPrincipalRef::AgentRunAgent { .. } => {
                agentdash_domain::operation::OperationOriginRef::AgentTool
            }
            agentdash_domain::operation::OperationPrincipalRef::User { .. } => {
                agentdash_domain::operation::OperationOriginRef::UserWorkshop
            }
            agentdash_domain::operation::OperationPrincipalRef::WorkflowNode { .. } => {
                agentdash_domain::operation::OperationOriginRef::Workflow
            }
            agentdash_domain::operation::OperationPrincipalRef::ExtensionInstallation {
                installation_id,
            } => agentdash_domain::operation::OperationOriginRef::ExtensionPanel {
                installation_id: *installation_id,
            },
        };
        let surface = gateway
            .surface_current(
                &self.principal,
                &self.scope_ref,
                &origin,
                CancellationToken::new(),
            )
            .await
            .map_err(|error| InteractionApplicationError::ContractUnavailable {
                reason: error.to_string(),
            })?;
        let descriptor = surface.catalog.get(operation_ref).ok_or_else(|| {
            InteractionApplicationError::ContractUnavailable {
                reason: format!("Operation 不在当前 actor surface: {operation_ref:?}"),
            }
        })?;
        if !matches!(descriptor.readiness, OperationReadiness::Ready) {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: "Operation 当前不可执行".into(),
            });
        }
        match descriptor.replay_policy {
            OperationReplayPolicy::ReplaySafe => {
                Ok(agentdash_domain::interaction::OperationEffectSafety::ReplaySafe)
            }
            OperationReplayPolicy::Idempotent => {
                Ok(agentdash_domain::interaction::OperationEffectSafety::Idempotent)
            }
            OperationReplayPolicy::NonReplayable => {
                Err(agentdash_domain::interaction::InteractionError::EffectNotReplaySafe.into())
            }
        }
    }
}

struct ApplicationSetupOperationAccess {
    repos: RepositorySet,
}

struct ApplicationSurfaceOperationAuthority {
    repos: RepositorySet,
}

struct ApplicationExtensionOperationContext {
    repos: RepositorySet,
    runtime_surface_query:
        Arc<dyn agentdash_application_agentrun::agent_run::AgentRunRuntimeSurfaceQueryPort>,
}

#[async_trait]
impl agentdash_application_runtime_gateway::ExtensionOperationContextPort
    for ApplicationExtensionOperationContext
{
    async fn resolve_context(
        &self,
        principal: &agentdash_application_runtime_gateway::OperationPrincipal,
        scope: &agentdash_application_runtime_gateway::OperationAuthorizationScope,
        _origin: &agentdash_application_runtime_gateway::OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<
        ExtensionOperationRuntimeContext,
        agentdash_application_runtime_gateway::OperationExecutionError,
    > {
        use agentdash_application_agentrun::agent_run::RuntimeSurfaceQueryPurpose;
        use agentdash_application_runtime_gateway::{
            OperationExecutionError, OperationPrincipalRef, OperationScopeRef,
        };
        use agentdash_domain::interaction::InteractionOwner;
        use agentdash_domain::workspace::WorkspaceBindingStatus;

        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        if let OperationPrincipalRef::AgentRunAgent { run_id, agent_id } = principal.principal_ref()
        {
            let surface = self
                .runtime_surface_query
                .current_runtime_surface_for_agent_run(
                    *run_id,
                    *agent_id,
                    RuntimeSurfaceQueryPurpose::new("extension_operation"),
                )
                .await
                .map_err(|error| OperationExecutionError::NotReady {
                    code: "agent_extension_surface_unavailable".to_string(),
                    message: error.to_string(),
                })?;
            let backend = surface.runtime_backend_anchor.as_ref().ok_or_else(|| {
                OperationExecutionError::NotReady {
                    code: "extension_backend_unavailable".to_string(),
                    message: "AgentRun current surface 缺少 backend anchor".to_string(),
                }
            })?;
            if agentdash_application_runtime_gateway::scope_project_id(&scope.scope_ref)
                != Some(surface.project_id)
            {
                return Err(OperationExecutionError::CapabilitiesDenied {
                    missing: vec!["agent_run.project_scope".to_string()],
                });
            }
            let workspace = agentdash_application_runtime_gateway::resolve_extension_invocation_workspace(
                &surface.vfs,
                backend,
            )
            .into_workspace()
            .map(|workspace| agentdash_application_ports::extension_runtime::ExtensionInvocationWorkspacePayload {
                mount_id: workspace.mount_id,
                root_ref: workspace.root_ref,
            });
            return Ok(ExtensionOperationRuntimeContext {
                project_id: surface.project_id,
                backend_id: Some(backend.backend_id().to_string()),
                workspace,
            });
        }

        let project_id = match &scope.scope_ref {
            OperationScopeRef::Project { project_id }
            | OperationScopeRef::WorkspaceBinding { project_id, .. } => *project_id,
            OperationScopeRef::InteractionInstance { instance_id } => {
                let instance = self
                    .repos
                    .interaction_instance_repo
                    .get(*instance_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "interaction_not_found".to_string(),
                        message: format!("InteractionInstance 不存在: {instance_id}"),
                    })?;
                match instance.owner {
                    InteractionOwner::Project(project_id) => project_id,
                    InteractionOwner::User(_) => {
                        return Err(OperationExecutionError::NotReady {
                            code: "extension_project_scope_required".to_string(),
                            message: "User-owned Interaction 没有 Project Extension surface"
                                .to_string(),
                        });
                    }
                }
            }
            OperationScopeRef::EnvironmentSetup { .. } => {
                return Err(OperationExecutionError::invalid_request(
                    "Extension Operation 不接受 Setup scope",
                ));
            }
        };
        let OperationScopeRef::WorkspaceBinding { workspace_id, .. } = &scope.scope_ref else {
            return Ok(ExtensionOperationRuntimeContext {
                project_id,
                backend_id: None,
                workspace: None,
            });
        };
        let workspace = self
            .repos
            .workspace_repo
            .get_by_id(*workspace_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
            .filter(|workspace| workspace.project_id == project_id)
            .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                missing: vec!["workspace.project_scope".to_string()],
            })?;
        let binding = workspace
            .default_binding_id
            .and_then(|binding_id| {
                workspace
                    .bindings
                    .iter()
                    .find(|binding| binding.id == binding_id)
            })
            .or_else(|| {
                workspace
                    .bindings
                    .iter()
                    .find(|binding| binding.status == WorkspaceBindingStatus::Ready)
            })
            .filter(|binding| binding.status == WorkspaceBindingStatus::Ready)
            .ok_or_else(|| OperationExecutionError::NotReady {
                code: "workspace_binding_unavailable".to_string(),
                message: format!("Workspace 没有 active binding: {workspace_id}"),
            })?;
        Ok(ExtensionOperationRuntimeContext {
            project_id,
            backend_id: Some(binding.backend_id.clone()),
            workspace: Some(
                agentdash_application_ports::extension_runtime::ExtensionInvocationWorkspacePayload {
                    mount_id: format!("workspace:{workspace_id}"),
                    root_ref: binding.root_ref.clone(),
                },
            ),
        })
    }
}

#[async_trait]
impl agentdash_application_runtime_gateway::OperationAuthorityResolver
    for ApplicationSurfaceOperationAuthority
{
    async fn resolve(
        &self,
        principal: &agentdash_application_runtime_gateway::OperationPrincipal,
        scope: &agentdash_application_runtime_gateway::OperationAuthorizationScope,
        origin: &agentdash_application_runtime_gateway::OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<
        agentdash_application_runtime_gateway::OperationAuthorityGrant,
        agentdash_application_runtime_gateway::OperationExecutionError,
    > {
        use agentdash_application_runtime_gateway::{
            OperationExecutionError, OperationOriginRef, OperationPrincipalRef, OperationScopeRef,
        };
        use agentdash_domain::interaction::InteractionOwner;

        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let mut facts = Vec::new();
        let mut capabilities = std::collections::BTreeSet::from(["operation.invoke".to_string()]);
        match principal.principal_ref() {
            OperationPrincipalRef::User { user_id } => {
                let identity = principal.user_identity().ok_or_else(|| {
                    OperationExecutionError::invalid_request("User principal 缺少认证 identity")
                })?;
                if identity.user_id != *user_id {
                    return Err(OperationExecutionError::CapabilitiesDenied {
                        missing: vec!["principal.user_identity_match".to_string()],
                    });
                }
                match &scope.scope_ref {
                    OperationScopeRef::Project { project_id } => {
                        self.require_project_use(identity, *project_id, &mut facts)
                            .await?;
                    }
                    OperationScopeRef::WorkspaceBinding {
                        project_id,
                        workspace_id,
                    } => {
                        self.require_project_use(identity, *project_id, &mut facts)
                            .await?;
                        self.require_workspace_binding(*project_id, *workspace_id, &mut facts)
                            .await?;
                    }
                    OperationScopeRef::InteractionInstance { instance_id } => {
                        let instance = self
                            .repos
                            .interaction_instance_repo
                            .get(*instance_id)
                            .await
                            .map_err(|error| {
                                OperationExecutionError::provider_failed(error.to_string())
                            })?
                            .ok_or_else(|| OperationExecutionError::NotReady {
                                code: "interaction_not_found".to_string(),
                                message: format!("InteractionInstance 不存在: {instance_id}"),
                            })?;
                        match &instance.owner {
                            InteractionOwner::User(owner) if owner == user_id => {}
                            InteractionOwner::Project(project_id) => {
                                self.require_project_use(identity, *project_id, &mut facts)
                                    .await?;
                            }
                            _ => {
                                return Err(OperationExecutionError::CapabilitiesDenied {
                                    missing: vec!["interaction.use".to_string()],
                                });
                            }
                        }
                        facts.push(format!(
                            "interaction:{}:{}:{}:{}",
                            instance.id,
                            instance.state_revision,
                            instance.status.as_str(),
                            instance.updated_at
                        ));
                        capabilities.insert("interaction.use".to_string());
                    }
                    OperationScopeRef::EnvironmentSetup { .. } => {
                        return Err(OperationExecutionError::invalid_request(
                            "Setup scope 不能进入 standalone authority",
                        ));
                    }
                }
                if let Some(project_id) =
                    agentdash_application_runtime_gateway::scope_project_id(&scope.scope_ref)
                {
                    for installation in self.enabled_installations(project_id).await? {
                        capabilities.insert(format!("extension:{}", installation.extension_key));
                        facts.push(format!(
                            "extension:{}:{}:{}",
                            installation.id, installation.enabled, installation.updated_at
                        ));
                    }
                }
                if let OperationOriginRef::ExtensionPanel { installation_id } = origin {
                    let project_id =
                        agentdash_application_runtime_gateway::scope_project_id(&scope.scope_ref)
                            .ok_or_else(|| {
                            OperationExecutionError::invalid_request(
                                "Extension panel 需要 Project scope",
                            )
                        })?;
                    let _ = self
                        .require_installation(project_id, *installation_id, &mut facts)
                        .await?;
                }
                if let OperationOriginRef::Canvas { definition_id } = origin {
                    let definition = self
                        .repos
                        .interaction_definition_repo
                        .get(*definition_id)
                        .await
                        .map_err(|error| {
                            OperationExecutionError::provider_failed(error.to_string())
                        })?
                        .ok_or_else(|| OperationExecutionError::NotReady {
                            code: "canvas_definition_not_found".to_string(),
                            message: format!("Canvas definition 不存在: {definition_id}"),
                        })?;
                    match &definition.owner {
                        InteractionOwner::User(owner) if owner == user_id => {}
                        InteractionOwner::Project(project_id) => {
                            self.require_project_use(identity, *project_id, &mut facts)
                                .await?;
                            if agentdash_application_runtime_gateway::scope_project_id(
                                &scope.scope_ref,
                            ) != Some(*project_id)
                            {
                                return Err(OperationExecutionError::CapabilitiesDenied {
                                    missing: vec!["canvas.project_scope".to_string()],
                                });
                            }
                        }
                        _ => {
                            return Err(OperationExecutionError::CapabilitiesDenied {
                                missing: vec!["canvas.definition.use".to_string()],
                            });
                        }
                    }
                    facts.push(format!(
                        "canvas-definition:{}:{}:{}",
                        definition.id, definition.current_revision_id, definition.updated_at
                    ));
                }
                facts.push(format!("user:{user_id}"));
            }
            OperationPrincipalRef::AgentRunAgent { run_id, agent_id } => {
                let run = self
                    .repos
                    .lifecycle_run_repo
                    .get_by_id(*run_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "agent_run_missing".to_string(),
                        message: format!("AgentRun 不存在: {run_id}"),
                    })?;
                let agent = self
                    .repos
                    .lifecycle_agent_repo
                    .get(*agent_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .filter(|agent| agent.run_id == run.id && agent.project_id == run.project_id)
                    .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                        missing: vec!["agent_run.membership".to_string()],
                    })?;
                if agent.project_id
                    != agentdash_application_runtime_gateway::scope_project_id(&scope.scope_ref)
                        .ok_or_else(|| {
                            OperationExecutionError::invalid_request(
                                "AgentRun Operation 需要 Project scope",
                            )
                        })?
                {
                    return Err(OperationExecutionError::CapabilitiesDenied {
                        missing: vec!["agent_run.project_scope".to_string()],
                    });
                }
                let frame = self
                    .repos
                    .agent_frame_repo
                    .get_current(*agent_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::NotReady {
                        code: "agent_frame_missing".to_string(),
                        message: format!("Agent current frame 不存在: {agent_id}"),
                    })?;
                facts.push(format!(
                    "agent:{run_id}:{agent_id}:{}:{}",
                    frame.id, frame.revision
                ));
                if let Some(capability_value) = frame.surface_document().capability_state {
                    let capability_state =
                        serde_json::from_value::<agentdash_spi::CapabilityState>(capability_value)
                            .map_err(|error| OperationExecutionError::NotReady {
                                code: "agent_capability_invalid".to_string(),
                                message: error.to_string(),
                            })?;
                    capabilities.extend(capability_state.capability_keys());
                    capabilities.extend(
                        capability_state
                            .tool
                            .mcp_servers
                            .iter()
                            .map(|server| format!("mcp:{}", server.name)),
                    );
                    for installation in self.enabled_installations(run.project_id).await? {
                        if capability_state
                            .workspace_module
                            .allows(&format!("ext:{}", installation.extension_key))
                        {
                            capabilities
                                .insert(format!("extension:{}", installation.extension_key));
                            facts.push(format!(
                                "extension:{}:{}:{}",
                                installation.id, installation.enabled, installation.updated_at
                            ));
                        }
                    }
                }
                capabilities.insert("agent.operation.invoke".to_string());
            }
            OperationPrincipalRef::ExtensionInstallation { installation_id } => {
                let project_id =
                    agentdash_application_runtime_gateway::scope_project_id(&scope.scope_ref)
                        .ok_or_else(|| {
                            OperationExecutionError::invalid_request(
                                "Extension service 需要 Project scope",
                            )
                        })?;
                let installation = self
                    .require_installation(project_id, *installation_id, &mut facts)
                    .await?;
                capabilities.insert(format!("extension:{}", installation.extension_key));
                capabilities.insert("extension.operation.invoke".to_string());
            }
            OperationPrincipalRef::WorkflowNode { .. } => {
                return Err(OperationExecutionError::NotReady {
                    code: "workflow_operation_authority_unavailable".to_string(),
                    message: "Workflow Operation authority 尚未装配".to_string(),
                });
            }
        }
        Ok(
            agentdash_application_runtime_gateway::OperationAuthorityGrant {
                authority_revision: authority_revision(facts),
                capabilities,
            },
        )
    }
}

impl ApplicationSurfaceOperationAuthority {
    async fn enabled_installations(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<
        Vec<agentdash_domain::shared_library::ProjectExtensionInstallation>,
        agentdash_application_runtime_gateway::OperationExecutionError,
    > {
        self.repos
            .project_extension_installation_repo
            .list_enabled_by_project(project_id)
            .await
            .map_err(|error| {
                agentdash_application_runtime_gateway::OperationExecutionError::provider_failed(
                    error.to_string(),
                )
            })
    }

    async fn require_workspace_binding(
        &self,
        project_id: uuid::Uuid,
        workspace_id: uuid::Uuid,
        facts: &mut Vec<String>,
    ) -> Result<(), agentdash_application_runtime_gateway::OperationExecutionError> {
        use agentdash_application_runtime_gateway::OperationExecutionError;
        let workspace = self
            .repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
            .filter(|workspace| workspace.project_id == project_id)
            .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                missing: vec!["workspace.project_scope".to_string()],
            })?;
        facts.push(format!(
            "workspace:{}:{}:{}",
            workspace.id, workspace.project_id, workspace.updated_at
        ));
        Ok(())
    }

    async fn require_project_use(
        &self,
        identity: &AuthIdentity,
        project_id: uuid::Uuid,
        facts: &mut Vec<String>,
    ) -> Result<(), agentdash_application_runtime_gateway::OperationExecutionError> {
        use agentdash_application::project::{
            ProjectAuthorizationService, ProjectPermission,
            project_authorization_context_from_identity,
        };
        use agentdash_application_runtime_gateway::OperationExecutionError;
        let project = self
            .repos
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
            .ok_or_else(|| OperationExecutionError::NotReady {
                code: "project_not_found".to_string(),
                message: format!("Project 不存在: {project_id}"),
            })?;
        let allowed = ProjectAuthorizationService::new(self.repos.project_repo.as_ref())
            .can_access_project(
                &project_authorization_context_from_identity(identity),
                &project,
                ProjectPermission::Use,
            )
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
        if !allowed {
            return Err(OperationExecutionError::CapabilitiesDenied {
                missing: vec!["project.use".to_string()],
            });
        }
        facts.push(format!("project:{project_id}:{}", project.updated_at));
        Ok(())
    }

    async fn require_installation(
        &self,
        project_id: uuid::Uuid,
        installation_id: uuid::Uuid,
        facts: &mut Vec<String>,
    ) -> Result<
        agentdash_domain::shared_library::ProjectExtensionInstallation,
        agentdash_application_runtime_gateway::OperationExecutionError,
    > {
        use agentdash_application_runtime_gateway::OperationExecutionError;
        let installation = self
            .repos
            .project_extension_installation_repo
            .get_by_project_and_id(project_id, installation_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
            .filter(|installation| installation.enabled)
            .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                missing: vec!["extension.installation.enabled".to_string()],
            })?;
        facts.push(format!(
            "extension:{}:{}:{}",
            installation.id, installation.enabled, installation.updated_at
        ));
        Ok(installation)
    }
}

fn authority_revision(mut facts: Vec<String>) -> String {
    facts.sort();
    let mut digest = Sha256::new();
    for fact in facts {
        digest.update(fact.as_bytes());
        digest.update([0]);
    }
    format!("sha256:{:x}", digest.finalize())
}

#[async_trait]
impl SetupOperationAccessPort for ApplicationSetupOperationAccess {
    async fn resolve_access(
        &self,
        identity: &AuthIdentity,
        scope: &agentdash_application_runtime_gateway::OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<
        agentdash_application_runtime_gateway::OperationAuthorityGrant,
        agentdash_application_runtime_gateway::OperationExecutionError,
    > {
        use agentdash_application::backend::{BackendAuthorizationService, BackendPermission};
        use agentdash_application::project::{
            ProjectAuthorizationService, ProjectPermission,
            project_authorization_context_from_identity,
        };
        use agentdash_application_runtime_gateway::{OperationExecutionError, OperationScopeRef};

        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        let OperationScopeRef::EnvironmentSetup {
            project_id,
            workspace_id,
            backend_id,
        } = &scope.scope_ref
        else {
            return Err(OperationExecutionError::invalid_request(
                "Setup authority 需要 EnvironmentSetup scope",
            ));
        };

        let mut revision_facts = vec![format!("user:{}", identity.user_id)];
        if let Some(project_id) = project_id {
            let project = self
                .repos
                .project_repo
                .get_by_id(*project_id)
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                .ok_or_else(|| OperationExecutionError::NotReady {
                    code: "project_not_found".to_string(),
                    message: format!("Project 不存在: {project_id}"),
                })?;
            let allowed = ProjectAuthorizationService::new(self.repos.project_repo.as_ref())
                .can_access_project(
                    &project_authorization_context_from_identity(identity),
                    &project,
                    ProjectPermission::Use,
                )
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
            if !allowed {
                return Err(OperationExecutionError::CapabilitiesDenied {
                    missing: vec!["project.use".to_string()],
                });
            }
            revision_facts.push(format!("project:{project_id}:{}", project.updated_at));
        }

        if let Some(workspace_id) = workspace_id {
            let workspace = self
                .repos
                .workspace_repo
                .get_by_id(*workspace_id)
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                .ok_or_else(|| OperationExecutionError::NotReady {
                    code: "workspace_not_found".to_string(),
                    message: format!("Workspace 不存在: {workspace_id}"),
                })?;
            if project_id.is_some_and(|project_id| workspace.project_id != project_id) {
                return Err(OperationExecutionError::CapabilitiesDenied {
                    missing: vec!["workspace.project_scope".to_string()],
                });
            }
            revision_facts.push(format!("workspace:{workspace_id}:{}", workspace.updated_at));
        }

        let mut capabilities = std::collections::BTreeSet::from(["setup.mcp_probe".to_string()]);
        if let Some(backend_id) = backend_id {
            let backend = BackendAuthorizationService::new(
                self.repos.backend_repo.as_ref(),
                self.repos.project_repo.as_ref(),
                self.repos.project_backend_access_repo.as_ref(),
            )
            .require_backend(identity, backend_id, BackendPermission::View)
            .await
            .map_err(|error| OperationExecutionError::CapabilitiesDenied {
                missing: vec![error.to_string()],
            })?;
            revision_facts.push(format!(
                "backend:{}:{}:{:?}:{:?}:{:?}",
                backend.id,
                backend.enabled,
                backend.owner_user_id,
                backend.visibility,
                backend.share_scope_id
            ));
            if let Some(project_id) = project_id {
                let grant = self
                    .repos
                    .project_backend_access_repo
                    .get_active_for_project_backend(*project_id, backend_id)
                    .await
                    .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?
                    .ok_or_else(|| OperationExecutionError::CapabilitiesDenied {
                        missing: vec!["project.backend.use".to_string()],
                    })?;
                revision_facts.push(format!("grant:{}:{}", grant.id, grant.updated_at));
            }
            capabilities.insert("setup.workspace".to_string());
        }

        revision_facts.sort();
        let mut digest = Sha256::new();
        for fact in revision_facts {
            digest.update(fact.as_bytes());
            digest.update([0]);
        }
        Ok(
            agentdash_application_runtime_gateway::OperationAuthorityGrant {
                authority_revision: format!("sha256:{:x}", digest.finalize()),
                capabilities,
            },
        )
    }
}

struct ApplicationMcpProbeSetupPort {
    relay: Option<Arc<dyn McpRelayProvider>>,
    target_resolver: McpProbeBackendTargetResolver,
    http_probe: Arc<dyn McpProbeTransport>,
}

impl ApplicationMcpProbeSetupPort {
    fn new(
        relay: Option<Arc<dyn McpRelayProvider>>,
        target_resolver: McpProbeBackendTargetResolver,
        http_probe: Arc<dyn McpProbeTransport>,
    ) -> Self {
        Self {
            relay,
            target_resolver,
            http_probe,
        }
    }
}

#[async_trait]
impl McpProbeSetupPort for ApplicationMcpProbeSetupPort {
    async fn probe_transport(
        &self,
        input: McpProbeTransportInput,
    ) -> Result<McpProbeTransportOutput, RuntimeGatewaySetupError> {
        let relay_target = if input.route_policy.uses_relay(&input.transport) {
            match self
                .target_resolver
                .resolve(&input.current_user, &input.probe_target)
                .await
            {
                Ok(target) => Some(target),
                Err(McpProbeBackendTargetResolutionError::Unavailable(message)) => {
                    return Ok(McpProbeTransportOutput::Unsupported { reason: message });
                }
                Err(McpProbeBackendTargetResolutionError::Failed(message)) => {
                    return Err(RuntimeGatewaySetupError::ProviderFailed(message));
                }
            }
        } else {
            None
        };
        let result = agentdash_application::mcp_preset::probe_transport_without_runtime_context(
            &input.transport,
            input.route_policy,
            input.runtime_binding.as_ref(),
            relay_target,
            self.relay.as_deref(),
            self.http_probe.as_ref(),
        )
        .await;
        Ok(match result {
            agentdash_application::mcp_preset::ProbeResult::Ok { latency_ms, tools } => {
                McpProbeTransportOutput::Ok {
                    latency_ms,
                    tools: tools
                        .into_iter()
                        .map(|tool| McpProbeToolOutput {
                            name: tool.name,
                            description: tool.description,
                        })
                        .collect(),
                }
            }
            agentdash_application::mcp_preset::ProbeResult::Error { error } => {
                McpProbeTransportOutput::Error { error }
            }
            agentdash_application::mcp_preset::ProbeResult::Unsupported { reason } => {
                McpProbeTransportOutput::Unsupported { reason }
            }
        })
    }
}

#[derive(Clone)]
struct McpProbeBackendTargetResolver {
    repos: RepositorySet,
    backend_registry: Arc<BackendRegistry>,
}

impl McpProbeBackendTargetResolver {
    fn new(repos: RepositorySet, backend_registry: Arc<BackendRegistry>) -> Self {
        Self {
            repos,
            backend_registry,
        }
    }

    async fn resolve(
        &self,
        identity: &AuthIdentity,
        target: &McpProbeTarget,
    ) -> Result<RelayProbeTarget, McpProbeBackendTargetResolutionError> {
        let online_backend_ids = self.backend_registry.list_online_ids().await;
        let resolved = resolve_mcp_probe_backend_target(
            self.repos.backend_repo.as_ref(),
            self.repos.project_repo.as_ref(),
            self.repos.project_backend_access_repo.as_ref(),
            identity,
            &mcp_probe_backend_target(target),
            &online_backend_ids,
        )
        .await?;
        Ok(RelayProbeTarget {
            backend_id: resolved.backend_id,
        })
    }
}

fn mcp_probe_backend_target(target: &McpProbeTarget) -> McpProbeBackendTarget {
    match target {
        McpProbeTarget::DefaultUserLocal => McpProbeBackendTarget::DefaultUserLocal,
        McpProbeTarget::Backend { backend_id } => McpProbeBackendTarget::Backend {
            backend_id: backend_id.clone(),
        },
    }
}

struct ApplicationWorkspaceSetupPort {
    transport: Arc<dyn BackendTransport>,
}

impl ApplicationWorkspaceSetupPort {
    fn new(transport: Arc<dyn BackendTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl WorkspaceDetectSetupPort for ApplicationWorkspaceSetupPort {
    async fn detect_workspace(
        &self,
        input: WorkspaceDetectInput,
    ) -> Result<WorkspaceDetectOutput, RuntimeGatewaySetupError> {
        let result = agentdash_application::workspace::detect_workspace_from_backend(
            self.transport.as_ref(),
            &input.backend_id,
            &input.root_ref,
        )
        .await
        .map_err(|error| match error {
            WorkspaceDetectionError::BadRequest(message) => {
                RuntimeGatewaySetupError::BadRequest(message)
            }
            WorkspaceDetectionError::BackendOffline(message) => {
                RuntimeGatewaySetupError::BackendOffline(message)
            }
            WorkspaceDetectionError::TransportFailed(message) => {
                RuntimeGatewaySetupError::TransportFailed(message)
            }
        })?;

        Ok(WorkspaceDetectOutput {
            identity_kind: result.identity_kind,
            identity_payload: result.identity_payload,
            binding: result.binding,
            confidence: result.confidence,
            warnings: result.warnings,
        })
    }
}

#[async_trait]
impl WorkspaceDetectGitSetupPort for ApplicationWorkspaceSetupPort {
    async fn detect_git(
        &self,
        input: WorkspaceDetectGitInput,
    ) -> Result<WorkspaceDetectGitOutput, RuntimeGatewaySetupError> {
        let backend_id = require_backend_id(&input.backend_id)?;
        let root_ref = require_root_ref(&input.root_ref)?;
        ensure_backend_online(self.transport.as_ref(), backend_id).await?;

        let info = self
            .transport
            .detect_git_repo(backend_id, root_ref)
            .await
            .map_err(setup_error_from_transport)?;
        Ok(WorkspaceDetectGitOutput {
            resolved_root_ref: root_ref.to_string(),
            is_git_repo: info.is_git_repo,
            source_repo: info.source_repo,
            branch: info.branch,
            commit_hash: info.commit_hash,
        })
    }
}

#[async_trait]
impl WorkspaceBrowseDirectorySetupPort for ApplicationWorkspaceSetupPort {
    async fn browse_directory(
        &self,
        input: WorkspaceBrowseDirectoryInput,
    ) -> Result<WorkspaceBrowseDirectoryOutput, RuntimeGatewaySetupError> {
        let backend_id = require_backend_id(&input.backend_id)?;
        ensure_backend_online(self.transport.as_ref(), backend_id).await?;

        let result = self
            .transport
            .browse_directory(backend_id, input.path.as_deref())
            .await
            .map_err(setup_error_from_transport)?;
        Ok(WorkspaceBrowseDirectoryOutput {
            current_path: result.current_path,
            entries: result
                .entries
                .into_iter()
                .map(|entry| WorkspaceBrowseDirectoryEntry {
                    name: entry.name,
                    path: entry.path,
                    is_dir: entry.is_dir,
                })
                .collect(),
        })
    }
}

#[async_trait]
impl WorkspaceDiscoverByIdentitySetupPort for ApplicationWorkspaceSetupPort {
    async fn discover_by_identity(
        &self,
        input: WorkspaceDiscoverByIdentityInput,
    ) -> Result<WorkspaceDiscoverByIdentityOutput, RuntimeGatewaySetupError> {
        let backend_id = require_backend_id(&input.backend_id)?;
        ensure_backend_online(self.transport.as_ref(), backend_id).await?;

        let result = self
            .transport
            .discover_workspace_by_identity(
                backend_id,
                input
                    .workspaces
                    .into_iter()
                    .map(|workspace| {
                        agentdash_application_ports::backend_transport::WorkspaceIdentityDiscoveryRequest {
                            workspace_id: workspace.workspace_id,
                            identity_kind: workspace.identity_kind,
                            identity_payload: workspace.identity_payload,
                        }
                    })
                    .collect(),
            )
            .await
            .map_err(setup_error_from_transport)?;

        Ok(WorkspaceDiscoverByIdentityOutput {
            candidates: result
                .candidates
                .into_iter()
                .map(|candidate| WorkspaceDiscoverByIdentityCandidateOutput {
                    workspace_id: candidate.workspace_id,
                    root_ref: candidate.root_ref,
                    identity_kind: candidate.identity_kind,
                    identity_payload: candidate.identity_payload,
                    detected_facts: candidate.detected_facts,
                    confidence: candidate.confidence,
                    display_name: candidate.display_name,
                    client_name: candidate.client_name,
                    server_address: candidate.server_address,
                    stream: candidate.stream,
                    warnings: candidate.warnings,
                })
                .collect(),
            skipped: result
                .skipped
                .into_iter()
                .map(|skipped| WorkspaceDiscoverByIdentitySkippedOutput {
                    workspace_id: skipped.workspace_id,
                    identity_kind: skipped.identity_kind,
                    reason: skipped.reason,
                    message: skipped.message,
                })
                .collect(),
            warnings: result.warnings,
        })
    }
}

fn require_backend_id(raw: &str) -> Result<&str, RuntimeGatewaySetupError> {
    let backend_id = raw.trim();
    if backend_id.is_empty() {
        return Err(RuntimeGatewaySetupError::BadRequest(
            "backend_id 不能为空".to_string(),
        ));
    }
    Ok(backend_id)
}

fn require_root_ref(raw: &str) -> Result<&str, RuntimeGatewaySetupError> {
    let root_ref = raw.trim();
    if root_ref.is_empty() {
        return Err(RuntimeGatewaySetupError::BadRequest(
            "root_ref 不能为空".to_string(),
        ));
    }
    Ok(root_ref)
}

async fn ensure_backend_online(
    transport: &dyn BackendTransport,
    backend_id: &str,
) -> Result<(), RuntimeGatewaySetupError> {
    if !transport.is_online(backend_id).await {
        return Err(RuntimeGatewaySetupError::BackendOffline(format!(
            "目标 Backend 当前不在线: {backend_id}"
        )));
    }
    Ok(())
}

fn setup_error_from_transport(error: TransportError) -> RuntimeGatewaySetupError {
    match error {
        TransportError::BackendOffline(message) => {
            RuntimeGatewaySetupError::BackendOffline(message)
        }
        TransportError::OperationFailed(message) => {
            RuntimeGatewaySetupError::TransportFailed(message)
        }
        TransportError::Timeout => RuntimeGatewaySetupError::Timeout,
    }
}
