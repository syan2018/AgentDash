use std::sync::Arc;

use agentdash_agent_types::{AgentRuntimeDelegateSet, DynRuntimeToolPolicyDelegate};
use agentdash_application_ports::frame_launch_envelope::{
    FrameLaunchEnvelope, TerminalHookEffectBinding,
};
use agentdash_application_ports::launch::{
    BackendSelectionInput, BackendSelectionInputMode, LaunchCommand, LaunchPlanningInput,
};
use agentdash_domain::backend::{BackendExecutionLease, RuntimeBackendAnchor};
use agentdash_domain::common::AgentBackendRequirement;
use agentdash_spi::{ConnectorError, RestoredSessionState, Vfs};

use super::deps::LaunchPlanningDeps;
use super::{
    LaunchFollowUpSource, LaunchPlan, LaunchPlanInput, LaunchRestoreMode,
    RuntimeDelegateCompositionPlan, RuntimeDelegateFacetPlan,
};
use crate::backend_execution_placement::{
    BackendSelectionIntent, BackendSelectionRequest, ExecutionPlacementPlan,
    has_available_relay_executor, resolve_backend_execution_placement,
};
use crate::session::hook_delegate::HookRuntimeDelegate;
use crate::session::hook_injection_sink::{
    DynRuntimeHookInjectionSink, SessionRuntimeHookInjectionSink,
};
use crate::session::manual_compaction_delegate::ManualContextCompactionDelegate;
use crate::session::post_turn_handler::DynPostTurnHandler;
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::types::{
    HookSnapshotReloadTrigger, PromptLaunchPath, RuntimeTraceLaunchState,
    SessionRepositoryRehydrateMode, resolve_launch_prompt_payload, resolve_prompt_launch_path,
};

pub(in crate::session) struct LaunchPlanner<'a> {
    deps: LaunchPlanningDeps,
    _marker: std::marker::PhantomData<&'a ()>,
}

pub(in crate::session) struct LaunchPlannerInput<'a> {
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub command: &'a LaunchCommand,
    pub planning_input: LaunchPlanningInput,
    pub had_existing_runtime: bool,
    pub runtime_trace_state: RuntimeTraceLaunchState,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
    pub launch_envelope: FrameLaunchEnvelope,
    /// 来自 LifecycleAgent.needs_bootstrap() — 取代原 SessionMeta.bootstrap_state 判断。
    pub agent_needs_bootstrap: bool,
}

impl<'a> LaunchPlanner<'a> {
    pub(super) fn new(deps: LaunchPlanningDeps) -> Self {
        Self {
            deps,
            _marker: std::marker::PhantomData,
        }
    }

    pub async fn plan(&self, input: LaunchPlannerInput<'_>) -> Result<LaunchPlan, ConnectorError> {
        let sid = input.session_id.to_string();
        let command = input.command;

        let working_directory = input.launch_envelope.runtime.working_directory.clone();
        let executor_config = input.launch_envelope.launch_executor_config().clone();
        let capability_state = input.launch_envelope.launch_capability_state().clone();

        let mut context_bundle = input.launch_envelope.context.context_bundle.clone();
        let terminal_hook_effect_binding = input
            .launch_envelope
            .command
            .terminal_hook_effect_binding
            .clone();
        let typed_vfs = input.launch_envelope.launch_vfs().clone();
        let environment_variables = input.launch_envelope.command.environment_variables.clone();
        let input_blocks = input.launch_envelope.command.input.clone();
        let base_capability_override = input.launch_envelope.runtime.base_capability_state.clone();

        let mut prompt_input = command.prompt().clone();
        if let Some(blocks) = input_blocks.clone() {
            prompt_input.input = Some(blocks);
        }
        if let Some(config) = Some(executor_config.clone()) {
            prompt_input.executor_config = Some(config);
        }
        if !environment_variables.is_empty() {
            prompt_input.environment_variables = environment_variables.clone();
        }
        let resolved_payload = resolve_launch_prompt_payload(&prompt_input)
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;
        let pending_capability_transitions = input
            .requested_runtime_commands
            .iter()
            .map(|command| command.pending_capability_state_transition())
            .collect::<Vec<_>>();
        let default_mount_root = typed_vfs
            .default_mount()
            .map(|mount| {
                crate::session::path_policy::resolve_session_working_directory(&mount.root_ref)
                    .map_err(ConnectorError::InvalidConfig)
            })
            .transpose()?
            .unwrap_or_else(|| working_directory.clone());
        let base_capability_state =
            base_capability_override.unwrap_or_else(|| capability_state.clone());

        let supports_repository_restore = self
            .deps
            .connector
            .supports_repository_restore(executor_config.executor.as_str());
        let prompt_launch_path = resolve_prompt_launch_path(
            &input.runtime_trace_state,
            input.had_existing_runtime,
            supports_repository_restore,
            input.agent_needs_bootstrap,
            command.source(),
        );
        let hook_snapshot_reload = if matches!(prompt_launch_path, PromptLaunchPath::OwnerBootstrap)
        {
            HookSnapshotReloadTrigger::Reload
        } else {
            HookSnapshotReloadTrigger::None
        };
        let is_owner_bootstrap = hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;
        let hook_runtime = match self
            .deps
            .hooks
            .resolve_hook_runtime(
                input.session_id,
                input.turn_id,
                input.launch_envelope.frame.surface.frame_id,
                input.launch_envelope.frame.pending_frame.as_ref(),
                &executor_config,
                &working_directory,
                is_owner_bootstrap,
            )
            .await
        {
            Ok(hs) => hs,
            Err(error) => return Err(error),
        };

        let hook_snapshot_contribution = hook_runtime.as_ref().map(|hs| {
            let snapshot = hs.snapshot();
            let contribution: crate::context::Contribution = (&snapshot).into();
            contribution.fragments
        });
        if let Some(bundle) = context_bundle.as_mut()
            && let Some(fragments) = hook_snapshot_contribution.as_ref()
        {
            bundle.merge(fragments.clone());
        }
        // context_bundle 在 hook contribution merge 后直接传递给 LaunchPlanInput

        let context_audit_bus = self.deps.current_context_audit_bus().await;
        let hook_runtime_delegate = hook_runtime.as_ref().map(|hs| {
            let injection_sink: DynRuntimeHookInjectionSink =
                Arc::new(SessionRuntimeHookInjectionSink::new(
                    self.deps.runtime_registry.clone(),
                    context_audit_bus.clone(),
                ));
            HookRuntimeDelegate::new_facets(
                hs.clone(),
                Some(default_mount_root.to_string_lossy().replace('\\', "/")),
                context_audit_bus.clone(),
                Some(injection_sink),
            )
        });
        let hook_tool_policy: Option<DynRuntimeToolPolicyDelegate> = hook_runtime_delegate
            .as_ref()
            .map(|delegate| delegate.clone() as DynRuntimeToolPolicyDelegate);
        let mut runtime_delegates = hook_runtime_delegate
            .as_ref()
            .map(|delegate| AgentRuntimeDelegateSet::from_all_facets(delegate.clone()))
            .unwrap_or_default();
        let mut runtime_delegate_composition = RuntimeDelegateCompositionPlan {
            hook_facets: hook_runtime_delegate.is_some(),
            mailbox_turn_boundary: false,
            admission_tool_policy: false,
        };
        if let Some(mailbox_port) = self.deps.current_mailbox_runtime_port().await {
            runtime_delegate_composition.mailbox_turn_boundary = true;
            runtime_delegates.turn_boundary = Some(mailbox_port.turn_boundary_delegate(
                input.session_id.to_string(),
                runtime_delegates.turn_boundary.take(),
            ));
        }
        if let Some(repo) = self.deps.manual_context_compaction_request_repo.as_ref() {
            runtime_delegates.compaction = Some(ManualContextCompactionDelegate::wrap(
                input.session_id.to_string(),
                input.turn_id.to_string(),
                repo.clone(),
                runtime_delegates.compaction.take(),
            ));
        }
        let restore_mode = match prompt_launch_path {
            PromptLaunchPath::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::SystemContext,
            ) => LaunchRestoreMode::SystemContext,
            PromptLaunchPath::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            ) => LaunchRestoreMode::ExecutorState,
            _ => LaunchRestoreMode::None,
        };
        let restored_session_state = match prompt_launch_path {
            PromptLaunchPath::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            ) => {
                let transcript = self
                    .deps
                    .eventing
                    .build_projected_transcript(input.session_id)
                    .await
                    .map_err(|error| {
                        ConnectorError::Runtime(format!(
                            "重建 session `{}` 历史消息失败: {error}",
                            input.session_id
                        ))
                    })?;
                let entries = transcript.entries;
                Some(RestoredSessionState {
                    messages: entries.iter().map(|entry| entry.message.clone()).collect(),
                    message_refs: entries
                        .iter()
                        .map(|entry| Some(entry.message_ref.clone()))
                        .collect(),
                })
            }
            _ => None,
        };

        let (resolved_follow_up_session_id, follow_up_source) = input
            .command
            .follow_up_session_id()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| (Some(value.to_string()), LaunchFollowUpSource::Explicit))
            .or_else(|| {
                input
                    .runtime_trace_state
                    .executor_session_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| (Some(value.to_string()), LaunchFollowUpSource::SessionMeta))
            })
            .unwrap_or((None, LaunchFollowUpSource::None));
        let post_turn_handler = self
            .resolve_terminal_hook_effect_handler(input.session_id, terminal_hook_effect_binding)
            .await?;
        let backend_execution = self
            .resolve_backend_execution_placement(
                input.session_id,
                input.turn_id,
                &input.planning_input,
                Some(&typed_vfs),
                input
                    .launch_envelope
                    .runtime
                    .runtime_backend_anchor
                    .as_ref(),
                &executor_config.executor,
                command.reason_tag(),
            )
            .await?;
        // 将更新后的 context_bundle 写回 envelope
        let mut launch_envelope = input.launch_envelope;
        launch_envelope.context.context_bundle = context_bundle.clone();
        let launch_plan = LaunchPlan::build(LaunchPlanInput {
            resolved_payload,
            launch_envelope,
            session_id: sid,
            turn_id: input.turn_id.to_string(),
            source: command.source(),
            input_source: command.input_source().cloned(),
            launch_path: prompt_launch_path,
            restore_mode,
            hook_snapshot_reload,
            hook_snapshot_contribution,
            follow_up_session_id: resolved_follow_up_session_id.clone(),
            follow_up_source,
            requested_runtime_commands: input.requested_runtime_commands,
            pending_capability_transitions,
            base_capability_state,
            environment_variables: prompt_input.environment_variables.clone(),
            hook_runtime: hook_runtime.clone(),
            capability_state: capability_state.clone(),
            runtime_delegates,
            runtime_delegate_facets: RuntimeDelegateFacetPlan {
                composition: runtime_delegate_composition,
                hook_tool_policy,
            },
            restored_session_state,
            post_turn_handler,
            backend_execution,
        });

        Ok(launch_plan)
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_backend_execution_placement(
        &self,
        session_id: &str,
        turn_id: &str,
        planning_input: &LaunchPlanningInput,
        typed_vfs: Option<&Vfs>,
        runtime_backend_anchor: Option<&RuntimeBackendAnchor>,
        executor_id: &str,
        reason_tag: &str,
    ) -> Result<Option<ExecutionPlacementPlan>, ConnectorError> {
        let backend_requirement = planning_input
            .backend_requirement
            .unwrap_or(AgentBackendRequirement::Optional);
        let backend_required = backend_requirement == AgentBackendRequirement::Required;
        let explicit_selection = planning_input.backend_selection.is_some();
        let Some(transport) = self.deps.backend_execution_transport.as_ref() else {
            if explicit_selection || backend_required {
                return Err(ConnectorError::InvalidConfig(
                    "backend placement 已要求，但 session runtime 未注入 backend execution placement transport"
                        .to_string(),
                ));
            }
            return Ok(None);
        };
        let Some(lease_repo) = self.deps.backend_execution_lease_repo.as_ref() else {
            if explicit_selection || backend_required {
                return Err(ConnectorError::InvalidConfig(
                    "backend placement 已要求，但 session runtime 未注入 backend execution lease repository"
                        .to_string(),
                ));
            }
            return Ok(None);
        };

        if backend_required
            && !explicit_selection
            && planning_input.authorized_backend_ids.is_empty()
        {
            return Err(ConnectorError::ConnectionFailed(
                "当前 Project 没有已授权的 backend".to_string(),
            ));
        }

        let request = match planning_input.backend_selection.as_ref() {
            Some(selection) => Some(selection_request_from_input(
                executor_id,
                selection,
                &planning_input.authorized_backend_ids,
                reason_tag,
            )?),
            None if runtime_backend_anchor.is_some()
                || backend_required
                || has_available_relay_executor(transport.as_ref(), executor_id) =>
            {
                Some(selection_request_from_runtime_anchor(
                    executor_id,
                    runtime_backend_anchor,
                    &planning_input.authorized_backend_ids,
                    reason_tag,
                ))
            }
            None => None,
        };
        let Some(request) = request else {
            return Ok(None);
        };

        let mut placement = match resolve_backend_execution_placement(
            transport.as_ref(),
            lease_repo.as_ref(),
            &request,
        )
        .await
        {
            Ok(placement) => placement,
            Err(error)
                if backend_requirement == AgentBackendRequirement::Optional
                    && !explicit_selection =>
            {
                return match error {
                    ConnectorError::ConnectionFailed(_) => Ok(None),
                    other => Err(other),
                };
            }
            Err(error) => return Err(error),
        };
        let mut lease = BackendExecutionLease::claimed(
            placement.backend_id.clone(),
            session_id.to_string(),
            turn_id.to_string(),
            placement.executor_id.clone(),
            placement.selection_mode,
            placement.claim_reason.clone(),
        );
        lease.workspace_id = None;
        lease.root_ref = typed_vfs
            .and_then(|vfs| vfs.default_mount())
            .map(|mount| mount.root_ref.clone());
        let lease_id = lease.id;
        lease_repo.claim(&lease).await.map_err(|error| {
            ConnectorError::Runtime(format!("创建 backend execution lease 失败: {error}"))
        })?;
        placement = placement.with_lease_id(lease_id);
        Ok(Some(placement))
    }

    async fn resolve_terminal_hook_effect_handler(
        &self,
        session_id: &str,
        binding: Option<TerminalHookEffectBinding>,
    ) -> Result<Option<DynPostTurnHandler>, ConnectorError> {
        let Some(binding) = binding else {
            return Ok(None);
        };
        let payload = serde_json::json!({
            "handler": binding.handler,
            "supported_effect_kinds": binding.supported_effect_kinds,
        });
        let Some(registry) = self.deps.hook_effect_handler_registry.read().await.clone() else {
            return Err(ConnectorError::Runtime(
                "terminal hook effect binding 存在，但 durable handler registry 未注入".to_string(),
            ));
        };
        registry
            .handler_for(session_id, &payload)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("解析 terminal hook effect handler 失败: {error}"))
            })
    }
}

fn selection_request_from_input(
    executor_id: &str,
    selection: &BackendSelectionInput,
    authorized_backend_ids: &[String],
    reason_tag: &str,
) -> Result<BackendSelectionRequest, ConnectorError> {
    let reason = Some(format!("session launch: {reason_tag}"));
    let request = match selection.mode {
        BackendSelectionInputMode::Explicit => {
            let backend_id = required_backend_id(selection, "explicit")?;
            BackendSelectionRequest::explicit(executor_id, backend_id, reason)
        }
        BackendSelectionInputMode::AutoIdle => {
            BackendSelectionRequest::auto_idle(executor_id, reason)
        }
        BackendSelectionInputMode::WorkspaceBinding => {
            let backend_id = required_backend_id(selection, "workspace_binding")?;
            BackendSelectionRequest::workspace_binding(executor_id, backend_id, reason)
        }
    };
    Ok(request.with_authorized_backend_ids(authorized_backend_ids.to_vec()))
}

fn selection_request_from_runtime_anchor(
    executor_id: &str,
    runtime_backend_anchor: Option<&RuntimeBackendAnchor>,
    authorized_backend_ids: &[String],
    reason_tag: &str,
) -> BackendSelectionRequest {
    let reason = Some(format!("session launch: {reason_tag}"));
    let request = runtime_backend_anchor
        .map(|anchor| anchor.backend_id().to_string())
        .map(|backend_id| BackendSelectionRequest {
            executor_id: executor_id.to_string(),
            intent: BackendSelectionIntent::WorkspaceBinding { backend_id },
            reason: reason.clone(),
            authorized_backend_ids: Vec::new(),
        })
        .unwrap_or_else(|| BackendSelectionRequest::auto_idle(executor_id, reason));
    request.with_authorized_backend_ids(authorized_backend_ids.to_vec())
}

fn required_backend_id(
    selection: &BackendSelectionInput,
    mode: &str,
) -> Result<String, ConnectorError> {
    selection
        .backend_id
        .as_deref()
        .map(str::trim)
        .filter(|backend_id| !backend_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "backend_selection.mode={mode} 时必须提供 backend_id"
            ))
        })
}
