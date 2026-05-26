use std::sync::Arc;

use agentdash_domain::backend::BackendExecutionLease;
use agentdash_spi::{ConnectorError, RestoredSessionState, Vfs};

use super::deps::LaunchPlanningDeps;
use super::{LaunchCommand, LaunchFollowUpSource, LaunchPlan, LaunchPlanInput, LaunchRestoreMode};
use crate::backend_execution_placement::{
    BackendSelectionIntent, BackendSelectionRequest, ExecutionPlacementPlan,
    has_available_relay_executor, resolve_backend_execution_placement,
};
use crate::session::construction::SessionConstructionPlan;
use crate::session::hook_delegate::{
    DynRuntimeHookInjectionSink, HookRuntimeDelegate, SessionRuntimeHookInjectionSink,
};
use crate::session::post_turn_handler::{DynPostTurnHandler, TerminalHookEffectBinding};
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::types::{
    BackendSelectionInput, BackendSelectionInputMode, HookSnapshotReloadTrigger, SessionMeta,
    SessionPromptLifecycle, SessionRepositoryRehydrateMode, resolve_session_prompt_lifecycle,
};

pub(in crate::session) struct LaunchPlanner<'a> {
    deps: LaunchPlanningDeps,
    _marker: std::marker::PhantomData<&'a ()>,
}

pub(in crate::session) struct LaunchPlannerInput<'a> {
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub command: &'a LaunchCommand,
    pub had_existing_runtime: bool,
    pub session_meta: &'a SessionMeta,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
    pub construction: SessionConstructionPlan,
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
        let mut construction = input.construction;
        construction
            .validate_for_launch()
            .map_err(ConnectorError::InvalidConfig)?;
        let mut context_bundle = construction.context.bundle.clone();
        let terminal_hook_effect_binding =
            construction.effects.terminal_hook_effect_binding.clone();
        let mut prompt_input = command.user_input().clone();
        if let Some(blocks) = construction.prompt.prompt_blocks.clone() {
            prompt_input.prompt_blocks = Some(blocks);
        }
        if let Some(config) = construction.execution_profile.executor_config.clone() {
            prompt_input.executor_config = Some(config);
        }
        if !construction.prompt.environment_variables.is_empty() {
            prompt_input.env = construction.prompt.environment_variables.clone();
        }
        let resolved_payload = prompt_input
            .resolve_prompt_payload()
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;
        let pending_capability_transitions = input
            .requested_runtime_commands
            .iter()
            .map(|command| command.transition.clone())
            .collect::<Vec<_>>();
        let working_directory = construction
            .workspace
            .working_directory
            .clone()
            .expect("validated construction must contain working_directory");
        let default_mount_root = construction
            .surface
            .vfs
            .as_ref()
            .and_then(|vfs| vfs.default_mount())
            .map(|mount| {
                crate::session::path_policy::resolve_session_working_directory(&mount.root_ref)
                    .map_err(ConnectorError::InvalidConfig)
            })
            .transpose()?
            .unwrap_or_else(|| working_directory.clone());
        let executor_config = construction
            .execution_profile
            .executor_config
            .clone()
            .expect("validated construction must contain executor_config");
        let capability_state = construction
            .projections
            .capability_state
            .clone()
            .expect("validated construction must contain capability_state");
        let base_capability_state = construction
            .resolution
            .runtime_base_capability_state
            .clone()
            .unwrap_or_else(|| capability_state.clone());

        let supports_repository_restore = self
            .deps
            .connector
            .supports_repository_restore(executor_config.executor.as_str());
        let prompt_lifecycle = resolve_session_prompt_lifecycle(
            input.session_meta,
            input.had_existing_runtime,
            supports_repository_restore,
        );
        let hook_snapshot_reload =
            if matches!(prompt_lifecycle, SessionPromptLifecycle::OwnerBootstrap) {
                HookSnapshotReloadTrigger::Reload
            } else {
                HookSnapshotReloadTrigger::None
            };
        let is_owner_bootstrap = hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;
        let hook_session = match self
            .deps
            .hooks
            .resolve_hook_session(
                input.session_id,
                input.turn_id,
                &executor_config,
                &working_directory,
                is_owner_bootstrap,
            )
            .await
        {
            Ok(hs) => hs,
            Err(error) => return Err(error),
        };

        let hook_snapshot_contribution = hook_session.as_ref().map(|hs| {
            let snapshot = hs.snapshot();
            let contribution: crate::context::Contribution = (&snapshot).into();
            contribution.fragments
        });
        if let Some(bundle) = context_bundle.as_mut()
            && let Some(fragments) = hook_snapshot_contribution.as_ref()
        {
            bundle.merge(fragments.clone());
        }
        construction.context.bundle = context_bundle.clone();
        construction.context.bundle_id = context_bundle.as_ref().map(|bundle| bundle.bundle_id);
        construction.context.bootstrap_fragment_count = context_bundle
            .as_ref()
            .map(|bundle| bundle.bootstrap_fragments.len())
            .unwrap_or_default();

        let context_audit_bus = self.deps.current_context_audit_bus().await;
        let runtime_delegate = hook_session.as_ref().map(|hs| {
            let injection_sink: DynRuntimeHookInjectionSink =
                Arc::new(SessionRuntimeHookInjectionSink::new(
                    self.deps.runtime_registry.clone(),
                    context_audit_bus.clone(),
                ));
            HookRuntimeDelegate::new_with_mount_root_audit_and_sink(
                hs.clone(),
                Some(default_mount_root.to_string_lossy().replace('\\', "/")),
                context_audit_bus.clone(),
                Some(injection_sink),
            )
        });
        let restore_mode = match prompt_lifecycle {
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::SystemContext,
            ) => LaunchRestoreMode::SystemContext,
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            ) => LaunchRestoreMode::ExecutorState,
            _ => LaunchRestoreMode::None,
        };
        let restored_session_state = match prompt_lifecycle {
            SessionPromptLifecycle::RepositoryRehydrate(
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
                (!transcript.is_empty()).then(|| {
                    let entries = transcript.entries;
                    RestoredSessionState {
                        messages: entries.iter().map(|entry| entry.message.clone()).collect(),
                        message_refs: entries
                            .iter()
                            .map(|entry| Some(entry.message_ref.clone()))
                            .collect(),
                    }
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
                    .session_meta
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
                &prompt_input,
                &construction,
                &executor_config.executor,
                command.reason_tag(),
            )
            .await?;
        let launch_plan = LaunchPlan::build(LaunchPlanInput {
            resolved_payload,
            construction,
            session_id: sid,
            turn_id: input.turn_id.to_string(),
            lifecycle: prompt_lifecycle,
            restore_mode,
            hook_snapshot_reload,
            hook_snapshot_contribution,
            follow_up_session_id: resolved_follow_up_session_id.clone(),
            follow_up_source,
            requested_runtime_commands: input.requested_runtime_commands,
            pending_capability_transitions,
            base_capability_state,
            environment_variables: prompt_input.env.clone(),
            hook_session: hook_session.clone(),
            capability_state: capability_state.clone(),
            runtime_delegate,
            restored_session_state,
            post_turn_handler,
            backend_execution,
        });

        Ok(launch_plan)
    }

    async fn resolve_backend_execution_placement(
        &self,
        session_id: &str,
        turn_id: &str,
        prompt_input: &crate::session::types::UserPromptInput,
        construction: &SessionConstructionPlan,
        executor_id: &str,
        reason_tag: &str,
    ) -> Result<Option<ExecutionPlacementPlan>, ConnectorError> {
        let Some(transport) = self.deps.backend_execution_transport.as_ref() else {
            if prompt_input.backend_selection.is_some() {
                return Err(ConnectorError::InvalidConfig(
                    "backend selection 已指定，但 session runtime 未注入 backend execution placement transport"
                        .to_string(),
                ));
            }
            return Ok(None);
        };
        let Some(lease_repo) = self.deps.backend_execution_lease_repo.as_ref() else {
            if prompt_input.backend_selection.is_some() {
                return Err(ConnectorError::InvalidConfig(
                    "backend selection 已指定，但 session runtime 未注入 backend execution lease repository"
                        .to_string(),
                ));
            }
            return Ok(None);
        };

        let request = match prompt_input.backend_selection.as_ref() {
            Some(selection) => Some(selection_request_from_input(
                executor_id,
                selection,
                reason_tag,
            )?),
            None if has_available_relay_executor(transport.as_ref(), executor_id) => {
                Some(selection_request_from_vfs_hint(
                    executor_id,
                    construction.surface.vfs.as_ref(),
                    reason_tag,
                ))
            }
            None => None,
        };
        let Some(request) = request else {
            return Ok(None);
        };

        let mut placement =
            resolve_backend_execution_placement(transport.as_ref(), lease_repo.as_ref(), &request)
                .await?;
        let mut lease = BackendExecutionLease::claimed(
            placement.backend_id.clone(),
            session_id.to_string(),
            turn_id.to_string(),
            placement.executor_id.clone(),
            placement.selection_mode,
            placement.claim_reason.clone(),
        );
        lease.workspace_id = construction.workspace.workspace_id;
        lease.root_ref = construction
            .surface
            .vfs
            .as_ref()
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
    reason_tag: &str,
) -> Result<BackendSelectionRequest, ConnectorError> {
    let reason = Some(format!("session launch: {reason_tag}"));
    match selection.mode {
        BackendSelectionInputMode::Explicit => {
            let backend_id = required_backend_id(selection, "explicit")?;
            Ok(BackendSelectionRequest::explicit(
                executor_id,
                backend_id,
                reason,
            ))
        }
        BackendSelectionInputMode::AutoIdle => {
            Ok(BackendSelectionRequest::auto_idle(executor_id, reason))
        }
        BackendSelectionInputMode::WorkspaceBinding => {
            let backend_id = required_backend_id(selection, "workspace_binding")?;
            Ok(BackendSelectionRequest::workspace_binding(
                executor_id,
                backend_id,
                reason,
            ))
        }
    }
}

fn selection_request_from_vfs_hint(
    executor_id: &str,
    vfs: Option<&Vfs>,
    reason_tag: &str,
) -> BackendSelectionRequest {
    let reason = Some(format!("session launch: {reason_tag}"));
    preferred_backend_id_from_vfs(vfs)
        .map(|backend_id| BackendSelectionRequest {
            executor_id: executor_id.to_string(),
            intent: BackendSelectionIntent::WorkspaceBinding { backend_id },
            reason: reason.clone(),
        })
        .unwrap_or_else(|| BackendSelectionRequest::auto_idle(executor_id, reason))
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

fn preferred_backend_id_from_vfs(vfs: Option<&Vfs>) -> Option<String> {
    let vfs = vfs?;
    if let Some(default_mount) = vfs.default_mount() {
        let backend_id = default_mount.backend_id.trim();
        if !backend_id.is_empty() {
            return Some(backend_id.to_string());
        }
    }

    let unique_backend_ids = vfs
        .mounts
        .iter()
        .filter_map(|mount| {
            let backend_id = mount.backend_id.trim();
            (!backend_id.is_empty()).then_some(backend_id.to_string())
        })
        .collect::<std::collections::BTreeSet<_>>();

    (unique_backend_ids.len() == 1)
        .then(|| unique_backend_ids.into_iter().next())
        .flatten()
}
