use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverThreadId, DriverTurnId, RuntimeBindingId, RuntimeDriverGeneration,
    RuntimeItemId, RuntimeThreadId, RuntimeTurnId, ToolSetRevision,
};
use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
};
use agentdash_integration_api::{
    AgentRuntimeToolCallback, AuthIdentity, DriverToolDefinition, DriverToolInvocation,
    DriverToolOutcome,
};
use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::context::{NativeBindingContext, NativeToolCallContext};

fn decode_completed_content(
    output: &serde_json::Value,
) -> Result<Vec<ContentPart>, AgentToolError> {
    let Some(items) = output.get("content_items") else {
        return Ok(Vec::new());
    };
    let items = serde_json::from_value::<
        Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    >(items.clone())
    .map_err(|error| {
        AgentToolError::ExecutionFailed(format!(
            "native tool callback returned invalid typed content_items: {error}"
        ))
    })?;
    Ok(items
        .into_iter()
        .map(|item| match item {
            agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText { text } => {
                ContentPart::text(text)
            }
            agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputImage {
                image_url,
            } => ContentPart::image("image/*", image_url),
        })
        .collect())
}

pub(crate) struct NativeRuntimeTool {
    definition: DriverToolDefinition,
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    source_thread_id: DriverThreadId,
    runtime_thread_id: RuntimeThreadId,
    active_turn: Arc<RwLock<Option<DriverTurnId>>>,
    active_runtime_turn: Arc<RwLock<Option<RuntimeTurnId>>>,
    tool_set_revision: ToolSetRevision,
    callback: Arc<dyn AgentRuntimeToolCallback>,
    authorization_identity: Option<AuthIdentity>,
    item_identities:
        Arc<RwLock<std::collections::BTreeMap<(DriverTurnId, DriverItemId), RuntimeItemId>>>,
}

impl NativeRuntimeTool {
    pub(crate) fn new(
        definition: DriverToolDefinition,
        binding: NativeBindingContext,
        call: NativeToolCallContext,
        callback: Arc<dyn AgentRuntimeToolCallback>,
    ) -> Self {
        Self {
            definition,
            binding_id: binding.binding_id,
            generation: binding.generation,
            source_thread_id: binding.source_thread_id,
            runtime_thread_id: binding.runtime_thread_id,
            active_turn: call.active_turn,
            active_runtime_turn: call.active_runtime_turn,
            tool_set_revision: call.tool_set_revision,
            callback,
            authorization_identity: binding.authorization_identity,
            item_identities: call.item_identities,
        }
    }
}

#[async_trait]
impl AgentTool for NativeRuntimeTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.definition.parameters_schema.clone()
    }
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        use agentdash_agent_runtime_contract::ToolProtocolProjection as P;
        Some(match &self.definition.protocol_projection {
            P::Command => agentdash_agent_types::ToolProtocolProjector::Command,
            P::FileChange => agentdash_agent_types::ToolProtocolProjector::FileChange,
            P::FsRead => agentdash_agent_types::ToolProtocolProjector::FsRead,
            P::FsGrep => agentdash_agent_types::ToolProtocolProjector::FsGrep,
            P::FsGlob => agentdash_agent_types::ToolProtocolProjector::FsGlob,
            P::Mcp { server_key } => agentdash_agent_types::ToolProtocolProjector::Mcp {
                server_key: server_key.clone(),
            },
            P::Dynamic { namespace } => agentdash_agent_types::ToolProtocolProjector::Dynamic {
                namespace: namespace.clone(),
            },
        })
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some(self.definition.parity_fixture_id.clone())
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let source_turn_id = self.active_turn.read().await.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed("native tool invoked without an active turn".into())
        })?;
        let source_item_id = tool_call_id.parse::<DriverItemId>().map_err(|error| {
            AgentToolError::ExecutionFailed(format!("invalid native tool call identity: {error}"))
        })?;
        let turn_id = self
            .active_runtime_turn
            .read()
            .await
            .clone()
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "native tool invoked without a canonical Runtime turn".into(),
                )
            })?;
        let identity_key = (source_turn_id.clone(), source_item_id.clone());
        let item_id = if let Some(item_id) = self
            .item_identities
            .read()
            .await
            .get(&identity_key)
            .cloned()
        {
            item_id
        } else {
            let allocated = RuntimeItemId::new(format!(
                "native-runtime-tool-{}-{}",
                source_turn_id, source_item_id
            ))
            .map_err(|error| {
                AgentToolError::ExecutionFailed(format!(
                    "invalid allocated canonical item identity: {error}"
                ))
            })?;
            self.item_identities
                .write()
                .await
                .entry(identity_key)
                .or_insert_with(|| allocated.clone())
                .clone()
        };
        let callback = self.callback.invoke(DriverToolInvocation {
            thread_id: self.runtime_thread_id.clone(),
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            binding_id: self.binding_id.clone(),
            generation: self.generation,
            source_thread_id: self.source_thread_id.clone(),
            source_turn_id: source_turn_id.clone(),
            source_item_id: source_item_id.clone(),
            tool_set_revision: self.tool_set_revision,
            tool_name: self.definition.name.clone(),
            arguments: args,
            timeout_ms: 120_000,
            authorization_identity: self.authorization_identity.clone(),
        });
        let outcome = tokio::select! {
            _ = cancel.cancelled() => Err("tool call cancelled".to_string()),
            outcome = callback => outcome.map_err(|error| error.to_string()),
        };
        let outcome = outcome.map_err(AgentToolError::ExecutionFailed)?;
        match outcome {
            DriverToolOutcome::Completed { output, is_error } => {
                let content = decode_completed_content(&output)?;
                Ok(AgentToolResult {
                    content,
                    is_error,
                    details: Some(output),
                })
            }
            DriverToolOutcome::InteractionRequired { reason, .. } => {
                Err(AgentToolError::ExecutionFailed(format!(
                    "tool interaction must be resolved before callback completion: {reason}"
                )))
            }
            DriverToolOutcome::Denied { reason } => Err(AgentToolError::ExecutionFailed(reason)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        str::FromStr,
    };

    use agentdash_agent_runtime::*;
    use agentdash_agent_runtime_contract::*;
    use agentdash_integration_api::{DriverToolCallbackError, DriverToolOutcome};

    use super::*;

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid test id")
    }

    #[test]
    fn malformed_native_callback_content_is_an_explicit_failure() {
        let error = decode_completed_content(&serde_json::json!({
            "content_items": [{"type":"inputText"}]
        }))
        .expect_err("missing typed text must not be silently dropped");
        assert!(
            error.to_string().contains("invalid typed content_items"),
            "{error}"
        );
    }

    struct Allow;
    #[async_trait]
    impl ToolBrokerPolicyPort for Allow {
        async fn validate_binding(
            &self,
            _: &ToolBrokerInvocation,
        ) -> Result<ToolGuardDecision, ToolBrokerError> {
            Ok(ToolGuardDecision::Allowed(ToolPolicyCheck { revision: 1 }))
        }
        async fn authorize_capability(
            &self,
            _: &ToolBrokerInvocation,
            _: &ToolContribution,
        ) -> Result<ToolGuardDecision, ToolBrokerError> {
            Ok(ToolGuardDecision::Allowed(ToolPolicyCheck { revision: 1 }))
        }
        async fn authorize_permission(
            &self,
            _: &ToolBrokerInvocation,
            _: &ToolContribution,
        ) -> Result<ToolPermissionDecision, ToolBrokerError> {
            Ok(ToolPermissionDecision::Allowed(ToolPolicyCheck {
                revision: 1,
            }))
        }
        async fn authorize_vfs(
            &self,
            _: &ToolBrokerInvocation,
            _: &ToolContribution,
        ) -> Result<ToolGuardDecision, ToolBrokerError> {
            Ok(ToolGuardDecision::Allowed(ToolPolicyCheck { revision: 1 }))
        }
    }

    struct NoCredentials;
    #[async_trait]
    impl ToolCredentialResolver for NoCredentials {
        async fn resolve(&self, _: &[String]) -> Result<CredentialMaterial, ToolBrokerError> {
            Ok(CredentialMaterial::new(BTreeMap::new()))
        }
    }

    struct Complete;
    #[async_trait]
    impl ToolExecutionPort for Complete {
        async fn execute(
            &self,
            request: ToolExecutionRequest,
        ) -> Result<ToolBrokerResult, ToolBrokerError> {
            Ok(ToolBrokerResult {
                output: serde_json::json!({"arguments": request.invocation.arguments, "changes": []}),
                is_error: false,
            })
        }
    }

    struct BrokerCallback {
        broker: PlatformToolBroker,
    }
    #[async_trait]
    impl AgentRuntimeToolCallback for BrokerCallback {
        async fn invoke(
            &self,
            request: DriverToolInvocation,
        ) -> Result<DriverToolOutcome, DriverToolCallbackError> {
            assert_ne!(request.item_id.as_str(), request.source_item_id.as_str());
            let outcome = self
                .broker
                .invoke(
                    ToolChannel::DirectCallback,
                    ToolBrokerInvocation {
                        coordinates: ToolCallCoordinates {
                            thread_id: request.thread_id,
                            turn_id: request.turn_id,
                            item_id: request.item_id,
                            binding_id: request.binding_id,
                            binding_generation: request.generation,
                            tool_set_revision: request.tool_set_revision,
                        },
                        tool_name: request.tool_name,
                        arguments: request.arguments,
                        timeout_ms: request.timeout_ms,
                    },
                    CancellationToken::new(),
                )
                .await
                .map_err(|error| DriverToolCallbackError::ProtocolViolation {
                    reason: error.to_string(),
                })?;
            match outcome {
                ToolBrokerOutcome::Terminal { result, .. } => Ok(DriverToolOutcome::Completed {
                    output: result.output,
                    is_error: result.is_error,
                }),
                ToolBrokerOutcome::ApprovalRequired {
                    interaction_id,
                    reason,
                } => Ok(DriverToolOutcome::InteractionRequired {
                    interaction_id,
                    reason,
                }),
                ToolBrokerOutcome::Denied { reason, .. } => {
                    Ok(DriverToolOutcome::Denied { reason })
                }
            }
        }
    }

    fn contribution(name: &str, projection: ToolProtocolProjection) -> ToolContribution {
        ToolContribution {
            meta: ContributionMeta {
                key: format!("tool:{name}"),
                source: SurfaceSourceRef {
                    layer: "test".into(),
                    key: name.into(),
                },
                priority: 1,
                requirement: ContributionRequirement::Required,
            },
            runtime_name: name.into(),
            description: name.into(),
            parameters_schema: serde_json::json!({"type":"object"}),
            capability_key: name.into(),
            tool_path: name.into(),
            allowed_channels: [ToolChannel::DirectCallback].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: projection,
            presentation_emitter: ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: format!("main_tool_{name}_lifecycle"),
        }
    }

    fn profile() -> RuntimeProfile {
        RuntimeProfile {
            reference_class: ReferenceRuntimeClass::ManagedThread,
            input: InputProfile {
                modalities: BTreeSet::new(),
            },
            instruction: InstructionProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            tools: ToolProfile {
                channels: [ToolChannel::DirectCallback].into(),
                configuration_boundary: ConfigurationBoundary::Binding,
                cancellation: true,
            },
            workspace: WorkspaceProfile {
                capabilities: BTreeSet::new(),
                mechanism: DeliveryMechanism::Native,
            },
            interactions: InteractionProfile {
                kinds: BTreeSet::new(),
                durable_correlation: true,
            },
            lifecycle: [
                LifecycleCapability::ThreadStart,
                LifecycleCapability::TurnStart,
            ]
            .into(),
            hooks: HookProfile {
                points: Vec::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            context: ContextProfile {
                capabilities: BTreeSet::new(),
                fidelity: ContextFidelity::Opaque,
                activation_idempotent: false,
            },
            telemetry_config: BTreeSet::new(),
        }
    }

    struct TestTerminalPresentationProjector;

    impl RuntimeApplicationPresentationProjector for TestTerminalPresentationProjector {
        fn project_terminal(
            &self,
            context: RuntimeTerminalPresentationContext,
        ) -> Result<Vec<RuntimePresentationInput>, RuntimeApplicationPresentationProjectionError>
        {
            let terminal_type = match context.terminal {
                RuntimeTurnTerminal::Completed => "turn_completed",
                RuntimeTurnTerminal::Interrupted => "turn_interrupted",
                RuntimeTurnTerminal::Lost => "turn_lost",
                RuntimeTurnTerminal::Refused
                | RuntimeTurnTerminal::LimitReached
                | RuntimeTurnTerminal::Failed => "turn_failed",
            };
            Ok(vec![RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: Some(context.runtime_turn_id.clone()),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(context.presentation_thread_id.to_string()),
                    source_turn_id: Some(context.presentation_turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: Some(format!(
                        "test-turn-terminal:{}:{terminal_type}",
                        context.runtime_turn_id
                    )),
                    source_entry_index: None,
                },
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                            key: "turn_terminal".into(),
                            value: serde_json::json!({
                                "terminal_type": terminal_type,
                                "message": context.message,
                                "diagnostic": context.diagnostic,
                                "started_at_ms": context.started_at_ms,
                                "completed_at_ms": context.completed_at_ms,
                            }),
                        },
                    ),
                ),
            }])
        }
    }

    #[tokio::test]
    async fn native_tool_uses_broker_owner_projection_for_patch_shell_and_replay() {
        let store = Arc::new(RuntimeStoreFixture::default());
        let runtime =
            ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector));
        runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: id("native-thread-start"),
                    idempotency_key: id("native-thread-key"),
                    expected_thread_revision: None,
                    actor: RuntimeActor::System {
                        component: "native-test".into(),
                    },
                },
                command: RuntimeCommand::ThreadStart {
                    thread_id: id("native-thread"),
                    presentation_thread_id: id("native-presentation-thread"),
                    presentation_turn_id: None,
                    binding_id: id("native-binding"),
                    driver_generation: RuntimeDriverGeneration(7),
                    source_thread_id: id("native-source-thread"),
                    profile_digest: id("native-profile"),
                    bound_profile: Box::new(profile()),
                    input: Vec::new(),
                    surface: Box::new(RuntimeSurfaceDescriptor {
                        source_frame_id: "native-tool-frame-1".to_string(),
                        surface_revision: SurfaceRevision(1),
                        surface_digest: id("native-surface"),
                        vfs_digest: "native-vfs".to_string(),
                        context_recipe_revision: ContextRecipeRevision(1),
                        context_digest: id("native-context"),
                        settings_revision: ThreadSettingsRevision(0),
                        tool_set_revision: ToolSetRevision(4),
                        tool_set_digest: "native-tool-set-4".to_string(),
                        hook_plan: BoundRuntimeHookPlan {
                            revision: HookPlanRevision(1),
                            digest: id("native-hooks"),
                            entries: Vec::new(),
                        },
                        terminal_hook_effect_binding: None,
                    }),
                    settings_revision: ThreadSettingsRevision(0),
                },
            })
            .await
            .expect("start thread");
        runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: id("native-turn-start"),
                    idempotency_key: id("native-turn-key"),
                    expected_thread_revision: Some(RuntimeRevision(3)),
                    actor: RuntimeActor::System {
                        component: "native-test".into(),
                    },
                },
                command: RuntimeCommand::TurnStart {
                    thread_id: id("native-thread"),
                    presentation_turn_id: id("native-source-turn"),
                    input: Vec::new(),
                },
            })
            .await
            .expect("start turn");
        let tools = vec![
            contribution("fs_apply_patch", ToolProtocolProjection::FileChange),
            contribution("shell_exec", ToolProtocolProjection::Command),
        ];
        let broker = PlatformToolBroker::new(
            ToolCatalogRevision {
                revision: ToolSetRevision(4),
                digest: "catalog".into(),
                tools: tools.clone(),
                mcp_servers: Vec::new(),
            },
            id("native-binding"),
            RuntimeDriverGeneration(7),
            PlatformToolBrokerDeps {
                repository: Arc::new(ToolBrokerRepositoryFixture::default()),
                journal: Arc::new(ManagedRuntimeToolJournal::new(store.clone())),
                policy: Arc::new(Allow),
                credentials: Arc::new(NoCredentials),
                executor: Arc::new(Complete),
            },
        );
        let callback: Arc<dyn AgentRuntimeToolCallback> = Arc::new(BrokerCallback { broker });
        let binding = NativeBindingContext {
            binding_id: id("native-binding"),
            generation: RuntimeDriverGeneration(7),
            source_thread_id: id("native-source-thread"),
            runtime_thread_id: id("native-thread"),
            authorization_identity: None,
        };
        let call = NativeToolCallContext {
            active_turn: Arc::new(RwLock::new(Some(id("native-source-turn")))),
            active_runtime_turn: Arc::new(RwLock::new(Some(id("turn-native-turn-start")))),
            tool_set_revision: ToolSetRevision(4),
            item_identities: Arc::new(RwLock::new(BTreeMap::new())),
        };
        let patch = NativeRuntimeTool::new(
            DriverToolDefinition {
                name: "fs_apply_patch".into(),
                description: "patch".into(),
                parameters_schema: serde_json::json!({}),
                channels: vec![ToolChannel::DirectCallback],
                protocol_projection: ToolProtocolProjection::FileChange,
                parity_fixture_id: "main_tool_fs_apply_patch_lifecycle".into(),
            },
            binding.clone(),
            call.clone(),
            callback.clone(),
        );
        let patch_args = serde_json::json!({"patch":"*** Begin Patch\n*** Add File: main://new.txt\n+hello\n*** End Patch"});
        patch
            .execute(
                "native-patch",
                patch_args.clone(),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("patch callback");
        patch
            .execute("native-patch", patch_args, CancellationToken::new(), None)
            .await
            .expect("patch replay");
        let shell = NativeRuntimeTool::new(
            DriverToolDefinition {
                name: "shell_exec".into(),
                description: "shell".into(),
                parameters_schema: serde_json::json!({}),
                channels: vec![ToolChannel::DirectCallback],
                protocol_projection: ToolProtocolProjection::Command,
                parity_fixture_id: "main_tool_shell_exec_lifecycle".into(),
            },
            binding,
            call,
            callback,
        );
        shell
            .execute(
                "native-shell-write",
                serde_json::json!({"operation":"write","terminal_id":"term-1","data":"hello"}),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("shell callback");
        let snapshot = store
            .load_thread(&id("native-thread"))
            .await
            .expect("load")
            .expect("thread");
        let patch_item = snapshot
            .items
            .get(&id("native-runtime-tool-native-source-turn-native-patch"))
            .expect("patch item");
        match patch_item.initial_content.item() {
            agentdash_agent_protocol::AgentDashThreadItem::Codex(
                agentdash_agent_protocol::CodexThreadItem::FileChange { changes, .. },
            ) => assert!(!changes.is_empty()),
            other => panic!("unexpected patch item: {other:?}"),
        }
        let shell_item = snapshot
            .items
            .get(&id(
                "native-runtime-tool-native-source-turn-native-shell-write",
            ))
            .expect("shell item");
        match shell_item.initial_content.item() {
            agentdash_agent_protocol::AgentDashThreadItem::AgentDash(
                agentdash_agent_protocol::AgentDashNativeThreadItem::TerminalControl {
                    input,
                    terminal_id,
                    ..
                },
            ) => {
                assert_eq!(input.as_deref(), Some("hello"));
                assert_eq!(terminal_id, "term-1");
            }
            other => panic!("unexpected shell item: {other:?}"),
        }
    }
}
