use agentdash_agent_protocol::SourceInfo;
use agentdash_domain::common::AgentConfig;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTrigger, HookTurnStartNotice, SharedHookSessionRuntime,
};
use agentdash_spi::{ConnectorError, ExecutionContext};

use super::{LaunchFollowUpSource, LaunchPlan, SessionLaunchDeps};
use crate::session::assignment_context_frame::build_assignment_context_frame;
use crate::session::hub::{
    HookTriggerInput, PendingRuntimeContextApplication, build_initial_capability_state_frame,
};
use crate::session::hub_support::{SessionProfile, TurnExecution};
use crate::session::identity_context_frame::{IdentityFrameInput, build_identity_context_frame};
use crate::session::pending_action_context_frame::build_pending_action_context_frame;
use crate::session::post_turn_handler::DynPostTurnHandler;
use crate::session::types::{
    HookSnapshotReloadTrigger, ResolvedPromptPayload, SessionPromptLifecycle,
};

pub(in crate::session) struct TurnPreparationInput {
    pub launch_plan: LaunchPlan,
    pub session_id: String,
    pub turn_id: String,
    pub had_existing_runtime: bool,
}

pub(in crate::session) struct PreparedTurn {
    pub session_id: String,
    pub turn_id: String,
    pub resolved_payload: ResolvedPromptPayload,
    pub resolved_follow_up_session_id: Option<String>,
    pub title_hint: String,
    pub source: SourceInfo,
    pub connector_context: Option<ExecutionContext>,
    pub accepted_context_frames_to_emit: Vec<ContextFrame>,
    pub pending_transition_application: PendingRuntimeContextApplication,
    pub pending_command_ids: Vec<uuid::Uuid>,
    pub executor_config_for_meta: AgentConfig,
    pub is_owner_bootstrap: bool,
    pub hook_session: Option<SharedHookSessionRuntime>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}

pub(in crate::session) struct TurnPreparer {
    deps: SessionLaunchDeps,
}

impl TurnPreparer {
    pub fn new(deps: SessionLaunchDeps) -> Self {
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
        let hook_session = launch_plan.context.turn.hook_session.clone();
        let hook_snapshot_contribution = launch_plan.hooks.snapshot_contribution.clone();
        let context_bundle = launch_plan.construction.context.bundle.clone();
        let discovered_guidelines = launch_plan.discovered_guidelines.clone();
        let base_capability_state = launch_plan.runtime_commands.base_capability_state.clone();
        let capability_state = launch_plan.context.turn.capability_state.clone();
        let capability_keys = capability_state.capability_keys();
        let is_owner_bootstrap =
            launch_plan.summary.hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;
        tracing::debug!(
            session_id = %launch_plan.summary.session_id,
            turn_id = %launch_plan.summary.turn_id,
            lifecycle = ?launch_plan.summary.lifecycle,
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

        context.turn.assembled_tools = deps
            .build_tools_for_execution_context(
                &session_id,
                &context,
                &capability_state.tool.mcp_servers,
            )
            .await;

        let include_connector_startup_context = should_include_connector_startup_context(
            launch_plan.summary.lifecycle,
            had_existing_runtime,
            &launch_plan.summary.follow_up_source,
        );
        let identity_frame = if include_connector_startup_context {
            build_identity_context_frame(&IdentityFrameInput {
                base_system_prompt: &deps.base_system_prompt,
                agent_system_prompt: context.session.executor_config.system_prompt.as_deref(),
                agent_system_prompt_mode: context.session.executor_config.system_prompt_mode,
                user_preferences: &deps.user_preferences,
                discovered_guidelines: &discovered_guidelines,
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
                    tracing::debug!(
                        session_id = %session_id,
                        "session_id 不是 UUID，使用临时审计 session_id"
                    );
                    uuid::Uuid::new_v4()
                });
                (uuid::Uuid::new_v4(), session_uuid)
            });

        deps.turn_supervisor
            .activate_turn(
                &session_id,
                SessionProfile {
                    capability_state: capability_state.clone(),
                },
                TurnExecution::new(
                    turn_id.clone(),
                    context.session.clone(),
                    capability_state.clone(),
                    audit_bundle_id,
                    audit_session_id,
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
            deps.capability
                .apply_pending_runtime_context_transitions_on_turn(
                    &session_id,
                    &turn_id,
                    hook_session.as_ref(),
                    base_capability_state,
                    &capability_state,
                    &launch_plan.runtime_commands.pending_capability_transitions,
                    &context.turn.assembled_tools,
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

        if is_owner_bootstrap {
            if let Some(hook_session) = hook_session.as_ref() {
                let initial_caps = capability_keys.clone();
                if !initial_caps.is_empty() {
                    let _ = hook_session.update_capabilities(initial_caps.clone());
                }

                let _start_effects = deps
                    .hooks
                    .emit_session_hook_trigger(
                        hook_session.as_ref(),
                        &HookTriggerInput {
                            session_id: &session_id,
                            turn_id: Some(&turn_id),
                            trigger: HookTrigger::SessionStart,
                            payload: Some(serde_json::json!({
                                "text_prompt": resolved_payload.text_prompt,
                                "user_block_count": resolved_payload.user_blocks.len(),
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
        }

        let mut accepted_context_frames_to_emit = Vec::new();
        let mut owner_bootstrap_frames = Vec::new();
        if is_owner_bootstrap {
            let frame = build_initial_capability_state_frame(
                &capability_state,
                &capability_keys,
                &context.turn.assembled_tools,
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
        if let Some(frame) = launch_plan
            .construction
            .context
            .continuation_context_frame
            .clone()
        {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        turn_context_frames.extend(owner_bootstrap_frames);
        turn_context_frames.extend(pending_transition_application.context_frames.clone());

        if let Some(hook_session_runtime) = hook_session.as_ref() {
            turn_context_frames.extend(collect_queued_turn_start_frames(
                hook_session_runtime.as_ref(),
            ));

            let snapshot = hook_session_runtime.snapshot();
            let runtime = hook_session_runtime.runtime_snapshot();
            let pending_action_frames = hook_session_runtime
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
            hook_session.as_ref(),
            &context.turn.context_frames,
        );
        let executor_config_for_meta = context.session.executor_config.clone();

        Ok(PreparedTurn {
            session_id,
            turn_id,
            resolved_payload,
            resolved_follow_up_session_id,
            title_hint,
            source,
            connector_context: Some(context),
            accepted_context_frames_to_emit,
            pending_transition_application,
            pending_command_ids,
            executor_config_for_meta,
            is_owner_bootstrap,
            hook_session,
            post_turn_handler,
        })
    }
}

fn collect_queued_turn_start_frames(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
) -> Vec<ContextFrame> {
    hook_session
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
    lifecycle: SessionPromptLifecycle,
    had_existing_runtime: bool,
    follow_up_source: &LaunchFollowUpSource,
) -> bool {
    match lifecycle {
        SessionPromptLifecycle::OwnerBootstrap | SessionPromptLifecycle::RepositoryRehydrate(_) => {
            true
        }
        SessionPromptLifecycle::Plain => {
            !had_existing_runtime && matches!(follow_up_source, LaunchFollowUpSource::None)
        }
    }
}

fn enqueue_context_frames_for_transform_context(
    hook_session: Option<&SharedHookSessionRuntime>,
    frames: &[ContextFrame],
) {
    let Some(hook_session) = hook_session else {
        return;
    };
    for frame in frames {
        if frame.kind == "identity" || frame.kind == "pending_action" {
            continue;
        }
        if frame.rendered_text.trim().is_empty() {
            continue;
        }
        hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
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
    use crate::session::types::{SessionPromptLifecycle, SessionRepositoryRehydrateMode};

    #[test]
    fn connector_startup_context_is_only_sent_when_connector_needs_initializing() {
        assert!(should_include_connector_startup_context(
            SessionPromptLifecycle::OwnerBootstrap,
            true,
            &LaunchFollowUpSource::SessionMeta,
        ));
        assert!(should_include_connector_startup_context(
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            ),
            false,
            &LaunchFollowUpSource::None,
        ));
        assert!(should_include_connector_startup_context(
            SessionPromptLifecycle::Plain,
            false,
            &LaunchFollowUpSource::None,
        ));
        assert!(!should_include_connector_startup_context(
            SessionPromptLifecycle::Plain,
            true,
            &LaunchFollowUpSource::None,
        ));
        assert!(!should_include_connector_startup_context(
            SessionPromptLifecycle::Plain,
            false,
            &LaunchFollowUpSource::SessionMeta,
        ));
    }
}
