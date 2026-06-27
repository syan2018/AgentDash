use agentdash_agent_protocol::SourceInfo;
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::settings::SettingScope;
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTrigger, HookTurnStartNotice, SharedHookRuntime,
};
use agentdash_spi::{CapabilityState, ConnectorError, ExecutionContext};

use super::deps::TurnPreparationDeps;
use super::{LaunchFollowUpSource, LaunchPlan};
use crate::session::assignment_context_frame::build_assignment_context_frame;
use crate::session::guidelines_context_frame::{
    GuidelinesFrameInput, SYSTEM_GUIDELINES_FRAME_KIND, build_guidelines_context_frame,
};
use crate::session::hub::{
    HookTriggerInput, PendingRuntimeContextApplication, build_initial_capability_state_frame,
};
use crate::session::hub_support::{SessionProfile, TurnExecution};
use crate::session::identity_context_frame::{IdentityFrameInput, build_identity_context_frame};
use crate::session::memory_context_frame::{
    MEMORY_CONTEXT_FRAME_KIND, MemoryContextFrameInput, build_memory_context_frame,
};
use crate::session::pending_action_context_frame::build_pending_action_context_frame;
use crate::session::post_turn_handler::DynPostTurnHandler;
use crate::session::types::{HookSnapshotReloadTrigger, PromptLaunchPath, ResolvedPromptPayload};

pub(in crate::session) struct TurnPreparationInput {
    pub launch_plan: LaunchPlan,
    pub session_id: String,
    pub turn_id: String,
    pub had_existing_runtime: bool,
}

pub(in crate::session) struct PreparedTurn {
    pub pending_frame: Option<AgentFrame>,
    pub session_id: String,
    pub turn_id: String,
    pub started_at_ms: i64,
    pub resolved_payload: ResolvedPromptPayload,
    pub resolved_follow_up_session_id: Option<String>,
    pub title_hint: String,
    pub source: SourceInfo,
    pub connector_context: Option<ExecutionContext>,
    pub accepted_context_frames_to_emit: Vec<ContextFrame>,
    pub pending_transition_application: PendingRuntimeContextApplication,
    pub pending_command_ids: Vec<uuid::Uuid>,
    pub accepted_capability_state: CapabilityState,
    pub is_owner_bootstrap: bool,
    pub hook_runtime: Option<SharedHookRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

pub(in crate::session) struct TurnPreparer {
    deps: TurnPreparationDeps,
}

impl TurnPreparer {
    pub(super) fn new(deps: TurnPreparationDeps) -> Self {
        Self { deps }
    }

    pub async fn prepare(
        &self,
        input: TurnPreparationInput,
    ) -> Result<PreparedTurn, ConnectorError> {
        let deps = &self.deps;
        let TurnPreparationInput {
            launch_plan,
            session_id,
            turn_id,
            had_existing_runtime,
        } = input;

        let resolved_payload = launch_plan.resolved_payload.clone();
        let title_hint = launch_plan.title_hint.clone();
        let resolved_follow_up_session_id = launch_plan.summary.follow_up_session_id.clone();
        let post_turn_handler = launch_plan.terminal_effects.post_turn_handler.clone();
        let hook_runtime = launch_plan.context.turn.hook_runtime.clone();
        let hook_snapshot_contribution = launch_plan.hooks.snapshot_contribution.clone();
        let context_bundle = launch_plan.context_bundle.clone();
        let discovered_guidelines = launch_plan.discovered_guidelines.clone();
        let discovered_memory = launch_plan.discovered_memory.clone();
        let base_capability_state = launch_plan.runtime_commands.base_capability_state.clone();
        let capability_state = launch_plan.context.turn.capability_state.clone();
        let capability_keys = capability_state.capability_keys();
        let is_owner_bootstrap =
            launch_plan.summary.hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;
        diag!(Debug, Subsystem::SessionLaunch,

            session_id = %launch_plan.summary.session_id,
            turn_id = %launch_plan.summary.turn_id,
            launch_path = ?launch_plan.summary.launch_path,
            restore_mode = ?launch_plan.summary.restore_mode,
            follow_up_source = ?launch_plan.summary.follow_up_source,
            pending_transition_count = launch_plan.summary.pending_transition_count,
            vfs_source = ?launch_plan.summary.vfs_source,
            pending_vfs_overlay_applied = launch_plan.summary.pending_vfs_overlay_applied,
            mcp_source = ?launch_plan.summary.mcp_source,
            capability_source = ?launch_plan.summary.capability_source,
            mcp_server_count = launch_plan.summary.mcp_server_count,
            has_vfs = launch_plan.summary.has_vfs,
            "prepared session launch plan"
        );
        let mut context = launch_plan.context;

        let assembled_tool_surface = deps.assemble_tool_surface(&session_id, &context).await;
        let assembled_tool_schemas = assembled_tool_surface.schemas;
        context.turn.assembled_tools = assembled_tool_surface.tools;

        let include_connector_startup_context = should_include_connector_startup_context(
            launch_plan.summary.launch_path,
            had_existing_runtime,
            &launch_plan.summary.follow_up_source,
        );
        let user_preferences = if include_connector_startup_context {
            load_user_preferences(
                deps.settings_repo.as_deref(),
                context.session.identity.as_ref(),
            )
            .await
        } else {
            Vec::new()
        };
        let identity_frame = if include_connector_startup_context {
            build_identity_context_frame(&IdentityFrameInput {
                base_system_prompt: &deps.base_system_prompt,
                agent_system_prompt: context.session.executor_config.system_prompt.as_deref(),
                agent_system_prompt_mode: context.session.executor_config.system_prompt_mode,
            })
        } else {
            None
        };
        // 用户偏好与项目指引迁出 identity 帧，走独立的系统级 guidelines 帧。
        let guidelines_frame = if include_connector_startup_context {
            build_guidelines_context_frame(&GuidelinesFrameInput {
                user_preferences: &user_preferences,
                discovered_guidelines: &discovered_guidelines,
            })
        } else {
            None
        };
        let memory_frame = if include_connector_startup_context {
            build_memory_context_frame(&MemoryContextFrameInput {
                inventory: &discovered_memory,
            })
        } else {
            None
        };

        let compose_fragments = context_bundle
            .as_ref()
            .map(|bundle| bundle.bootstrap_fragments.clone())
            .or_else(|| hook_snapshot_contribution.clone())
            .unwrap_or_default();
        let (audit_bundle_id, audit_session_id) = context_bundle
            .as_ref()
            .map(|bundle| (bundle.bundle_id, bundle.session_id))
            .unwrap_or_else(|| {
                let session_uuid = uuid::Uuid::parse_str(&session_id).unwrap_or_else(|_| {
                    diag!(Debug, Subsystem::SessionLaunch,

                        session_id = %session_id,
                        "session_id 不是 UUID，使用临时审计 session_id"
                    );
                    uuid::Uuid::new_v4()
                });
                (uuid::Uuid::new_v4(), session_uuid)
            });

        let started_at_ms = chrono::Utc::now().timestamp_millis();
        deps.turn_supervisor
            .activate_turn(
                &session_id,
                SessionProfile {
                    capability_state: capability_state.clone(),
                },
                TurnExecution::new_with_started_at(
                    turn_id.clone(),
                    context.session.clone(),
                    capability_state.clone(),
                    audit_bundle_id,
                    audit_session_id,
                    started_at_ms,
                ),
            )
            .await;

        let pending_command_ids = launch_plan
            .runtime_commands
            .requested_commands
            .iter()
            .map(|command| command.id)
            .collect::<Vec<_>>();
        let pending_transition_application = if !launch_plan
            .runtime_commands
            .pending_capability_transitions
            .is_empty()
        {
            deps.runtime_transition
                .apply_pending_runtime_context_transitions_on_turn(
                    crate::session::hub::ApplyPendingRuntimeContextTransitionInput {
                        session_id: &session_id,
                        hook_runtime: hook_runtime.as_ref(),
                        before_state: base_capability_state,
                        final_capability_state: &capability_state,
                        transitions: &launch_plan.runtime_commands.pending_capability_transitions,
                        tool_schemas: &assembled_tool_schemas,
                    },
                )
                .await
        } else {
            Default::default()
        };

        let connector_type = match deps.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = SourceInfo {
            connector_id: deps.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id: Some(context.session.executor_config.executor.to_string()),
        };

        if is_owner_bootstrap && let Some(hook_runtime) = hook_runtime.as_ref() {
            let initial_caps = capability_keys.clone();
            if !initial_caps.is_empty() {
                let _ = hook_runtime.update_capabilities(initial_caps.clone());
            }

            let _start_effects = deps
                .hooks
                .emit_session_hook_trigger(
                    hook_runtime.as_ref(),
                    &HookTriggerInput {
                        session_id: &session_id,
                        turn_id: Some(&turn_id),
                        trigger: HookTrigger::SessionStart,
                        payload: Some(serde_json::json!({
                            "text_prompt": resolved_payload.text_prompt,
                            "user_block_count": resolved_payload.input.len(),
                            "tool_capabilities": {
                                "current": initial_caps.iter().collect::<Vec<_>>(),
                            },
                        })),
                        refresh_reason: "trigger:session_start",
                        source: source.clone(),
                    },
                )
                .await;
        }

        let mut accepted_context_frames_to_emit = Vec::new();
        let mut owner_bootstrap_frames = Vec::new();
        if is_owner_bootstrap {
            let frame = build_initial_capability_state_frame(
                &capability_state,
                &capability_keys,
                &assembled_tool_schemas,
            );
            accepted_context_frames_to_emit.push(frame.clone());
            owner_bootstrap_frames.push(frame);

            if let Some(frame) = build_assignment_context_frame(
                context_bundle
                    .as_ref()
                    .map(|bundle| bundle.phase_tag.as_str()),
                &compose_fragments,
            ) {
                accepted_context_frames_to_emit.push(frame.clone());
                owner_bootstrap_frames.push(frame);
            }
        }

        let mut turn_context_frames: Vec<ContextFrame> = Vec::new();
        if let Some(frame) = identity_frame {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        if let Some(frame) = guidelines_frame {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        if let Some(frame) = memory_frame {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        if let Some(frame) = launch_plan.continuation_context_frame.clone() {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        turn_context_frames.extend(owner_bootstrap_frames);
        turn_context_frames.extend(pending_transition_application.context_frames.clone());

        if let Some(hook_runtime_for) = hook_runtime.as_ref() {
            turn_context_frames.extend(collect_queued_turn_start_frames(hook_runtime_for.as_ref()));

            let snapshot = hook_runtime_for.snapshot();
            let runtime = hook_runtime_for.runtime_snapshot();
            let pending_action_frames = hook_runtime_for
                .unresolved_pending_actions()
                .into_iter()
                .filter_map(|action| {
                    build_pending_action_context_frame(&snapshot, &action, &runtime)
                })
                .collect::<Vec<_>>();
            for frame in &pending_action_frames {
                accepted_context_frames_to_emit.push(frame.clone());
            }
            turn_context_frames.extend(pending_action_frames);
        }
        context.turn.context_frames = dedupe_context_frames(turn_context_frames);

        enqueue_context_frames_for_transform_context(
            hook_runtime.as_ref(),
            &context.turn.context_frames,
        );

        Ok(PreparedTurn {
            pending_frame: launch_plan.pending_frame,
            session_id,
            turn_id,
            started_at_ms,
            resolved_payload,
            resolved_follow_up_session_id,
            title_hint,
            source,
            connector_context: Some(context),
            accepted_context_frames_to_emit,
            pending_transition_application,
            pending_command_ids,
            accepted_capability_state: capability_state,
            is_owner_bootstrap,
            hook_runtime,
            post_turn_handler,
        })
    }
}

async fn load_user_preferences(
    settings_repo: Option<&dyn agentdash_domain::settings::SettingsRepository>,
    identity: Option<&agentdash_spi::AuthIdentity>,
) -> Vec<String> {
    let (Some(settings_repo), Some(identity)) = (settings_repo, identity) else {
        return Vec::new();
    };
    let scope = SettingScope::user(identity.user_id.clone());
    let setting = match settings_repo.get(&scope, "agent.pi.user_preferences").await {
        Ok(Some(setting)) => setting,
        Ok(None) => return Vec::new(),
        Err(error) => {
            diag!(Warn, Subsystem::SessionLaunch,

                user_id = %identity.user_id,
                error = %error,
                "读取 Pi Agent 用户偏好失败"
            );
            return Vec::new();
        }
    };
    setting
        .value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn collect_queued_turn_start_frames(
    hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
) -> Vec<ContextFrame> {
    hook_runtime
        .collect_turn_start_notices_for_injection()
        .into_iter()
        .filter_map(notice_to_context_frame)
        .collect()
}

fn notice_to_context_frame(notice: HookTurnStartNotice) -> Option<ContextFrame> {
    if let Some(frame) = notice.context_frame {
        return Some(frame);
    }
    let content = notice.content.trim();
    if content.is_empty() {
        return None;
    }
    Some(ContextFrame {
        id: notice.id,
        kind: "system_notice".to_string(),
        source: notice.source,
        phase_node: None,
        apply_mode: None,
        delivery_status: "queued_for_transform_context".to_string(),
        delivery_channel: "turn_start".to_string(),
        message_role: "user".to_string(),
        rendered_text: content.to_string(),
        sections: vec![ContextFrameSection::SystemNotice {
            title: "TurnStart Notice".to_string(),
            summary: "TurnStart notice 已桥接为 ContextFrame。".to_string(),
            body: Some(content.to_string()),
        }],
        created_at_ms: notice.created_at_ms,
    })
}

fn dedupe_context_frames(frames: Vec<ContextFrame>) -> Vec<ContextFrame> {
    let mut ids = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for frame in frames {
        if frame.rendered_text.trim().is_empty() {
            continue;
        }
        if ids.insert(frame.id.clone()) {
            deduped.push(frame);
        }
    }
    deduped
}

fn should_include_connector_startup_context(
    launch_path: PromptLaunchPath,
    had_existing_runtime: bool,
    follow_up_source: &LaunchFollowUpSource,
) -> bool {
    match launch_path {
        PromptLaunchPath::OwnerBootstrap | PromptLaunchPath::RepositoryRehydrate(_) => true,
        PromptLaunchPath::Plain => {
            !had_existing_runtime && matches!(follow_up_source, LaunchFollowUpSource::None)
        }
    }
}

fn enqueue_context_frames_for_transform_context(
    hook_runtime: Option<&SharedHookRuntime>,
    frames: &[ContextFrame],
) {
    let Some(hook_runtime) = hook_runtime else {
        return;
    };
    for frame in frames {
        // identity / system_guidelines 走系统通道（由连接器拼进 system prompt），
        // 不再作为 turn-start notice 重复投递。
        if frame.kind == "identity"
            || frame.kind == SYSTEM_GUIDELINES_FRAME_KIND
            || frame.kind == MEMORY_CONTEXT_FRAME_KIND
            || frame.kind == "pending_action"
        {
            continue;
        }
        if frame.rendered_text.trim().is_empty() {
            continue;
        }
        hook_runtime.enqueue_turn_start_notice(HookTurnStartNotice {
            id: frame.id.clone(),
            created_at_ms: frame.created_at_ms,
            source: frame.source.clone(),
            content: frame.rendered_text.clone(),
            context_frame: Some(frame.clone()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{PromptLaunchPath, SessionRepositoryRehydrateMode};

    #[test]
    fn connector_startup_context_is_only_sent_when_connector_needs_initializing() {
        assert!(should_include_connector_startup_context(
            PromptLaunchPath::OwnerBootstrap,
            true,
            &LaunchFollowUpSource::SessionMeta,
        ));
        assert!(should_include_connector_startup_context(
            PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::ExecutorState,),
            false,
            &LaunchFollowUpSource::None,
        ));
        assert!(should_include_connector_startup_context(
            PromptLaunchPath::Plain,
            false,
            &LaunchFollowUpSource::None,
        ));
        assert!(!should_include_connector_startup_context(
            PromptLaunchPath::Plain,
            true,
            &LaunchFollowUpSource::None,
        ));
        assert!(!should_include_connector_startup_context(
            PromptLaunchPath::Plain,
            false,
            &LaunchFollowUpSource::SessionMeta,
        ));
    }
}
