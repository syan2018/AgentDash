use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::{
    BusinessFrameSurfaceQuery, RuntimeSurfaceQueryPurpose,
};
use agentdash_application_ports::agent_run_surface::{
    AgentRunAdmissionRequest, AgentRunEffectiveCapabilityPort,
};
use agentdash_domain::common::AgentConfig;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_infrastructure::persistence::postgres::PostgresToolBrokerRepository;
use agentdash_integration_api::*;
use agentdash_spi::{
    AgentFrameHookEvaluationQuery, AgentFrameHookSnapshotQuery, DynAgentTool, ExecutionContext,
    ExecutionHookProvider, ExecutionSessionFrame, ExecutionTurnFrame, HookControlTarget,
    HookTrigger, RuntimeAdapterProvenance, connector::RuntimeToolProvider,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use super::agent_runtime::{
    AgentRunPlatformToolBrokerResolver, AgentRunRuntimeSurfaceSourceError,
    NativeAgentRunSurfaceCompiler, NativeAgentRunSurfacePlan,
};

#[derive(Clone)]
pub struct CompiledAgentRunToolBinding {
    pub runtime_session_id: String,
    pub run_id: uuid::Uuid,
    pub agent_id: uuid::Uuid,
    pub frame_id: uuid::Uuid,
    pub catalog: agentdash_agent_runtime::ToolCatalogRevision,
    pub tools: BTreeMap<String, DynAgentTool>,
}

pub struct CanonicalAgentRuntimeHookCallback {
    runtime: Arc<
        agentdash_agent_runtime::ManagedAgentRuntime<
            agentdash_infrastructure::PostgresRuntimeRepository,
        >,
    >,
    provider: Arc<dyn ExecutionHookProvider>,
    registry: Arc<CompiledAgentRunToolRegistry>,
}

impl CanonicalAgentRuntimeHookCallback {
    pub fn new(
        runtime: Arc<
            agentdash_agent_runtime::ManagedAgentRuntime<
                agentdash_infrastructure::PostgresRuntimeRepository,
            >,
        >,
        provider: Arc<dyn ExecutionHookProvider>,
        registry: Arc<CompiledAgentRunToolRegistry>,
    ) -> Self {
        Self {
            runtime,
            provider,
            registry,
        }
    }
}

#[async_trait]
impl AgentRuntimeHookCallback for CanonicalAgentRuntimeHookCallback {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        let binding = self
            .registry
            .get(&request.binding_id)
            .await
            .ok_or(DriverHookCallbackError::Stale)?;
        let trigger = match request.point {
            HookPoint::BeforeTool => HookTrigger::BeforeTool,
            HookPoint::AfterTool => HookTrigger::AfterTool,
            _ => {
                return Err(DriverHookCallbackError::ProtocolViolation {
                    reason: format!("canonical hook route does not support {:?}", request.point),
                });
            }
        };
        let correlation_key = request
            .item_id
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| request.turn_id.as_ref().map(ToString::to_string))
            .unwrap_or_else(|| request.thread_id.to_string());
        let hook_run_id = HookRunId::new(format!(
            "hook-{}-{}",
            request.definition_id, correlation_key
        ))
        .map_err(|error| DriverHookCallbackError::ProtocolViolation {
            reason: error.to_string(),
        })?;
        let admission = self
            .runtime
            .accept_hook(agentdash_agent_runtime::RuntimeHookInvocation {
                hook_run_id: hook_run_id.clone(),
                thread_id: request.thread_id.clone(),
                definition_id: request.definition_id.clone(),
                point: request.point,
                correlation: agentdash_agent_runtime::HookCorrelation {
                    operation_id: None,
                    turn_id: request.turn_id.clone(),
                    item_id: request.item_id.clone(),
                    interaction_id: None,
                },
                input: request.payload.clone(),
            })
            .await
            .map_err(hook_callback_error)?;
        if matches!(
            admission,
            agentdash_agent_runtime::HookAdmission::SilentObserver
        ) {
            return Ok(DriverHookDecision::Continue {
                payload: request.payload,
            });
        }
        self.runtime
            .start_hook(&hook_run_id)
            .await
            .map_err(hook_callback_error)?;
        let resolution = self
            .provider
            .evaluate_frame_hook(AgentFrameHookEvaluationQuery {
                target: HookControlTarget {
                    run_id: binding.run_id,
                    agent_id: binding.agent_id,
                    frame_id: binding.frame_id,
                },
                provenance: RuntimeAdapterProvenance::runtime_session(
                    binding.runtime_session_id,
                    request.turn_id.as_ref().map(ToString::to_string),
                    "canonical_agent_runtime_hook",
                ),
                trigger,
                tool_name: request
                    .payload
                    .get("tool_name")
                    .or_else(|| request.payload.get("toolName"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                tool_call_id: request.item_id.as_ref().map(ToString::to_string),
                subagent_type: None,
                snapshot: None,
                payload: Some(request.payload.clone()),
                token_stats: None,
            })
            .await
            .map_err(|error| DriverHookCallbackError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        if let Some(approval) = resolution.approval_request {
            let interaction_id = RuntimeInteractionId::new(format!("interaction-{hook_run_id}"))
                .map_err(|error| DriverHookCallbackError::ProtocolViolation {
                    reason: error.to_string(),
                })?;
            self.runtime
                .request_hook_interaction(
                    &hook_run_id,
                    interaction_id.clone(),
                    approval.reason.clone(),
                )
                .await
                .map_err(hook_callback_error)?;
            return Ok(DriverHookDecision::InteractionRequired {
                interaction_id,
                reason: approval.reason,
            });
        }
        let effects = resolution
            .effects
            .into_iter()
            .enumerate()
            .map(|(index, effect)| {
                let effect_id = HookEffectId::new(format!("effect-{hook_run_id}-{index}"))
                    .expect("derived hook effect id");
                let payload_digest =
                    agentdash_agent_runtime::hook_effect_payload_digest(&effect.payload);
                agentdash_agent_runtime::HookEffect {
                    effect_id,
                    hook_run_id: hook_run_id.clone(),
                    thread_id: request.thread_id.clone(),
                    idempotency_key: format!("{hook_run_id}:{index}"),
                    descriptor: agentdash_agent_runtime::HookEffectDescriptor {
                        effect_type: effect.kind,
                        schema_version: 1,
                        target_authority: "agentdash_hook_effect_dispatcher".to_string(),
                        retry_limit: 3,
                        payload_digest,
                    },
                    payload: effect.payload,
                }
            })
            .collect::<Vec<_>>();
        if let Some(reason) = resolution.block_reason {
            self.runtime
                .complete_hook(
                    &hook_run_id,
                    agentdash_agent_runtime::HookCompletion {
                        status: agentdash_agent_runtime::HookRunStatus::Blocked,
                        decision: HookRunDecision::Block,
                        message: Some(reason.clone()),
                    },
                    effects,
                )
                .await
                .map_err(hook_callback_error)?;
            return Ok(DriverHookDecision::Block { reason });
        }
        self.runtime
            .complete_hook(
                &hook_run_id,
                agentdash_agent_runtime::HookCompletion {
                    status: agentdash_agent_runtime::HookRunStatus::Completed,
                    decision: HookRunDecision::Continue,
                    message: None,
                },
                effects,
            )
            .await
            .map_err(hook_callback_error)?;
        Ok(DriverHookDecision::Continue {
            payload: resolution.rewritten_tool_input.unwrap_or(request.payload),
        })
    }
}

fn hook_callback_error(error: impl ToString) -> DriverHookCallbackError {
    DriverHookCallbackError::ProtocolViolation {
        reason: error.to_string(),
    }
}

pub struct RegistryToolBrokerPolicy {
    registry: Arc<CompiledAgentRunToolRegistry>,
    capabilities: Arc<dyn AgentRunEffectiveCapabilityPort>,
}

impl RegistryToolBrokerPolicy {
    pub fn new(
        registry: Arc<CompiledAgentRunToolRegistry>,
        capabilities: Arc<dyn AgentRunEffectiveCapabilityPort>,
    ) -> Self {
        Self {
            registry,
            capabilities,
        }
    }
}

#[async_trait]
impl agentdash_agent_runtime::ToolBrokerPolicyPort for RegistryToolBrokerPolicy {
    async fn validate_binding(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        self.registry
            .get(&invocation.coordinates.binding_id)
            .await
            .ok_or(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates)?;
        Ok(agentdash_agent_runtime::ToolGuardDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
        ))
    }

    async fn authorize_capability(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        let binding = self
            .registry
            .get(&invocation.coordinates.binding_id)
            .await
            .ok_or(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates)?;
        let decision = self
            .capabilities
            .admit_tool(AgentRunAdmissionRequest::tool(
                binding.runtime_session_id,
                tool.capability_key.clone(),
                tool.runtime_name.clone(),
                None,
            ))
            .await
            .map_err(|error| {
                agentdash_agent_runtime::ToolBrokerError::Execution(error.to_string())
            })?;
        Ok(if decision.allowed {
            agentdash_agent_runtime::ToolGuardDecision::Allowed(
                agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
            )
        } else {
            agentdash_agent_runtime::ToolGuardDecision::Denied {
                reason: decision
                    .reason
                    .unwrap_or_else(|| "AgentFrame capability denied the tool".to_string()),
            }
        })
    }

    async fn authorize_permission(
        &self,
        _invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        _tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<
        agentdash_agent_runtime::ToolPermissionDecision,
        agentdash_agent_runtime::ToolBrokerError,
    > {
        Ok(agentdash_agent_runtime::ToolPermissionDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
        ))
    }

    async fn authorize_vfs(
        &self,
        _invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        _tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        Ok(agentdash_agent_runtime::ToolGuardDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
        ))
    }
}

struct EmbeddedToolCredentialResolver;

#[async_trait]
impl agentdash_agent_runtime::ToolCredentialResolver for EmbeddedToolCredentialResolver {
    async fn resolve(
        &self,
        credential_refs: &[String],
    ) -> Result<agentdash_agent_runtime::CredentialMaterial, agentdash_agent_runtime::ToolBrokerError>
    {
        if !credential_refs.is_empty() {
            return Err(agentdash_agent_runtime::ToolBrokerError::Credential(
                "embedded platform tools cannot receive broker credential material".to_string(),
            ));
        }
        Ok(agentdash_agent_runtime::CredentialMaterial::new(
            BTreeMap::new(),
        ))
    }
}

struct RegistryToolExecutor {
    registry: Arc<CompiledAgentRunToolRegistry>,
}

#[async_trait]
impl agentdash_agent_runtime::ToolExecutionPort for RegistryToolExecutor {
    async fn execute(
        &self,
        request: agentdash_agent_runtime::ToolExecutionRequest,
    ) -> Result<agentdash_agent_runtime::ToolBrokerResult, agentdash_agent_runtime::ToolBrokerError>
    {
        let binding = self
            .registry
            .get(&request.invocation.coordinates.binding_id)
            .await
            .ok_or(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates)?;
        let tool = binding
            .tools
            .get(&request.invocation.tool_name)
            .ok_or_else(|| {
                agentdash_agent_runtime::ToolBrokerError::UnknownTool(
                    request.invocation.tool_name.clone(),
                )
            })?;
        let result = tool
            .execute(
                request.idempotency_key.as_str(),
                request.invocation.arguments,
                request.cancellation,
                None,
            )
            .await
            .map_err(|error| {
                agentdash_agent_runtime::ToolBrokerError::Execution(error.to_string())
            })?;
        let output = result.details.unwrap_or_else(|| {
            serde_json::to_value(result.content).unwrap_or_else(
                |error| serde_json::json!({"serialization_error": error.to_string()}),
            )
        });
        Ok(agentdash_agent_runtime::ToolBrokerResult {
            output,
            is_error: result.is_error,
        })
    }
}

pub struct PostgresAgentRunToolBrokerResolver {
    registry: Arc<CompiledAgentRunToolRegistry>,
    repository: Arc<PostgresToolBrokerRepository>,
    journal: Arc<
        agentdash_agent_runtime::ManagedRuntimeToolJournal<
            agentdash_infrastructure::PostgresRuntimeRepository,
        >,
    >,
    policy: Arc<RegistryToolBrokerPolicy>,
    executor: Arc<RegistryToolExecutor>,
}

impl PostgresAgentRunToolBrokerResolver {
    pub fn new(
        pool: sqlx::PgPool,
        runtime_repository: Arc<agentdash_infrastructure::PostgresRuntimeRepository>,
        registry: Arc<CompiledAgentRunToolRegistry>,
        capabilities: Arc<dyn AgentRunEffectiveCapabilityPort>,
    ) -> Self {
        Self {
            repository: Arc::new(PostgresToolBrokerRepository::new(pool)),
            journal: Arc::new(agentdash_agent_runtime::ManagedRuntimeToolJournal::new(
                runtime_repository,
            )),
            policy: Arc::new(RegistryToolBrokerPolicy::new(
                registry.clone(),
                capabilities,
            )),
            executor: Arc::new(RegistryToolExecutor {
                registry: registry.clone(),
            }),
            registry,
        }
    }
}

#[async_trait]
impl AgentRunPlatformToolBrokerResolver for PostgresAgentRunToolBrokerResolver {
    async fn resolve(
        &self,
        request: &DriverToolInvocation,
    ) -> Result<agentdash_agent_runtime::PlatformToolBroker, DriverToolCallbackError> {
        let binding = self
            .registry
            .get(&request.binding_id)
            .await
            .ok_or(DriverToolCallbackError::Stale)?;
        if binding.catalog.revision != request.tool_set_revision {
            return Err(DriverToolCallbackError::Stale);
        }
        Ok(agentdash_agent_runtime::PlatformToolBroker::new(
            binding.catalog,
            request.binding_id.clone(),
            request.generation,
            agentdash_agent_runtime::PlatformToolBrokerDeps {
                repository: self.repository.clone(),
                journal: self.journal.clone(),
                policy: self.policy.clone(),
                credentials: Arc::new(EmbeddedToolCredentialResolver),
                executor: self.executor.clone(),
            },
        ))
    }
}

#[derive(Default)]
pub struct CompiledAgentRunToolRegistry {
    bindings: RwLock<BTreeMap<RuntimeBindingId, CompiledAgentRunToolBinding>>,
}

impl CompiledAgentRunToolRegistry {
    pub async fn put(
        &self,
        binding_id: RuntimeBindingId,
        binding: CompiledAgentRunToolBinding,
    ) -> Result<(), AgentRunRuntimeSurfaceSourceError> {
        let mut bindings = self.bindings.write().await;
        if let Some(existing) = bindings.get(&binding_id) {
            if existing.catalog != binding.catalog
                || existing.runtime_session_id != binding.runtime_session_id
                || existing.run_id != binding.run_id
                || existing.agent_id != binding.agent_id
                || existing.frame_id != binding.frame_id
            {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: "binding-scoped tool catalog is immutable".to_string(),
                });
            }
            return Ok(());
        }
        bindings.insert(binding_id, binding);
        Ok(())
    }

    pub async fn get(&self, binding_id: &RuntimeBindingId) -> Option<CompiledAgentRunToolBinding> {
        self.bindings.read().await.get(binding_id).cloned()
    }
}

pub struct AgentFrameNativeSurfaceCompiler {
    surface_query: Arc<BusinessFrameSurfaceQuery>,
    frame_repository: Arc<dyn AgentFrameRepository>,
    runtime_tools: Arc<dyn RuntimeToolProvider>,
    hooks: Arc<dyn ExecutionHookProvider>,
    tool_registry: Arc<CompiledAgentRunToolRegistry>,
}

impl AgentFrameNativeSurfaceCompiler {
    pub fn new(
        surface_query: Arc<BusinessFrameSurfaceQuery>,
        frame_repository: Arc<dyn AgentFrameRepository>,
        runtime_tools: Arc<dyn RuntimeToolProvider>,
        hooks: Arc<dyn ExecutionHookProvider>,
        tool_registry: Arc<CompiledAgentRunToolRegistry>,
    ) -> Self {
        Self {
            surface_query,
            frame_repository,
            runtime_tools,
            hooks,
            tool_registry,
        }
    }
}

#[async_trait]
impl NativeAgentRunSurfaceCompiler for AgentFrameNativeSurfaceCompiler {
    async fn compile(
        &self,
        request: &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        binding_id: &RuntimeBindingId,
    ) -> Result<NativeAgentRunSurfacePlan, AgentRunRuntimeSurfaceSourceError> {
        let surface = self
            .surface_query
            .surface_for_provision_target(
                &request.target,
                thread_id,
                RuntimeSurfaceQueryPurpose::new("canonical_agent_runtime_surface"),
            )
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: false,
            })?;
        let frame = self
            .frame_repository
            .get_current(request.target.agent_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentRun has no current AgentFrame".to_string(),
            })?;
        if frame.id != surface.current_surface_frame_id {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "surface query and AgentFrame repository returned different revisions"
                    .to_string(),
            });
        }
        let execution_profile = frame
            .surface
            .as_ref()
            .and_then(|document| document.execution_profile.clone())
            .or_else(|| frame.execution_profile_json.clone())
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentFrame has no execution profile".to_string(),
            })?;
        let executor: AgentConfig = serde_json::from_value(execution_profile).map_err(|error| {
            AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: format!("AgentFrame execution profile is invalid: {error}"),
            }
        })?;
        let provider = executor
            .provider_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "Native Agent execution profile requires provider_id".to_string(),
            })?;
        let model = executor
            .model_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "Native Agent execution profile requires model_id".to_string(),
            })?;
        let working_directory = surface
            .vfs
            .default_mount()
            .map(|mount| PathBuf::from(mount.root_ref.trim()))
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentRun VFS has no usable default mount".to_string(),
            })?;
        let execution_context = ExecutionContext {
            session: ExecutionSessionFrame {
                turn_id: surface.active_turn_id.clone().unwrap_or_else(|| {
                    format!("surface-bootstrap-{}", surface.current_surface_frame_id)
                }),
                working_directory,
                environment_variables: Default::default(),
                executor_config: executor,
                mcp_servers: surface.mcp_servers.clone(),
                vfs: Some(surface.vfs.clone()),
                vfs_access_policy: Some(surface.vfs_access_policy.clone()),
                backend_execution: None,
                runtime_backend_anchor: surface.runtime_backend_anchor.clone(),
                identity: request.identity.clone().or(surface.identity.clone()),
            },
            turn: ExecutionTurnFrame {
                capability_state: surface.capability_state.clone(),
                ..Default::default()
            },
        };
        let tools = self
            .runtime_tools
            .build_tools(&execution_context)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        let revision = u64::try_from(surface.surface_revision).map_err(|_| {
            AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentFrame surface revision must be positive".to_string(),
            }
        })?;
        if revision == 0 {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentFrame surface revision must be positive".to_string(),
            });
        }
        let tool_set_revision = ToolSetRevision(revision);
        let mut direct_tools = BTreeMap::new();
        let mut driver_tools = Vec::new();
        let mut catalog_tools = Vec::new();
        for tool in tools {
            let name = tool.name().trim().to_string();
            if name.is_empty() || direct_tools.insert(name.clone(), tool.clone()).is_some() {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: format!("assembled runtime tool name is empty or duplicated: {name}"),
                });
            }
            let capability_key = capability_for_tool(&surface.capability_state, &name)?;
            let parameters_schema = tool.parameters_schema();
            driver_tools.push(DriverToolDefinition {
                name: name.clone(),
                description: tool.description().to_string(),
                parameters_schema: parameters_schema.clone(),
                channels: vec![ToolChannel::DirectCallback],
            });
            catalog_tools.push(agentdash_agent_runtime::ToolContribution {
                meta: agentdash_agent_runtime::ContributionMeta {
                    key: format!("tool:{capability_key}:{name}"),
                    source: agentdash_agent_runtime::SurfaceSourceRef {
                        layer: "agent_frame".to_string(),
                        key: surface.current_surface_frame_id.to_string(),
                    },
                    priority: 0,
                    requirement: agentdash_agent_runtime::ContributionRequirement::Required,
                },
                runtime_name: name.clone(),
                description: tool.description().to_string(),
                parameters_schema,
                capability_key: capability_key.clone(),
                tool_path: format!("{capability_key}::{name}"),
                allowed_channels: [ToolChannel::DirectCallback].into(),
                configuration_boundary: ConfigurationBoundary::Binding,
            });
        }
        driver_tools.sort_by(|left, right| left.name.cmp(&right.name));
        catalog_tools.sort_by(|left, right| left.runtime_name.cmp(&right.runtime_name));
        let catalog_digest = digest_json(&(tool_set_revision, &catalog_tools))?;
        let catalog = agentdash_agent_runtime::ToolCatalogRevision {
            revision: tool_set_revision,
            digest: catalog_digest.clone(),
            tools: catalog_tools,
            mcp_servers: Vec::new(),
        };
        self.tool_registry
            .put(
                binding_id.clone(),
                CompiledAgentRunToolBinding {
                    runtime_session_id: surface.runtime_session_id.clone(),
                    run_id: request.target.run_id,
                    agent_id: request.target.agent_id,
                    frame_id: frame.id,
                    catalog,
                    tools: direct_tools,
                },
            )
            .await?;

        let hook_snapshot = self
            .hooks
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: HookControlTarget {
                    run_id: request.target.run_id,
                    agent_id: request.target.agent_id,
                    frame_id: frame.id,
                },
                provenance: RuntimeAdapterProvenance::runtime_session(
                    surface.runtime_session_id.clone(),
                    surface.active_turn_id.clone(),
                    "canonical_agent_runtime_surface",
                ),
            })
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        let instructions = hook_snapshot
            .injections
            .iter()
            .map(|injection| injection.content.clone())
            .collect::<Vec<_>>();
        let context_value = frame
            .surface
            .as_ref()
            .and_then(|document| document.context_slice.clone())
            .or_else(|| frame.context_slice_json.clone())
            .unwrap_or(serde_json::Value::Null);
        let recipe = ContextRecipe {
            revision: ContextRecipeRevision(revision),
            provenance: ContextProvenance {
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision,
            },
            source_item_ids: Vec::new(),
        };
        let blocks = vec![ContextBlock::Input {
            input: vec![RuntimeInput::Structured {
                schema: "agentdash.agent_frame.context_slice.v1".to_string(),
                value: context_value,
            }],
        }];
        let context_digest = ContextDigest::new(digest_json(&(&recipe, &instructions, &blocks))?)
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        })?;
        let hook_bindings = canonical_hook_bindings();
        let hook_digest =
            HookPlanDigest::new(digest_json(&(revision, &hook_bindings))?).map_err(|error| {
                AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: error.to_string(),
                }
            })?;
        let workspace_capabilities = workspace_capabilities(&surface.vfs);
        let workspace_roots = surface
            .vfs
            .mounts
            .iter()
            .map(|mount| mount.root_ref.clone())
            .collect::<Vec<_>>();
        let surface_revision = SurfaceRevision(revision);
        let surface_digest = SurfaceDigest::new(digest_json(&(
            surface_revision,
            &context_digest,
            &catalog_digest,
            &hook_digest,
            &workspace_capabilities,
            &workspace_roots,
        ))?)
        .map_err(|error| AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        })?;
        Ok(NativeAgentRunSurfacePlan {
            provider,
            model,
            surface: MaterializedDriverSurface {
                runtime_thread_id: thread_id.clone(),
                revision: surface_revision,
                digest: surface_digest,
                authorization_identity: request.identity.clone().or(surface.identity),
                context: DriverContextSurface {
                    recipe,
                    instructions: vec![DriverInstructionSet {
                        channel: InstructionChannel::System,
                        entries: instructions,
                    }],
                    blocks,
                    digest: context_digest,
                    fidelity: ContextFidelity::PlatformExact,
                },
                tools: DriverToolSurface {
                    revision: tool_set_revision,
                    digest: catalog_digest,
                    tools: driver_tools,
                },
                hooks: DriverHookSurface {
                    revision: HookPlanRevision(1),
                    digest: hook_digest,
                    artifact_digest: None,
                    configuration_boundary: ConfigurationBoundary::Binding,
                    bindings: hook_bindings,
                },
                workspace: DriverWorkspaceSurface {
                    capabilities: workspace_capabilities,
                    roots: workspace_roots,
                },
            },
        })
    }
}

fn capability_for_tool(
    state: &agentdash_spi::CapabilityState,
    tool_name: &str,
) -> Result<String, AgentRunRuntimeSurfaceSourceError> {
    if let Some((key, _)) = state
        .tool
        .tool_policy
        .iter()
        .find(|(_, filter)| filter.allows(tool_name))
    {
        return Ok(key.clone());
    }
    if state.tool.capabilities.len() == 1 {
        return Ok(state
            .tool
            .capabilities
            .iter()
            .next()
            .expect("one capability")
            .key()
            .to_string());
    }
    Err(AgentRunRuntimeSurfaceSourceError::Invalid {
        reason: format!(
            "assembled tool `{tool_name}` has no unambiguous AgentFrame capability identity"
        ),
    })
}

fn canonical_hook_bindings() -> Vec<DriverHookBinding> {
    vec![
        DriverHookBinding {
            definition_id: HookDefinitionId::new("agentdash.platform.before_tool")
                .expect("static hook definition"),
            point: HookPoint::BeforeTool,
            actions: vec![
                HookAction::RewriteInput,
                HookAction::Block,
                HookAction::RequestApproval,
                HookAction::EmitEffect,
            ],
            strength: SemanticStrength::ExactSynchronous,
            failure_policy: HookFailurePolicy::FailClosed,
            required: true,
        },
        DriverHookBinding {
            definition_id: HookDefinitionId::new("agentdash.platform.after_tool")
                .expect("static hook definition"),
            point: HookPoint::AfterTool,
            actions: vec![HookAction::RewriteResult, HookAction::EmitEffect],
            strength: SemanticStrength::ExactSynchronous,
            failure_policy: HookFailurePolicy::FailOpenWithDiagnostic,
            required: true,
        },
    ]
}

fn workspace_capabilities(vfs: &agentdash_spi::Vfs) -> Vec<WorkspaceCapability> {
    let mut values = BTreeSet::new();
    for mount in &vfs.mounts {
        for capability in &mount.capabilities {
            match capability {
                agentdash_domain::common::MountCapability::Read
                | agentdash_domain::common::MountCapability::List => {
                    values.insert(WorkspaceCapability::Read);
                }
                agentdash_domain::common::MountCapability::Search => {
                    values.insert(WorkspaceCapability::Search);
                }
                agentdash_domain::common::MountCapability::Write => {
                    values.insert(WorkspaceCapability::Write);
                }
                agentdash_domain::common::MountCapability::Exec
                | agentdash_domain::common::MountCapability::Watch => {}
            }
        }
    }
    if vfs.mounts.len() > 1 {
        values.insert(WorkspaceCapability::MultipleRoots);
    }
    values.insert(WorkspaceCapability::VirtualFileSystem);
    values.into_iter().collect()
}

fn digest_json(value: &impl serde::Serialize) -> Result<String, AgentRunRuntimeSurfaceSourceError> {
    let value = serde_json::to_value(value).map_err(|error| {
        AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        }
    })?;
    let bytes = agentdash_agent_runtime_host::canonical_json(&value);
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
