use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client_protocol::{SessionNotification, SessionUpdate};
use futures::StreamExt;

use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::{ConnectorError, ExecutionContext};
use agentdash_spi::hooks::{
    HookTrigger, SessionHookSnapshotQuery, SharedHookSessionRuntime,
};

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
        let store = self.store.clone();
        let sid = session_id.to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let mut session_meta = match store.read_meta(&sid).await {
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
            .unwrap_or_default();

        let hook_session = match self
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
        };

        session_meta.updated_at = now;
        session_meta.last_execution_status = "running".to_string();
        session_meta.last_turn_id = Some(turn_id.clone());
        session_meta.last_terminal_message = None;
        session_meta.executor_config = Some(context.executor_config.clone());
        if session_meta.title.trim().is_empty() {
            session_meta.title = title_hint;
        }
        let _ = store.write_meta(&session_meta).await;

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
        source.variant = context.executor_config.variant.clone();
        let user_notifications = build_user_message_notifications(
            session_id,
            &source,
            &turn_id,
            &resolved_payload.user_blocks,
        );
        for notification in user_notifications {
            let _ = store.append(&sid, &notification).await;
            let _ = tx.send(notification);
        }

        let started = build_turn_lifecycle_notification(
            session_id,
            &source,
            &turn_id,
            "turn_started",
            "info",
            Some("开始执行".to_string()),
        );
        let _ = store.append(&sid, &started).await;
        let _ = tx.send(started);

        if let Some(hook_session) = hook_session.as_ref() {
            self.emit_session_hook_trigger(
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
                let mut sessions = self.sessions.lock().await;
                if let Some(runtime) = sessions.get_mut(session_id) {
                    runtime.running = false;
                    runtime.current_turn_id = None;
                    runtime.cancel_requested = false;
                    runtime.hook_session = None;
                }
                let failed = build_turn_terminal_notification(
                    &sid,
                    &source,
                    &turn_id,
                    TurnTerminalKind::Failed,
                    Some(error.to_string()),
                );
                let _ = store.append(&sid, &failed).await;
                if let Ok(Some(mut meta)) = store.read_meta(&sid).await {
                    meta.last_execution_status = "failed".to_string();
                    meta.last_turn_id = Some(turn_id.clone());
                    meta.last_terminal_message = Some(error.to_string());
                    meta.updated_at = chrono::Utc::now().timestamp_millis();
                    let _ = store.write_meta(&meta).await;
                }
                let _ = tx.send(failed);
                return Err(error);
            }
        };
        let sessions = self.sessions.clone();
        let session_id = session_id.to_string();
        let hook_session_for_spawn = hook_session;
        let hub = self.clone();

        let turn_id_for_spawn = turn_id.clone();
        tokio::spawn(async move {
            let mut terminal_notification: Option<SessionNotification> = None;
            let mut last_executor_session_id: Option<String> = None;
            let mut terminal_kind = TurnTerminalKind::Completed;
            let mut terminal_message: Option<String> = None;
            while let Some(next) = stream.next().await {
                match next {
                    Ok(notification) => {
                        let meta = match &notification.update {
                            SessionUpdate::SessionInfoUpdate(info) => info.meta.as_ref(),
                            _ => None,
                        };
                        if let Some(executor_session_id) =
                            parse_executor_session_bound(meta, &turn_id_for_spawn)
                            && last_executor_session_id.as_deref()
                                != Some(executor_session_id.as_str())
                        {
                            last_executor_session_id = Some(executor_session_id.clone());
                            if let Ok(Some(mut meta)) = store.read_meta(&session_id).await
                                && meta.executor_session_id.as_deref()
                                    != Some(executor_session_id.as_str())
                            {
                                meta.executor_session_id = Some(executor_session_id);
                                meta.updated_at = chrono::Utc::now().timestamp_millis();
                                let _ = store.write_meta(&meta).await;
                            }
                        }
                        let _ = store.append(&session_id, &notification).await;
                        let _ = tx.send(notification);
                    }
                    Err(e) => {
                        tracing::error!("执行流错误 session_id={}: {}", session_id, e);
                        let (cancel_requested, live_turn_matches) = {
                            let guard = sessions.lock().await;
                            match guard.get(&session_id) {
                                Some(runtime) => (
                                    runtime.cancel_requested,
                                    runtime.current_turn_id.as_deref()
                                        == Some(turn_id_for_spawn.as_str()),
                                ),
                                None => (false, false),
                            }
                        };
                        if cancel_requested && live_turn_matches {
                            terminal_kind = TurnTerminalKind::Interrupted;
                            terminal_message = Some("执行已取消".to_string());
                        } else {
                            terminal_kind = TurnTerminalKind::Failed;
                            terminal_message = Some(e.to_string());
                        }
                        terminal_notification = Some(build_turn_terminal_notification(
                            &session_id,
                            &source,
                            &turn_id_for_spawn,
                            terminal_kind,
                            terminal_message.clone(),
                        ));
                        break;
                    }
                }
            }

            if terminal_notification.is_none() {
                let (cancel_requested, live_turn_matches) = {
                    let guard = sessions.lock().await;
                    match guard.get(&session_id) {
                        Some(runtime) => (
                            runtime.cancel_requested,
                            runtime.current_turn_id.as_deref()
                                == Some(turn_id_for_spawn.as_str()),
                        ),
                        None => (false, false),
                    }
                };
                if cancel_requested && live_turn_matches {
                    terminal_kind = TurnTerminalKind::Interrupted;
                    if terminal_message.is_none() {
                        terminal_message = Some("执行已取消".to_string());
                    }
                }
                terminal_notification = Some(build_turn_terminal_notification(
                    &session_id,
                    &source,
                    &turn_id_for_spawn,
                    terminal_kind,
                    terminal_message.clone(),
                ));
            }

            if let Some(done) = terminal_notification {
                let _ = store.append(&session_id, &done).await;
                let _ = tx.send(done);
            }

            let status_str = terminal_kind.state_tag().to_string();
            if let Ok(Some(mut meta)) = store.read_meta(&session_id).await {
                meta.last_execution_status = status_str;
                meta.last_turn_id = Some(turn_id_for_spawn.clone());
                meta.last_terminal_message = terminal_message.clone();
                meta.updated_at = chrono::Utc::now().timestamp_millis();
                let _ = store.write_meta(&meta).await;
            }

            if let Some(hook_session) = hook_session_for_spawn {
                hub.emit_session_hook_trigger(
                    hook_session.as_ref(),
                    &HookTriggerInput {
                        session_id: &session_id,
                        turn_id: Some(&turn_id_for_spawn),
                        trigger: HookTrigger::SessionTerminal,
                        payload: Some(serde_json::json!({
                            "terminal_state": terminal_kind.state_tag(),
                            "message": terminal_message,
                        })),
                        refresh_reason: "trigger:session_terminal",
                        source: source.clone(),
                    },
                    &tx,
                )
                .await;
            }

            let mut guard = sessions.lock().await;
            if let Some(runtime) = guard.get_mut(&session_id) {
                runtime.running = false;
                runtime.current_turn_id = None;
                runtime.cancel_requested = false;
            }
        });

        Ok(turn_id)
    }

    pub(super) async fn load_session_hook_runtime(
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
