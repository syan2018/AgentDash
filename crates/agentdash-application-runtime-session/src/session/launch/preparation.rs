use agentdash_agent_protocol::SourceInfo;
use agentdash_application_ports::frame_launch_envelope::FrameRuntimeSurface;
use agentdash_application_ports::launch::LaunchSource;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_domain::settings::SettingScope;
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::hooks::{
    ContextAgentConsumptionMode, ContextConnectorProfile, ContextDeliveryAppliedFrame,
    ContextDeliveryConnectorInput, ContextDeliveryEntry, ContextDeliveryMetadata,
    ContextDeliveryPlan, ContextDeliveryRecord, ContextDeliveryTarget, ContextFrame,
    ContextFrameSection, ContextModelChannel, HookTrigger, HookTurnStartNotice, RuntimeEventSource,
    SharedHookRuntime,
};
use agentdash_spi::{
    CapabilityState, ConnectorError, ExecutionContext, McpServerReadinessSummary, PromptPayload,
    RuntimeMcpServer, RuntimeMcpSourceReadiness,
};

use super::deps::TurnPreparationDeps;
use super::{LaunchFollowUpSource, LaunchPlan, RuntimeDelegateCompositionPlan};
use crate::session::admission_delegate::{AgentRunAdmissionToolPolicyFacet, ToolAdmissionMetadata};
use crate::session::assignment_context_frame::build_assignment_context_frame;
use crate::session::environment_context_frame::{
    EnvironmentFrameInput, build_environment_context_frame,
};
use crate::session::guidelines_context_frame::{
    GuidelinesFrameInput, build_guidelines_context_frame,
};
use crate::session::hub::{
    HookTriggerInput, PendingRuntimeContextApplication, build_initial_capability_state_frame,
};
use crate::session::hub_support::{SessionProfile, TurnExecution};
use crate::session::identity_context_frame::{IdentityFrameInput, build_identity_context_frames};
use crate::session::memory_context_frame::{MemoryContextFrameInput, build_memory_context_frame};
use crate::session::pending_action_context_frame::build_pending_action_context_frame;
use crate::session::post_turn_handler::DynPostTurnHandler;
use crate::session::types::{HookSnapshotReloadTrigger, PromptLaunchPath, ResolvedPromptPayload};
use crate::session::user_context_frame::{UserContextFrameInput, build_user_context_frame};

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
    pub launch_source: LaunchSource,
    pub source: SourceInfo,
    pub connector_context: Option<ExecutionContext>,
    pub context_delivery_record: Option<ContextDeliveryRecord>,
    pub accepted_context_frames_to_emit: Vec<ContextFrame>,
    pub pending_transition_application: PendingRuntimeContextApplication,
    pub pending_command_ids: Vec<uuid::Uuid>,
    pub accepted_capability_state: CapabilityState,
    pub mcp_readiness_notice: Vec<McpServerReadinessSummary>,
    pub is_owner_bootstrap: bool,
    pub runtime_delegate_composition: RuntimeDelegateCompositionPlan,
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

        let mut resolved_payload = launch_plan.resolved_payload.clone();
        let title_hint = launch_plan.title_hint.clone();
        let launch_source = launch_plan.source;
        let system_delivery_context_frame = build_system_delivery_context_frame(
            &session_id,
            &turn_id,
            launch_source,
            &resolved_payload.text_prompt,
        );
        if system_delivery_context_frame.is_some() {
            resolved_payload.prompt_payload = PromptPayload::Text(
                "Continue from the AgentDash system delivery context for this turn.".to_string(),
            );
        }
        let resolved_follow_up_session_id = launch_plan.summary.follow_up_session_id.clone();
        let post_turn_handler = launch_plan.terminal_boundary.post_turn_handler.clone();
        let hook_runtime = launch_plan.context.turn.hook_runtime.clone();
        let mut runtime_delegate_facets = launch_plan.runtime_delegate_facets.clone();
        let hook_snapshot_contribution = launch_plan.hooks.snapshot_contribution.clone();
        let context_bundle = launch_plan.context_bundle.clone();
        let compose_fragments = context_bundle
            .as_ref()
            .map(|bundle| bundle.bootstrap_fragments.clone())
            .or_else(|| hook_snapshot_contribution.clone())
            .unwrap_or_default();
        let agent_identity_markdown = find_agent_identity_markdown(&compose_fragments);
        let discovered_guidelines = launch_plan.discovered_guidelines.clone();
        let discovered_memory = launch_plan.discovered_memory.clone();
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
        if !assembled_tool_surface.mcp_sources.is_empty() {
            context.session.mcp_servers = merge_mcp_source_readiness(
                context.session.mcp_servers,
                assembled_tool_surface.mcp_sources,
            );
        }
        context.turn.capability_state.tool.mcp_servers = context.session.mcp_servers.clone();
        let mcp_readiness_notice =
            unavailable_mcp_source_summaries(&context.turn.capability_state.tool.mcp_servers);
        let base_capability_state = launch_plan.runtime_commands.base_capability_state.clone();
        let capability_state = context.turn.capability_state.clone();
        let capability_keys = capability_state.capability_keys();
        if let Some(port) = deps.agent_run_effective_capability_port.as_ref() {
            let admission_metadata =
                ToolAdmissionMetadata::from_schema_entries(&assembled_tool_schemas);
            let inner_tool_policy = context
                .turn
                .runtime_delegates
                .tool_policy
                .take()
                .or_else(|| runtime_delegate_facets.hook_tool_policy.take());
            let admission_tool_policy = AgentRunAdmissionToolPolicyFacet::wrap(
                session_id.clone(),
                port.clone(),
                inner_tool_policy,
                admission_metadata,
            );
            runtime_delegate_facets.composition.admission_tool_policy = true;
            context.turn.runtime_delegates.tool_policy = Some(admission_tool_policy);
        }

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
        let identity_frames = if include_connector_startup_context {
            build_identity_context_frames(&IdentityFrameInput {
                base_system_prompt: &deps.base_system_prompt,
                agent_identity_markdown,
                agent_system_prompt: context.session.executor_config.system_prompt.as_deref(),
            })
        } else {
            Vec::new()
        };
        let user_context_frame = if include_connector_startup_context {
            build_user_context_frame(&UserContextFrameInput {
                auth_identity: context.session.identity.as_ref(),
            })
        } else {
            None
        };
        let environment_frame = if include_connector_startup_context {
            let date_utc = chrono::Utc::now().format("%Y-%m-%d").to_string();
            build_environment_context_frame(&EnvironmentFrameInput {
                date_utc: &date_utc,
                platform: std::env::consts::OS,
                arch: std::env::consts::ARCH,
                model_id: context.session.executor_config.model_id.as_deref(),
                executor: &context.session.executor_config.executor,
                working_directory: Some(&context.session.working_directory),
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
        if let Some(frame) = system_delivery_context_frame {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        for frame in identity_frames {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        if let Some(frame) = user_context_frame {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        if let Some(frame) = environment_frame {
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
        let connector_profile = context_connector_profile(
            deps.connector.connector_id(),
            &context.session.executor_config,
        );
        context.turn.context_frames = apply_delivery_target_to_frames(
            dedupe_context_frames(turn_context_frames),
            &connector_profile,
            deps.connector.connector_id(),
            &context.session.executor_config.executor,
        );
        context.turn.context_delivery_plan = Some(build_context_delivery_plan(
            &context.turn.context_frames,
            &session_id,
            &turn_id,
            deps.connector.connector_id(),
            &context.session.executor_config.executor,
            connector_profile,
        ));
        sync_emitted_frame_delivery_metadata(
            &mut accepted_context_frames_to_emit,
            &context.turn.context_frames,
        );

        enqueue_context_frames_for_transform_context(
            hook_runtime.as_ref(),
            &context.turn.context_frames,
        );

        let context_delivery_record = Some(build_context_delivery_record(
            &session_id,
            &turn_id,
            &launch_plan.frame_surface,
            launch_plan.pending_frame.as_ref(),
            deps.connector.connector_id(),
            &context.session.executor_config.executor,
            &context.session.working_directory,
            context.turn.context_delivery_plan.as_ref(),
            &pending_transition_application.context_frames,
            &accepted_context_frames_to_emit,
            started_at_ms,
        ));

        Ok(PreparedTurn {
            pending_frame: launch_plan.pending_frame,
            session_id,
            turn_id,
            started_at_ms,
            resolved_payload,
            resolved_follow_up_session_id,
            title_hint,
            launch_source,
            source,
            connector_context: Some(context),
            context_delivery_record,
            accepted_context_frames_to_emit,
            pending_transition_application,
            pending_command_ids,
            accepted_capability_state: capability_state,
            mcp_readiness_notice,
            is_owner_bootstrap,
            runtime_delegate_composition: runtime_delegate_facets.composition,
            hook_runtime,
            post_turn_handler,
        })
    }
}

fn merge_mcp_source_readiness(
    current_servers: Vec<RuntimeMcpServer>,
    discovered_sources: Vec<RuntimeMcpServer>,
) -> Vec<RuntimeMcpServer> {
    let readiness_by_name = discovered_sources
        .into_iter()
        .map(|server| (server.name, server.readiness))
        .collect::<std::collections::BTreeMap<_, _>>();
    current_servers
        .into_iter()
        .map(|mut server| {
            if let Some(readiness) = readiness_by_name.get(&server.name) {
                server.readiness = readiness.clone();
            }
            server
        })
        .collect()
}

fn unavailable_mcp_source_summaries(
    servers: &[RuntimeMcpServer],
) -> Vec<McpServerReadinessSummary> {
    servers
        .iter()
        .filter_map(|server| match &server.readiness {
            RuntimeMcpSourceReadiness::Unavailable {
                reason_code,
                message,
            } => Some(McpServerReadinessSummary {
                name: server.name.clone(),
                reason_code: reason_code.clone(),
                message: message.clone(),
            }),
            RuntimeMcpSourceReadiness::Pending | RuntimeMcpSourceReadiness::Ready { .. } => None,
        })
        .collect()
}

fn find_agent_identity_markdown(fragments: &[agentdash_spi::ContextFragment]) -> Option<&str> {
    fragments
        .iter()
        .find(|fragment| fragment.slot == "agent_identity" && !fragment.content.trim().is_empty())
        .map(|fragment| fragment.content.as_str())
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
            let context =
                DiagnosticErrorContext::new("session.launch.preparation", "load_user_preferences");
            diag_error!(
                Warn,
                Subsystem::SessionLaunch,
                context = &context,
                error = &error,
                user_id = %identity.user_id,
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

fn build_system_delivery_context_frame(
    session_id: &str,
    turn_id: &str,
    launch_source: LaunchSource,
    text_prompt: &str,
) -> Option<ContextFrame> {
    if should_remain_human_prompt(launch_source, text_prompt) {
        return None;
    }

    let kind = system_delivery_kind(launch_source, text_prompt);
    let source_kind = launch_source_tag(launch_source);
    let summary = bounded_system_delivery_summary(text_prompt);
    let rendered_text = format!(
        "## AgentDash System Delivery\n\n- kind: {kind}\n- source: {source_kind}\n- status: delivered\n- turn_id: {turn_id}\n\n{summary}"
    );
    Some(ContextFrame {
        id: format!("{turn_id}:system-delivery-context"),
        kind: "system_delivery".to_string(),
        source: system_delivery_runtime_source(launch_source, text_prompt),
        phase_node: None,
        apply_mode: None,
        delivery_status: "accepted".to_string(),
        delivery_channel: "connector_context".to_string(),
        message_role: "system".to_string(),
        delivery_metadata: ContextDeliveryMetadata::for_frame(
            "system_delivery",
            "connector_context",
            "system",
        ),
        rendered_text: rendered_text.clone(),
        sections: vec![ContextFrameSection::SystemNotice {
            title: "AgentDash System Delivery".to_string(),
            summary: format!("{kind} from {source_kind} for session {session_id}."),
            body: Some(rendered_text),
        }],
        created_at_ms: chrono::Utc::now().timestamp_millis(),
    })
}

fn should_remain_human_prompt(launch_source: LaunchSource, text_prompt: &str) -> bool {
    matches!(
        launch_source,
        LaunchSource::HttpPrompt
            | LaunchSource::LifecycleAgentUserMessage
            | LaunchSource::LocalRelayPrompt
    ) && !contains_project_subagent_notification_marker(text_prompt)
}

fn contains_project_subagent_notification_marker(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("<subagent_notification>") || trimmed.contains("\n<subagent_notification>")
}

fn system_delivery_kind(launch_source: LaunchSource, text_prompt: &str) -> &'static str {
    if contains_project_subagent_notification_marker(text_prompt) {
        return "subagent_notification";
    }
    match launch_source {
        LaunchSource::CompanionDispatch | LaunchSource::CompanionParentResume => {
            "companion_delivery"
        }
        LaunchSource::SystemDelivery => "system_delivery",
        LaunchSource::HookAutoResume => "hook_auto_resume",
        LaunchSource::WorkflowOrchestrator => "workflow_delivery",
        LaunchSource::RoutineExecutor => "routine_delivery",
        LaunchSource::HttpPrompt
        | LaunchSource::LifecycleAgentUserMessage
        | LaunchSource::LocalRelayPrompt => "system_delivery",
    }
}

fn system_delivery_runtime_source(
    launch_source: LaunchSource,
    text_prompt: &str,
) -> RuntimeEventSource {
    if contains_project_subagent_notification_marker(text_prompt)
        || matches!(
            launch_source,
            LaunchSource::CompanionDispatch | LaunchSource::CompanionParentResume
        )
    {
        RuntimeEventSource::CompanionResult
    } else {
        RuntimeEventSource::RuntimeContextUpdate
    }
}

fn launch_source_tag(launch_source: LaunchSource) -> &'static str {
    match launch_source {
        LaunchSource::HttpPrompt => "http_prompt",
        LaunchSource::LifecycleAgentUserMessage => "lifecycle_agent_user_message",
        LaunchSource::HookAutoResume => "hook_auto_resume",
        LaunchSource::CompanionDispatch => "companion_dispatch",
        LaunchSource::CompanionParentResume => "companion_parent_resume",
        LaunchSource::SystemDelivery => "system_delivery",
        LaunchSource::WorkflowOrchestrator => "workflow_orchestrator",
        LaunchSource::RoutineExecutor => "routine_executor",
        LaunchSource::LocalRelayPrompt => "local_relay_prompt",
    }
}

fn bounded_system_delivery_summary(text: &str) -> String {
    const MAX_CHARS: usize = 2_000;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let mut summary = trimmed.chars().take(MAX_CHARS).collect::<String>();
    summary.push_str("...");
    summary
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
        delivery_metadata: ContextDeliveryMetadata::for_frame(
            "system_notice",
            "turn_start",
            "user",
        ),
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

fn apply_delivery_target_to_frames(
    frames: Vec<ContextFrame>,
    connector_profile: &ContextConnectorProfile,
    connector_id: &str,
    executor: &str,
) -> Vec<ContextFrame> {
    let target = delivery_target_name(connector_id, executor);
    frames
        .into_iter()
        .map(|mut frame| {
            frame.delivery_metadata.connector_profile = connector_profile.clone();
            frame.delivery_metadata.agent_consumption.target = target.clone();
            frame.delivery_metadata.agent_consumption.mode = agent_consumption_mode_for_frame(
                connector_id,
                connector_profile,
                &frame.delivery_metadata,
            );
            frame.delivery_metadata.agent_consumption.reason =
                format!("{}_{}_delivery", connector_profile.profile_id, frame.kind);
            frame
        })
        .collect()
}

fn build_context_delivery_plan(
    frames: &[ContextFrame],
    session_id: &str,
    turn_id: &str,
    connector_id: &str,
    executor: &str,
    connector_profile: ContextConnectorProfile,
) -> ContextDeliveryPlan {
    let mut entries = frames
        .iter()
        .map(ContextDeliveryEntry::from_frame)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        (
            entry.delivery_phase,
            entry.delivery_order,
            entry.frame_id.clone(),
        )
    });
    ContextDeliveryPlan {
        plan_id: format!("context-delivery-plan-{session_id}-{turn_id}"),
        target_agent: ContextDeliveryTarget {
            agent_id: Some(executor.to_string()),
            connector_id: Some(connector_id.to_string()),
            profile: connector_profile,
        },
        entries,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_context_delivery_record(
    session_id: &str,
    turn_id: &str,
    frame_surface: &FrameRuntimeSurface,
    pending_frame: Option<&AgentFrame>,
    connector_id: &str,
    executor_id: &str,
    working_directory: &std::path::Path,
    plan: Option<&ContextDeliveryPlan>,
    transition_frames: &[ContextFrame],
    emitted_frames: &[ContextFrame],
    created_at_ms: i64,
) -> ContextDeliveryRecord {
    let context_frame_ids = plan
        .map(|plan| {
            plan.entries
                .iter()
                .map(|entry| entry.frame_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let target_agent = plan
        .map(|plan| plan.target_agent.clone())
        .unwrap_or_default();
    let delivery_plan_id = plan.map(|plan| plan.plan_id.clone());
    let mut emitted_context_frame_ids = Vec::new();
    let mut seen_emitted = std::collections::HashSet::new();
    for frame in transition_frames.iter().chain(emitted_frames.iter()) {
        if seen_emitted.insert(frame.id.clone()) {
            emitted_context_frame_ids.push(frame.id.clone());
        }
    }

    ContextDeliveryRecord {
        record_id: format!("context-delivery-record-{session_id}-{turn_id}"),
        runtime_session_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
        applied_frame: ContextDeliveryAppliedFrame {
            agent_id: frame_surface.agent_id,
            frame_id: frame_surface.frame_id,
            frame_revision: frame_surface.frame_revision,
            pending_frame_id: pending_frame.map(|frame| frame.id),
            pending_frame_revision: pending_frame.map(|frame| frame.revision),
        },
        connector_input: ContextDeliveryConnectorInput {
            connector_id: connector_id.to_string(),
            executor_id: executor_id.to_string(),
            working_directory: working_directory.display().to_string(),
            target_agent,
        },
        delivery_plan_id,
        context_frame_ids,
        emitted_context_frame_ids,
        created_at_ms,
    }
}

fn sync_emitted_frame_delivery_metadata(
    emitted_frames: &mut [ContextFrame],
    planned_frames: &[ContextFrame],
) {
    let metadata_by_id = planned_frames
        .iter()
        .map(|frame| (frame.id.as_str(), &frame.delivery_metadata))
        .collect::<std::collections::HashMap<_, _>>();
    for frame in emitted_frames {
        if let Some(metadata) = metadata_by_id.get(frame.id.as_str()) {
            frame.delivery_metadata = (*metadata).clone();
        }
    }
}

fn context_connector_profile(
    connector_id: &str,
    executor_config: &agentdash_domain::common::AgentConfig,
) -> ContextConnectorProfile {
    let declared_consumption_modes = vec![
        ContextAgentConsumptionMode::Consume,
        ContextAgentConsumptionMode::Ignore,
        ContextAgentConsumptionMode::ConnectorNative,
        ContextAgentConsumptionMode::SystemAppend,
    ];
    ContextConnectorProfile {
        profile_id: delivery_target_name(connector_id, &executor_config.executor),
        declared_consumption_modes,
    }
}

fn agent_consumption_mode_for_frame(
    connector_id: &str,
    _connector_profile: &ContextConnectorProfile,
    metadata: &ContextDeliveryMetadata,
) -> ContextAgentConsumptionMode {
    if connector_id == "pi-agent" {
        return ContextAgentConsumptionMode::Consume;
    }
    match metadata.model_channel {
        ContextModelChannel::System | ContextModelChannel::Developer => {
            ContextAgentConsumptionMode::SystemAppend
        }
        ContextModelChannel::Ignored => ContextAgentConsumptionMode::Ignore,
        ContextModelChannel::AuditOnly => ContextAgentConsumptionMode::AuditOnly,
        ContextModelChannel::Context | ContextModelChannel::User => {
            ContextAgentConsumptionMode::Consume
        }
    }
}

fn delivery_target_name(connector_id: &str, executor: &str) -> String {
    format!("{connector_id}:{executor}")
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
        // system/developer entries 由 connector 按 delivery plan 消费，不再作为
        // turn-start notice 重复投递；pending action 有专门的 runtime 投递通道。
        if matches!(
            frame.delivery_metadata.model_channel,
            ContextModelChannel::System | ContextModelChannel::Developer
        ) || frame.kind == "pending_action"
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
    use agentdash_domain::common::AgentConfig;
    use uuid::Uuid;

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

    #[test]
    fn project_subagent_notification_enters_system_delivery_context_frame() {
        let text = "<subagent_notification>{\"status\":\"completed\"}</subagent_notification>";
        let frame = build_system_delivery_context_frame(
            "session-1",
            "turn-1",
            LaunchSource::HttpPrompt,
            text,
        )
        .expect("marker should be classified as system delivery");

        assert_eq!(frame.kind, "system_delivery");
        assert_eq!(frame.message_role, "system");
        assert_eq!(
            frame.delivery_metadata.model_channel,
            ContextModelChannel::System
        );
        assert!(frame.rendered_text.contains("kind: subagent_notification"));
        assert!(frame.rendered_text.contains("source: http_prompt"));
        assert!(frame.rendered_text.contains("status: delivered"));
        assert!(frame.rendered_text.contains(text));
        assert!(
            build_system_delivery_context_frame(
                "session-1",
                "turn-2",
                LaunchSource::HttpPrompt,
                "hello from a human",
            )
            .is_none()
        );
    }

    #[test]
    fn delivery_plan_orders_frames_and_keeps_pi_memory_out_of_system() {
        let profile = context_connector_profile("pi-agent", &AgentConfig::new("PI_AGENT"));
        let frames = apply_delivery_target_to_frames(
            vec![
                test_frame("memory-1", "memory_context", "turn_start", "user", 3),
                test_frame(
                    "guidelines-1",
                    "system_guidelines",
                    "connector_context",
                    "system",
                    2,
                ),
                test_frame("identity-1", "identity", "connector_context", "system", 1),
            ],
            &profile,
            "pi-agent",
            "PI_AGENT",
        );

        let plan = build_context_delivery_plan(
            &frames,
            "session-1",
            "turn-1",
            "pi-agent",
            "PI_AGENT",
            profile,
        );

        let order = plan
            .entries
            .iter()
            .map(|entry| entry.frame_kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            vec!["identity", "system_guidelines", "memory_context"]
        );
        // system_guidelines 落在 session_policy #20，走 system model channel。
        let guidelines = plan
            .entries
            .iter()
            .find(|entry| entry.frame_kind == "system_guidelines")
            .expect("system_guidelines entry");
        assert_eq!(
            guidelines.delivery_phase,
            agentdash_spi::hooks::ContextDeliveryPhase::SessionPolicy
        );
        assert_eq!(guidelines.delivery_order, 20);
        assert_eq!(guidelines.model_channel, ContextModelChannel::System);
        let memory = plan
            .entries
            .iter()
            .find(|entry| entry.frame_kind == "memory_context")
            .expect("memory entry");
        assert_eq!(memory.model_channel, ContextModelChannel::Context);
        assert_eq!(
            memory.agent_consumption.mode,
            ContextAgentConsumptionMode::Consume
        );
    }

    #[test]
    fn non_pi_connector_declares_system_append_consumption() {
        let config = AgentConfig::new("CLAUDE_CODE");
        let profile = context_connector_profile("codex-bridge", &config);
        let frames = apply_delivery_target_to_frames(
            vec![test_frame(
                "identity-1",
                "identity",
                "connector_context",
                "system",
                1,
            )],
            &profile,
            "codex-bridge",
            "CLAUDE_CODE",
        );

        assert!(
            profile
                .declared_consumption_modes
                .contains(&ContextAgentConsumptionMode::SystemAppend)
        );
        assert_eq!(
            frames[0].delivery_metadata.agent_consumption.mode,
            ContextAgentConsumptionMode::SystemAppend
        );
    }

    #[test]
    fn context_delivery_record_links_connector_turn_frame_and_emissions() {
        let agent_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let frame_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let mut pending_frame = AgentFrame::new_revision(agent_id, 8, "accepted_launch");
        pending_frame.id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let frame_surface = FrameRuntimeSurface {
            agent_id,
            frame_id,
            frame_revision: 7,
            capability_surface: serde_json::Value::Null,
            context_slice: serde_json::Value::Null,
            vfs_surface: serde_json::Value::Null,
            mcp_surface: serde_json::Value::Null,
            runtime_session_id: Some("session-1".to_string()),
        };
        let config = AgentConfig::new("CLAUDE_CODE");
        let profile = context_connector_profile("codex-bridge", &config);
        let frames = apply_delivery_target_to_frames(
            vec![
                test_frame(
                    "transition-1",
                    "capability_transition",
                    "turn_start",
                    "user",
                    1,
                ),
                test_frame("identity-1", "identity", "connector_context", "system", 2),
            ],
            &profile,
            "codex-bridge",
            "CLAUDE_CODE",
        );
        let plan = build_context_delivery_plan(
            &frames,
            "session-1",
            "turn-1",
            "codex-bridge",
            "CLAUDE_CODE",
            profile,
        );

        let record = build_context_delivery_record(
            "session-1",
            "turn-1",
            &frame_surface,
            Some(&pending_frame),
            "codex-bridge",
            "CLAUDE_CODE",
            std::path::Path::new("F:/workspace"),
            Some(&plan),
            &frames[0..1],
            &frames[1..],
            1234,
        );

        assert_eq!(record.runtime_session_id, "session-1");
        assert_eq!(record.turn_id, "turn-1");
        assert_eq!(record.applied_frame.agent_id, agent_id);
        assert_eq!(record.applied_frame.frame_id, frame_id);
        assert_eq!(record.applied_frame.frame_revision, 7);
        assert_eq!(
            record.applied_frame.pending_frame_id,
            Some(pending_frame.id)
        );
        assert_eq!(record.applied_frame.pending_frame_revision, Some(8));
        assert_eq!(record.connector_input.connector_id, "codex-bridge");
        assert_eq!(record.connector_input.executor_id, "CLAUDE_CODE");
        assert_eq!(
            record.delivery_plan_id.as_deref(),
            Some(plan.plan_id.as_str())
        );
        assert_eq!(
            record.emitted_context_frame_ids,
            vec!["transition-1".to_string(), "identity-1".to_string()]
        );
        assert!(
            record
                .context_frame_ids
                .iter()
                .any(|frame_id| frame_id == "identity-1")
        );
        assert!(
            record
                .context_frame_ids
                .iter()
                .any(|frame_id| frame_id == "transition-1")
        );
    }

    fn test_frame(
        id: &str,
        kind: &str,
        delivery_channel: &str,
        message_role: &str,
        created_at_ms: i64,
    ) -> ContextFrame {
        ContextFrame {
            id: id.to_string(),
            kind: kind.to_string(),
            source: agentdash_spi::hooks::RuntimeEventSource::RuntimeContextUpdate,
            phase_node: None,
            apply_mode: None,
            delivery_status: "accepted".to_string(),
            delivery_channel: delivery_channel.to_string(),
            message_role: message_role.to_string(),
            delivery_metadata: ContextDeliveryMetadata::for_frame(
                kind,
                delivery_channel,
                message_role,
            ),
            rendered_text: kind.to_string(),
            sections: Vec::new(),
            created_at_ms,
        }
    }
}
