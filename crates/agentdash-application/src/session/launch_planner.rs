use std::path::PathBuf;
use std::sync::Arc;

use agentdash_spi::{ConnectorError, RestoredSessionState};

use super::baseline_capabilities::build_session_baseline_capabilities;
use super::capability_state::merge_vfs_overlay;
use super::construction::SessionConstructionSeed;
use super::construction_planner::{SessionConstructionPlanner, SessionConstructionPlannerInput};
use super::hook_delegate::{
    DynRuntimeHookInjectionSink, HookRuntimeDelegate, SessionRuntimeHookInjectionSink,
};
use super::hub::SessionHub;
use super::hub_support::SessionProfile;
use super::launch::{
    LaunchCapabilitySource, LaunchExecution, LaunchExecutionInput, LaunchFollowUpSource,
    LaunchMcpSource, LaunchRestoreMode, LaunchVfsSource,
};
use super::path_policy::resolve_working_dir;
use super::post_turn_handler::{DynPostTurnHandler, TerminalHookEffectBinding};
use super::runtime_commands::PendingRuntimeCommandRecord;
use super::types::{
    HookSnapshotReloadTrigger, SessionMeta, SessionPromptLifecycle, SessionRepositoryRehydrateMode,
    UserPromptInput, resolve_session_prompt_lifecycle,
};

pub(super) struct SessionLaunchPlanner<'a> {
    hub: &'a SessionHub,
}

pub(super) struct SessionLaunchPlannerInput<'a> {
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub follow_up_session_id: Option<String>,
    pub had_existing_runtime: bool,
    pub cached_continuation: Option<SessionProfile>,
    pub session_meta: &'a SessionMeta,
    pub pending_runtime_commands: Vec<PendingRuntimeCommandRecord>,
    pub user_input: UserPromptInput,
    pub construction_seed: SessionConstructionSeed,
}

impl<'a> SessionLaunchPlanner<'a> {
    pub fn new(hub: &'a SessionHub) -> Self {
        Self { hub }
    }

    pub async fn plan(
        &self,
        input: SessionLaunchPlannerInput<'_>,
    ) -> Result<LaunchExecution, ConnectorError> {
        let sid = input.session_id.to_string();
        let SessionConstructionSeed {
            owner: construction_owner,
            source_contract,
            working_dir_input,
            local_relay_workspace_root,
            mcp_servers: seed_mcp_servers,
            vfs: seed_vfs,
            capability_state: seed_capability_state,
            context_bundle,
            continuation_context_frame,
            identity,
            terminal_hook_effect_binding,
        } = input.construction_seed;
        let mut context_bundle = context_bundle;
        let resolved_payload = input
            .user_input
            .resolve_prompt_payload()
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;
        let pending_capability_transitions = input
            .pending_runtime_commands
            .iter()
            .map(|command| command.transition.clone())
            .collect::<Vec<_>>();
        let pending_capability_state = pending_capability_transitions
            .last()
            .map(|transition| transition.state.clone());

        let (base_effective_vfs, vfs_source) = if let Some(vfs) = seed_vfs.clone() {
            (vfs, LaunchVfsSource::Request)
        } else if let Some(root) = local_relay_workspace_root.as_ref() {
            (
                super::local_workspace_vfs(root),
                LaunchVfsSource::LocalRelayWorkspaceRoot,
            )
        } else if let Some(vfs) = input
            .cached_continuation
            .as_ref()
            .and_then(|c| c.capability_state.vfs.active.clone())
        {
            (vfs, LaunchVfsSource::CachedSessionProfile)
        } else if let Some(vfs) = self.hub.default_vfs.clone() {
            (vfs, LaunchVfsSource::HubDefault)
        } else {
            return Err(ConnectorError::InvalidConfig(
                "prompt 缺少 vfs，且 session 无缓存、SessionHub 未配置默认 vfs".to_string(),
            ));
        };
        let mut effective_vfs = base_effective_vfs.clone();
        let mut pending_vfs_overlay_applied = false;
        if let Some(pending_surface) = pending_capability_state.as_ref()
            && let Some(pending_vfs) = pending_surface.vfs.active.as_ref()
        {
            effective_vfs = merge_vfs_overlay(effective_vfs, pending_vfs);
            pending_vfs_overlay_applied = true;
        }
        let default_mount_root = effective_vfs
            .default_mount()
            .map(|m| PathBuf::from(m.root_ref.trim()))
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("vfs 缺少 default_mount 或 root_ref 无效".to_string())
            })?;
        let working_directory =
            resolve_working_dir(&default_mount_root, working_dir_input.as_deref())
                .map_err(|error| ConnectorError::InvalidConfig(error.to_string()))?;

        let executor_config = input
            .user_input
            .executor_config
            .clone()
            .or_else(|| input.session_meta.executor_config.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "当前 prompt 缺少 executor_config，且 session meta 中也没有可复用配置"
                        .to_string(),
                )
            })?;

        let supports_repository_restore = self
            .hub
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
            .hub
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
            Err(error) => {
                self.hub
                    .turn_supervisor
                    .clear_turn_and_hook(input.session_id)
                    .await;
                return Err(error);
            }
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

        let context_audit_bus = self.hub.current_context_audit_bus().await;
        let runtime_delegate = hook_session.as_ref().map(|hs| {
            let injection_sink: DynRuntimeHookInjectionSink =
                Arc::new(SessionRuntimeHookInjectionSink::new(
                    self.hub.runtime_registry.clone(),
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
                    .hub
                    .build_projected_transcript(input.session_id)
                    .await
                    .map_err(|error| {
                        ConnectorError::Runtime(format!(
                            "重建 session `{}` 历史消息失败: {error}",
                            input.session_id
                        ))
                    })?;
                (!transcript.is_empty()).then(|| RestoredSessionState {
                    messages: transcript.into_messages(),
                })
            }
            _ => None,
        };

        let discovered_skills = self.hub.discover_skills(&effective_vfs).await;
        let session_capabilities = build_session_baseline_capabilities(&discovered_skills);
        let discovered_guidelines = self.hub.discover_guidelines(&effective_vfs).await;

        let (base_mcp_servers, base_mcp_source) = if seed_mcp_servers.is_empty() {
            input
                .cached_continuation
                .as_ref()
                .map(|c| {
                    (
                        c.capability_state.tool.mcp_servers.clone(),
                        LaunchMcpSource::CachedSessionProfile,
                    )
                })
                .unwrap_or_else(|| (Vec::new(), LaunchMcpSource::Empty))
        } else {
            (seed_mcp_servers.clone(), LaunchMcpSource::Request)
        };
        let (mcp_servers, mcp_source) =
            if let Some(pending_state) = pending_capability_state.as_ref() {
                (
                    pending_state.tool.mcp_servers.clone(),
                    LaunchMcpSource::PendingCapabilityTransition,
                )
            } else {
                (base_mcp_servers.clone(), base_mcp_source)
            };
        let base_capability_source = if seed_capability_state.is_some() {
            LaunchCapabilitySource::Request
        } else if input.cached_continuation.is_some() {
            LaunchCapabilitySource::CachedSessionProfile
        } else {
            LaunchCapabilitySource::Default
        };
        let base_capability_state = {
            let mut state = seed_capability_state
                .clone()
                .or_else(|| {
                    input
                        .cached_continuation
                        .as_ref()
                        .map(|c| c.capability_state.clone())
                })
                .unwrap_or_default();
            state.tool.mcp_servers = base_mcp_servers.clone();
            state.vfs.active = Some(base_effective_vfs.clone());
            state.skill.skills = session_capabilities.skills.clone();
            state
        };
        let (capability_state, capability_source) =
            if let Some(pending_state) = pending_capability_state.as_ref() {
                let mut state = pending_state.clone();
                state.tool.mcp_servers = mcp_servers.clone();
                state.vfs.active = Some(effective_vfs.clone());
                state.skill.skills = session_capabilities.skills.clone();
                (state, LaunchCapabilitySource::PendingCapabilityTransition)
            } else {
                (base_capability_state.clone(), base_capability_source)
            };
        let (resolved_follow_up_session_id, follow_up_source) = input
            .follow_up_session_id
            .as_deref()
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
        let construction_plan =
            SessionConstructionPlanner::plan_launch(SessionConstructionPlannerInput {
                session_id: sid.clone(),
                owner: construction_owner,
                source: source_contract,
                working_dir_input: working_dir_input.clone(),
                local_relay_workspace_root,
                working_directory: working_directory.clone(),
                executor_config: executor_config.clone(),
                vfs: capability_state.vfs.active.clone(),
                context_bundle: context_bundle.clone(),
                continuation_context_frame,
                identity: identity.clone(),
                terminal_hook_effect_binding: terminal_hook_effect_binding.clone(),
                mcp_servers: capability_state.tool.mcp_servers.clone(),
                capability_state: capability_state.clone(),
                session_capabilities: session_capabilities.clone(),
                prompt_lifecycle,
                capability_source: capability_source.clone(),
                vfs_source: vfs_source.clone(),
            })
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "launch 缺少 resolved session owner，无法构建 SessionConstructionPlan"
                        .to_string(),
                )
            })?;
        let post_turn_handler = self
            .resolve_terminal_hook_effect_handler(input.session_id, terminal_hook_effect_binding)
            .await?;
        let launch_execution = LaunchExecution::build(LaunchExecutionInput {
            resolved_payload,
            construction: construction_plan,
            session_id: sid,
            turn_id: input.turn_id.to_string(),
            lifecycle: prompt_lifecycle,
            restore_mode,
            hook_snapshot_reload,
            hook_snapshot_contribution,
            follow_up_session_id: resolved_follow_up_session_id.clone(),
            follow_up_source,
            pending_runtime_commands: input.pending_runtime_commands,
            pending_capability_transitions,
            base_capability_state: base_capability_state.clone(),
            vfs_source,
            pending_vfs_overlay_applied,
            mcp_source,
            capability_source,
            working_dir_input,
            working_directory,
            environment_variables: input.user_input.env.clone(),
            executor_config,
            mcp_servers: capability_state.tool.mcp_servers.clone(),
            vfs: capability_state.vfs.active.clone(),
            identity,
            hook_session: hook_session.clone(),
            capability_state: capability_state.clone(),
            runtime_delegate,
            restored_session_state,
            post_turn_handler,
            discovered_guidelines,
        });

        Ok(launch_execution)
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
        let Some(registry) = self.hub.hook_effect_handler_registry.read().await.clone() else {
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
