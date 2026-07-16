use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, ConfigurationBoundary, ContextFidelity, ContextProfile, DeliveryMechanism,
    DriverBindIntent, DriverBindRequest, DriverBinding, DriverBindingId, DriverCommandEnvelope,
    DriverDescribeRequest, DriverDispatchReceipt, DriverError, DriverEventEnvelope,
    DriverEventSink, DriverInspection, DriverInspectionQuery, DriverProjectedItem,
    DriverSurfaceApplyReceipt, DriverThreadId, HookAction, HookFailurePolicy, HookPoint,
    HookPointCapability, HookProfile, ImmutablePresentationEvent, InputModality, InputProfile,
    InstructionChannel, InstructionProfile, InteractionProfile, LifecycleCapability,
    PresentationDurability, ProfileDigest, ReferenceRuntimeClass, RuntimeCommand,
    RuntimeDescriptor, RuntimeEvent, RuntimeInteractionKind, RuntimeJournalFact, RuntimeProfile,
    RuntimeTurnId, RuntimeTurnTerminal, SemanticStrength, TelemetryCapability, ToolChannel,
    ToolPresentationEmitter, ToolProfile, WorkspaceCapability, WorkspaceProfile,
};
use agentdash_integration_api::{
    ActivatedAgentServiceInstance, AgentRuntimeDriverFactory, AgentRuntimeFactoryKey,
    DriverFactoryError, DriverSurfaceRequest, DriverToolInvocation, DriverToolOutcome,
    MaterializedDriverSurface, RuntimeDriverHostPorts,
};
use agentdash_process::{ProcessDomain, background_tokio_command_with_cwd};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout},
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

use crate::{
    artifact::{
        HookArtifactPlan, MaterializedHookArtifact, materialize_hook_artifact, native_hook_config,
    },
    contribution::{CODEX_APP_SERVER_PACKAGE, CODEX_PROTOCOL_REVISION},
    hook_bridge::{HookBridgeLease, start_hook_bridge},
    mapping::{
        MappedEvent, MappingError, SourceCoordinateMap, dynamic_tool_interaction_request,
        item_content, main_automatic_server_response, map_input,
    },
    rpc::{RpcInbound, RpcNotification, RpcRequest, error_response, response},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodexDriverConfig {
    cwd: PathBuf,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    model_provider: Option<String>,
    #[serde(default)]
    base_instructions: Option<String>,
    #[serde(default)]
    developer_instructions: Option<String>,
    #[serde(default)]
    runtime_workspace_roots: Vec<PathBuf>,
    artifact_root: PathBuf,
}

pub trait CodexAppServerLauncher: Send + Sync {
    fn spawn(&self, cwd: &std::path::Path, hook_endpoint: Option<&str>) -> Result<Child, String>;
}

pub struct ProductionCodexAppServerLauncher;

impl CodexAppServerLauncher for ProductionCodexAppServerLauncher {
    fn spawn(&self, cwd: &std::path::Path, hook_endpoint: Option<&str>) -> Result<Child, String> {
        let mut command =
            background_tokio_command_with_cwd(ProcessDomain::CodexAppServer, "npx", cwd);
        command
            .args(["-y", CODEX_APP_SERVER_PACKAGE, "app-server"])
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("NPM_CONFIG_LOGLEVEL", "error")
            .env("NO_COLOR", "1");
        if let Some(endpoint) = hook_endpoint {
            command.env("AGENTDASH_HOOK_ENDPOINT", endpoint);
        }
        command.spawn().map_err(|error| error.to_string())
    }
}

pub struct CodexRuntimeDriverFactory {
    key: AgentRuntimeFactoryKey,
    launcher: Arc<dyn CodexAppServerLauncher>,
}

impl CodexRuntimeDriverFactory {
    pub fn new(key: AgentRuntimeFactoryKey) -> Self {
        Self::with_launcher(key, Arc::new(ProductionCodexAppServerLauncher))
    }

    pub fn with_launcher(
        key: AgentRuntimeFactoryKey,
        launcher: Arc<dyn CodexAppServerLauncher>,
    ) -> Self {
        Self { key, launcher }
    }
}

#[async_trait]
impl AgentRuntimeDriverFactory for CodexRuntimeDriverFactory {
    fn factory_key(&self) -> &AgentRuntimeFactoryKey {
        &self.key
    }

    async fn create(
        &self,
        instance: ActivatedAgentServiceInstance,
        host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFactoryError> {
        let config: CodexDriverConfig =
            serde_json::from_value(instance.config.clone()).map_err(|error| {
                DriverFactoryError::InvalidConfiguration {
                    reason: error.to_string(),
                }
            })?;
        if !config.cwd.is_absolute() || !config.artifact_root.is_absolute() {
            return Err(DriverFactoryError::InvalidConfiguration {
                reason: "cwd and artifactRoot must be absolute".to_string(),
            });
        }
        if config
            .runtime_workspace_roots
            .iter()
            .any(|root| !root.is_absolute())
        {
            return Err(DriverFactoryError::InvalidConfiguration {
                reason: "runtimeWorkspaceRoots must contain only absolute paths".to_string(),
            });
        }
        Ok(Arc::new(CodexRuntimeDriver {
            instance,
            config,
            host,
            request_counter: AtomicI64::new(1),
            sessions: Mutex::new(BTreeMap::new()),
            launcher: self.launcher.clone(),
        }))
    }
}

struct CodexRuntimeDriver {
    instance: ActivatedAgentServiceInstance,
    config: CodexDriverConfig,
    host: RuntimeDriverHostPorts,
    request_counter: AtomicI64,
    sessions: Mutex<BTreeMap<String, Arc<Mutex<CodexSession>>>>,
    launcher: Arc<dyn CodexAppServerLauncher>,
}

struct CodexSession {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Option<Lines<BufReader<ChildStdout>>>,
    source_thread_id: DriverThreadId,
    binding_id: agentdash_agent_runtime_contract::RuntimeBindingId,
    state: Arc<Mutex<CodexPumpState>>,
    bootstrap_inbound: VecDeque<RpcInbound>,
    surface: MaterializedDriverSurface,
    _hook_bridge: Option<HookBridgeLease>,
    pump: Option<JoinHandle<()>>,
    receipts: BTreeMap<agentdash_agent_runtime_contract::DriverRequestId, DriverDispatchReceipt>,
    pending_bind_presentations: Vec<ImmutablePresentationEvent>,
}

#[derive(Default)]
struct CodexPumpState {
    coordinates: SourceCoordinateMap,
    pending_interactions: BTreeMap<String, PendingServerRequest>,
    rpc_waiters: BTreeMap<i64, PendingRpc>,
    sink: Option<Arc<dyn DriverEventSink>>,
    active_turns: BTreeMap<RuntimeTurnId, ActiveCodexTurn>,
    turn_idle: Arc<tokio::sync::Notify>,
    native_hook_runs: BTreeMap<String, bool>,
    context_delivered: bool,
}

impl CodexPumpState {
    fn finish_turn(&mut self, turn_id: &RuntimeTurnId) {
        if self.active_turns.remove(turn_id).is_some() && self.active_turns.is_empty() {
            self.turn_idle.notify_waiters();
        }
    }

    fn clear_turns(&mut self) {
        let had_active_turn = !self.active_turns.is_empty();
        self.active_turns.clear();
        if had_active_turn {
            self.turn_idle.notify_waiters();
        }
    }

    fn take_turns(&mut self) -> BTreeMap<RuntimeTurnId, ActiveCodexTurn> {
        let turns = std::mem::take(&mut self.active_turns);
        if !turns.is_empty() {
            self.turn_idle.notify_waiters();
        }
        turns
    }
}

struct PendingRpc {
    sender: oneshot::Sender<Result<Value, DriverError>>,
    turn: Option<PendingCodexTurn>,
}

#[derive(Debug, Clone)]
struct PendingCodexTurn {
    runtime_turn_id: RuntimeTurnId,
    operation_id: agentdash_agent_runtime_contract::RuntimeOperationId,
}

#[derive(Debug, Clone)]
struct ActiveCodexTurn {
    source_turn_id: agentdash_agent_runtime_contract::DriverTurnId,
    operation_id: agentdash_agent_runtime_contract::RuntimeOperationId,
}

#[derive(Debug, Clone)]
struct PendingServerRequest {
    rpc_id: Value,
    method: String,
    params: Value,
    turn_id: RuntimeTurnId,
    source_turn_id: agentdash_agent_runtime_contract::DriverTurnId,
    source_item_id: Option<agentdash_agent_runtime_contract::DriverItemId>,
    source_request_id: String,
    operation_id: agentdash_agent_runtime_contract::RuntimeOperationId,
}

impl Drop for CodexSession {
    fn drop(&mut self) {
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
        let _ = self.child.start_kill();
    }
}

#[async_trait]
impl AgentRuntimeDriver for CodexRuntimeDriver {
    async fn describe(
        &self,
        request: DriverDescribeRequest,
    ) -> Result<RuntimeDescriptor, DriverError> {
        if request.service_instance_id != self.instance.instance_id {
            return Err(DriverError::Rejected {
                reason: "service instance does not belong to this driver".to_string(),
            });
        }
        let profile = codex_runtime_profile();
        Ok(RuntimeDescriptor {
            protocol_revision: CODEX_PROTOCOL_REVISION,
            service_instance_id: self.instance.instance_id.clone(),
            profile_digest: digest_profile(&profile),
            profile,
        })
    }

    async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
        if request.service_instance_id != self.instance.instance_id {
            return Err(DriverError::Rejected {
                reason: "service instance does not belong to this driver".to_string(),
            });
        }
        let binding_key = request.binding_id.as_str().to_string();
        if self.sessions.lock().await.contains_key(&binding_key) {
            return Err(DriverError::Rejected {
                reason: "binding already has an active Codex session".to_string(),
            });
        }
        let surface = self
            .host
            .surfaces
            .materialize(DriverSurfaceRequest {
                binding_id: request.binding_id.clone(),
                surface_revision: request.surface_revision,
                surface_digest: request.surface_digest.clone(),
            })
            .await
            .map_err(|error| DriverError::Rejected {
                reason: error.to_string(),
            })?;
        if surface.revision != request.surface_revision || surface.digest != request.surface_digest
        {
            return Err(DriverError::ProtocolViolation {
                reason: "surface broker returned a different revision or digest".to_string(),
                critical: true,
            });
        }
        if surface
            .workspace
            .roots
            .iter()
            .any(|root| !PathBuf::from(root).is_absolute())
        {
            return Err(DriverError::Rejected {
                reason: "materialized workspace roots must be absolute for Codex".to_string(),
            });
        }
        let hook_bridge = if surface.hooks.bindings.is_empty() {
            None
        } else {
            Some(
                start_hook_bridge(
                    self.host.hooks.clone(),
                    request.binding_id.clone(),
                    self.instance.generation,
                    surface.hooks.revision,
                    surface.hooks.digest.clone(),
                    surface.hooks.bindings.clone(),
                    surface.runtime_thread_id.clone(),
                    surface.authorization_identity.clone(),
                )
                .await
                .map_err(|error| DriverError::Unavailable {
                    reason: error.to_string(),
                    retryable: true,
                })?,
            )
        };
        let hook_artifact = if hook_bridge.is_some() {
            Some(
                materialize_hook_artifact(
                    &self.config.artifact_root,
                    &HookArtifactPlan {
                        plan_revision: surface.hooks.revision.0,
                        plan_digest: surface.hooks.digest.as_str().to_string(),
                        required_timeout_ms: 30_000,
                    },
                )
                .map_err(|error| DriverError::Rejected {
                    reason: error.to_string(),
                })?,
            )
        } else {
            None
        };
        if let (Some(expected), Some(applied)) = (
            surface.hooks.artifact_digest.as_deref(),
            hook_artifact.as_ref(),
        ) && expected != applied.digest
        {
            return Err(DriverError::Rejected {
                reason: "materialized hook artifact does not match the bound artifact digest"
                    .to_string(),
            });
        }
        if !matches!(request.intent, DriverBindIntent::Start) && !surface.tools.tools.is_empty() {
            return Err(DriverError::Unsupported {
                reason: "Codex thread/resume and thread/fork cannot reapply dynamicTools; a new ThreadStart binding is required".to_string(),
            });
        }
        let mut session = self
            .spawn_and_initialize(request.binding_id.clone(), surface.clone(), hook_bridge)
            .await?;
        let (method, params) = match &request.intent {
            DriverBindIntent::Start => (
                "thread/start",
                self.thread_start_params(&surface, hook_artifact.as_ref()),
            ),
            DriverBindIntent::Resume { source_thread_id } => {
                let mut params = self.thread_start_params(&surface, hook_artifact.as_ref());
                params
                    .as_object_mut()
                    .expect("thread params are an object")
                    .remove("dynamicTools");
                params
                    .as_object_mut()
                    .expect("thread params are an object")
                    .insert("threadId".to_string(), json!(source_thread_id.as_str()));
                ("thread/resume", params)
            }
            DriverBindIntent::Fork {
                source_thread_id,
                through_source_turn_id,
            } => {
                let mut params = self.thread_start_params(&surface, hook_artifact.as_ref());
                let params = params.as_object_mut().expect("thread params are an object");
                params.remove("dynamicTools");
                params.insert("threadId".to_string(), json!(source_thread_id.as_str()));
                params.insert(
                    "lastTurnId".to_string(),
                    json!(through_source_turn_id.as_ref().map(|id| id.as_str())),
                );
                ("thread/fork", Value::Object(params.clone()))
            }
        };
        let result = self.rpc_request(&mut session, method, params).await?;
        let thread_id = result
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| DriverError::ProtocolViolation {
                reason: "thread/start response misses thread.id".to_string(),
                critical: true,
            })?;
        session.source_thread_id =
            DriverThreadId::new(thread_id).map_err(|error| DriverError::ProtocolViolation {
                reason: error.to_string(),
                critical: true,
            })?;
        let source_thread_id = session.source_thread_id.clone();
        session.pending_bind_presentations = bind_presentations(source_thread_id.as_str(), &result);
        session.state.lock().await.context_delivered =
            !matches!(request.intent, DriverBindIntent::Start);
        if let Some(bridge) = session._hook_bridge.as_ref() {
            bridge.bind_source_thread(source_thread_id.clone()).await;
        }
        self.start_pump(&mut session)?;
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(&binding_key) {
            return Err(DriverError::Rejected {
                reason: "binding became active while the Codex session was starting".to_string(),
            });
        }
        sessions.insert(binding_key, Arc::new(Mutex::new(session)));
        Ok(DriverBinding {
            driver_binding_id: DriverBindingId::new(format!("codex:{}", request.binding_id))
                .expect("binding id is non-empty"),
            source_thread_id,
            applied_surface_revision: request.surface_revision,
            applied_surface_digest: request.surface_digest,
            applied_tool_set_revision: surface.tools.revision,
            applied_tool_set_digest: surface.tools.digest,
            applied_hook_plan_revision: Some(surface.hooks.revision),
            applied_hook_plan_digest: Some(surface.hooks.digest),
            applied_hooks: surface
                .hooks
                .bindings
                .iter()
                .map(
                    |binding| agentdash_agent_runtime_contract::DriverHookApplyStatus {
                        point: binding.point,
                        acknowledged: hook_artifact.is_some(),
                        artifact_digest: hook_artifact
                            .as_ref()
                            .map(|artifact| artifact.digest.clone()),
                    },
                )
                .collect(),
        })
    }

    async fn dispatch(
        &self,
        envelope: DriverCommandEnvelope,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError> {
        if envelope.generation != self.instance.generation {
            return Err(DriverError::StaleGeneration);
        }
        let session = self
            .sessions
            .lock()
            .await
            .get(envelope.binding_id.as_str())
            .cloned()
            .ok_or_else(|| DriverError::Unavailable {
                reason: "Codex binding is not active".to_string(),
                retryable: true,
            })?;
        let surface_adoption = matches!(&envelope.command, RuntimeCommand::SurfaceAdopt { .. });
        let mut session = loop {
            let session_guard = session.lock().await;
            if !surface_adoption || session_guard.state.lock().await.active_turns.is_empty() {
                break session_guard;
            }
            let state = session_guard.state.clone();
            drop(session_guard);
            wait_for_codex_turn_idle(&state).await;
        };
        if session.source_thread_id != envelope.source_thread_id {
            return Err(DriverError::StaleGeneration);
        }
        if let Some(receipt) = session.receipts.get(&envelope.request_id) {
            let mut duplicate = receipt.clone();
            duplicate.duplicate = true;
            return Ok(duplicate);
        }
        session.state.lock().await.sink = Some(sink.clone());
        if !session.pending_bind_presentations.is_empty() {
            sink.emit(DriverEventEnvelope {
                binding_id: envelope.binding_id.clone(),
                generation: envelope.generation,
                operation_id: None,
                source_thread_id: envelope.source_thread_id.clone(),
                source_turn_id: None,
                source_item_id: None,
                source_request_id: None,
                source_entry_index: None,
                facts: session
                    .pending_bind_presentations
                    .iter()
                    .cloned()
                    .map(RuntimeJournalFact::Presentation)
                    .collect(),
            })
            .await?;
            session.pending_bind_presentations.clear();
        }

        let mut applied_surface = None;
        match &envelope.command {
            RuntimeCommand::ThreadStart { input, .. } | RuntimeCommand::TurnStart { input, .. } => {
                self.start_turn(&mut session, &envelope, input).await?;
            }
            RuntimeCommand::ThreadResume { .. } | RuntimeCommand::ThreadFork { .. } => {
                return Err(DriverError::Unsupported {
                    reason: "resume/fork are binding intents and cannot mutate an existing sticky binding".to_string(),
                });
            }
            RuntimeCommand::ThreadRebind { .. } => {
                return Err(DriverError::Unsupported {
                    reason: "ThreadRebind is a Managed Runtime transition and cannot be dispatched to a driver".to_string(),
                });
            }
            RuntimeCommand::ThreadSettingsUpdate { instructions, .. } => {
                let source_thread_id = session.source_thread_id.as_str().to_string();
                self.rpc_request(
                    &mut session,
                    "thread/settings/update",
                    json!({
                        "threadId": source_thread_id,
                        "developerInstructions": instructions.join("\n\n")
                    }),
                )
                .await?;
            }
            RuntimeCommand::TurnSteer {
                expected_turn_id,
                input,
                ..
            } => {
                let source_turn =
                    source_turn_for(&session.state.lock().await.coordinates, expected_turn_id)?;
                let source_thread_id = session.source_thread_id.as_str().to_string();
                let (native, additional) = map_input(input);
                self.rpc_request(
                    &mut session,
                    "turn/steer",
                    json!({
                        "threadId": source_thread_id, "expectedTurnId": source_turn,
                        "input": native, "additionalContext": additional
                    }),
                )
                .await?;
            }
            RuntimeCommand::TurnInterrupt {
                expected_turn_id, ..
            } => {
                let source_turn =
                    source_turn_for(&session.state.lock().await.coordinates, expected_turn_id)?;
                let source_thread_id = session.source_thread_id.as_str().to_string();
                self.rpc_request(
                    &mut session,
                    "turn/interrupt",
                    json!({ "threadId": source_thread_id, "turnId": source_turn }),
                )
                .await?;
            }
            RuntimeCommand::InteractionRespond {
                interaction_id,
                response: interaction_response,
                ..
            } => {
                let key = interaction_id.as_str();
                let pending = session
                    .state
                    .lock()
                    .await
                    .pending_interactions
                    .get(key)
                    .cloned()
                    .ok_or_else(|| DriverError::Rejected {
                        reason: "interaction is no longer pending".to_string(),
                    })?;
                let payload = interaction_result(&pending, interaction_response)?;
                write_value(&session.stdin, &response(pending.rpc_id.clone(), payload)).await?;
                session.state.lock().await.pending_interactions.remove(key);
            }
            RuntimeCommand::ContextCompact { .. } => {
                return Err(DriverError::Unsupported {
                    reason: "Codex compaction is opaque/observed and cannot satisfy canonical managed compaction".to_string(),
                });
            }
            RuntimeCommand::ToolSetReplace { .. } => {
                return Err(DriverError::Unsupported {
                    reason:
                        "Codex dynamic tools are thread-static; rebind at ThreadStart is required"
                            .to_string(),
                });
            }
            RuntimeCommand::SurfaceAdopt { target, .. } => {
                let surface = self
                    .host
                    .surfaces
                    .materialize(DriverSurfaceRequest {
                        binding_id: envelope.binding_id.clone(),
                        surface_revision: target.surface_revision,
                        surface_digest: target.surface_digest.clone(),
                    })
                    .await
                    .map_err(|error| DriverError::Rejected {
                        reason: error.to_string(),
                    })?;
                validate_surface_descriptor(target, &surface)?;
                if surface
                    .workspace
                    .roots
                    .iter()
                    .any(|root| !PathBuf::from(root).is_absolute())
                {
                    return Err(DriverError::Rejected {
                        reason: "materialized workspace roots must be absolute for Codex"
                            .to_string(),
                    });
                }
                let source_thread_id = session.source_thread_id.clone();
                let hook_bridge = if surface.hooks.bindings.is_empty() {
                    None
                } else {
                    Some(
                        start_hook_bridge(
                            self.host.hooks.clone(),
                            envelope.binding_id.clone(),
                            envelope.generation,
                            surface.hooks.revision,
                            surface.hooks.digest.clone(),
                            surface.hooks.bindings.clone(),
                            surface.runtime_thread_id.clone(),
                            surface.authorization_identity.clone(),
                        )
                        .await
                        .map_err(|error| DriverError::Unavailable {
                            reason: error.to_string(),
                            retryable: true,
                        })?,
                    )
                };
                let hook_artifact = if hook_bridge.is_some() {
                    Some(
                        materialize_hook_artifact(
                            &self.config.artifact_root,
                            &HookArtifactPlan {
                                plan_revision: surface.hooks.revision.0,
                                plan_digest: surface.hooks.digest.as_str().to_string(),
                                required_timeout_ms: 30_000,
                            },
                        )
                        .map_err(|error| DriverError::Rejected {
                            reason: error.to_string(),
                        })?,
                    )
                } else {
                    None
                };
                if let (Some(expected), Some(applied)) = (
                    surface.hooks.artifact_digest.as_deref(),
                    hook_artifact.as_ref(),
                ) && expected != applied.digest
                {
                    return Err(DriverError::Rejected {
                        reason:
                            "materialized hook artifact does not match the adopted artifact digest"
                                .to_string(),
                    });
                }
                let mut replacement = self
                    .spawn_and_initialize(envelope.binding_id.clone(), surface.clone(), hook_bridge)
                    .await?;
                let mut params = self.thread_start_params(&surface, hook_artifact.as_ref());
                params
                    .as_object_mut()
                    .expect("thread params are an object")
                    .insert("threadId".to_string(), json!(source_thread_id.as_str()));
                let result = self
                    .rpc_request(&mut replacement, "thread/resume", params)
                    .await?;
                let resumed_thread_id = result
                    .pointer("/thread/id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| DriverError::ProtocolViolation {
                        reason: "thread/resume response misses thread.id".to_string(),
                        critical: true,
                    })?;
                if resumed_thread_id != source_thread_id.as_str() {
                    return Err(DriverError::ProtocolViolation {
                        reason: "surface adoption resumed a different Codex thread".to_string(),
                        critical: true,
                    });
                }
                replacement.source_thread_id = source_thread_id.clone();
                if let Some(bridge) = replacement._hook_bridge.as_ref() {
                    bridge.bind_source_thread(source_thread_id).await;
                }
                let (sink, receipts) = {
                    let state = session.state.lock().await;
                    (state.sink.clone(), session.receipts.clone())
                };
                {
                    let mut state = replacement.state.lock().await;
                    state.sink = sink;
                    state.context_delivered = false;
                }
                replacement.receipts = receipts;
                replacement.pending_bind_presentations.clear();
                self.start_pump(&mut replacement)?;
                *session = replacement;
                applied_surface = Some(DriverSurfaceApplyReceipt {
                    descriptor: target.as_ref().clone(),
                    applied_hooks: surface
                        .hooks
                        .bindings
                        .iter()
                        .map(
                            |binding| agentdash_agent_runtime_contract::DriverHookApplyStatus {
                                point: binding.point,
                                acknowledged: true,
                                artifact_digest: hook_artifact
                                    .as_ref()
                                    .map(|artifact| artifact.digest.clone()),
                            },
                        )
                        .collect(),
                });
            }
        }

        let receipt = DriverDispatchReceipt {
            request_id: envelope.request_id,
            duplicate: false,
            applied_tool_set: None,
            applied_surface,
        };
        session
            .receipts
            .insert(receipt.request_id.clone(), receipt.clone());
        Ok(receipt)
    }

    async fn inspect(&self, query: DriverInspectionQuery) -> Result<DriverInspection, DriverError> {
        let sessions = self.sessions.lock().await;
        match query {
            DriverInspectionQuery::Binding { driver_binding_id } => Ok(DriverInspection::Binding {
                active: driver_binding_id
                    .as_str()
                    .strip_prefix("codex:")
                    .is_some_and(|binding_id| sessions.contains_key(binding_id)),
            }),
            DriverInspectionQuery::CompactionActivation { .. } => {
                Ok(DriverInspection::CompactionActivation {
                    applied: false,
                    digest: None,
                    driver_context_revision: None,
                })
            }
            DriverInspectionQuery::Checkpoint { .. } => Ok(DriverInspection::Checkpoint {
                available: false,
                digest: None,
            }),
            DriverInspectionQuery::ThreadProjection { source_thread_id } => {
                let candidates = sessions.values().cloned().collect::<Vec<_>>();
                drop(sessions);
                let mut selected = None;
                for candidate in candidates {
                    let session = candidate.lock().await;
                    if session.source_thread_id == source_thread_id {
                        drop(session);
                        selected = Some(candidate);
                        break;
                    }
                }
                let selected = selected.ok_or(DriverError::StaleGeneration)?;
                let mut session = selected.lock().await;
                let result = self
                    .rpc_request(
                        &mut session,
                        "thread/read",
                        json!({ "threadId": source_thread_id.as_str(), "includeTurns": true }),
                    )
                    .await?;
                Ok(DriverInspection::ThreadProjection {
                    source_thread_id,
                    items: projected_items(&result)?,
                    fidelity: ContextFidelity::EventProjected,
                })
            }
            DriverInspectionQuery::ContextRead { source_thread_id } => {
                let candidates = sessions.values().cloned().collect::<Vec<_>>();
                drop(sessions);
                let mut found = false;
                for candidate in candidates {
                    if candidate.lock().await.source_thread_id == source_thread_id {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(DriverError::StaleGeneration);
                }
                Ok(DriverInspection::ContextRead {
                    source_thread_id,
                    fidelity: ContextFidelity::Opaque,
                    digest: None,
                })
            }
        }
    }
}

async fn wait_for_codex_turn_idle(state: &Arc<Mutex<CodexPumpState>>) {
    loop {
        let idle = state.lock().await.turn_idle.clone();
        let notified = idle.notified_owned();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if state.lock().await.active_turns.is_empty() {
            return;
        }
        notified.await;
    }
}

fn projected_items(result: &Value) -> Result<Vec<DriverProjectedItem>, DriverError> {
    let turns = result
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| DriverError::ProtocolViolation {
            reason: "thread/read result misses typed thread.turns array".to_string(),
            critical: true,
        })?;
    let mut projected = Vec::new();
    for turn in turns {
        let source_turn = turn.get("id").and_then(Value::as_str).ok_or_else(|| {
            DriverError::ProtocolViolation {
                reason: "thread/read turn misses id".to_string(),
                critical: true,
            }
        })?;
        let items = turn.get("items").and_then(Value::as_array).ok_or_else(|| {
            DriverError::ProtocolViolation {
                reason: format!("thread/read turn {source_turn} misses typed items array"),
                critical: true,
            }
        })?;
        for item in items {
            let source_item = item.get("id").and_then(Value::as_str).ok_or_else(|| {
                DriverError::ProtocolViolation {
                    reason: "thread/read item misses id".to_string(),
                    critical: true,
                }
            })?;
            projected.push(DriverProjectedItem {
                source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(source_turn)
                    .map_err(|error| DriverError::ProtocolViolation {
                        reason: error.to_string(),
                        critical: true,
                    })?,
                source_item_id: agentdash_agent_runtime_contract::DriverItemId::new(source_item)
                    .map_err(|error| DriverError::ProtocolViolation {
                        reason: error.to_string(),
                        critical: true,
                    })?,
                content: item_content(item).map_err(|error| DriverError::ProtocolViolation {
                    reason: error.to_string(),
                    critical: true,
                })?,
            });
        }
    }
    Ok(projected)
}

fn validate_surface_descriptor(
    target: &agentdash_agent_runtime_contract::RuntimeSurfaceDescriptor,
    surface: &MaterializedDriverSurface,
) -> Result<(), DriverError> {
    let matches = surface.revision == target.surface_revision
        && surface.digest == target.surface_digest
        && surface.workspace.digest == target.vfs_digest
        && surface.context.recipe.revision == target.context_recipe_revision
        && surface.context.digest == target.context_digest
        && surface.context.recipe.provenance.settings_revision == target.settings_revision
        && surface.tools.revision == target.tool_set_revision
        && surface.tools.digest == target.tool_set_digest
        && surface.hooks.revision == target.hook_plan.revision
        && surface.hooks.digest == target.hook_plan.digest;
    if matches {
        Ok(())
    } else {
        Err(DriverError::ProtocolViolation {
            reason: "surface broker materialization does not match the requested Runtime surface descriptor".to_string(),
            critical: true,
        })
    }
}

fn effective_workspace_roots(
    config: &CodexDriverConfig,
    surface: &MaterializedDriverSurface,
) -> Vec<String> {
    let mut roots = config
        .runtime_workspace_roots
        .iter()
        .map(|root| root.display().to_string())
        .chain(surface.workspace.roots.iter().cloned())
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

impl CodexRuntimeDriver {
    async fn spawn_and_initialize(
        &self,
        binding_id: agentdash_agent_runtime_contract::RuntimeBindingId,
        surface: MaterializedDriverSurface,
        hook_bridge: Option<HookBridgeLease>,
    ) -> Result<CodexSession, DriverError> {
        let mut child = self
            .launcher
            .spawn(
                &self.config.cwd,
                hook_bridge.as_ref().map(|bridge| bridge.endpoint.as_str()),
            )
            .map_err(|reason| DriverError::Unavailable {
                reason,
                retryable: true,
            })?;
        let stdin = child.stdin.take().ok_or_else(|| DriverError::Unavailable {
            reason: "Codex app-server has no stdin".to_string(),
            retryable: true,
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DriverError::Unavailable {
                reason: "Codex app-server has no stdout".to_string(),
                retryable: true,
            })?;
        let mut session = CodexSession {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Some(BufReader::new(stdout).lines()),
            source_thread_id: DriverThreadId::new("codex-thread-pending").expect("static id"),
            binding_id,
            state: Arc::new(Mutex::new(CodexPumpState::default())),
            bootstrap_inbound: VecDeque::new(),
            surface,
            _hook_bridge: hook_bridge,
            pump: None,
            receipts: BTreeMap::new(),
            pending_bind_presentations: Vec::new(),
        };
        self.rpc_request(&mut session, "initialize", json!({
            "clientInfo": { "name": "agentdash", "title": "AgentDash", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "experimentalApi": true }
        })).await?;
        write_value(
            &session.stdin,
            &serde_json::to_value(RpcNotification {
                method: "initialized",
                params: None,
            })
            .expect("serialize notification"),
        )
        .await?;
        Ok(session)
    }

    fn thread_start_params(
        &self,
        surface: &MaterializedDriverSurface,
        hook_artifact: Option<&MaterializedHookArtifact>,
    ) -> Value {
        let system = surface
            .context
            .instructions
            .iter()
            .filter(|entry| entry.channel == InstructionChannel::System)
            .flat_map(|entry| entry.entries.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");
        let developer = surface
            .context
            .instructions
            .iter()
            .filter(|entry| entry.channel == InstructionChannel::Developer)
            .flat_map(|entry| entry.entries.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");
        let base_instructions = [
            self.config.base_instructions.as_deref(),
            (!system.is_empty()).then_some(system.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n");
        let developer_instructions = [
            self.config.developer_instructions.as_deref(),
            (!developer.is_empty()).then_some(developer.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n");
        let dynamic_tools = surface.tools.tools.iter().map(|tool| json!({
            "name": tool.name, "description": tool.description, "inputSchema": tool.parameters_schema
        })).collect::<Vec<_>>();
        let config = hook_artifact.map(|artifact| native_hook_config(artifact, 30_000));
        let workspace_roots = effective_workspace_roots(&self.config, surface);
        json!({
            "cwd": self.config.cwd,
            "model": self.config.model,
            "modelProvider": self.config.model_provider,
            "baseInstructions": (!base_instructions.is_empty()).then_some(base_instructions),
            "developerInstructions": (!developer_instructions.is_empty()).then_some(developer_instructions),
            "runtimeWorkspaceRoots": workspace_roots,
            "dynamicTools": dynamic_tools,
            "config": config,
            "sandbox": "workspace-write",
            "approvalPolicy": "on-request",
            "approvalsReviewer": "user"
        })
    }

    async fn start_turn(
        &self,
        session: &mut CodexSession,
        envelope: &DriverCommandEnvelope,
        input: &[agentdash_agent_runtime_contract::RuntimeInput],
    ) -> Result<(), DriverError> {
        let (native, additional) = map_input(input);
        let mut additional = additional.unwrap_or_default();
        let deliver_context = !session.state.lock().await.context_delivered;
        if deliver_context {
            for (index, block) in session.surface.context.blocks.iter().enumerate() {
                additional.insert(format!("agentdash.context.{index}"), json!({
                    "value": serde_json::to_string(block).expect("owned context block serializes"),
                    "kind": "application"
                }));
            }
            for (index, instruction) in session
                .surface
                .context
                .instructions
                .iter()
                .filter(|entry| entry.channel == InstructionChannel::AdditionalContext)
                .flat_map(|entry| entry.entries.iter())
                .enumerate()
            {
                additional.insert(
                    format!("agentdash.instruction.additional.{index}"),
                    json!({
                        "value": instruction, "kind": "application"
                    }),
                );
            }
        }
        let runtime_turn =
            envelope
                .runtime_turn_id
                .clone()
                .ok_or_else(|| DriverError::ProtocolViolation {
                    reason: "Codex turn command is missing the Managed Runtime turn identity"
                        .to_string(),
                    critical: true,
                })?;
        let _presentation_turn_id = envelope.presentation_turn_id.clone().ok_or_else(|| {
            DriverError::ProtocolViolation {
                reason: "Codex turn command is missing the Session presentation turn identity"
                    .to_string(),
                critical: true,
            }
        })?;
        let pending_turn = PendingCodexTurn {
            runtime_turn_id: runtime_turn.clone(),
            operation_id: envelope.operation_id.clone(),
        };
        let result = self.rpc_request_inner(session, "turn/start", json!({
            "threadId": session.source_thread_id.as_str(), "input": native,
            "additionalContext": (!additional.is_empty()).then_some(additional), "runtimeWorkspaceRoots": effective_workspace_roots(&self.config, &session.surface)
        }), Some(pending_turn.clone())).await?;
        if deliver_context {
            session.state.lock().await.context_delivered = true;
        }
        let source_turn = result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| DriverError::ProtocolViolation {
                reason: "turn/start response misses turn.id".to_string(),
                critical: true,
            })?;
        let source_turn_id = agentdash_agent_runtime_contract::DriverTurnId::new(source_turn)
            .map_err(|error| DriverError::ProtocolViolation {
                reason: error.to_string(),
                critical: true,
            })?;
        let mut state = session.state.lock().await;
        state
            .coordinates
            .register_turn(source_turn, runtime_turn.clone());
        state.active_turns.insert(
            runtime_turn,
            ActiveCodexTurn {
                source_turn_id,
                operation_id: pending_turn.operation_id,
            },
        );
        Ok(())
    }

    async fn rpc_request(
        &self,
        session: &mut CodexSession,
        method: &str,
        params: Value,
    ) -> Result<Value, DriverError> {
        self.rpc_request_inner(session, method, params, None).await
    }

    async fn rpc_request_inner(
        &self,
        session: &mut CodexSession,
        method: &str,
        params: Value,
        turn: Option<PendingCodexTurn>,
    ) -> Result<Value, DriverError> {
        let id = self.request_counter.fetch_add(1, Ordering::Relaxed);
        let value =
            serde_json::to_value(RpcRequest { id, method, params }).expect("request serializes");
        if session.stdout.is_none() {
            let (sender, receiver) = oneshot::channel();
            session
                .state
                .lock()
                .await
                .rpc_waiters
                .insert(id, PendingRpc { sender, turn });
            if let Err(error) = write_value(&session.stdin, &value).await {
                session.state.lock().await.rpc_waiters.remove(&id);
                return Err(error);
            }
            return receiver.await.map_err(|_| DriverError::Lost {
                reason: "Codex response pump stopped before RPC response".to_string(),
                retryable: true,
            })?;
        }
        write_value(&session.stdin, &value).await?;
        loop {
            let line = session
                .stdout
                .as_mut()
                .expect("bootstrap stdout is present")
                .next_line()
                .await
                .map_err(|error| DriverError::Lost {
                    reason: error.to_string(),
                    retryable: true,
                })?
                .ok_or_else(|| DriverError::Lost {
                    reason: "Codex app-server EOF before RPC response".to_string(),
                    retryable: true,
                })?;
            let inbound: RpcInbound =
                serde_json::from_str(&line).map_err(|error| DriverError::ProtocolViolation {
                    reason: format!("malformed Codex JSON-RPC frame: {error}"),
                    critical: true,
                })?;
            match inbound {
                RpcInbound::Response(response) if response.id == json!(id) => {
                    return Ok(response.result);
                }
                RpcInbound::Error(error) if error.id == json!(id) => {
                    return Err(DriverError::Rejected {
                        reason: format!("Codex RPC {}: {}", error.error.code, error.error.message),
                    });
                }
                inbound @ (RpcInbound::Request(_) | RpcInbound::Notification(_)) => {
                    session.bootstrap_inbound.push_back(inbound);
                }
                RpcInbound::Response(_) | RpcInbound::Error(_) => {}
            }
        }
    }

    fn start_pump(&self, session: &mut CodexSession) -> Result<(), DriverError> {
        let stdout = session
            .stdout
            .take()
            .ok_or_else(|| DriverError::ProtocolViolation {
                reason: "Codex response pump already started".to_string(),
                critical: true,
            })?;
        let state = session.state.clone();
        let stdin = session.stdin.clone();
        let host = self.host.clone();
        let binding_id = session.binding_id.clone();
        let generation = self.instance.generation;
        let source_thread_id = session.source_thread_id.clone();
        let tool_set_revision = session.surface.tools.revision;
        let runtime_thread_id = session.surface.runtime_thread_id.clone();
        let authorization_identity = session.surface.authorization_identity.clone();
        let tool_presentation_emitters = session
            .surface
            .tools
            .tools
            .iter()
            .map(|tool| (tool.name.clone(), tool.presentation_emitter))
            .collect();
        let initial = std::mem::take(&mut session.bootstrap_inbound);
        session.pump = Some(tokio::spawn(async move {
            run_pump(
                stdout,
                CodexPumpContext {
                    stdin,
                    state,
                    host,
                    binding_id,
                    generation,
                    source_thread_id,
                    tool_set_revision,
                    runtime_thread_id,
                    authorization_identity,
                    tool_presentation_emitters,
                },
                initial,
            )
            .await;
        }));
        Ok(())
    }
}

struct CodexPumpContext {
    stdin: Arc<Mutex<ChildStdin>>,
    state: Arc<Mutex<CodexPumpState>>,
    host: RuntimeDriverHostPorts,
    binding_id: agentdash_agent_runtime_contract::RuntimeBindingId,
    generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration,
    source_thread_id: DriverThreadId,
    tool_set_revision: agentdash_agent_runtime_contract::ToolSetRevision,
    runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId,
    authorization_identity: Option<agentdash_integration_api::AuthIdentity>,
    tool_presentation_emitters: BTreeMap<String, ToolPresentationEmitter>,
}

fn mapped_dynamic_tool_name(mapped: &MappedEvent) -> Option<&str> {
    let item = match &mapped.presentation.event {
        agentdash_agent_protocol::BackboneEvent::ItemStarted(notification) => &notification.item,
        agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification) => &notification.item,
        _ => return None,
    };
    match item {
        agentdash_agent_protocol::AgentDashThreadItem::Codex(
            agentdash_agent_protocol::codex_app_server_protocol::ThreadItem::DynamicToolCall {
                tool,
                ..
            },
        ) => Some(tool.as_str()),
        agentdash_agent_protocol::AgentDashThreadItem::AgentDash(item) => Some(item.tool_name()),
        agentdash_agent_protocol::AgentDashThreadItem::Codex(_) => None,
    }
}

fn route_bound_dynamic_tool_event(
    mut mapped: MappedEvent,
    emitters: &BTreeMap<String, ToolPresentationEmitter>,
) -> Result<Option<MappedEvent>, MappingError> {
    let Some(tool_name) = mapped_dynamic_tool_name(&mapped) else {
        return Ok(Some(mapped));
    };
    let emitter =
        emitters
            .get(tool_name)
            .ok_or_else(|| MappingError::MissingToolPresentationRoute {
                tool: tool_name.to_string(),
            })?;
    match emitter {
        ToolPresentationEmitter::VendorStream => {
            // PlatformToolBroker owns the canonical Runtime Item. Codex keeps only the
            // app-server presentation for a VendorStream route.
            mapped.runtime_event = None;
            Ok(Some(mapped))
        }
        ToolPresentationEmitter::ToolBroker => {
            // Broker owns both the canonical Item and its protocol presentation.
            Ok(None)
        }
    }
}

async fn run_pump(
    mut stdout: Lines<BufReader<ChildStdout>>,
    context: CodexPumpContext,
    mut initial: VecDeque<RpcInbound>,
) {
    loop {
        let inbound = if let Some(inbound) = initial.pop_front() {
            inbound
        } else {
            let line = match stdout.next_line().await {
                Ok(Some(line)) => line,
                Ok(None) | Err(_) => {
                    settle_pump_lost(
                        &context.state,
                        &context.binding_id,
                        context.generation,
                        &context.source_thread_id,
                    )
                    .await;
                    return;
                }
            };
            match serde_json::from_str::<RpcInbound>(&line) {
                Ok(inbound) => inbound,
                Err(_) => {
                    settle_pump_lost(
                        &context.state,
                        &context.binding_id,
                        context.generation,
                        &context.source_thread_id,
                    )
                    .await;
                    return;
                }
            }
        };
        match inbound {
            RpcInbound::Response(response) => {
                let waiter = if let Some(id) = response.id.as_i64() {
                    context.state.lock().await.rpc_waiters.remove(&id)
                } else {
                    None
                };
                if let Some(waiter) = waiter {
                    if let Some(turn) = waiter.turn
                        && let Some(source_turn) =
                            response.result.pointer("/turn/id").and_then(Value::as_str)
                    {
                        let mut state = context.state.lock().await;
                        state
                            .coordinates
                            .register_turn(source_turn, turn.runtime_turn_id.clone());
                        if let Ok(source_turn_id) =
                            agentdash_agent_runtime_contract::DriverTurnId::new(source_turn)
                        {
                            state.active_turns.insert(
                                turn.runtime_turn_id,
                                ActiveCodexTurn {
                                    source_turn_id,
                                    operation_id: turn.operation_id,
                                },
                            );
                        }
                    }
                    let _ = waiter.sender.send(Ok(response.result));
                }
            }
            RpcInbound::Error(error) => {
                if let Some(id) = error.id.as_i64()
                    && let Some(waiter) = context.state.lock().await.rpc_waiters.remove(&id)
                {
                    let _ = waiter.sender.send(Err(DriverError::Rejected {
                        reason: format!("Codex RPC {}: {}", error.error.code, error.error.message),
                    }));
                }
            }
            RpcInbound::Notification(notification) => {
                let (mapped, sink, operation_id) = {
                    let mut state = context.state.lock().await;
                    reconcile_native_hook(&mut state, &notification);
                    let mapped =
                        state
                            .coordinates
                            .map_notification(notification)
                            .and_then(|mapped| match mapped {
                                Some(mapped) => route_bound_dynamic_tool_event(
                                    mapped,
                                    &context.tool_presentation_emitters,
                                ),
                                None => Ok(None),
                            });
                    let operation_id = mapped
                        .as_ref()
                        .ok()
                        .and_then(Option::as_ref)
                        .and_then(|mapped| mapped.source_turn_id.as_ref())
                        .and_then(|source_turn_id| {
                            state
                                .coordinates
                                .canonical_turn(source_turn_id.as_str())
                                .ok()
                        })
                        .and_then(|runtime_turn_id| {
                            state
                                .active_turns
                                .get(&runtime_turn_id)
                                .map(|turn| turn.operation_id.clone())
                        });
                    (mapped, state.sink.clone(), operation_id)
                };
                match (mapped, sink) {
                    (Ok(Some(mapped)), Some(sink)) => {
                        if emit_mapped_notification(
                            &context.state,
                            sink,
                            &context.binding_id,
                            context.generation,
                            &context.source_thread_id,
                            operation_id,
                            mapped,
                        )
                        .await
                        {
                            return;
                        }
                    }
                    (Err(error), Some(sink)) => {
                        let result = sink
                            .emit(DriverEventEnvelope {
                                binding_id: context.binding_id.clone(),
                                generation: context.generation,
                                operation_id: None,
                                source_thread_id: context.source_thread_id.clone(),
                                source_turn_id: None,
                                source_item_id: None,
                                source_request_id: None,
                                source_entry_index: None,
                                facts: vec![RuntimeJournalFact::Internal(
                                    RuntimeEvent::ProtocolViolation {
                                        code: agentdash_agent_runtime_contract::RuntimeProtocolViolationCode::InvalidLifecycleTransition,
                                        message: error.to_string(),
                                        critical: false,
                                    },
                                )],
                            })
                            .await;
                        if let Err(error @ DriverError::Terminalized { .. }) = result {
                            settle_runtime_terminalized(&context.state, error).await;
                            return;
                        }
                    }
                    (Ok(Some(_) | None) | Err(_), None) | (Ok(None), Some(_)) => {}
                }
            }
            RpcInbound::Request(request) => {
                match main_automatic_server_response(&request) {
                    Ok(Some(result)) => {
                        let _ = write_value(&context.stdin, &response(request.id, result)).await;
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = write_value(
                            &context.stdin,
                            &error_response(request.id, -32601, error.to_string()),
                        )
                        .await;
                        continue;
                    }
                }
                if request.method == "item/tool/call" {
                    if handle_pump_dynamic_tool(&context, request).await {
                        return;
                    }
                    continue;
                }
                let mapped = context
                    .state
                    .lock()
                    .await
                    .coordinates
                    .map_server_request(&request);
                match mapped {
                    Ok(mapped) => {
                        let (sink, operation_id) = {
                            let mut state = context.state.lock().await;
                            let Some(operation_id) = state
                                .active_turns
                                .get(&mapped.turn_id)
                                .map(|turn| turn.operation_id.clone())
                            else {
                                drop(state);
                                let _ = write_value(
                                    &context.stdin,
                                    &error_response(
                                        request.id,
                                        -32602,
                                        "Codex server request targets inactive turn",
                                    ),
                                )
                                .await;
                                continue;
                            };
                            state.pending_interactions.insert(
                                mapped.interaction_id.as_str().to_string(),
                                PendingServerRequest {
                                    rpc_id: request.id,
                                    method: request.method,
                                    params: request.params,
                                    turn_id: mapped.turn_id.clone(),
                                    source_turn_id: mapped.source_turn_id.clone(),
                                    source_item_id: mapped.source_item_id.clone(),
                                    source_request_id: mapped.source_request_id.clone(),
                                    operation_id: operation_id.clone(),
                                },
                            );
                            (state.sink.clone(), operation_id)
                        };
                        if let Some(sink) = sink {
                            let result = sink
                                .emit(DriverEventEnvelope {
                                    binding_id: context.binding_id.clone(),
                                    generation: context.generation,
                                    operation_id: Some(operation_id),
                                    source_thread_id: context.source_thread_id.clone(),
                                    source_turn_id: Some(mapped.source_turn_id),
                                    source_item_id: mapped.source_item_id,
                                    source_request_id: Some(mapped.source_request_id),
                                    source_entry_index: None,
                                    facts: std::iter::once(RuntimeJournalFact::Internal(
                                        mapped.event,
                                    ))
                                    .chain(
                                        mapped.presentation.map(RuntimeJournalFact::Presentation),
                                    )
                                    .collect(),
                                })
                                .await;
                            if let Err(error @ DriverError::Terminalized { .. }) = result {
                                settle_runtime_terminalized(&context.state, error).await;
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        let _ = write_value(
                            &context.stdin,
                            &error_response(request.id, -32601, error.to_string()),
                        )
                        .await;
                    }
                }
            }
        }
    }
}

/// Delivers one mapped Codex notification without forgetting a terminal turn before Managed
/// Runtime has committed it. A terminal envelope can be rejected by the application presentation
/// projector even though the vendor turn has already ended; in that case an internal-only
/// `BindingLost` fact provides a projection-independent path that closes the canonical binding,
/// turn, interactions and owning operation.
async fn emit_mapped_notification(
    state: &Arc<Mutex<CodexPumpState>>,
    sink: Arc<dyn DriverEventSink>,
    binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
    generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration,
    source_thread_id: &DriverThreadId,
    operation_id: Option<agentdash_agent_runtime_contract::RuntimeOperationId>,
    mapped: MappedEvent,
) -> bool {
    let terminal_turn_id = match mapped.runtime_event.as_ref() {
        Some(RuntimeEvent::TurnTerminal { turn_id, .. }) => Some(turn_id.clone()),
        _ => None,
    };
    let source_turn_id = mapped.source_turn_id.clone();
    let source_request_id = mapped.source_request_id();
    let mut facts = mapped
        .runtime_event
        .filter(|event| !event.is_transient())
        .map(RuntimeJournalFact::Internal)
        .into_iter()
        .collect::<Vec<_>>();
    facts.push(RuntimeJournalFact::Presentation(mapped.presentation));
    match sink
        .emit(DriverEventEnvelope {
            binding_id: binding_id.clone(),
            generation,
            operation_id: operation_id.clone(),
            source_thread_id: source_thread_id.clone(),
            source_turn_id: source_turn_id.clone(),
            source_item_id: mapped.source_item_id,
            source_request_id,
            source_entry_index: None,
            facts,
        })
        .await
    {
        Ok(()) => {
            if let Some(turn_id) = terminal_turn_id {
                state.lock().await.finish_turn(&turn_id);
            }
            false
        }
        Err(error @ DriverError::Terminalized { .. }) => {
            settle_runtime_terminalized(state, error).await;
            true
        }
        Err(error) => {
            let Some(turn_id) = terminal_turn_id else {
                return false;
            };
            settle_terminal_sink_failure(
                state,
                sink,
                binding_id,
                generation,
                source_thread_id,
                source_turn_id,
                operation_id,
                &turn_id,
                error,
            )
            .await
        }
    }
}

async fn settle_runtime_terminalized(state: &Arc<Mutex<CodexPumpState>>, error: DriverError) {
    let waiters = {
        let mut state = state.lock().await;
        state.clear_turns();
        state.pending_interactions.clear();
        std::mem::take(&mut state.rpc_waiters)
    };
    for (_, waiter) in waiters {
        let _ = waiter.sender.send(Err(error.clone()));
    }
}

async fn settle_terminal_sink_failure(
    state: &Arc<Mutex<CodexPumpState>>,
    sink: Arc<dyn DriverEventSink>,
    binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
    generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration,
    source_thread_id: &DriverThreadId,
    source_turn_id: Option<agentdash_agent_runtime_contract::DriverTurnId>,
    operation_id: Option<agentdash_agent_runtime_contract::RuntimeOperationId>,
    terminal_turn_id: &RuntimeTurnId,
    error: DriverError,
) -> bool {
    let reason = format!(
        "Codex terminal delivery for {terminal_turn_id} failed; binding was lost to close the accepted Runtime operation: {error}"
    );
    if sink
        .emit(DriverEventEnvelope {
            binding_id: binding_id.clone(),
            generation,
            operation_id,
            source_thread_id: source_thread_id.clone(),
            source_turn_id,
            source_item_id: None,
            source_request_id: None,
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::BindingLost {
                binding_id: binding_id.clone(),
                reason: reason.clone(),
            })],
        })
        .await
        .is_err()
    {
        // The terminal remains active locally so transport-loss/recovery paths can retry a
        // projection-independent terminal instead of silently forgetting the operation.
        return false;
    }

    let waiters = {
        let mut state = state.lock().await;
        state.clear_turns();
        state.pending_interactions.clear();
        std::mem::take(&mut state.rpc_waiters)
    };
    for (_, waiter) in waiters {
        let _ = waiter.sender.send(Err(DriverError::Lost {
            reason: reason.clone(),
            retryable: true,
        }));
    }
    true
}

fn reconcile_native_hook(
    state: &mut CodexPumpState,
    notification: &crate::rpc::RpcServerNotification,
) {
    if !matches!(
        notification.method.as_str(),
        "hook/started" | "hook/completed"
    ) {
        return;
    }
    let source = notification
        .params
        .pointer("/run/source")
        .and_then(Value::as_str);
    if !matches!(source, Some("sessionFlags" | "plugin")) {
        return;
    }
    let Some(run_id) = notification
        .params
        .pointer("/run/id")
        .and_then(Value::as_str)
    else {
        return;
    };
    let completed = notification.method == "hook/completed";
    state
        .native_hook_runs
        .entry(run_id.to_string())
        .and_modify(|seen_completed| {
            *seen_completed |= completed;
        })
        .or_insert(completed);
}

async fn handle_pump_dynamic_tool(
    context: &CodexPumpContext,
    request: crate::rpc::RpcServerRequest,
) -> bool {
    let validated = match validate_dynamic_tool_call(&request.params) {
        Ok(validated) => validated,
        Err(message) => {
            let _ = write_value(&context.stdin, &error_response(request.id, -32602, message)).await;
            return false;
        }
    };
    let coordinates = {
        let mut state = context.state.lock().await;
        let canonical_turn = match state
            .coordinates
            .canonical_turn(validated.source_turn_id.as_str())
        {
            Ok(value) => value,
            Err(error) => {
                drop(state);
                let _ = write_value(
                    &context.stdin,
                    &error_response(request.id, -32602, error.to_string()),
                )
                .await;
                return false;
            }
        };
        let canonical_item = state
            .coordinates
            .register_item(validated.source_item_id.as_str());
        (canonical_turn, canonical_item)
    };
    let invocation = DriverToolInvocation {
        thread_id: context.runtime_thread_id.clone(),
        turn_id: coordinates.0.clone(),
        item_id: coordinates.1.clone(),
        presentation_item_id: validated.presentation_item_id.clone(),
        binding_id: context.binding_id.clone(),
        generation: context.generation,
        source_thread_id: context.source_thread_id.clone(),
        source_turn_id: validated.source_turn_id.clone(),
        source_item_id: validated.source_item_id.clone(),
        tool_set_revision: context.tool_set_revision,
        tool_name: validated.tool_name.clone(),
        arguments: request
            .params
            .get("arguments")
            .cloned()
            .unwrap_or(Value::Null),
        timeout_ms: 120_000,
        authorization_identity: context.authorization_identity.clone(),
    };
    match context.host.tools.invoke(invocation).await {
        Ok(DriverToolOutcome::Completed { output, is_error }) => {
            let content_items = match dynamic_tool_content(&output) {
                Ok(content_items) => content_items,
                Err(error) => {
                    let _ = write_value(
                        &context.stdin,
                        &error_response(request.id, -32603, error.to_string()),
                    )
                    .await;
                    return false;
                }
            };
            let _ = write_value(
                &context.stdin,
                &response(
                    request.id,
                    json!({ "contentItems": content_items, "success": !is_error }),
                ),
            )
            .await;
        }
        Ok(DriverToolOutcome::Denied { reason }) => {
            let _ = write_value(&context.stdin, &response(request.id, json!({ "contentItems": [{ "type": "inputText", "text": reason }], "success": false }))).await;
        }
        Ok(DriverToolOutcome::InteractionRequired {
            interaction_id,
            reason: _,
        }) => {
            let interaction_request = match dynamic_tool_interaction_request(request.params.clone())
            {
                Ok(request) => request,
                Err(error) => {
                    let _ = write_value(
                        &context.stdin,
                        &error_response(request.id, -32602, error.to_string()),
                    )
                    .await;
                    return false;
                }
            };
            let (sink, operation_id) = {
                let state = context.state.lock().await;
                let Some(operation_id) = state
                    .active_turns
                    .get(&coordinates.0)
                    .map(|turn| turn.operation_id.clone())
                else {
                    drop(state);
                    let _ = write_value(
                        &context.stdin,
                        &error_response(
                            request.id,
                            -32602,
                            "Codex dynamic tool call targets inactive turn",
                        ),
                    )
                    .await;
                    return false;
                };
                (state.sink.clone(), operation_id)
            };
            let pending = PendingServerRequest {
                rpc_id: request.id.clone(),
                method: request.method,
                params: request.params.clone(),
                turn_id: coordinates.0.clone(),
                source_turn_id: validated.source_turn_id.clone(),
                source_item_id: Some(validated.source_item_id.clone()),
                source_request_id: crate::mapping::rpc_coordinate(&request.id),
                operation_id: operation_id.clone(),
            };
            let envelope = DriverEventEnvelope {
                binding_id: context.binding_id.clone(),
                generation: context.generation,
                operation_id: Some(operation_id),
                source_thread_id: context.source_thread_id.clone(),
                source_turn_id: Some(validated.source_turn_id),
                source_item_id: Some(validated.source_item_id),
                source_request_id: Some(crate::mapping::rpc_coordinate(&request.id)),
                source_entry_index: None,
                facts: vec![RuntimeJournalFact::Internal(
                    RuntimeEvent::InteractionRequested {
                        turn_id: coordinates.0.clone(),
                        item_id: Some(coordinates.1),
                        interaction_id: interaction_id.clone(),
                        request: interaction_request,
                    },
                )],
            };
            if let Err(error) = publish_dynamic_tool_interaction(
                &context.state,
                sink,
                interaction_id.as_str(),
                pending,
                envelope,
            )
            .await
            {
                let _ = write_value(
                    &context.stdin,
                    &error_response(request.id, -32002, error.to_string()),
                )
                .await;
                if matches!(error, DriverError::Terminalized { .. }) {
                    settle_runtime_terminalized(&context.state, error).await;
                    return true;
                }
            }
        }
        Err(error) => {
            let _ = write_value(
                &context.stdin,
                &error_response(request.id, -32002, error.to_string()),
            )
            .await;
        }
    }
    false
}

struct ValidatedDynamicToolCall {
    source_turn_id: agentdash_agent_runtime_contract::DriverTurnId,
    source_item_id: agentdash_agent_runtime_contract::DriverItemId,
    presentation_item_id: agentdash_agent_runtime_contract::PresentationItemId,
    tool_name: String,
}

fn validate_dynamic_tool_call(params: &Value) -> Result<ValidatedDynamicToolCall, String> {
    let source_turn = params
        .get("turnId")
        .and_then(Value::as_str)
        .ok_or_else(|| "item/tool/call misses turnId".to_string())?;
    let source_item = params
        .get("callId")
        .and_then(Value::as_str)
        .ok_or_else(|| "item/tool/call misses callId".to_string())?;
    let tool_name = params
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| "item/tool/call misses tool".to_string())?;
    if tool_name.trim().is_empty() {
        return Err("item/tool/call has an empty tool".to_string());
    }
    let source_turn_id = agentdash_agent_runtime_contract::DriverTurnId::new(source_turn)
        .map_err(|error| format!("item/tool/call has an invalid turnId: {error}"))?;
    let source_item_id = agentdash_agent_runtime_contract::DriverItemId::new(source_item)
        .map_err(|error| format!("item/tool/call has an invalid callId: {error}"))?;
    let presentation_item_id =
        agentdash_agent_runtime_contract::PresentationItemId::new(source_item)
            .map_err(|error| format!("item/tool/call has an invalid callId: {error}"))?;
    Ok(ValidatedDynamicToolCall {
        source_turn_id,
        source_item_id,
        presentation_item_id,
        tool_name: tool_name.to_string(),
    })
}

async fn publish_dynamic_tool_interaction(
    state: &Arc<Mutex<CodexPumpState>>,
    sink: Option<Arc<dyn DriverEventSink>>,
    interaction_key: &str,
    pending: PendingServerRequest,
    envelope: DriverEventEnvelope,
) -> Result<(), DriverError> {
    if state
        .lock()
        .await
        .pending_interactions
        .contains_key(interaction_key)
    {
        return Err(DriverError::ProtocolViolation {
            reason: "Codex dynamic tool interaction identity is already pending".to_string(),
            critical: false,
        });
    }
    let sink = sink.ok_or_else(|| DriverError::Unavailable {
        reason: "Codex dynamic tool interaction has no Runtime event sink".to_string(),
        retryable: true,
    })?;
    sink.emit(envelope).await?;
    state
        .lock()
        .await
        .pending_interactions
        .insert(interaction_key.to_string(), pending);
    Ok(())
}

async fn settle_pump_lost(
    state: &Arc<Mutex<CodexPumpState>>,
    binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
    generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration,
    source_thread_id: &DriverThreadId,
) {
    let (waiters, turns, interactions, sink) = {
        let mut state = state.lock().await;
        (
            std::mem::take(&mut state.rpc_waiters),
            state.take_turns(),
            std::mem::take(&mut state.pending_interactions),
            state.sink.clone(),
        )
    };
    for (_, waiter) in waiters {
        let _ = waiter.sender.send(Err(DriverError::Lost {
            reason: "Codex app-server transport closed".to_string(),
            retryable: true,
        }));
    }
    if let Some(sink) = sink {
        for (turn_id, active_turn) in turns {
            let result = sink
                .emit(DriverEventEnvelope {
                    binding_id: binding_id.clone(),
                    generation,
                    operation_id: Some(active_turn.operation_id),
                    source_thread_id: source_thread_id.clone(),
                    source_turn_id: Some(active_turn.source_turn_id),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: None,
                    facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                        turn_id,
                        terminal: RuntimeTurnTerminal::Lost,
                        message: Some(
                            "Codex app-server transport closed before terminal".to_string(),
                        ),
                        diagnostic: None,
                    })],
                })
                .await;
            if matches!(result, Err(DriverError::Terminalized { .. })) {
                return;
            }
        }
        for (interaction_id, pending) in interactions {
            let Ok(interaction_id) =
                agentdash_agent_runtime_contract::RuntimeInteractionId::new(interaction_id)
            else {
                continue;
            };
            let result = sink
                .emit(DriverEventEnvelope {
                    binding_id: binding_id.clone(),
                    generation,
                    operation_id: Some(pending.operation_id),
                    source_thread_id: source_thread_id.clone(),
                    source_turn_id: Some(pending.source_turn_id),
                    source_item_id: pending.source_item_id,
                    source_request_id: Some(pending.source_request_id),
                    source_entry_index: None,
                    facts: vec![RuntimeJournalFact::Internal(
                        RuntimeEvent::InteractionTerminal {
                            turn_id: pending.turn_id,
                            interaction_id,
                            terminal:
                                agentdash_agent_runtime_contract::RuntimeInteractionTerminal::Lost,
                        },
                    )],
                })
                .await;
            if matches!(result, Err(DriverError::Terminalized { .. })) {
                return;
            }
        }
    }
}

async fn write_value(stdin: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<(), DriverError> {
    let mut bytes = serde_json::to_vec(value).map_err(|error| DriverError::ProtocolViolation {
        reason: error.to_string(),
        critical: true,
    })?;
    bytes.push(b'\n');
    let mut stdin = stdin.lock().await;
    stdin
        .write_all(&bytes)
        .await
        .map_err(|error| DriverError::Lost {
            reason: error.to_string(),
            retryable: true,
        })?;
    stdin.flush().await.map_err(|error| DriverError::Lost {
        reason: error.to_string(),
        retryable: true,
    })
}

fn bind_presentations(thread_id: &str, result: &Value) -> Vec<ImmutablePresentationEvent> {
    let mut presentations = vec![ImmutablePresentationEvent::new(
        PresentationDurability::Durable,
        agentdash_agent_protocol::BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::ExecutorSessionBound {
                executor_session_id: thread_id.to_string(),
            },
        ),
    )];
    let title = result
        .pointer("/thread/name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty());
    let preview = result.pointer("/thread/preview").and_then(Value::as_str);
    if let Some(title) =
        title.filter(|title| preview.is_none_or(|preview| preview.trim() != *title))
    {
        presentations.push(ImmutablePresentationEvent::new(
            PresentationDurability::Durable,
            agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SourceSessionTitleUpdated {
                    executor_session_id: Some(thread_id.to_string()),
                    title: title.to_string(),
                    preview: preview.map(str::to_string),
                    source: "codex".to_string(),
                },
            ),
        ));
    }
    presentations
}

fn source_turn_for(
    map: &SourceCoordinateMap,
    expected: &RuntimeTurnId,
) -> Result<String, DriverError> {
    map.source_turn(expected)
        .map_err(|_| DriverError::Rejected {
            reason: "expected turn is not mapped to this Codex binding".to_string(),
        })
}

fn interaction_result(
    pending: &PendingServerRequest,
    response_value: &agentdash_agent_runtime_contract::InteractionResponse,
) -> Result<Value, DriverError> {
    use agentdash_agent_runtime_contract::InteractionResponse;
    let method = pending.method.as_str();
    match (method, response_value) {
        ("item/commandExecution/requestApproval", InteractionResponse::Approved) => {
            Ok(json!({ "decision": "accept" }))
        }
        ("item/commandExecution/requestApproval", InteractionResponse::Denied { .. }) => {
            Ok(json!({ "decision": "decline" }))
        }
        ("item/fileChange/requestApproval", InteractionResponse::Approved) => {
            Ok(json!({ "decision": "accept" }))
        }
        ("item/fileChange/requestApproval", InteractionResponse::Denied { .. }) => {
            Ok(json!({ "decision": "decline" }))
        }
        ("item/permissions/requestApproval", InteractionResponse::Approved) => Ok(
            json!({ "permissions": pending.params.get("permissions").cloned().unwrap_or_else(|| json!({})), "scope": "turn" }),
        ),
        ("item/permissions/requestApproval", InteractionResponse::Denied { .. }) => {
            Ok(json!({ "permissions": {}, "scope": "turn" }))
        }
        ("item/tool/requestUserInput", InteractionResponse::UserInput { input }) => {
            let questions = pending
                .params
                .get("questions")
                .and_then(Value::as_array)
                .ok_or_else(|| DriverError::Rejected {
                    reason: "Codex user-input request is missing typed questions".into(),
                })?;
            if questions.len() != input.len() {
                return Err(DriverError::Rejected {
                    reason: format!(
                        "Codex user-input response count {} does not match question count {}",
                        input.len(),
                        questions.len()
                    ),
                });
            }
            let mut answers = serde_json::Map::new();
            for (index, (question, answer)) in questions.iter().zip(input).enumerate() {
                let id = question
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| DriverError::Rejected {
                        reason: format!(
                            "Codex user-input question at index {index} is missing a typed id"
                        ),
                    })?;
                if answers.contains_key(id) {
                    return Err(DriverError::Rejected {
                        reason: format!("Codex user-input question id `{id}` is duplicated"),
                    });
                }
                let agentdash_agent_runtime_contract::RuntimeInput::UserInput {
                    block: agentdash_agent_protocol::UserInputBlock::Text { text, .. },
                } = answer
                else {
                    return Err(DriverError::Rejected {
                        reason: format!("Codex user-input answer for question `{id}` must be text"),
                    });
                };
                answers.insert(id.to_string(), json!({ "answers": [text] }));
            }
            Ok(json!({ "answers": answers }))
        }
        ("item/tool/call", InteractionResponse::DynamicToolResult { output }) => {
            Ok(json!({ "contentItems": dynamic_tool_content(output)?, "success": true }))
        }
        ("mcpServer/elicitation/request", InteractionResponse::McpElicitation { value }) => {
            Ok(value.clone())
        }
        _ => Err(DriverError::Rejected {
            reason: format!("interaction response does not match {method}"),
        }),
    }
}

fn dynamic_tool_content(output: &Value) -> Result<Vec<Value>, DriverError> {
    if let Some(items) = output.get("contentItems").and_then(Value::as_array) {
        let typed = serde_json::from_value::<
            Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
        >(Value::Array(items.clone()))
        .map_err(|error| DriverError::Rejected {
            reason: format!("dynamic tool result contains invalid typed contentItems: {error}"),
        })?;
        return typed
            .into_iter()
            .map(|item| {
                serde_json::to_value(item).map_err(|error| DriverError::Rejected {
                    reason: format!("dynamic tool result content serialization failed: {error}"),
                })
            })
            .collect();
    }
    if let Some(items) = output.as_array() {
        return items
            .iter()
            .map(|item| match item.get("type").and_then(Value::as_str) {
                Some("image") | Some("input_image") => {
                    let image_url = item
                        .get("url")
                        .or_else(|| item.get("imageUrl"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| DriverError::Rejected {
                            reason: "dynamic tool image result requires a string url/imageUrl"
                                .to_string(),
                        })?;
                    Ok(json!({ "type": "inputImage", "imageUrl": image_url }))
                }
                Some("text") | Some("input_text") => {
                    let text = item.get("text").and_then(Value::as_str).ok_or_else(|| {
                        DriverError::Rejected {
                            reason: "dynamic tool text result requires a string text".to_string(),
                        }
                    })?;
                    Ok(json!({ "type": "inputText", "text": text }))
                }
                _ => Err(DriverError::Rejected {
                    reason: "dynamic tool result content item must be typed text/image".to_string(),
                }),
            })
            .collect::<Result<Vec<_>, _>>();
    }
    Err(DriverError::Rejected {
        reason: "dynamic tool result requires typed contentItems".to_string(),
    })
}

fn digest_profile(profile: &RuntimeProfile) -> ProfileDigest {
    agentdash_agent_runtime_contract::runtime_profile_digest(profile)
}

pub(crate) fn codex_runtime_profile() -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::Interactive,
        input: InputProfile {
            modalities: BTreeSet::from([
                InputModality::Text,
                InputModality::Image,
                InputModality::LocalImage,
                InputModality::Skill,
                InputModality::Mention,
                InputModality::Structured,
            ]),
        },
        instruction: InstructionProfile {
            channels: BTreeSet::from([
                InstructionChannel::System,
                InstructionChannel::Developer,
                InstructionChannel::AdditionalContext,
            ]),
            configuration_boundary: ConfigurationBoundary::ThreadStart,
        },
        tools: ToolProfile {
            channels: BTreeSet::from([ToolChannel::DirectCallback, ToolChannel::DriverNative]),
            configuration_boundary: ConfigurationBoundary::ThreadStart,
            cancellation: false,
        },
        workspace: WorkspaceProfile {
            capabilities: BTreeSet::from([
                WorkspaceCapability::Read,
                WorkspaceCapability::Write,
                WorkspaceCapability::Search,
                WorkspaceCapability::MultipleRoots,
            ]),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::from([
                RuntimeInteractionKind::CommandApproval,
                RuntimeInteractionKind::FileChangeApproval,
                RuntimeInteractionKind::PermissionApproval,
                RuntimeInteractionKind::UserInputRequest,
                RuntimeInteractionKind::McpElicitation,
                RuntimeInteractionKind::DynamicToolExecution,
            ]),
            durable_correlation: true,
        },
        lifecycle: BTreeSet::from([
            LifecycleCapability::ThreadStart,
            LifecycleCapability::ThreadResume,
            LifecycleCapability::ThreadFork,
            LifecycleCapability::ThreadRead,
            LifecycleCapability::TurnStart,
            LifecycleCapability::TurnSteer,
            LifecycleCapability::TurnInterrupt,
            LifecycleCapability::SurfaceAdopt,
        ]),
        hooks: HookProfile {
            configuration_boundary: ConfigurationBoundary::ThreadStart,
            points: vec![
                hook(
                    HookPoint::BeforeTool,
                    &[
                        HookAction::Observe,
                        HookAction::Block,
                        HookAction::RewriteInput,
                    ],
                    SemanticStrength::ExactSynchronous,
                ),
                hook(
                    HookPoint::AfterTool,
                    &[
                        HookAction::Observe,
                        HookAction::RewriteResult,
                        HookAction::EmitEffect,
                    ],
                    SemanticStrength::ExactSynchronous,
                ),
                hook(
                    HookPoint::BeforeContextCompact,
                    &[HookAction::Observe, HookAction::Block],
                    SemanticStrength::ExactSynchronous,
                ),
                hook(
                    HookPoint::AfterContextCompact,
                    &[HookAction::Observe, HookAction::EmitEffect],
                    SemanticStrength::ExactDurableBoundary,
                ),
                hook(
                    HookPoint::BeforeStop,
                    &[HookAction::Observe, HookAction::ContinueTurn],
                    SemanticStrength::ExactSynchronous,
                ),
            ],
        },
        context: ContextProfile {
            capabilities: BTreeSet::new(),
            fidelity: ContextFidelity::Opaque,
            activation_idempotent: false,
        },
        telemetry_config: BTreeSet::from([
            TelemetryCapability::Reasoning,
            TelemetryCapability::Deltas,
            TelemetryCapability::ConfigurationEvidence,
        ]),
    }
}

fn hook(
    point: HookPoint,
    actions: &[HookAction],
    strength: SemanticStrength,
) -> HookPointCapability {
    let failure_policies = match point {
        HookPoint::BeforeTool | HookPoint::BeforeContextCompact | HookPoint::BeforeStop => {
            BTreeSet::from([
                HookFailurePolicy::FailClosed,
                HookFailurePolicy::FailOpenWithDiagnostic,
                HookFailurePolicy::ObserveOnly,
            ])
        }
        _ => BTreeSet::from([
            HookFailurePolicy::FailOpenWithDiagnostic,
            HookFailurePolicy::ObserveOnly,
        ]),
    };
    HookPointCapability {
        point,
        actions: actions.iter().copied().collect(),
        strength,
        mechanism: DeliveryMechanism::Native,
        failure_policies,
        acknowledged: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex as TokioMutex;

    #[derive(Default)]
    struct RecordingSink(TokioMutex<Vec<DriverEventEnvelope>>);

    #[async_trait]
    impl DriverEventSink for RecordingSink {
        async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
            self.0.lock().await.push(event);
            Ok(())
        }
    }

    #[derive(Default)]
    struct TerminalProjectionRejectingSink(TokioMutex<Vec<DriverEventEnvelope>>);

    #[async_trait]
    impl DriverEventSink for TerminalProjectionRejectingSink {
        async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
            let rejects_terminal_projection = event.facts.iter().any(|fact| {
                matches!(
                    fact,
                    RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal { .. })
                )
            }) && event
                .facts
                .iter()
                .any(|fact| matches!(fact, RuntimeJournalFact::Presentation(_)));
            self.0.lock().await.push(event);
            if rejects_terminal_projection {
                Err(DriverError::ProtocolViolation {
                    reason: "injected terminal presentation projection failure".to_string(),
                    critical: false,
                })
            } else {
                Ok(())
            }
        }
    }

    #[derive(Default)]
    struct AlwaysRejectingSink(TokioMutex<Vec<DriverEventEnvelope>>);

    #[async_trait]
    impl DriverEventSink for AlwaysRejectingSink {
        async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
            self.0.lock().await.push(event);
            Err(DriverError::Unavailable {
                reason: "injected dynamic interaction sink failure".to_string(),
                retryable: true,
            })
        }
    }

    #[derive(Default)]
    struct TerminalizingSink(TokioMutex<Vec<DriverEventEnvelope>>);

    #[async_trait]
    impl DriverEventSink for TerminalizingSink {
        async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
            self.0.lock().await.push(event);
            Err(DriverError::Terminalized {
                reason: "Managed Runtime committed a critical violation".into(),
            })
        }
    }

    fn dynamic_interaction_fixture() -> (String, PendingServerRequest, DriverEventEnvelope) {
        let interaction_id = "dynamic-interaction-1".to_string();
        let turn_id = RuntimeTurnId::new("runtime-turn-dynamic-interaction").unwrap();
        let source_turn_id =
            agentdash_agent_runtime_contract::DriverTurnId::new("source-turn-dynamic-interaction")
                .unwrap();
        let source_item_id =
            agentdash_agent_runtime_contract::DriverItemId::new("source-item-dynamic-interaction")
                .unwrap();
        let operation_id = agentdash_agent_runtime_contract::RuntimeOperationId::new(
            "operation-dynamic-interaction",
        )
        .unwrap();
        let pending = PendingServerRequest {
            rpc_id: json!(701),
            method: "item/tool/call".to_string(),
            params: json!({}),
            turn_id: turn_id.clone(),
            source_turn_id: source_turn_id.clone(),
            source_item_id: Some(source_item_id.clone()),
            source_request_id: "701".to_string(),
            operation_id: operation_id.clone(),
        };
        let envelope = DriverEventEnvelope {
            binding_id: agentdash_agent_runtime_contract::RuntimeBindingId::new("binding-dynamic")
                .unwrap(),
            generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration(1),
            operation_id: Some(operation_id),
            source_thread_id: DriverThreadId::new("source-thread-dynamic").unwrap(),
            source_turn_id: Some(source_turn_id),
            source_item_id: Some(source_item_id),
            source_request_id: Some("701".to_string()),
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(
                RuntimeEvent::InteractionRequested {
                    turn_id,
                    item_id: None,
                    interaction_id: agentdash_agent_runtime_contract::RuntimeInteractionId::new(
                        interaction_id.clone(),
                    )
                    .unwrap(),
                    request: dynamic_tool_interaction_request(json!({
                        "threadId": "source-thread-dynamic",
                        "turnId": "source-turn-dynamic-interaction",
                        "callId": "source-item-dynamic-interaction",
                        "namespace": "platform",
                        "tool": "surface_update",
                        "arguments": {}
                    }))
                    .unwrap(),
                },
            )],
        };
        (interaction_id, pending, envelope)
    }

    #[test]
    fn profile_is_truthful_about_opaque_compaction_and_thread_start_updates() {
        let profile = codex_runtime_profile();
        assert_eq!(profile.reference_class, ReferenceRuntimeClass::Interactive);
        assert_eq!(
            profile.input.modalities,
            BTreeSet::from([
                InputModality::Text,
                InputModality::Image,
                InputModality::LocalImage,
                InputModality::Skill,
                InputModality::Mention,
                InputModality::Structured,
            ])
        );
        assert_eq!(profile.context.fidelity, ContextFidelity::Opaque);
        assert!(
            !profile
                .context
                .capabilities
                .contains(&agentdash_agent_runtime_contract::ContextCapability::PrepareCompaction)
        );
        assert_eq!(
            profile.tools.configuration_boundary,
            ConfigurationBoundary::ThreadStart
        );
        assert!(
            !profile
                .lifecycle
                .contains(&LifecycleCapability::ToolSetReplace)
        );
    }

    #[test]
    fn interaction_response_never_auto_accepts() {
        let pending = PendingServerRequest {
            rpc_id: json!(1),
            method: "item/commandExecution/requestApproval".to_string(),
            params: json!({}),
            turn_id: RuntimeTurnId::new("turn").unwrap(),
            source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new("source-turn")
                .unwrap(),
            source_item_id: None,
            source_request_id: "1".to_string(),
            operation_id: agentdash_agent_runtime_contract::RuntimeOperationId::new("operation-1")
                .unwrap(),
        };
        assert!(
            interaction_result(
                &pending,
                &agentdash_agent_runtime_contract::InteractionResponse::UserInput { input: vec![] }
            )
            .is_err()
        );
        assert_eq!(
            interaction_result(
                &pending,
                &agentdash_agent_runtime_contract::InteractionResponse::Denied {
                    reason: Some("no".to_string())
                }
            )
            .unwrap()["decision"],
            "decline"
        );
    }

    #[test]
    fn dynamic_tool_validation_rejects_empty_turn_and_call_ids_before_registration() {
        for (params, expected) in [
            (
                json!({"turnId":"","callId":"source-call","tool":"surface_update"}),
                "turnId",
            ),
            (
                json!({"turnId":"source-turn","callId":"","tool":"surface_update"}),
                "callId",
            ),
        ] {
            let error = validate_dynamic_tool_call(&params)
                .err()
                .expect("empty dynamic tool coordinate must be rejected");
            assert!(error.contains(expected), "{error}");
            let response = error_response(json!(701), -32602, error);
            assert_eq!(response["error"]["code"], -32602);
        }
    }

    #[tokio::test]
    async fn dynamic_tool_interaction_without_sink_never_becomes_pending() {
        let state = Arc::new(Mutex::new(CodexPumpState::default()));
        let (interaction_key, pending, envelope) = dynamic_interaction_fixture();
        let error =
            publish_dynamic_tool_interaction(&state, None, &interaction_key, pending, envelope)
                .await
                .expect_err("missing sink must fail the dynamic interaction");
        assert!(matches!(error, DriverError::Unavailable { .. }));
        assert!(state.lock().await.pending_interactions.is_empty());
        let response = error_response(json!(701), -32002, error.to_string());
        assert_eq!(response["error"]["code"], -32002);
    }

    #[tokio::test]
    async fn dynamic_tool_interaction_sink_failure_never_becomes_pending() {
        let state = Arc::new(Mutex::new(CodexPumpState::default()));
        let sink = Arc::new(AlwaysRejectingSink::default());
        let (interaction_key, pending, envelope) = dynamic_interaction_fixture();
        let error = publish_dynamic_tool_interaction(
            &state,
            Some(sink.clone()),
            &interaction_key,
            pending,
            envelope,
        )
        .await
        .expect_err("sink failure must fail the dynamic interaction");
        assert!(matches!(error, DriverError::Unavailable { .. }));
        assert_eq!(sink.0.lock().await.len(), 1);
        assert!(state.lock().await.pending_interactions.is_empty());
        let response = error_response(json!(701), -32002, error.to_string());
        assert_eq!(response["error"]["code"], -32002);
    }

    #[test]
    fn user_input_response_requires_exact_typed_text_answers() {
        let pending = PendingServerRequest {
            rpc_id: json!(2),
            method: "item/tool/requestUserInput".to_string(),
            params: json!({"questions": [{"id": "name"}]}),
            turn_id: RuntimeTurnId::new("turn").unwrap(),
            source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new("source-turn")
                .unwrap(),
            source_item_id: None,
            source_request_id: "2".to_string(),
            operation_id: agentdash_agent_runtime_contract::RuntimeOperationId::new("operation-2")
                .unwrap(),
        };
        let response = interaction_result(
            &pending,
            &agentdash_agent_runtime_contract::InteractionResponse::UserInput {
                input: vec![agentdash_agent_runtime_contract::RuntimeInput::text(
                    "AgentDash",
                )],
            },
        )
        .unwrap();
        assert_eq!(response["answers"]["name"]["answers"], json!(["AgentDash"]));

        assert!(
            interaction_result(
                &pending,
                &agentdash_agent_runtime_contract::InteractionResponse::UserInput { input: vec![] }
            )
            .is_err()
        );
        assert!(
            interaction_result(
                &pending,
                &agentdash_agent_runtime_contract::InteractionResponse::UserInput {
                    input: vec![agentdash_agent_runtime_contract::RuntimeInput::user_input(
                        agentdash_agent_protocol::UserInputBlock::Image {
                            detail: None,
                            url: "data:image/png;base64,AA==".into(),
                        },
                    )],
                }
            )
            .is_err()
        );
    }

    #[test]
    fn bind_response_restores_main_binding_and_source_title_order() {
        let presentations = bind_presentations(
            "source-thread",
            &json!({"thread":{"name":" Codex Title ","preview":"preview"}}),
        );
        assert_eq!(presentations.len(), 2);
        let bodies = presentations
            .into_iter()
            .map(|presentation| serde_json::to_value(presentation.event).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(bodies[0]["payload"]["kind"], "executor_session_bound");
        assert_eq!(
            bodies[0]["payload"]["data"]["executor_session_id"],
            "source-thread"
        );
        assert_eq!(
            bodies[1],
            json!({
                "type":"platform",
                "payload":{
                    "kind":"source_session_title_updated",
                    "data":{
                        "executor_session_id":"source-thread",
                        "title":"Codex Title",
                        "preview":"preview",
                        "source":"codex"
                    }
                }
            })
        );

        let duplicate = bind_presentations(
            "source-thread",
            &json!({"thread":{"name":"same","preview":" same "}}),
        );
        assert_eq!(duplicate.len(), 1, "preview-equivalent title is suppressed");
    }

    #[test]
    fn dynamic_tool_result_preserves_image_content() {
        let content = dynamic_tool_content(&json!([
            { "type": "image", "url": "data:image/png;base64,AA==" },
            { "type": "text", "text": "done" }
        ]))
        .expect("typed dynamic content");
        assert_eq!(content[0]["type"], "inputImage");
        assert_eq!(content[0]["imageUrl"], "data:image/png;base64,AA==");
        assert_eq!(content[1]["text"], "done");
    }

    #[test]
    fn dynamic_tool_result_rejects_malformed_typed_content() {
        for malformed in [
            json!({"contentItems":[{"type":"inputText"}]}),
            json!([{"type":"text"}]),
            json!([{"type":"image"}]),
            json!({"contentItems":[{"type":"unknown","value":1}]}),
        ] {
            let error = dynamic_tool_content(&malformed)
                .expect_err("malformed dynamic content must fail explicitly");
            assert!(error.to_string().contains("dynamic tool"), "{error}");
        }
    }

    #[tokio::test]
    async fn transport_loss_settles_active_turn_with_source_coordinate() {
        let sink = Arc::new(RecordingSink::default());
        let runtime_turn = RuntimeTurnId::new("runtime-turn").unwrap();
        let source_turn =
            agentdash_agent_runtime_contract::DriverTurnId::new("source-turn").unwrap();
        let state = Arc::new(Mutex::new(CodexPumpState::default()));
        {
            let mut state = state.lock().await;
            state.sink = Some(sink.clone());
            state.active_turns.insert(
                runtime_turn.clone(),
                ActiveCodexTurn {
                    source_turn_id: source_turn.clone(),
                    operation_id: agentdash_agent_runtime_contract::RuntimeOperationId::new(
                        "operation-turn",
                    )
                    .unwrap(),
                },
            );
            state.pending_interactions.insert(
                "interaction-1".to_string(),
                PendingServerRequest {
                    rpc_id: json!(1),
                    method: "item/commandExecution/requestApproval".to_string(),
                    params: json!({}),
                    turn_id: runtime_turn.clone(),
                    source_turn_id: source_turn.clone(),
                    source_item_id: None,
                    source_request_id: "1".to_string(),
                    operation_id: agentdash_agent_runtime_contract::RuntimeOperationId::new(
                        "operation-turn",
                    )
                    .unwrap(),
                },
            );
        }
        settle_pump_lost(
            &state,
            &agentdash_agent_runtime_contract::RuntimeBindingId::new("binding").unwrap(),
            agentdash_agent_runtime_contract::RuntimeDriverGeneration(4),
            &DriverThreadId::new("source-thread").unwrap(),
        )
        .await;
        let events = sink.0.lock().await;
        assert!(matches!(
            events[0].facts.as_slice(),
            [RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                terminal: RuntimeTurnTerminal::Lost,
                ..
            })]
        ));
        assert_eq!(events[0].source_turn_id.as_ref(), Some(&source_turn));
        assert!(matches!(
            events[1].facts.as_slice(),
            [RuntimeJournalFact::Internal(
                RuntimeEvent::InteractionTerminal {
                    terminal: agentdash_agent_runtime_contract::RuntimeInteractionTerminal::Lost,
                    ..
                }
            )]
        ));
        assert!(state.lock().await.active_turns.is_empty());
        assert!(state.lock().await.pending_interactions.is_empty());
    }

    #[tokio::test]
    async fn full_surface_sync_defers_until_the_active_codex_turn_is_terminal() {
        let runtime_turn = RuntimeTurnId::new("runtime-turn-surface-sync").unwrap();
        let state = Arc::new(Mutex::new(CodexPumpState::default()));
        state.lock().await.active_turns.insert(
            runtime_turn.clone(),
            ActiveCodexTurn {
                source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(
                    "source-turn-surface-sync",
                )
                .unwrap(),
                operation_id: agentdash_agent_runtime_contract::RuntimeOperationId::new(
                    "operation-surface-sync",
                )
                .unwrap(),
            },
        );

        let waiting_state = state.clone();
        let waiter = tokio::spawn(async move {
            wait_for_codex_turn_idle(&waiting_state).await;
        });
        tokio::task::yield_now().await;
        assert!(
            !waiter.is_finished(),
            "connector full-surface sync must remain deferred while the turn is active"
        );

        state.lock().await.finish_turn(&runtime_turn);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("terminal turn wakes deferred surface sync")
            .expect("surface sync waiter");
    }

    #[tokio::test]
    async fn terminal_projection_failure_loses_binding_without_forgetting_active_operation() {
        let sink = Arc::new(TerminalProjectionRejectingSink::default());
        let binding_id =
            agentdash_agent_runtime_contract::RuntimeBindingId::new("binding").expect("binding id");
        let source_thread_id = DriverThreadId::new("source-thread").expect("source thread");
        let runtime_turn = RuntimeTurnId::new("runtime-turn").expect("runtime turn");
        let source_turn = agentdash_agent_runtime_contract::DriverTurnId::new("source-turn")
            .expect("source turn");
        let operation_id = agentdash_agent_runtime_contract::RuntimeOperationId::new(
            "terminal-projection-operation",
        )
        .expect("operation id");
        let state = Arc::new(Mutex::new(CodexPumpState::default()));
        let mapped = {
            let mut state = state.lock().await;
            state
                .coordinates
                .register_turn(source_turn.as_str(), runtime_turn.clone());
            state.active_turns.insert(
                runtime_turn.clone(),
                ActiveCodexTurn {
                    source_turn_id: source_turn.clone(),
                    operation_id: operation_id.clone(),
                },
            );
            state
                .coordinates
                .map_notification(crate::rpc::RpcServerNotification {
                    method: "turn/completed".to_string(),
                    params: json!({
                        "threadId": source_thread_id.as_str(),
                        "turn": {
                            "id": source_turn.as_str(),
                            "status": "completed",
                            "items": [],
                            "itemsView": "full",
                            "error": null,
                            "startedAt": null,
                            "completedAt": null,
                            "durationMs": null
                        }
                    }),
                })
                .expect("map terminal notification")
                .expect("mapped terminal notification")
        };

        let binding_lost = emit_mapped_notification(
            &state,
            sink.clone(),
            &binding_id,
            agentdash_agent_runtime_contract::RuntimeDriverGeneration(4),
            &source_thread_id,
            Some(operation_id.clone()),
            mapped,
        )
        .await;

        assert!(binding_lost, "pump must stop after the binding is lost");
        assert!(state.lock().await.active_turns.is_empty());
        let events = sink.0.lock().await;
        assert_eq!(events.len(), 2);
        assert!(events[0].facts.iter().any(|fact| matches!(
            fact,
            RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                turn_id,
                terminal: RuntimeTurnTerminal::Completed,
                ..
            }) if turn_id == &runtime_turn
        )));
        assert_eq!(events[0].operation_id.as_ref(), Some(&operation_id));
        assert!(matches!(
            events[1].facts.as_slice(),
            [RuntimeJournalFact::Internal(RuntimeEvent::BindingLost {
                binding_id: lost_binding_id,
                reason,
            })] if lost_binding_id == &binding_id
                && reason.contains("terminal presentation projection failure")
        ));
        assert_eq!(events[1].operation_id.as_ref(), Some(&operation_id));
        assert_eq!(events[1].source_turn_id.as_ref(), Some(&source_turn));
    }

    #[tokio::test]
    async fn runtime_terminalized_terminal_stops_codex_pump_without_binding_lost_fallback() {
        let sink = Arc::new(TerminalizingSink::default());
        let binding_id =
            agentdash_agent_runtime_contract::RuntimeBindingId::new("binding").unwrap();
        let source_thread_id = DriverThreadId::new("source-thread").unwrap();
        let runtime_turn = RuntimeTurnId::new("runtime-turn-terminalized").unwrap();
        let source_turn =
            agentdash_agent_runtime_contract::DriverTurnId::new("source-turn-terminalized")
                .unwrap();
        let operation_id =
            agentdash_agent_runtime_contract::RuntimeOperationId::new("operation-terminalized")
                .unwrap();
        let state = Arc::new(Mutex::new(CodexPumpState::default()));
        let (waiter_tx, waiter_rx) = oneshot::channel();
        let mapped = {
            let mut state = state.lock().await;
            state.rpc_waiters.insert(
                91,
                PendingRpc {
                    sender: waiter_tx,
                    turn: None,
                },
            );
            state
                .coordinates
                .register_turn(source_turn.as_str(), runtime_turn.clone());
            state.active_turns.insert(
                runtime_turn.clone(),
                ActiveCodexTurn {
                    source_turn_id: source_turn,
                    operation_id: operation_id.clone(),
                },
            );
            state
                .coordinates
                .map_notification(crate::rpc::RpcServerNotification {
                    method: "turn/completed".to_string(),
                    params: json!({
                        "threadId": source_thread_id.as_str(),
                        "turn": {
                            "id": "source-turn-terminalized",
                            "status": "completed",
                            "items": [],
                            "itemsView": "full",
                            "error": null,
                            "startedAt": null,
                            "completedAt": null,
                            "durationMs": null
                        }
                    }),
                })
                .unwrap()
                .unwrap()
        };

        assert!(
            emit_mapped_notification(
                &state,
                sink.clone(),
                &binding_id,
                agentdash_agent_runtime_contract::RuntimeDriverGeneration(4),
                &source_thread_id,
                Some(operation_id),
                mapped,
            )
            .await
        );
        assert!(state.lock().await.active_turns.is_empty());
        assert!(matches!(
            waiter_rx.await.unwrap(),
            Err(DriverError::Terminalized { .. })
        ));
        let events = sink.0.lock().await;
        assert_eq!(events.len(), 1);
        assert!(events.iter().flat_map(|event| &event.facts).all(|fact| {
            !matches!(
                fact,
                RuntimeJournalFact::Internal(RuntimeEvent::BindingLost { .. })
            )
        }));
    }

    #[tokio::test]
    async fn ordinary_nonterminal_sink_error_does_not_stop_codex_pump() {
        let sink = Arc::new(AlwaysRejectingSink::default());
        let state = Arc::new(Mutex::new(CodexPumpState::default()));
        let runtime_turn = RuntimeTurnId::new("runtime-turn-nonterminal").unwrap();
        state.lock().await.active_turns.insert(
            runtime_turn.clone(),
            ActiveCodexTurn {
                source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(
                    "source-turn-nonterminal",
                )
                .unwrap(),
                operation_id: agentdash_agent_runtime_contract::RuntimeOperationId::new(
                    "operation-nonterminal",
                )
                .unwrap(),
            },
        );
        let mapped = MappedEvent {
            source_turn_id: None,
            source_item_id: None,
            runtime_event: None,
            presentation: ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                        key: "nonterminal".into(),
                        value: json!({"value": true}),
                    },
                ),
            ),
        };

        assert!(
            !emit_mapped_notification(
                &state,
                sink.clone(),
                &agentdash_agent_runtime_contract::RuntimeBindingId::new("binding").unwrap(),
                agentdash_agent_runtime_contract::RuntimeDriverGeneration(4),
                &DriverThreadId::new("source-thread").unwrap(),
                None,
                mapped,
            )
            .await
        );
        assert!(state.lock().await.active_turns.contains_key(&runtime_turn));
        assert_eq!(sink.0.lock().await.len(), 1);
    }

    #[test]
    fn thread_projection_keeps_source_coordinates_and_final_item() {
        let items = projected_items(&json!({ "thread": { "turns": [{
            "id": "source-turn", "items": [{ "id": "source-item", "type": "agentMessage", "text": "final" }]
        }] } })).expect("projection");
        assert_eq!(items[0].source_turn_id.as_str(), "source-turn");
        assert_eq!(items[0].source_item_id.as_str(), "source-item");
        assert_eq!(items[0].content.agent_message_text(), Some("final"));
    }

    #[test]
    fn thread_projection_rejects_missing_typed_turn_and_item_arrays() {
        for malformed in [
            json!({ "thread": {} }),
            json!({ "thread": { "turns": null } }),
            json!({ "thread": { "turns": [{ "id": "source-turn" }] } }),
            json!({ "thread": { "turns": [{ "id": "source-turn", "items": null }] } }),
        ] {
            assert!(matches!(
                projected_items(&malformed),
                Err(DriverError::ProtocolViolation { critical: true, .. })
            ));
        }
    }

    #[test]
    fn codex_dynamic_tool_presentation_obeys_the_effective_emitter_route() {
        let mapped = || {
            let item = agentdash_agent_protocol::AgentDashThreadItem::Codex(
                agentdash_agent_protocol::backbone::thread_item::dynamic_tool_call(
                    "source-item",
                    "platform_read",
                    json!({"path":"README.md"}),
                    agentdash_agent_protocol::codex_app_server_protocol::DynamicToolCallStatus::InProgress,
                    None,
                    None,
                ),
            );
            MappedEvent {
                source_turn_id: None,
                source_item_id: None,
                runtime_event: Some(RuntimeEvent::ItemStarted {
                    turn_id: RuntimeTurnId::new("runtime-turn").expect("runtime turn"),
                    item_id: agentdash_agent_runtime_contract::RuntimeItemId::new(
                        "runtime-tool-item",
                    )
                    .expect("runtime item"),
                    initial_content: item_content(&json!({
                        "id":"source-item", "type":"dynamicToolCall",
                        "tool":"platform_read", "status":"inProgress",
                        "arguments":{"path":"README.md"}, "success":null,
                        "namespace":null, "durationMs":null, "contentItems":null
                    }))
                    .expect("dynamic tool content"),
                }),
                presentation: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::ItemStarted(
                        agentdash_agent_protocol::ItemStartedNotification::new(
                            item,
                            "source-thread",
                            "source-turn",
                        ),
                    ),
                ),
            }
        };
        assert!(
            route_bound_dynamic_tool_event(
                mapped(),
                &BTreeMap::from([(
                    "platform_read".to_string(),
                    ToolPresentationEmitter::ToolBroker,
                )]),
            )
            .unwrap()
            .is_none()
        );
        let vendor = route_bound_dynamic_tool_event(
            mapped(),
            &BTreeMap::from([(
                "platform_read".to_string(),
                ToolPresentationEmitter::VendorStream,
            )]),
        )
        .unwrap()
        .expect("VendorStream presentation");
        assert!(vendor.runtime_event.is_none());
        assert!(matches!(
            vendor.presentation.event,
            agentdash_agent_protocol::BackboneEvent::ItemStarted(_)
        ));
        assert!(matches!(
            route_bound_dynamic_tool_event(mapped(), &BTreeMap::new()),
            Err(MappingError::MissingToolPresentationRoute { tool }) if tool == "platform_read"
        ));
    }

    #[test]
    fn codex_vendor_tool_callback_keeps_broker_internal_lifecycle_and_continues() {
        let mut coordinates = SourceCoordinateMap::default();
        let runtime_turn = RuntimeTurnId::new("runtime-turn").expect("runtime turn");
        coordinates.register_turn("source-turn", runtime_turn.clone());
        let emitters = BTreeMap::from([(
            "platform_read".to_string(),
            ToolPresentationEmitter::VendorStream,
        )]);

        let started = coordinates
            .map_notification(crate::rpc::RpcServerNotification {
                method: "item/started".to_string(),
                params: json!({
                    "threadId":"source-thread", "turnId":"source-turn", "startedAtMs":1,
                    "item": {
                        "id":"source-tool", "type":"dynamicToolCall",
                        "tool":"platform_read", "status":"inProgress",
                        "arguments":{"path":"README.md"}, "success":null,
                        "namespace":null, "durationMs":null, "contentItems":null
                    }
                }),
            })
            .expect("map vendor tool start")
            .expect("vendor tool start");
        let broker_start = started
            .runtime_event
            .clone()
            .expect("broker canonical start fixture");
        let started = route_bound_dynamic_tool_event(started, &emitters)
            .expect("route vendor tool start")
            .expect("vendor start presentation");

        let completed = coordinates
            .map_notification(crate::rpc::RpcServerNotification {
                method: "item/completed".to_string(),
                params: json!({
                    "threadId":"source-thread", "turnId":"source-turn", "completedAtMs":2,
                    "item": {
                        "id":"source-tool", "type":"dynamicToolCall",
                        "tool":"platform_read", "status":"completed",
                        "arguments":{"path":"README.md"}, "success":true,
                        "namespace":null, "durationMs":1,
                        "contentItems":[{"type":"inputText","text":"done"}]
                    }
                }),
            })
            .expect("map vendor tool terminal")
            .expect("vendor tool terminal");
        let broker_terminal = completed
            .runtime_event
            .clone()
            .expect("broker canonical terminal fixture");
        let completed = route_bound_dynamic_tool_event(completed, &emitters)
            .expect("route vendor tool terminal")
            .expect("vendor terminal presentation");

        assert!(started.runtime_event.is_none());
        assert!(completed.runtime_event.is_none());
        assert!(matches!(
            broker_start,
            RuntimeEvent::ItemStarted { ref turn_id, .. } if turn_id == &runtime_turn
        ));
        assert!(matches!(
            broker_terminal,
            RuntimeEvent::ItemTerminal { ref turn_id, .. } if turn_id == &runtime_turn
        ));
        let visible = [&started.presentation.event, &completed.presentation.event];
        assert_eq!(
            visible
                .iter()
                .filter(|event| matches!(
                    event,
                    agentdash_agent_protocol::BackboneEvent::ItemStarted(_)
                ))
                .count(),
            1
        );
        assert_eq!(
            visible
                .iter()
                .filter(|event| matches!(
                    event,
                    agentdash_agent_protocol::BackboneEvent::ItemCompleted(_)
                ))
                .count(),
            1
        );

        let final_message_started = coordinates
            .map_notification(crate::rpc::RpcServerNotification {
                method: "item/started".to_string(),
                params: json!({
                    "threadId":"source-thread", "turnId":"source-turn", "startedAtMs":3,
                    "item": {
                        "id":"source-message", "type":"agentMessage", "text":"",
                        "phase":null, "memoryCitation":null
                    }
                }),
            })
            .expect("map provider continuation start")
            .expect("final assistant message start");
        let final_message_started =
            route_bound_dynamic_tool_event(final_message_started, &emitters)
                .expect("route final assistant message start")
                .expect("final assistant message start remains visible");
        let final_message = coordinates
            .map_notification(crate::rpc::RpcServerNotification {
                method: "item/completed".to_string(),
                params: json!({
                    "threadId":"source-thread", "turnId":"source-turn", "completedAtMs":4,
                    "item": {
                        "id":"source-message", "type":"agentMessage", "text":"finished",
                        "phase":null, "memoryCitation":null
                    }
                }),
            })
            .expect("map provider continuation")
            .expect("final assistant message");
        let final_message = route_bound_dynamic_tool_event(final_message, &emitters)
            .expect("route final assistant message")
            .expect("final assistant message remains visible");
        assert!(matches!(
            final_message_started.runtime_event,
            Some(RuntimeEvent::ItemStarted { .. })
        ));
        assert!(matches!(
            final_message.runtime_event,
            Some(RuntimeEvent::ItemTerminal { .. })
        ));
        assert!(matches!(
            final_message.presentation.event,
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(_)
        ));
    }

    #[test]
    fn hook_started_completed_reconcile_is_idempotent_and_scoped_to_adapter_sources() {
        let mut state = CodexPumpState::default();
        let started = crate::rpc::RpcServerNotification {
            method: "hook/started".to_string(),
            params: json!({ "run": { "id": "run-1", "source": "sessionFlags" } }),
        };
        let completed = crate::rpc::RpcServerNotification {
            method: "hook/completed".to_string(),
            params: json!({ "run": { "id": "run-1", "source": "sessionFlags" } }),
        };
        reconcile_native_hook(&mut state, &started);
        reconcile_native_hook(&mut state, &completed);
        reconcile_native_hook(&mut state, &completed);
        assert_eq!(state.native_hook_runs.get("run-1"), Some(&true));

        reconcile_native_hook(
            &mut state,
            &crate::rpc::RpcServerNotification {
                method: "hook/completed".to_string(),
                params: json!({ "run": { "id": "user-run", "source": "user" } }),
            },
        );
        assert!(!state.native_hook_runs.contains_key("user-run"));
    }
}
