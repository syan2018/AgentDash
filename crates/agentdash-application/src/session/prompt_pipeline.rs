use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;

use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::hooks::{
    HookTrigger, SessionHookSnapshot, SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
use agentdash_spi::{
    ConnectorError, ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame,
    RestoredSessionState,
};

use super::baseline_capabilities::build_session_baseline_capabilities;
use super::hub::HookTriggerInput;
use super::hook_delegate::HookRuntimeDelegate;
use super::hook_runtime::HookSessionRuntime;
use super::hub::SessionHub;
use super::hub_support::*;
use super::path_policy::resolve_working_dir;
pub use super::types::*;

impl SessionHub {
    /// 多轮对话（支持底层执行器 follow-up 会话续跑）。
    pub async fn start_prompt_with_follow_up(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        mut req: PromptSessionRequest,
    ) -> Result<String, ConnectorError> {
        let resolved_payload = req
            .user_input
            .resolve_prompt_payload()
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());
        let had_existing_runtime = self.connector.has_live_session(session_id).await;

        {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = tokio::sync::broadcast::channel(1024);
                build_session_runtime(tx)
            });
            if runtime.running {
                return Err(ConnectorError::Runtime(
                    "该会话有正在执行的 prompt，请等待完成或取消后再试".into(),
                ));
            }
            runtime.running = true;
            runtime.current_turn_id = Some(turn_id.clone());
            runtime.cancel_requested = false;
        }

        let effective_vfs = req
            .vfs
            .clone()
            .or_else(|| self.default_vfs.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "prompt 缺少 vfs，且 SessionHub 未配置默认 vfs".to_string(),
                )
            })?;
        let default_mount_root = effective_vfs
            .default_mount()
            .map(|m| PathBuf::from(m.root_ref.trim()))
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("vfs 缺少 default_mount 或 root_ref 无效".to_string())
            })?;
        let working_directory =
            resolve_working_dir(&default_mount_root, req.user_input.working_dir.as_deref());

        let title_hint = resolved_payload
            .text_prompt
            .chars()
            .take(30)
            .collect::<String>();
        let persistence = self.persistence.clone();
        let sid = session_id.to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let mut session_meta = match persistence.get_session_meta(&sid).await {
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
        let existing_hook_session = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(session_id)
                .and_then(|rt| rt.hook_session.clone())
        };

        // hook runtime 的语义与 owner bootstrap 解耦：
        // - owner 首轮 bootstrap：总是重新 load snapshot，并触发 SessionStart
        // - 同进程续跑：复用已有 hook_session，只 refresh snapshot
        // - 冷启动恢复：若内存里没有 runtime，则重建 snapshot，但不触发 SessionStart
        let hook_session: Option<SharedHookSessionRuntime> =
            if is_owner_bootstrap || existing_hook_session.is_none() {
                match self
                    .load_session_hook_runtime(
                        session_id,
                        &turn_id,
                        executor_config.executor.as_str(),
                        executor_config.permission_policy.as_deref(),
                        working_directory.as_path(),
                    )
                    .await
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let mut sessions = self.sessions.lock().await;
                        if let Some(runtime) = sessions.get_mut(session_id) {
                            runtime.running = false;
                            runtime.current_turn_id = None;
                            runtime.cancel_requested = false;
                            runtime.hook_session = None;
                        }
                        return Err(error);
                    }
                }
            } else {
                if let Some(ref hs) = existing_hook_session {
                    let _ = hs
                        .refresh(agentdash_spi::hooks::SessionHookRefreshQuery {
                            session_id: session_id.to_string(),
                            turn_id: Some(turn_id.clone()),
                            reason: Some("subsequent_turn_refresh".to_string()),
                        })
                        .await;
                }
                existing_hook_session
            };

        {
            let mut sessions = self.sessions.lock().await;
            if let Some(runtime) = sessions.get_mut(session_id) {
                runtime.hook_session = hook_session.clone();
            }
        }

        // 把 hook snapshot 里的 injection 合并到 Bundle 的 bootstrap_fragments —
        // 这是 PR 4（04-30-session-pipeline-architecture-refactor）的核心动作：
        // companion_agents / workflow / constraint 等 hook 注入不再通过 SP 独立
        // section 或 user_message 渲染，而是由 Bundle `render_section` 统一产出。
        // `From<&SessionHookSnapshot> for Contribution` 封装了 slot → order 映射。
        if let Some(ref hs) = hook_session
            && let Some(bundle) = req.context_bundle.as_mut()
        {
            let snapshot = hs.snapshot();
            let contribution: crate::context::Contribution = (&snapshot).into();
            bundle.merge(contribution.fragments);
        }

        let context_audit_bus = self.current_context_audit_bus().await;
        let runtime_delegate = hook_session.as_ref().map(|hs| {
            HookRuntimeDelegate::new_with_mount_root_and_audit(
                hs.clone(),
                Some(default_mount_root.to_string_lossy().replace('\\', "/")),
                context_audit_bus.clone(),
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
        let restored_session_state = match prompt_lifecycle {
            SessionPromptLifecycle::RepositoryRehydrate(
                SessionRepositoryRehydrateMode::ExecutorState,
            ) => {
                let messages = self
                    .build_restored_session_messages(session_id)
                    .await
                    .map_err(|error| {
                        ConnectorError::Runtime(format!(
                            "重建 session `{session_id}` 历史消息失败: {error}"
                        ))
                    })?;
                (!messages.is_empty()).then_some(RestoredSessionState { messages })
            }
            _ => None,
        };

        // 通过 VFS service 扫描所有 mount 的 skill
        let mut discovered_skills = if let Some(service) = &self.vfs_service {
            let skill_result = crate::skill::load_skills_from_vfs(service, &effective_vfs).await;
            for diag in &skill_result.diagnostics {
                tracing::warn!(
                    skill_name = %diag.name,
                    path = %diag.file_path.display(),
                    "skill 诊断: {}",
                    diag.message
                );
            }
            skill_result.skills
        } else {
            Vec::new()
        };

        // 合并插件提供的额外 skill 目录（优先级低于 mount 内发现的同名 skill）
        if !self.extra_skill_dirs.is_empty() {
            let existing_names: std::collections::HashMap<String, String> = discovered_skills
                .iter()
                .map(|s| (s.name.clone(), s.file_path.to_string_lossy().to_string()))
                .collect();
            let plugin_result =
                crate::skill::load_skills_from_local_dirs(&self.extra_skill_dirs, &existing_names);
            for diag in &plugin_result.diagnostics {
                tracing::warn!(
                    skill_name = %diag.name,
                    path = %diag.file_path.display(),
                    "skill 诊断 (plugin): {}",
                    diag.message
                );
            }
            discovered_skills.extend(plugin_result.skills);
        }

        let session_capabilities =
            build_session_baseline_capabilities(hook_session.as_deref(), &discovered_skills);

        // 通过 VFS service 扫描项目级约定文件（AGENTS.md / MEMORY.md）
        let discovered_guidelines = if let Some(service) = &self.vfs_service {
            let discovery_result = crate::context::mount_file_discovery::discover_mount_files(
                service,
                &effective_vfs,
                crate::context::mount_file_discovery::BUILTIN_GUIDELINE_RULES,
            )
            .await;
            for diag in &discovery_result.diagnostics {
                tracing::warn!(
                    rule_key = %diag.rule_key,
                    mount_id = %diag.mount_id,
                    path = %diag.path,
                    "guideline 发现诊断: {}",
                    diag.message
                );
            }
            discovery_result
                .files
                .into_iter()
                .map(|f| agentdash_spi::DiscoveredGuideline {
                    file_name: f.path.rsplit('/').next().unwrap_or(&f.path).to_string(),
                    mount_id: f.mount_id,
                    path: f.path,
                    content: f.content,
                })
                .collect()
        } else {
            Vec::new()
        };

        let mcp_servers = req.mcp_servers.clone();
        let relay_mcp_server_names = req.relay_mcp_server_names.clone();
        let flow_capabilities = req.flow_capabilities.clone().unwrap_or_default();
        let effective_capability_keys = req.effective_capability_keys.clone().unwrap_or_default();
        let identity = req.identity.clone();

        let session_frame = ExecutionSessionFrame {
            turn_id: turn_id.clone(),
            working_directory,
            environment_variables: req.user_input.env,
            executor_config,
            mcp_servers: mcp_servers.clone(),
            vfs: Some(effective_vfs.clone()),
            identity: identity.clone(),
        };
        // 主数据面：Bundle 下发到 TurnFrame，connector 侧优先消费它；
        // backward-compat：仍保留 `assembled_system_prompt` 兜底给 Relay / vibe_kanban。
        #[allow(deprecated)]
        let turn_frame = ExecutionTurnFrame {
            hook_session: hook_session.clone(),
            flow_capabilities: flow_capabilities.clone(),
            runtime_delegate,
            restored_session_state,
            context_bundle: req.context_bundle.clone(),
            assembled_system_prompt: None,
            assembled_tools: Vec::new(),
        };
        let mut context = ExecutionContext {
            session: session_frame,
            turn: turn_frame,
        };

        // pipeline 层预构建工具列表：runtime + direct MCP + relay MCP
        context.turn.assembled_tools = self
            .build_tools_for_execution_context(
                session_id,
                &context,
                &mcp_servers,
                &relay_mcp_server_names,
            )
            .await;

        // pipeline 层预组装 system prompt
        if !self.base_system_prompt.is_empty() {
            let prompt_input = super::system_prompt_assembler::SystemPromptInput {
                base_system_prompt: &self.base_system_prompt,
                agent_system_prompt: context.session.executor_config.system_prompt.as_deref(),
                agent_system_prompt_mode: context.session.executor_config.system_prompt_mode,
                user_preferences: &self.user_preferences,
                discovered_guidelines: &discovered_guidelines,
                context_bundle: req.context_bundle.as_ref(),
                session_capabilities: Some(&session_capabilities),
                vfs: Some(&effective_vfs),
                working_directory: &context.session.working_directory,
                runtime_tools: &context.turn.assembled_tools,
                mcp_servers: &mcp_servers,
                hook_session: hook_session.as_deref(),
            };
            #[allow(deprecated)]
            {
                context.turn.assembled_system_prompt = Some(
                    super::system_prompt_assembler::assemble_system_prompt(&prompt_input),
                );
            }
        }

        {
            let mut sessions = self.sessions.lock().await;
            if let Some(runtime) = sessions.get_mut(session_id) {
                runtime.active_execution = Some(ActiveSessionExecutionState {
                    session_frame: context.session.clone(),
                    relay_mcp_server_names: relay_mcp_server_names.clone(),
                    flow_capabilities: flow_capabilities.clone(),
                });
            }
        }

        session_meta.updated_at = now;
        session_meta.last_execution_status = "running".to_string();
        session_meta.last_turn_id = Some(turn_id.clone());
        session_meta.last_terminal_message = None;
        session_meta.executor_config = Some(context.session.executor_config.clone());
        if is_owner_bootstrap {
            session_meta.bootstrap_state = SessionBootstrapState::Bootstrapped;
        }
        if session_meta.title.trim().is_empty() {
            session_meta.title = title_hint.clone();
        }
        let _ = persistence.save_session_meta(&session_meta).await;

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

        let resolved_follow_up_session_id = follow_up_session_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                session_meta
                    .executor_session_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
            });

        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let mut source = AgentDashSourceV1::new(self.connector.connector_id(), connector_type);
        source.executor_id = Some(context.session.executor_config.executor.to_string());

        // PR 4（04-30-session-pipeline-architecture-refactor）删除 `session-capabilities://`
        // resource block 注入路径：companion_agents 已改由 Bundle 渲染到 SP
        // `## Project Context`；skills 由 `<available_skills>` XML 块承载；
        // capabilities 结构本身如有持久化需求应走 SessionMeta，而非 user_blocks。
        let user_notifications = build_user_message_notifications(
            session_id,
            &source,
            &turn_id,
            &resolved_payload.user_blocks,
        );
        for notification in user_notifications {
            let _ = self.persist_notification(&sid, notification).await;
        }

        let started = build_turn_lifecycle_notification(
            session_id,
            &source,
            &turn_id,
            "turn_started",
            "info",
            Some("开始执行".to_string()),
        );
        let _ = self.persist_notification(&sid, started).await;

        // SessionStart 只代表 owner 首轮 bootstrap，不再与“进程内第几轮”绑定。
        if is_owner_bootstrap {
            if let Some(hook_session) = hook_session.as_ref() {
                let initial_caps = effective_capability_keys.clone();
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
                                "capabilities": initial_caps.iter().collect::<Vec<_>>(),
                            })),
                            refresh_reason: "trigger:session_start",
                            source: source.clone(),
                        },
                    )
                    .await;
            }
        }

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
                {
                    let mut sessions = self.sessions.lock().await;
                    if let Some(runtime) = sessions.get_mut(session_id) {
                        runtime.running = false;
                        runtime.current_turn_id = None;
                        runtime.cancel_requested = false;
                        runtime.hook_session = None;
                    }
                }
                let failed = build_turn_terminal_notification(
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

        // 注册 processor_tx 到 SessionRuntime，供 relay 路径使用
        {
            let mut sessions = self.sessions.lock().await;
            if let Some(runtime) = sessions.get_mut(&session_id) {
                runtime.processor_tx = Some(processor_tx.clone());
            }
        }

        // connector stream → processor channel 适配器
        let sessions = self.sessions.clone();
        let turn_id_for_adapter = turn_id.clone();
        tokio::spawn(async move {
            while let Some(next) = stream.next().await {
                match next {
                    Ok(notification) => {
                        let _ = processor_tx
                            .send(super::turn_processor::TurnEvent::Notification(notification));
                    }
                    Err(e) => {
                        tracing::error!("执行流错误 session_id={}: {}", session_id, e);
                        let (cancel_requested, live_turn_matches) = {
                            let guard = sessions.lock().await;
                            match guard.get(&session_id) {
                                Some(runtime) => (
                                    runtime.cancel_requested,
                                    runtime.current_turn_id.as_deref()
                                        == Some(turn_id_for_adapter.as_str()),
                                ),
                                None => (false, false),
                            }
                        };
                        let (kind, message) = if cancel_requested && live_turn_matches {
                            (
                                TurnTerminalKind::Interrupted,
                                Some("执行已取消".to_string()),
                            )
                        } else {
                            (TurnTerminalKind::Failed, Some(e.to_string()))
                        };
                        let _ = processor_tx
                            .send(super::turn_processor::TurnEvent::Terminal { kind, message });
                        return;
                    }
                }
            }

            // stream 正常结束 → 发送显式 Terminal（不能依赖 drop sender，因为还有其他 clone 存活）
            let (cancel_requested, live_turn_matches) = {
                let guard = sessions.lock().await;
                match guard.get(&session_id) {
                    Some(runtime) => (
                        runtime.cancel_requested,
                        runtime.current_turn_id.as_deref() == Some(turn_id_for_adapter.as_str()),
                    ),
                    None => (false, false),
                }
            };
            let (kind, message) = if cancel_requested && live_turn_matches {
                (
                    TurnTerminalKind::Interrupted,
                    Some("执行已取消".to_string()),
                )
            } else {
                (TurnTerminalKind::Completed, None)
            };
            let _ = processor_tx.send(super::turn_processor::TurnEvent::Terminal { kind, message });
        });

        Ok(turn_id)
    }

    pub async fn load_session_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        working_directory: &Path,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let Some(provider) = self.hook_provider.as_ref() else {
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

        Ok(Some(Arc::new(HookSessionRuntime::new(
            session_id.to_string(),
            provider.clone(),
            snapshot,
        ))))
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

/// 从 McpServer 提取 server name（与 system_prompt_assembler 同逻辑）。
pub(super) fn extract_mcp_server_name(server: &agent_client_protocol::McpServer) -> String {
    serde_json::to_value(server)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// 将 MCP servers 按 relay 标记分为两组，返回 (relay_names, direct_servers)。
pub(super) fn partition_mcp_servers(
    servers: &[agent_client_protocol::McpServer],
    relay_names_set: &std::collections::HashSet<String>,
) -> (Vec<String>, Vec<agent_client_protocol::McpServer>) {
    let mut relay_names = Vec::new();
    let mut direct = Vec::new();
    for server in servers {
        let name = extract_mcp_server_name(server);
        if relay_names_set.contains(&name) {
            relay_names.push(name);
        } else {
            direct.push(server.clone());
        }
    }
    (relay_names, direct)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_spi::{HookInjection, NoopExecutionHookProvider};

    use super::*;

    #[test]
    fn baseline_capabilities_built_and_attached_to_context() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-pipeline".to_string(),
            injections: vec![HookInjection {
                slot: "companion_agents".to_string(),
                content: "## Companion Agents\n\n- **agent** (executor: `PI_AGENT`): Agent"
                    .to_string(),
                source: "builtin:companion_agents".to_string(),
            }],
            ..SessionHookSnapshot::default()
        };
        let runtime = HookSessionRuntime::new(
            "sess-pipeline".to_string(),
            Arc::new(NoopExecutionHookProvider),
            snapshot,
        );
        let caps = build_session_baseline_capabilities(
            Some(&runtime as &dyn agentdash_spi::hooks::HookSessionRuntimeAccess),
            &[agentdash_spi::SkillRef {
                name: "my-skill".to_string(),
                description: "test".to_string(),
                file_path: "/ws/SKILL.md".into(),
                base_dir: "/ws".into(),
                disable_model_invocation: false,
            }],
        );
        assert_eq!(caps.companion_agents.len(), 1);
        assert_eq!(caps.skills.len(), 1);
        assert_eq!(caps.companion_agents[0].name, "agent");
    }
}
