use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTrigger, HookTurnStartNotice, SessionHookSnapshot,
    SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
use agentdash_spi::{ConnectorError, RestoredSessionState};

use super::assignment_context_frame::build_assignment_context_frame;
use super::baseline_capabilities::build_session_baseline_capabilities;
use super::capability_state::merge_vfs_overlay;
use super::hook_delegate::{
    DynRuntimeHookInjectionSink, HookRuntimeDelegate, SessionRuntimeHookInjectionSink,
};
use super::hook_runtime::HookSessionRuntime;
use super::hub::SessionHub;
use super::hub::{HookTriggerInput, build_initial_capability_state_frame};
use super::hub_support::*;
use super::identity_context_frame::{IdentityFrameInput, build_identity_context_frame};
use super::launch::{
    LaunchCapabilitySource, LaunchExecution, LaunchExecutionInput, LaunchFollowUpSource,
    LaunchMcpSource, LaunchRestoreMode, LaunchVfsSource,
};
use super::path_policy::resolve_working_dir;
use super::pending_action_context_frame::build_pending_action_context_frame;
pub use super::types::*;

impl SessionHub {
    /// 多轮对话（支持底层执行器 follow-up 会话续跑）。
    pub async fn start_prompt_with_follow_up(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        mut req: PreparedLaunchPrompt,
    ) -> Result<String, ConnectorError> {
        let resolved_payload = req
            .user_input
            .resolve_prompt_payload()
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());
        let had_existing_runtime = self.connector.has_live_session(session_id).await;

        let cached_continuation = self.turn_supervisor.claim_prompt(session_id).await?;

        let meta_store = self.stores.meta.clone();
        let runtime_command_store = self.stores.runtime_commands.clone();
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
        let pending_capability_transitions = pending_runtime_commands
            .iter()
            .map(|command| command.transition.clone())
            .collect::<Vec<_>>();
        let pending_capability_state = pending_capability_transitions
            .last()
            .map(|transition| transition.state.clone());

        // 三级 fallback：① 请求级（Init/Rehydrate 注入） → ② session 缓存（Continue 复用） → ③ hub 默认
        let (base_effective_vfs, vfs_source) = if let Some(vfs) = req.vfs.clone() {
            (vfs, LaunchVfsSource::Request)
        } else if let Some(vfs) = cached_continuation
            .as_ref()
            .and_then(|c| c.capability_state.vfs.active.clone())
        {
            (vfs, LaunchVfsSource::CachedSessionProfile)
        } else if let Some(vfs) = self.default_vfs.clone() {
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
            resolve_working_dir(&default_mount_root, req.user_input.working_dir.as_deref())
                .map_err(|error| ConnectorError::InvalidConfig(error.to_string()))?;
        let working_dir_input = req.user_input.working_dir.clone();

        let title_hint = resolved_payload
            .text_prompt
            .chars()
            .take(30)
            .collect::<String>();
        let executor_config = req
            .user_input
            .executor_config
            .clone()
            .or_else(|| session_meta.executor_config.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "当前 prompt 缺少 executor_config，且 session meta 中也没有可复用配置"
                        .to_string(),
                )
            })?;

        let is_owner_bootstrap = req.hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;

        let hook_session: Option<SharedHookSessionRuntime> = match self
            .resolve_hook_session(
                session_id,
                &turn_id,
                &executor_config,
                &working_directory,
                is_owner_bootstrap,
            )
            .await
        {
            Ok(hs) => hs,
            Err(error) => {
                self.turn_supervisor.clear_turn_and_hook(session_id).await;
                return Err(error);
            }
        };

        // 把 hook snapshot 里的 injection 合并到 compose 期 fragment 集合。
        // Bundle 仍可作为 compose 中间载体，但 connector 不再直接消费 Bundle。
        let hook_snapshot_contribution = hook_session.as_ref().map(|hs| {
            let snapshot = hs.snapshot();
            let contribution: crate::context::Contribution = (&snapshot).into();
            contribution.fragments
        });
        if let Some(bundle) = req.context_bundle.as_mut()
            && let Some(fragments) = hook_snapshot_contribution.as_ref()
        {
            bundle.merge(fragments.clone());
        }

        let context_audit_bus = self.current_context_audit_bus().await;
        let runtime_delegate = hook_session.as_ref().map(|hs| {
            let injection_sink: DynRuntimeHookInjectionSink =
                Arc::new(SessionRuntimeHookInjectionSink::new(
                    self.runtime_registry.clone(),
                    context_audit_bus.clone(),
                ));
            HookRuntimeDelegate::new_with_mount_root_audit_and_sink(
                hs.clone(),
                Some(default_mount_root.to_string_lossy().replace('\\', "/")),
                context_audit_bus.clone(),
                Some(injection_sink),
            )
        });
        let supports_repository_restore = self
            .connector
            .supports_repository_restore(executor_config.executor.as_str());
        let prompt_lifecycle = resolve_session_prompt_lifecycle(
            &session_meta,
            had_existing_runtime,
            supports_repository_restore,
        );
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
                let transcript =
                    self.build_projected_transcript(session_id)
                        .await
                        .map_err(|error| {
                            ConnectorError::Runtime(format!(
                                "重建 session `{session_id}` 历史消息失败: {error}"
                            ))
                        })?;
                (!transcript.is_empty()).then(|| RestoredSessionState {
                    messages: transcript.into_messages(),
                })
            }
            _ => None,
        };

        let discovered_skills = self.discover_skills(&effective_vfs).await;
        let session_capabilities = build_session_baseline_capabilities(&discovered_skills);
        let discovered_guidelines = self.discover_guidelines(&effective_vfs).await;

        // session 级配置：请求未提供时回退到 session_profile 缓存
        let (base_mcp_servers, base_mcp_source) = if req.mcp_servers.is_empty() {
            cached_continuation
                .as_ref()
                .map(|c| {
                    (
                        c.capability_state.tool.mcp_servers.clone(),
                        LaunchMcpSource::CachedSessionProfile,
                    )
                })
                .unwrap_or_else(|| (Vec::new(), LaunchMcpSource::Empty))
        } else {
            (req.mcp_servers.clone(), LaunchMcpSource::Request)
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
        // base capability state: resolver 产出 + session-level MCP/VFS 完整合入
        let base_capability_source = if req.capability_state.is_some() {
            LaunchCapabilitySource::Request
        } else if cached_continuation.is_some() {
            LaunchCapabilitySource::CachedSessionProfile
        } else {
            LaunchCapabilitySource::Default
        };
        let base_capability_state = {
            let mut state = req
                .capability_state
                .clone()
                .or_else(|| {
                    cached_continuation
                        .as_ref()
                        .map(|c| c.capability_state.clone())
                })
                .unwrap_or_default();
            state.tool.mcp_servers = base_mcp_servers.clone();
            state.vfs.active = Some(base_effective_vfs.clone());
            state.skill.skills = session_capabilities.skills.clone();
            state
        };
        // 最终 capability state: 若有 pending transition 则使用其状态（补全 MCP/VFS）
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
        let capability_keys = capability_state.capability_keys();
        let (resolved_follow_up_session_id, follow_up_source) = follow_up_session_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| (Some(value.to_string()), LaunchFollowUpSource::Explicit))
            .or_else(|| {
                session_meta
                    .executor_session_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| (Some(value.to_string()), LaunchFollowUpSource::SessionMeta))
            })
            .unwrap_or((None, LaunchFollowUpSource::None));
        let launch_execution = LaunchExecution::build(LaunchExecutionInput {
            session_id: sid.clone(),
            turn_id: turn_id.clone(),
            lifecycle: prompt_lifecycle,
            restore_mode,
            hook_snapshot_reload: req.hook_snapshot_reload,
            follow_up_session_id: resolved_follow_up_session_id.clone(),
            follow_up_source,
            pending_transition_count: pending_capability_transitions.len(),
            vfs_source,
            pending_vfs_overlay_applied,
            mcp_source,
            capability_source,
            working_dir_input,
            working_directory,
            environment_variables: req.user_input.env,
            executor_config,
            mcp_servers: capability_state.tool.mcp_servers.clone(),
            vfs: capability_state.vfs.active.clone(),
            identity: req.identity.clone(),
            hook_session: hook_session.clone(),
            capability_state: capability_state.clone(),
            runtime_delegate,
            restored_session_state,
        });
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
        context.turn.assembled_tools = self
            .build_tools_for_execution_context(
                session_id,
                &context,
                &capability_state.tool.mcp_servers,
            )
            .await;

        let identity_frame = build_identity_context_frame(&IdentityFrameInput {
            base_system_prompt: &self.base_system_prompt,
            agent_system_prompt: context.session.executor_config.system_prompt.as_deref(),
            agent_system_prompt_mode: context.session.executor_config.system_prompt_mode,
            user_preferences: &self.user_preferences,
            discovered_guidelines: &discovered_guidelines,
        });

        let compose_fragments = req
            .context_bundle
            .as_ref()
            .map(|bundle| bundle.bootstrap_fragments.clone())
            .or_else(|| hook_snapshot_contribution.clone())
            .unwrap_or_default();
        let (audit_bundle_id, audit_session_id) = req
            .context_bundle
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
            self.turn_supervisor
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

        Self::apply_turn_start_meta(
            &mut session_meta,
            now,
            &turn_id,
            &context.session.executor_config,
            is_owner_bootstrap,
            &title_hint,
        );
        let _ = meta_store.save_session_meta(&session_meta).await;

        let pending_transition_frames = if !pending_capability_transitions.is_empty() {
            let frames = self
                .apply_pending_runtime_context_transitions_on_turn(
                    &sid,
                    &turn_id,
                    hook_session.as_ref(),
                    base_capability_state,
                    &pending_capability_transitions,
                    &context.turn.assembled_tools,
                )
                .await;
            let command_ids = pending_runtime_commands
                .iter()
                .map(|command| command.id)
                .collect::<Vec<_>>();
            if let Err(error) = runtime_command_store
                .mark_runtime_commands_applied(&command_ids)
                .await
            {
                tracing::warn!(
                    session_id = %sid,
                    error = %error,
                    "标记 pending runtime commands applied 失败"
                );
            }
            frames
        } else {
            Vec::new()
        };

        // 首轮 prompt 且 title_source 非 User 时，异步触发标题生成
        let is_first_turn = session_meta.last_event_seq <= 1;
        if is_first_turn
            && session_meta.title_source != super::types::TitleSource::User
            && self.title_generator.is_some()
        {
            self.spawn_title_generation(
                session_id.to_string(),
                resolved_payload.text_prompt.clone(),
            );
        }

        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = SourceInfo {
            connector_id: self.connector.connector_id().to_string(),
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
            let _ = self.persist_notification(&sid, envelope).await;
        }

        let started = build_turn_started_envelope(session_id, &source, &turn_id);
        let _ = self.persist_notification(&sid, started).await;

        // SessionStart 只代表 owner 首轮 bootstrap，不再与“进程内第几轮”绑定。
        if is_owner_bootstrap {
            if let Some(hook_session) = hook_session.as_ref() {
                let initial_caps = capability_keys.clone();
                if !initial_caps.is_empty() {
                    let _ = hook_session.update_capabilities(initial_caps.clone());
                }

                let _start_effects = self
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
            let _ = self.emit_context_frame(&sid, Some(&turn_id), &frame).await;
            owner_bootstrap_frames.push(frame);

            if let Some(frame) = build_assignment_context_frame(
                req.context_bundle
                    .as_ref()
                    .map(|bundle| bundle.phase_tag.as_str()),
                &compose_fragments,
            ) {
                let _ = self.emit_context_frame(&sid, Some(&turn_id), &frame).await;
                owner_bootstrap_frames.push(frame);
            }
        }

        let continuation_context_frame = req.continuation_context_frame.take();
        let mut turn_context_frames: Vec<ContextFrame> = Vec::new();
        if let Some(frame) = identity_frame {
            let _ = self.emit_context_frame(&sid, Some(&turn_id), &frame).await;
            turn_context_frames.push(frame);
        }
        if let Some(frame) = continuation_context_frame {
            let _ = self.emit_context_frame(&sid, Some(&turn_id), &frame).await;
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
                let _ = self.emit_context_frame(&sid, Some(&turn_id), frame).await;
            }
            turn_context_frames.extend(pending_action_frames);
        }
        context.turn.context_frames = dedupe_context_frames(turn_context_frames);

        enqueue_context_frames_for_transform_context(
            hook_session.as_ref(),
            &context.turn.context_frames,
        );

        let mut stream = match self
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
                self.turn_supervisor.clear_turn_and_hook(session_id).await;
                let failed = build_turn_terminal_envelope(
                    &sid,
                    &source,
                    &turn_id,
                    TurnTerminalKind::Failed,
                    Some(error.to_string()),
                );
                let _ = self.persist_notification(&sid, failed).await;
                return Err(error);
            }
        };
        let session_id = session_id.to_string();

        // 创建 SessionTurnProcessor — cloud-native 和 relay 共用的事件处理核心
        let processor = super::turn_processor::SessionTurnProcessor::spawn(
            self.clone(),
            super::turn_processor::SessionTurnProcessorConfig {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                source: source.clone(),
                hook_session,
                post_turn_handler: req.post_turn_handler,
            },
        );

        let processor_tx = processor.tx();

        // 注册 processor_tx 到 SessionRuntime.current_turn，供 relay / cancel 路径使用
        self.turn_supervisor
            .register_processor_tx(&session_id, processor_tx.clone())
            .await;

        Self::spawn_stream_adapter(
            self.turn_supervisor.clone(),
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

    /// 解析 hook runtime：决定 reload / refresh / skip。
    ///
    /// 三条路径：
    /// - **reload**：owner bootstrap 首轮 或 进程内没有 runtime（冷启动恢复）
    /// - **refresh**：已有 hook_session 且非 bootstrap → 只 refresh snapshot
    /// - **skip**：无 hook provider → 返回 None
    async fn resolve_hook_session(
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
    /// `start_prompt_with_follow_up` 自己的 happy path）只读取不回写。
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

    async fn discover_skills(&self, vfs: &agentdash_spi::Vfs) -> Vec<agentdash_spi::SkillRef> {
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

    async fn discover_guidelines(
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
    use super::*;

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
