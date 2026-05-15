use std::path::Path;
use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTrigger, HookTurnStartNotice, SessionHookSnapshot,
    SessionHookSnapshotQuery, SharedHookSessionRuntime,
};

use super::assignment_context_frame::build_assignment_context_frame;
use super::hook_runtime::HookSessionRuntime;
use super::hub::SessionHub;
use super::hub::{HookTriggerInput, build_initial_capability_state_frame};
use super::hub_support::*;
use super::identity_context_frame::{IdentityFrameInput, build_identity_context_frame};
use super::launch_planner::{SessionLaunchPlanner, SessionLaunchPlannerInput};
use super::pending_action_context_frame::build_pending_action_context_frame;
pub use super::types::*;

pub(super) struct SessionLaunchExecutor<'a> {
    hub: &'a SessionHub,
}

impl<'a> SessionLaunchExecutor<'a> {
    pub fn new(hub: &'a SessionHub) -> Self {
        Self { hub }
    }

    /// 多轮对话（支持底层执行器 follow-up 会话续跑）。
    pub async fn execute(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        req: SessionLaunchPlan,
    ) -> Result<String, ConnectorError> {
        let hub = self.hub;
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());
        let had_existing_runtime = hub.connector.has_live_session(session_id).await;

        let cached_continuation = hub.turn_supervisor.claim_prompt(session_id).await?;

        let meta_store = hub.stores.meta.clone();
        let runtime_command_store = hub.stores.runtime_commands.clone();
        let sid = session_id.to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let mut session_meta = match meta_store.get_session_meta(&sid).await {
            Ok(Some(meta)) => meta,
            Ok(None) => {
                return Err(ConnectorError::Runtime(format!(
                    "session {sid} 不存在，请先调用 create_session 再 prompt"
                )));
            }
            Err(e) => {
                return Err(ConnectorError::Runtime(format!(
                    "读取 session {sid} meta 失败: {e}"
                )));
            }
        };
        let pending_runtime_commands = runtime_command_store
            .list_pending_runtime_commands(&sid)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "读取 session `{sid}` pending runtime commands 失败: {error}"
                ))
            })?;
        let planned_launch = SessionLaunchPlanner::new(hub)
            .plan(SessionLaunchPlannerInput {
                session_id,
                turn_id: &turn_id,
                follow_up_session_id: follow_up_session_id.map(ToString::to_string),
                had_existing_runtime,
                cached_continuation,
                session_meta: &session_meta,
                pending_runtime_commands,
                request: req,
            })
            .await?;
        let resolved_payload = planned_launch.resolved_payload;
        let title_hint = planned_launch.title_hint;
        let launch_execution = planned_launch.launch_execution;
        let hook_session = planned_launch.hook_session;
        let hook_snapshot_contribution = planned_launch.hook_snapshot_contribution;
        let context_bundle = planned_launch.context_bundle;
        let continuation_context_frame = planned_launch.continuation_context_frame;
        let post_turn_handler = planned_launch.post_turn_handler;
        let discovered_guidelines = planned_launch.discovered_guidelines;
        let pending_runtime_commands = planned_launch.pending_runtime_commands;
        let pending_capability_transitions = planned_launch.pending_capability_transitions;
        let base_capability_state = planned_launch.base_capability_state;
        let capability_state = planned_launch.capability_state;
        let capability_keys = planned_launch.capability_keys;
        let resolved_follow_up_session_id = planned_launch.resolved_follow_up_session_id;
        let is_owner_bootstrap =
            launch_execution.summary.hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;
        tracing::debug!(
            session_id = %launch_execution.summary.session_id,
            turn_id = %launch_execution.summary.turn_id,
            lifecycle = ?launch_execution.summary.lifecycle,
            restore_mode = ?launch_execution.summary.restore_mode,
            follow_up_source = ?launch_execution.summary.follow_up_source,
            pending_transition_count = launch_execution.summary.pending_transition_count,
            vfs_source = ?launch_execution.summary.vfs_source,
            pending_vfs_overlay_applied = launch_execution.summary.pending_vfs_overlay_applied,
            mcp_source = ?launch_execution.summary.mcp_source,
            capability_source = ?launch_execution.summary.capability_source,
            mcp_server_count = launch_execution.summary.mcp_server_count,
            has_vfs = launch_execution.summary.has_vfs,
            "prepared session launch execution"
        );
        let mut context = launch_execution.context;

        // pipeline 层预构建工具列表：runtime + direct MCP + relay MCP
        context.turn.assembled_tools = hub
            .build_tools_for_execution_context(
                session_id,
                &context,
                &capability_state.tool.mcp_servers,
            )
            .await;

        let identity_frame = build_identity_context_frame(&IdentityFrameInput {
            base_system_prompt: &hub.base_system_prompt,
            agent_system_prompt: context.session.executor_config.system_prompt.as_deref(),
            agent_system_prompt_mode: context.session.executor_config.system_prompt_mode,
            user_preferences: &hub.user_preferences,
            discovered_guidelines: &discovered_guidelines,
        });

        let compose_fragments = context_bundle
            .as_ref()
            .map(|bundle| bundle.bootstrap_fragments.clone())
            .or_else(|| hook_snapshot_contribution.clone())
            .unwrap_or_default();
        let (audit_bundle_id, audit_session_id) = context_bundle
            .as_ref()
            .map(|bundle| (bundle.bundle_id, bundle.session_id))
            .unwrap_or_else(|| {
                let session_uuid = uuid::Uuid::parse_str(session_id).unwrap_or_else(|_| {
                    tracing::debug!(
                        session_id = %session_id,
                        "session_id 不是 UUID，使用临时审计 session_id"
                    );
                    uuid::Uuid::new_v4()
                });
                (uuid::Uuid::new_v4(), session_uuid)
            });

        {
            hub.turn_supervisor
                .activate_turn(
                    session_id,
                    super::hub_support::SessionProfile {
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
        }

        let pending_command_ids = pending_runtime_commands
            .iter()
            .map(|command| command.id)
            .collect::<Vec<_>>();
        let pending_transition_frames = if !pending_capability_transitions.is_empty() {
            let frames = hub
                .apply_pending_runtime_context_transitions_on_turn(
                    &sid,
                    &turn_id,
                    hook_session.as_ref(),
                    base_capability_state,
                    &pending_capability_transitions,
                    &context.turn.assembled_tools,
                )
                .await;
            frames
        } else {
            Vec::new()
        };

        let connector_type = match hub.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = SourceInfo {
            connector_id: hub.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id: Some(context.session.executor_config.executor.to_string()),
        };

        // PR 4（04-30-session-pipeline-architecture-refactor）删除 `session-capabilities://`
        // resource block 注入路径。动态能力面由独立 ContextFrame 投递，不再混入
        // user_blocks 或 core system prompt。
        let user_envelopes = build_user_message_envelopes(
            session_id,
            &source,
            &turn_id,
            &resolved_payload.user_blocks,
        );
        for envelope in user_envelopes {
            let _ = hub.persist_notification(&sid, envelope).await;
        }

        let started = build_turn_started_envelope(session_id, &source, &turn_id);
        let _ = hub.persist_notification(&sid, started).await;

        // SessionStart 只代表 owner 首轮 bootstrap，不再与“进程内第几轮”绑定。
        if is_owner_bootstrap {
            if let Some(hook_session) = hook_session.as_ref() {
                let initial_caps = capability_keys.clone();
                if !initial_caps.is_empty() {
                    let _ = hook_session.update_capabilities(initial_caps.clone());
                }

                let _start_effects = hub
                    .emit_session_hook_trigger(
                        hook_session.as_ref(),
                        &HookTriggerInput {
                            session_id: &sid,
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

        let mut owner_bootstrap_frames = Vec::new();
        if is_owner_bootstrap {
            let frame = build_initial_capability_state_frame(
                &capability_state,
                &capability_keys,
                &context.turn.assembled_tools,
            );
            let _ = hub.emit_context_frame(&sid, Some(&turn_id), &frame).await;
            owner_bootstrap_frames.push(frame);

            if let Some(frame) = build_assignment_context_frame(
                context_bundle
                    .as_ref()
                    .map(|bundle| bundle.phase_tag.as_str()),
                &compose_fragments,
            ) {
                let _ = hub.emit_context_frame(&sid, Some(&turn_id), &frame).await;
                owner_bootstrap_frames.push(frame);
            }
        }

        let mut turn_context_frames: Vec<ContextFrame> = Vec::new();
        if let Some(frame) = identity_frame {
            let _ = hub.emit_context_frame(&sid, Some(&turn_id), &frame).await;
            turn_context_frames.push(frame);
        }
        if let Some(frame) = continuation_context_frame {
            let _ = hub.emit_context_frame(&sid, Some(&turn_id), &frame).await;
            turn_context_frames.push(frame);
        }
        turn_context_frames.extend(owner_bootstrap_frames);
        turn_context_frames.extend(pending_transition_frames);

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
                let _ = hub.emit_context_frame(&sid, Some(&turn_id), frame).await;
            }
            turn_context_frames.extend(pending_action_frames);
        }
        context.turn.context_frames = dedupe_context_frames(turn_context_frames);

        enqueue_context_frames_for_transform_context(
            hook_session.as_ref(),
            &context.turn.context_frames,
        );
        let executor_config_for_meta = context.session.executor_config.clone();

        let mut stream = match hub
            .connector
            .prompt(
                session_id,
                resolved_follow_up_session_id.as_deref(),
                &resolved_payload.prompt_payload,
                context,
            )
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                hub.turn_supervisor.clear_turn_and_hook(session_id).await;
                let failed = build_turn_terminal_envelope(
                    &sid,
                    &source,
                    &turn_id,
                    TurnTerminalKind::Failed,
                    Some(error.to_string()),
                );
                let _ = hub.persist_notification(&sid, failed).await;
                return Err(error);
            }
        };

        Self::apply_turn_start_meta(
            &mut session_meta,
            now,
            &turn_id,
            &executor_config_for_meta,
            is_owner_bootstrap,
            &title_hint,
        );
        let _ = meta_store.save_session_meta(&session_meta).await;

        if !pending_command_ids.is_empty()
            && let Err(error) = runtime_command_store
                .mark_runtime_commands_applied(&pending_command_ids)
                .await
        {
            tracing::warn!(
                session_id = %sid,
                error = %error,
                "标记 pending runtime commands applied 失败"
            );
        }

        // 首轮 prompt 且 title_source 非 User 时，异步触发标题生成。
        // connector.prompt 已接受后再触发，避免启动失败时产生额外副作用。
        let is_first_turn = session_meta.last_event_seq <= 1;
        if is_first_turn
            && session_meta.title_source != super::types::TitleSource::User
            && hub.title_generator.is_some()
        {
            hub.spawn_title_generation(
                session_id.to_string(),
                resolved_payload.text_prompt.clone(),
            );
        }
        let session_id = session_id.to_string();

        // 创建 SessionTurnProcessor — cloud-native 和 relay 共用的事件处理核心
        let processor = super::turn_processor::SessionTurnProcessor::spawn(
            hub.clone(),
            super::turn_processor::SessionTurnProcessorConfig {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                source: source.clone(),
                hook_session,
                post_turn_handler,
            },
        );

        let processor_tx = processor.tx();

        // 注册 processor_tx 到 SessionRuntime.current_turn，供 relay / cancel 路径使用
        hub.turn_supervisor
            .register_processor_tx(&session_id, processor_tx.clone())
            .await;

        Self::spawn_stream_adapter(
            hub.turn_supervisor.clone(),
            session_id.to_string(),
            turn_id.clone(),
            &mut stream,
            processor_tx,
        );

        Ok(turn_id)
    }

    /// 将 connector stream 桥接到 processor channel 的后台任务。
    fn spawn_stream_adapter(
        turn_supervisor: super::turn_supervisor::TurnSupervisor,
        session_id: String,
        turn_id: String,
        stream: &mut agentdash_spi::ExecutionStream,
        processor_tx: tokio::sync::mpsc::UnboundedSender<super::turn_processor::TurnEvent>,
    ) {
        use futures::StreamExt;
        let mut stream = std::mem::replace(stream, Box::pin(futures::stream::empty()));
        tokio::spawn(async move {
            while let Some(next) = stream.next().await {
                match next {
                    Ok(notification) => {
                        let _ = processor_tx.send(super::turn_processor::TurnEvent::Notification(
                            Box::new(notification),
                        ));
                    }
                    Err(e) => {
                        tracing::error!("执行流错误 session_id={}: {}", session_id, e);
                        let (kind, message) = Self::resolve_stream_terminal(
                            &turn_supervisor,
                            &session_id,
                            &turn_id,
                            Some(e),
                        )
                        .await;
                        let _ = processor_tx
                            .send(super::turn_processor::TurnEvent::Terminal { kind, message });
                        return;
                    }
                }
            }
            let (kind, message) =
                Self::resolve_stream_terminal(&turn_supervisor, &session_id, &turn_id, None).await;
            let _ = processor_tx.send(super::turn_processor::TurnEvent::Terminal { kind, message });
        });
    }

    /// 根据 cancel 状态和错误信息决定 stream 结束时的 terminal kind。
    async fn resolve_stream_terminal(
        turn_supervisor: &super::turn_supervisor::TurnSupervisor,
        session_id: &str,
        turn_id: &str,
        error: Option<agentdash_spi::ConnectorError>,
    ) -> (TurnTerminalKind, Option<String>) {
        if let Some(terminal) = turn_supervisor
            .cancel_interrupted_terminal(session_id, turn_id)
            .await
        {
            terminal
        } else if let Some(e) = error {
            (TurnTerminalKind::Failed, Some(e.to_string()))
        } else {
            (TurnTerminalKind::Completed, None)
        }
    }

    /// Turn 开始时更新 SessionMeta 的快照字段。
    ///
    /// 包含 bootstrap state 的 `Pending → Bootstrapped` 转移。
    fn apply_turn_start_meta(
        meta: &mut SessionMeta,
        now: i64,
        turn_id: &str,
        executor_config: &agentdash_domain::common::AgentConfig,
        is_owner_bootstrap: bool,
        title_hint: &str,
    ) {
        meta.updated_at = now;
        meta.last_execution_status = ExecutionStatus::Running;
        meta.last_turn_id = Some(turn_id.to_string());
        meta.last_terminal_message = None;
        meta.executor_config = Some(executor_config.clone());
        if is_owner_bootstrap {
            meta.bootstrap_state = SessionBootstrapState::Bootstrapped;
        }
        if meta.title.trim().is_empty() {
            meta.title = title_hint.to_string();
        }
    }
}

impl SessionHub {
    /// 解析 hook runtime：决定 reload / refresh / skip。
    ///
    /// 三条路径：
    /// - **reload**：owner bootstrap 首轮 或 进程内没有 runtime（冷启动恢复）
    /// - **refresh**：已有 hook_session 且非 bootstrap → 只 refresh snapshot
    /// - **skip**：无 hook provider → 返回 None
    pub(super) async fn resolve_hook_session(
        &self,
        session_id: &str,
        turn_id: &str,
        executor_config: &agentdash_domain::common::AgentConfig,
        working_directory: &Path,
        is_owner_bootstrap: bool,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let existing = self.runtime_registry.hook_session_runtime(session_id).await;

        if is_owner_bootstrap || existing.is_none() {
            return self
                .reload_session_hook_runtime(
                    session_id,
                    turn_id,
                    executor_config.executor.as_str(),
                    executor_config.permission_policy.as_deref(),
                    working_directory,
                )
                .await;
        }

        if let Some(ref hs) = existing {
            let _ = hs
                .refresh(agentdash_spi::hooks::SessionHookRefreshQuery {
                    session_id: session_id.to_string(),
                    turn_id: Some(turn_id.to_string()),
                    reason: Some("subsequent_turn_refresh".to_string()),
                })
                .await;
        }
        Ok(existing)
    }

    /// 重载 session hook runtime 并写入 `SessionRuntime.hook_session` 单一权威字段。
    ///
    /// PR 7c：只有本函数（owner bootstrap 入口）以及 `ensure_hook_session_runtime`
    /// （冷启动恢复入口）可以写 `SessionRuntime.hook_session`；其他调用方（包括
    /// `SessionLaunchExecutor` 的 happy path）只读取不回写。
    pub async fn reload_session_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        working_directory: &Path,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let Some(provider) = self.hook_provider.as_ref() else {
            // 无 hook provider 场景下清空 runtime 记录，保证"单一权威"不滞留旧值。
            self.runtime_registry
                .with_runtime_mut(session_id, |runtime| {
                    if let Some(runtime) = runtime {
                        runtime.hook_session = None;
                    }
                })
                .await;
            return Ok(None);
        };

        let mut snapshot = provider
            .load_session_snapshot(SessionHookSnapshotQuery {
                session_id: session_id.to_string(),
                turn_id: Some(turn_id.to_string()),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("加载会话 Hook snapshot 失败: {error}"))
            })?;
        enrich_hook_snapshot_runtime_metadata(
            &mut snapshot,
            turn_id,
            self.connector.connector_id(),
            executor,
            permission_policy,
            working_directory,
        );

        let runtime = Arc::new(HookSessionRuntime::new(
            session_id.to_string(),
            provider.clone(),
            snapshot,
        ));

        // 写回 SessionRuntime.hook_session —— 单一权威字段仅在此处写入。
        self.runtime_registry
            .with_runtime_mut(session_id, |session_runtime| {
                if let Some(session_runtime) = session_runtime {
                    session_runtime.hook_session = Some(runtime.clone());
                }
            })
            .await;

        Ok(Some(runtime))
    }

    pub(super) async fn discover_skills(
        &self,
        vfs: &agentdash_spi::Vfs,
    ) -> Vec<agentdash_spi::SkillRef> {
        let mut skills = if let Some(service) = &self.vfs_service {
            let result = crate::skill::load_skills_from_vfs(service, vfs).await;
            for diag in &result.diagnostics {
                tracing::warn!(
                    skill_name = %diag.name,
                    path = %diag.file_path.display(),
                    "skill 诊断: {}",
                    diag.message
                );
            }
            result.skills
        } else {
            Vec::new()
        };

        if !self.extra_skill_dirs.is_empty() {
            let existing_names: std::collections::HashMap<String, String> = skills
                .iter()
                .map(|s| (s.name.clone(), s.file_path.to_string_lossy().to_string()))
                .collect();
            let result =
                crate::skill::load_skills_from_local_dirs(&self.extra_skill_dirs, &existing_names);
            for diag in &result.diagnostics {
                tracing::warn!(
                    skill_name = %diag.name,
                    path = %diag.file_path.display(),
                    "skill 诊断 (plugin): {}",
                    diag.message
                );
            }
            skills.extend(result.skills);
        }
        skills
    }

    pub(super) async fn discover_guidelines(
        &self,
        vfs: &agentdash_spi::Vfs,
    ) -> Vec<agentdash_spi::DiscoveredGuideline> {
        let Some(service) = &self.vfs_service else {
            return Vec::new();
        };
        let result = crate::context::mount_file_discovery::discover_mount_files(
            service,
            vfs,
            crate::context::mount_file_discovery::BUILTIN_GUIDELINE_RULES,
        )
        .await;
        for diag in &result.diagnostics {
            tracing::warn!(
                rule_key = %diag.rule_key,
                mount_id = %diag.mount_id,
                path = %diag.path,
                "guideline 发现诊断: {}",
                diag.message
            );
        }
        result
            .files
            .into_iter()
            .map(|f| agentdash_spi::DiscoveredGuideline {
                file_name: f.path.rsplit('/').next().unwrap_or(&f.path).to_string(),
                mount_id: f.mount_id,
                path: f.path,
                content: f.content,
            })
            .collect()
    }
}

fn enrich_hook_snapshot_runtime_metadata(
    snapshot: &mut SessionHookSnapshot,
    turn_id: &str,
    connector_id: &str,
    executor: &str,
    permission_policy: Option<&str>,
    working_directory: &Path,
) {
    let metadata = snapshot
        .metadata
        .get_or_insert_with(agentdash_spi::SessionSnapshotMetadata::default);
    metadata.turn_id = Some(turn_id.to_string());
    metadata.connector_id = Some(connector_id.to_string());
    metadata.executor = Some(executor.to_string());
    metadata.permission_policy = permission_policy.map(ToString::to_string);
    metadata.working_directory = Some(working_directory.to_string_lossy().replace('\\', "/"));
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
            title: "Legacy TurnStart Notice".to_string(),
            summary: "历史 notice 已桥接为 ContextFrame。".to_string(),
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

/// 统一将已组装的 ContextFrame 投递到 Hook session transform_context 队列。
///
/// 排除规则（与原 executor 层行为一致）：
/// - `identity` frame 走 connector 的 set_system_prompt，不进入 transform_context；
/// - `pending_action` frame 不参与 transform_context（由独立通道投递）。
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
    use super::super::baseline_capabilities::build_session_baseline_capabilities;

    #[test]
    fn baseline_capabilities_built_from_skills() {
        let caps = build_session_baseline_capabilities(&[agentdash_spi::SkillRef {
            name: "my-skill".to_string(),
            description: "test".to_string(),
            file_path: "/ws/SKILL.md".into(),
            base_dir: "/ws".into(),
            disable_model_invocation: false,
        }]);
        assert_eq!(caps.skills.len(), 1);
        assert_eq!(caps.skills[0].name, "my-skill");
    }
}
