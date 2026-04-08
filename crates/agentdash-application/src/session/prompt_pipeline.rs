use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;

use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::hooks::{HookTrigger, SessionHookSnapshotQuery, SharedHookSessionRuntime};
use agentdash_spi::{ConnectorError, ExecutionContext, RestoredSessionState};
use crate::skill::load_skills_for_workspace;

use super::event_bridge::HookTriggerInput;
use super::hook_delegate::HookRuntimeDelegate;
use super::hook_runtime::HookSessionRuntime;
use super::hub::SessionHub;
use super::hub_support::*;
pub use super::types::*;

impl SessionHub {
    /// 多轮对话（支持底层执行器 follow-up 会话续跑）。
    pub async fn start_prompt_with_follow_up(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        req: PromptSessionRequest,
    ) -> Result<String, ConnectorError> {
        let resolved_payload = req
            .user_input
            .resolve_prompt_payload()
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());
        let had_existing_runtime = self.connector.has_live_session(session_id).await;

        let tx = {
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
            runtime.tx.clone()
        };

        let workspace_root = req
            .workspace_root
            .clone()
            .unwrap_or_else(|| self.workspace_root.clone());
        let working_directory =
            resolve_working_dir(&workspace_root, req.user_input.working_dir.as_deref());

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

        let is_owner_bootstrap = req.bootstrap_action == SessionBootstrapAction::OwnerContext;
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
        let hook_session: Option<SharedHookSessionRuntime> = if is_owner_bootstrap
            || existing_hook_session.is_none()
        {
            match self
                .load_session_hook_runtime(
                    session_id,
                    &turn_id,
                    executor_config.executor.as_str(),
                    executor_config.permission_policy.as_deref(),
                    workspace_root.as_path(),
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

        let runtime_delegate = hook_session
            .as_ref()
            .map(|hs| HookRuntimeDelegate::new(hs.clone()));
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

        // 扫描工作区 skill（first-wins，诊断信息输出到日志）
        let discovered_skills = {
            let skill_result = load_skills_for_workspace(&workspace_root);
            for diag in &skill_result.diagnostics {
                tracing::warn!(
                    skill_name = %diag.name,
                    path = %diag.file_path.display(),
                    "skill 诊断: {}",
                    diag.message
                );
            }
            skill_result.skills
        };

        let context = ExecutionContext {
            turn_id: turn_id.clone(),
            workspace_root,
            working_directory,
            environment_variables: req.user_input.env,
            executor_config,
            mcp_servers: req.mcp_servers,
            address_space: req.address_space,
            hook_session: hook_session.clone(),
            flow_capabilities: req.flow_capabilities.unwrap_or_default(),
            system_context: req.system_context,
            runtime_delegate,
            identity: req.identity,
            restored_session_state,
            skills: discovered_skills,
        };

        session_meta.updated_at = now;
        session_meta.last_execution_status = "running".to_string();
        session_meta.last_turn_id = Some(turn_id.clone());
        session_meta.last_terminal_message = None;
        session_meta.executor_config = Some(context.executor_config.clone());
        if is_owner_bootstrap {
            session_meta.bootstrap_state = SessionBootstrapState::Bootstrapped;
        }
        if session_meta.title.trim().is_empty() {
            session_meta.title = title_hint;
        }
        let _ = persistence.save_session_meta(&session_meta).await;

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
        source.executor_id = Some(context.executor_config.executor.to_string());
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
                            })),
                            refresh_reason: "trigger:session_start",
                            source: source.clone(),
                        },
                        &tx,
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
                            (TurnTerminalKind::Interrupted, Some("执行已取消".to_string()))
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
                        runtime.current_turn_id.as_deref()
                            == Some(turn_id_for_adapter.as_str()),
                    ),
                    None => (false, false),
                }
            };
            let (kind, message) = if cancel_requested && live_turn_matches {
                (TurnTerminalKind::Interrupted, Some("执行已取消".to_string()))
            } else {
                (TurnTerminalKind::Completed, None)
            };
            let _ = processor_tx
                .send(super::turn_processor::TurnEvent::Terminal { kind, message });
        });

        Ok(turn_id)
    }

    pub async fn load_session_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        workspace_root: &Path,
        working_directory: &Path,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let Some(provider) = self.hook_provider.as_ref() else {
            return Ok(None);
        };

        let snapshot = provider
            .load_session_snapshot(SessionHookSnapshotQuery {
                session_id: session_id.to_string(),
                turn_id: Some(turn_id.to_string()),
                connector_id: Some(self.connector.connector_id().to_string()),
                executor: Some(executor.to_string()),
                permission_policy: permission_policy.map(ToString::to_string),
                working_directory: Some(working_directory.to_string_lossy().replace('\\', "/")),
                workspace_root: Some(workspace_root.to_string_lossy().replace('\\', "/")),
                owners: Vec::new(),
                tags: Vec::new(),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("加载会话 Hook snapshot 失败: {error}"))
            })?;

        Ok(Some(Arc::new(HookSessionRuntime::new(
            session_id.to_string(),
            provider.clone(),
            snapshot,
        ))))
    }
}

fn resolve_working_dir(workspace_root: &Path, working_dir: Option<&str>) -> PathBuf {
    match working_dir {
        Some(rel) if !rel.trim().is_empty() => workspace_root.join(rel),
        _ => workspace_root.to_path_buf(),
    }
}
