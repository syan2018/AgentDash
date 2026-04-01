use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use agent_client_protocol::{SessionNotification, SessionUpdate};
use tokio::sync::{Mutex, broadcast};

use super::hook_messages as msg;
use super::hook_runtime::HookSessionRuntime;
use super::hub_support::*;
use super::persistence::SessionPersistence;
pub use super::types::*;
use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::hooks::{
    ExecutionHookProvider, SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
use agentdash_spi::{AgentConnector, ConnectorError};

#[derive(Clone)]
pub struct SessionHub {
    pub(super) workspace_root: PathBuf,
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
    pub(super) sessions: Arc<Mutex<HashMap<String, SessionRuntime>>>,
    pub(super) persistence: Arc<dyn SessionPersistence>,
}

impl SessionHub {
    pub fn new_with_hooks_and_persistence(
        workspace_root: PathBuf,
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
        persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self {
            workspace_root,
            connector,
            hook_provider,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            persistence,
        }
    }

    /// 启动时调用：将上次进程异常退出时残留的 `running` 状态修正为 `interrupted`。
    pub async fn recover_interrupted_sessions(&self) -> std::io::Result<()> {
        let sessions = self.persistence.list_sessions().await?;
        for mut meta in sessions {
            if meta.last_execution_status == "running" {
                tracing::warn!(
                    session_id = %meta.id,
                    "启动恢复：session 上次未正常结束，标记为 interrupted"
                );
                if let Some(turn_id) = meta.last_turn_id.clone() {
                    let source = AgentDashSourceV1::new("agentdash-server", "system");
                    let notification = build_turn_terminal_notification(
                        &meta.id,
                        &source,
                        &turn_id,
                        TurnTerminalKind::Interrupted,
                        Some("检测到进程重启，已将上次未完成执行标记为 interrupted".to_string()),
                    );
                    let _ = self.persist_notification(&meta.id, notification).await?;
                    continue;
                }
                meta.last_execution_status = "interrupted".to_string();
                meta.updated_at = chrono::Utc::now().timestamp_millis();
                self.persistence.save_session_meta(&meta).await?;
            }
        }
        Ok(())
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub async fn create_session(&self, title: &str) -> std::io::Result<SessionMeta> {
        let id = format!(
            "sess-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let now = chrono::Utc::now().timestamp_millis();
        let meta = SessionMeta {
            id: id.clone(),
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            last_event_seq: 0,
            last_execution_status: "idle".to_string(),
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
        };
        self.persistence.create_session(&meta).await?;
        Ok(meta)
    }

    pub async fn list_sessions(&self) -> std::io::Result<Vec<SessionMeta>> {
        self.persistence.list_sessions().await
    }

    pub async fn get_session_meta(&self, session_id: &str) -> std::io::Result<Option<SessionMeta>> {
        self.persistence.get_session_meta(session_id).await
    }

    /// 批量获取多个 session 的 meta，并发读取。
    pub async fn get_session_metas_bulk(
        &self,
        session_ids: &[String],
    ) -> std::io::Result<std::collections::HashMap<String, SessionMeta>> {
        use futures::future::join_all;

        let futures: Vec<_> = session_ids
            .iter()
            .map(|id| {
                let persistence = self.persistence.clone();
                let id = id.clone();
                async move {
                    let meta = persistence.get_session_meta(&id).await?;
                    Ok::<_, std::io::Error>((id, meta))
                }
            })
            .collect();

        let results = join_all(futures).await;
        let mut map = std::collections::HashMap::with_capacity(session_ids.len());
        for result in results {
            let (id, maybe_meta) = result?;
            if let Some(meta) = maybe_meta {
                map.insert(id, meta);
            }
        }
        Ok(map)
    }

    /// 批量查询 session 执行状态。
    ///
    /// 优先从内存 map 判断是否正在运行（无延迟），
    /// 否则读 meta 的 last_execution_status（持久化的终态）。
    pub async fn inspect_execution_states_bulk(
        &self,
        session_ids: &[String],
    ) -> std::collections::HashMap<String, SessionExecutionState> {
        let running_set: std::collections::HashSet<String> = {
            let sessions = self.sessions.lock().await;
            session_ids
                .iter()
                .filter(|id| sessions.get(id.as_str()).is_some_and(|r| r.running))
                .cloned()
                .collect()
        };

        let mut result = std::collections::HashMap::with_capacity(session_ids.len());
        for id in session_ids {
            if running_set.contains(id) {
                result.insert(id.clone(), SessionExecutionState::Running { turn_id: None });
            } else {
                let status = self
                    .persistence
                    .get_session_meta(id)
                    .await
                    .ok()
                    .flatten()
                    .map(|meta| meta_to_execution_state(&meta, id))
                    .unwrap_or(SessionExecutionState::Idle);
                result.insert(id.clone(), status);
            }
        }
        result
    }

    pub async fn update_session_meta<F>(
        &self,
        session_id: &str,
        updater: F,
    ) -> std::io::Result<Option<SessionMeta>>
    where
        F: FnOnce(&mut SessionMeta),
    {
        let Some(mut meta) = self.persistence.get_session_meta(session_id).await? else {
            return Ok(None);
        };
        updater(&mut meta);
        meta.updated_at = chrono::Utc::now().timestamp_millis();
        self.persistence.save_session_meta(&meta).await?;
        Ok(Some(meta))
    }

    /// 查询单个 session 的执行状态。
    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> std::io::Result<SessionExecutionState> {
        let (running, live_turn_id) = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(session_id)
                .map(|runtime| (runtime.running, runtime.current_turn_id.clone()))
        }
        .unwrap_or((false, None));

        if running {
            return Ok(SessionExecutionState::Running {
                turn_id: live_turn_id,
            });
        }

        let Some(meta) = self.persistence.get_session_meta(session_id).await? else {
            return Ok(SessionExecutionState::Idle);
        };

        Ok(meta_to_execution_state(&meta, session_id))
    }

    pub async fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(session_id);
        }
        self.persistence.delete_session(session_id).await
    }

    pub async fn ensure_session(
        &self,
        session_id: &str,
    ) -> broadcast::Receiver<super::persistence::PersistedSessionEvent> {
        let mut sessions = self.sessions.lock().await;
        let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            build_session_runtime(tx)
        });
        runtime.tx.subscribe()
    }

    pub async fn get_hook_session_runtime(
        &self,
        session_id: &str,
    ) -> Option<SharedHookSessionRuntime> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .and_then(|runtime| runtime.hook_session.clone())
    }

    pub async fn ensure_hook_session_runtime(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        {
            let sessions = self.sessions.lock().await;
            if let Some(runtime) = sessions
                .get(session_id)
                .and_then(|runtime| runtime.hook_session.clone())
            {
                return Ok(Some(runtime));
            }
        }

        if self
            .persistence
            .get_session_meta(session_id)
            .await?
            .is_none()
        {
            return Ok(None);
        }

        let Some(provider) = self.hook_provider.as_ref() else {
            return Ok(None);
        };

        let snapshot = provider
            .load_session_snapshot(SessionHookSnapshotQuery {
                session_id: session_id.to_string(),
                turn_id: turn_id.map(ToString::to_string),
                connector_id: None,
                executor: None,
                permission_policy: None,
                working_directory: None,
                workspace_root: None,
                owners: Vec::new(),
                tags: Vec::new(),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("重建会话 Hook snapshot 失败: {error}"))
            })?;

        let rebuilt_runtime = Arc::new(HookSessionRuntime::new(
            session_id.to_string(),
            provider.clone(),
            snapshot,
        ));

        let mut sessions = self.sessions.lock().await;
        let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            build_session_runtime(tx)
        });
        if runtime.hook_session.is_none() {
            runtime.hook_session = Some(rebuilt_runtime.clone());
        }
        Ok(runtime.hook_session.clone())
    }

    /// 多轮对话：同一 session 允许多次调用，但同一时间只允许一次活跃执行。
    pub async fn start_prompt(
        &self,
        session_id: &str,
        req: PromptSessionRequest,
    ) -> Result<String, ConnectorError> {
        self.start_prompt_with_follow_up(session_id, None, req)
            .await
    }

    pub async fn subscribe_with_history(
        &self,
        session_id: &str,
    ) -> io::Result<SessionEventSubscription> {
        self.subscribe_after(session_id, 0).await
    }

    pub async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventSubscription> {
        let rx = self.ensure_session(session_id).await;
        let backlog = self.persistence.read_backlog(session_id, after_seq).await?;
        Ok(SessionEventSubscription {
            snapshot_seq: backlog.snapshot_seq,
            backlog: backlog.events,
            rx,
        })
    }

    pub async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<super::persistence::SessionEventPage> {
        self.persistence
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    /// 向指定 session 主动注入通知：先持久化，再广播。
    pub async fn inject_notification(
        &self,
        session_id: &str,
        notification: SessionNotification,
    ) -> std::io::Result<()> {
        let _ = self.persist_notification(session_id, notification).await?;
        Ok(())
    }

    pub async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        let (running, current_turn_id, tx) = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
                build_session_runtime(tx)
            });
            if runtime.running {
                runtime.cancel_requested = true;
            }
            (
                runtime.running,
                runtime.current_turn_id.clone(),
                runtime.tx.clone(),
            )
        };

        if running {
            self.connector.cancel(session_id).await?;
            return Ok(());
        }

        let history = self
            .persistence
            .list_all_events(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let mut latest_turn_id = current_turn_id;
        let mut terminal_by_turn: HashMap<String, (TurnTerminalKind, Option<String>)> =
            HashMap::new();
        for event in history {
            match &event.notification.update {
                SessionUpdate::UserMessageChunk(chunk) => {
                    if let Some(turn_id) = parse_turn_id(chunk.meta.as_ref()) {
                        latest_turn_id = Some(turn_id);
                    }
                }
                SessionUpdate::SessionInfoUpdate(info) => {
                    if let Some((turn_id, terminal_kind, message)) =
                        parse_turn_terminal_event(info.meta.as_ref())
                    {
                        terminal_by_turn.insert(turn_id, (terminal_kind, message));
                    }
                }
                _ => {}
            }
        }

        let Some(turn_id) = latest_turn_id else {
            return Ok(());
        };
        if terminal_by_turn.contains_key(&turn_id) {
            return Ok(());
        }

        let source = AgentDashSourceV1::new(self.connector.connector_id(), "local_executor");
        let interrupted = build_turn_terminal_notification(
            session_id,
            &source,
            &turn_id,
            TurnTerminalKind::Interrupted,
            Some("检测到未收尾的旧执行，已手动标记为 interrupted".to_string()),
        );
        let _ = tx;
        let _ = self
            .persist_notification(session_id, interrupted)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        Ok(())
    }

    pub(super) async fn persist_notification(
        &self,
        session_id: &str,
        notification: SessionNotification,
    ) -> io::Result<super::persistence::PersistedSessionEvent> {
        let persisted = self
            .persistence
            .append_event(session_id, &notification)
            .await?;
        let tx = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
                build_session_runtime(tx)
            });
            runtime.tx.clone()
        };
        let _ = tx.send(persisted.clone());
        Ok(persisted)
    }

    /// Hook auto-resume: schedule a delayed follow-up prompt in a separate task.
    /// Uses fire-and-forget to avoid awaiting `start_prompt` directly inside
    /// the stream-processing spawn block (whose Future is not Send).
    pub(super) fn schedule_hook_auto_resume(&self, session_id: String) {
        let hub = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let resume_req = PromptSessionRequest::from_user_input(UserPromptInput {
                prompt: Some(msg::AUTO_RESUME_PROMPT.to_string()),
                prompt_blocks: None,
                working_dir: None,
                env: std::collections::HashMap::new(),
                executor_config: None,
            });
            if let Err(e) = hub.start_prompt(&session_id, resume_req).await {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "Hook auto-resume failed"
                );
            }
        });
    }

    pub async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        self.connector
            .approve_tool_call(session_id, tool_call_id)
            .await
    }

    pub async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        self.connector
            .reject_tool_call(session_id, tool_call_id, reason)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MemorySessionPersistence;
    use agent_client_protocol::ContentBlock;
    use agentdash_spi::PromptPayload;
    use agentdash_spi::hooks::HookTrigger;
    use agentdash_spi::hooks::SessionHookRefreshQuery;
    use agentdash_spi::hooks::{HookEvaluationQuery, HookResolution, SessionHookSnapshot};
    use futures::stream;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::{Mutex as TokioMutex, mpsc};
    use tokio_stream::wrappers::ReceiverStream;

    fn test_hub(
        workspace_root: PathBuf,
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn agentdash_spi::hooks::ExecutionHookProvider>>,
    ) -> SessionHub {
        SessionHub::new_with_hooks_and_persistence(
            workspace_root,
            connector,
            hook_provider,
            Arc::new(MemorySessionPersistence::default()),
        )
    }

    fn simple_prompt_request(prompt: &str) -> PromptSessionRequest {
        PromptSessionRequest {
            user_input: UserPromptInput {
                prompt: Some(prompt.to_string()),
                prompt_blocks: None,
                working_dir: None,
                env: HashMap::new(),
                executor_config: None,
            },
            mcp_servers: vec![],
            workspace_root: None,
            address_space: None,
            flow_capabilities: None,
            system_context: None,
            identity: None,
        }
    }

    #[test]
    fn resolve_prompt_payload_from_text_prompt() {
        let input = UserPromptInput {
            prompt: Some("  hello world  ".to_string()),
            prompt_blocks: None,
            working_dir: None,
            env: std::collections::HashMap::new(),
            executor_config: None,
        };

        let payload = input
            .resolve_prompt_payload()
            .expect("resolve should succeed");
        assert_eq!(payload.text_prompt, "hello world");
        assert_eq!(payload.user_blocks.len(), 1);
        assert!(matches!(payload.prompt_payload, PromptPayload::Text(_)));

        let serialized =
            serde_json::to_value(&payload.user_blocks[0]).expect("serialize content block");
        assert_eq!(
            serialized.get("type").and_then(|v| v.as_str()),
            Some("text")
        );
    }

    #[test]
    fn resolve_prompt_payload_supports_multiple_block_types() {
        let input = UserPromptInput {
            prompt: None,
            prompt_blocks: Some(vec![
                json!({ "type": "text", "text": "请分析 @src/main.ts" }),
                json!({ "type": "resource_link", "uri": "file:///workspace/src/main.ts", "name": "src/main.ts" }),
                json!({ "type": "image", "mimeType": "image/png", "data": "AAAA" }),
            ]),
            working_dir: None,
            env: std::collections::HashMap::new(),
            executor_config: None,
        };

        let payload = input
            .resolve_prompt_payload()
            .expect("resolve should succeed");
        assert_eq!(payload.user_blocks.len(), 3);
        assert!(matches!(payload.prompt_payload, PromptPayload::Blocks(_)));
        assert!(payload.text_prompt.contains("请分析 @src/main.ts"));
        assert!(
            payload
                .text_prompt
                .contains("[引用文件: src/main.ts (file:///workspace/src/main.ts)]")
        );
        assert!(
            payload
                .text_prompt
                .contains("[引用图片: mimeType=image/png")
        );
    }

    #[test]
    fn build_user_notifications_preserves_block_types_and_index() {
        let blocks = vec![
            serde_json::from_value::<ContentBlock>(json!({
                "type": "text",
                "text": "hello"
            }))
            .expect("text block"),
            serde_json::from_value::<ContentBlock>(json!({
                "type": "resource_link",
                "uri": "file:///workspace/src/main.ts",
                "name": "src/main.ts"
            }))
            .expect("resource_link block"),
        ];

        let mut source = AgentDashSourceV1::new("unit-test", "local_executor");
        source.executor_id = Some("CLAUDE_CODE".to_string());

        let notifications = build_user_message_notifications("sess-test", &source, "t100", &blocks);
        assert_eq!(notifications.len(), 2);

        let first = serde_json::to_value(&notifications[0]).expect("serialize first");
        let second = serde_json::to_value(&notifications[1]).expect("serialize second");

        assert_eq!(
            first
                .get("update")
                .and_then(|u| u.get("content"))
                .and_then(|c| c.get("type"))
                .and_then(|v| v.as_str()),
            Some("text")
        );
        assert_eq!(
            second
                .get("update")
                .and_then(|u| u.get("content"))
                .and_then(|c| c.get("type"))
                .and_then(|v| v.as_str()),
            Some("resource_link")
        );
        assert_eq!(
            second
                .get("update")
                .and_then(|u| u.get("_meta"))
                .and_then(|m| m.get("agentdash"))
                .and_then(|m| m.get("trace"))
                .and_then(|t| t.get("entryIndex"))
                .and_then(|v| v.as_u64()),
            Some(1)
        );
    }

    #[tokio::test]
    async fn start_prompt_triggers_session_start_before_connector_prompt() {
        #[derive(Default)]
        struct SessionStartAwareConnector {
            session_start_seen: Arc<TokioMutex<Vec<bool>>>,
        }

        #[async_trait::async_trait]
        impl AgentConnector for SessionStartAwareConnector {
            fn connector_id(&self) -> &'static str {
                "session-start-aware"
            }
            fn connector_type(&self) -> agentdash_spi::ConnectorType {
                agentdash_spi::ConnectorType::LocalExecutor
            }
            fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
                agentdash_spi::ConnectorCapabilities::default()
            }
            fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
                Vec::new()
            }
            async fn discover_options_stream(
                &self,
                _executor: &str,
                _variant: Option<&str>,
                _working_dir: Option<PathBuf>,
            ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
            {
                Ok(Box::pin(stream::empty()))
            }

            async fn prompt(
                &self,
                _session_id: &str,
                _follow_up_session_id: Option<&str>,
                _prompt: &PromptPayload,
                context: agentdash_spi::ExecutionContext,
            ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
                let seen = context.hook_session.as_ref().is_some_and(|runtime| {
                    runtime
                        .trace()
                        .iter()
                        .any(|trace| matches!(&trace.trigger, HookTrigger::SessionStart))
                });
                self.session_start_seen.lock().await.push(seen);
                Ok(Box::pin(stream::empty()))
            }

            async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn approve_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn reject_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
                _reason: Option<String>,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
        }

        struct RecordingHookProvider {
            queries: Arc<TokioMutex<Vec<HookEvaluationQuery>>>,
        }

        #[async_trait::async_trait]
        impl agentdash_spi::hooks::ExecutionHookProvider for RecordingHookProvider {
            async fn load_session_snapshot(
                &self,
                query: SessionHookSnapshotQuery,
            ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
                Ok(SessionHookSnapshot {
                    session_id: query.session_id,
                    ..SessionHookSnapshot::default()
                })
            }
            async fn refresh_session_snapshot(
                &self,
                query: SessionHookRefreshQuery,
            ) -> Result<SessionHookSnapshot, agentdash_spi::hooks::HookError> {
                Ok(SessionHookSnapshot {
                    session_id: query.session_id,
                    ..SessionHookSnapshot::default()
                })
            }
            async fn evaluate_hook(
                &self,
                query: HookEvaluationQuery,
            ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
                self.queries.lock().await.push(query);
                Ok(HookResolution::default())
            }
        }

        let base = tempfile::tempdir().expect("tempdir");
        let connector = Arc::new(SessionStartAwareConnector::default());
        let queries = Arc::new(TokioMutex::new(Vec::new()));
        let hook_provider = Arc::new(RecordingHookProvider {
            queries: queries.clone(),
        });
        let hub = test_hub(
            base.path().to_path_buf(),
            connector.clone(),
            Some(hook_provider),
        );
        let session = hub.create_session("test").await.expect("create session");

        hub.start_prompt(&session.id, simple_prompt_request("hello"))
            .await
            .expect("prompt should start");

        let seen = connector.session_start_seen.lock().await;
        assert_eq!(seen.as_slice(), &[true]);

        let queries = queries.lock().await;
        assert!(
            queries
                .iter()
                .any(|query| matches!(query.trigger, HookTrigger::SessionStart))
        );
    }

    #[tokio::test]
    async fn start_prompt_uses_request_workspace_root_override() {
        #[derive(Default)]
        struct RecordingConnector {
            contexts: Arc<TokioMutex<Vec<agentdash_spi::ExecutionContext>>>,
        }

        #[async_trait::async_trait]
        impl AgentConnector for RecordingConnector {
            fn connector_id(&self) -> &'static str {
                "recording"
            }
            fn connector_type(&self) -> agentdash_spi::ConnectorType {
                agentdash_spi::ConnectorType::LocalExecutor
            }
            fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
                agentdash_spi::ConnectorCapabilities::default()
            }
            fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
                Vec::new()
            }
            async fn discover_options_stream(
                &self,
                _executor: &str,
                _variant: Option<&str>,
                _working_dir: Option<PathBuf>,
            ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
            {
                Ok(Box::pin(stream::empty()))
            }
            async fn prompt(
                &self,
                _session_id: &str,
                _follow_up_session_id: Option<&str>,
                _prompt: &PromptPayload,
                context: agentdash_spi::ExecutionContext,
            ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
                self.contexts.lock().await.push(context);
                Ok(Box::pin(stream::empty()))
            }
            async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn approve_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn reject_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
                _reason: Option<String>,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
        }

        let base = tempfile::tempdir().expect("tempdir");
        let workspace = tempfile::tempdir().expect("workspace");
        let connector = Arc::new(RecordingConnector::default());
        let hub = test_hub(base.path().to_path_buf(), connector.clone(), None);
        let session = hub.create_session("test").await.expect("create session");

        hub.start_prompt(
            &session.id,
            PromptSessionRequest {
                user_input: UserPromptInput {
                    prompt: Some("hello".to_string()),
                    prompt_blocks: None,
                    working_dir: Some("src".to_string()),
                    env: HashMap::new(),
                    executor_config: None,
                },
                mcp_servers: vec![],
                workspace_root: Some(workspace.path().to_path_buf()),
                address_space: None,
                flow_capabilities: None,
                system_context: None,
                identity: None,
            },
        )
        .await
        .expect("prompt should start");

        let contexts = connector.contexts.lock().await;
        let context = contexts.last().expect("context should be recorded");
        assert_eq!(context.workspace_root, workspace.path().to_path_buf());
        assert_eq!(context.working_directory, workspace.path().join("src"));
    }

    #[tokio::test]
    async fn start_prompt_reuses_existing_session_executor_config() {
        #[derive(Default)]
        struct RecordingConnector {
            contexts: Arc<TokioMutex<Vec<agentdash_spi::ExecutionContext>>>,
        }

        #[async_trait::async_trait]
        impl AgentConnector for RecordingConnector {
            fn connector_id(&self) -> &'static str {
                "recording"
            }
            fn connector_type(&self) -> agentdash_spi::ConnectorType {
                agentdash_spi::ConnectorType::LocalExecutor
            }
            fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
                agentdash_spi::ConnectorCapabilities::default()
            }
            fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
                vec![agentdash_spi::AgentInfo {
                    id: "PI_AGENT".to_string(),
                    name: "PI Agent".to_string(),
                    variants: Vec::new(),
                    available: true,
                }]
            }
            async fn discover_options_stream(
                &self,
                _executor: &str,
                _variant: Option<&str>,
                _working_dir: Option<PathBuf>,
            ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
            {
                Ok(Box::pin(stream::empty()))
            }
            async fn prompt(
                &self,
                _session_id: &str,
                _follow_up_session_id: Option<&str>,
                _prompt: &PromptPayload,
                context: agentdash_spi::ExecutionContext,
            ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
                self.contexts.lock().await.push(context);
                Ok(Box::pin(stream::empty()))
            }
            async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn approve_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn reject_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
                _reason: Option<String>,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
        }

        let base = tempfile::tempdir().expect("tempdir");
        let connector = Arc::new(RecordingConnector::default());
        let hub = test_hub(base.path().to_path_buf(), connector.clone(), None);

        let session = hub
            .create_session("reuse existing executor")
            .await
            .expect("create session");
        hub.update_session_meta(&session.id, |meta| {
            meta.executor_config = Some(agentdash_spi::AgentConfig::new("PI_AGENT"));
        })
        .await
        .expect("update meta should succeed");

        hub.start_prompt(&session.id, simple_prompt_request("hello"))
            .await
            .expect("prompt should start");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let contexts = connector.contexts.lock().await;
        let context = contexts.last().expect("context should be recorded");
        assert_eq!(context.executor_config.executor, "PI_AGENT");
    }

    #[tokio::test]
    async fn start_prompt_records_failed_terminal_when_connector_setup_fails() {
        struct FailingConnector;

        #[async_trait::async_trait]
        impl AgentConnector for FailingConnector {
            fn connector_id(&self) -> &'static str {
                "failing"
            }
            fn connector_type(&self) -> agentdash_spi::ConnectorType {
                agentdash_spi::ConnectorType::LocalExecutor
            }
            fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
                agentdash_spi::ConnectorCapabilities::default()
            }
            fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
                Vec::new()
            }
            async fn discover_options_stream(
                &self,
                _executor: &str,
                _variant: Option<&str>,
                _working_dir: Option<PathBuf>,
            ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
            {
                Ok(Box::pin(stream::empty()))
            }
            async fn prompt(
                &self,
                _session_id: &str,
                _follow_up_session_id: Option<&str>,
                _prompt: &PromptPayload,
                _context: agentdash_spi::ExecutionContext,
            ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
                Err(ConnectorError::Runtime(
                    "connector setup failed".to_string(),
                ))
            }
            async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn approve_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn reject_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
                _reason: Option<String>,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
        }

        let base = tempfile::tempdir().expect("tempdir");
        let hub = test_hub(base.path().to_path_buf(), Arc::new(FailingConnector), None);
        let session = hub.create_session("test").await.expect("create session");

        let error = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            hub.start_prompt(&session.id, simple_prompt_request("hello")),
        )
        .await
        .expect("prompt should not hang")
        .expect_err("prompt should fail");
        assert!(error.to_string().contains("connector setup failed"));

        let history = hub
            .persistence
            .list_all_events(&session.id)
            .await
            .expect("history should load");
        let terminal = history
            .iter()
            .filter_map(|event| match &event.notification.update {
                SessionUpdate::SessionInfoUpdate(info) => {
                    parse_turn_terminal_event(info.meta.as_ref())
                }
                _ => None,
            })
            .last()
            .expect("terminal event should exist");
        assert_eq!(terminal.1, TurnTerminalKind::Failed);
        assert_eq!(
            terminal.2.as_deref(),
            Some("执行器运行错误: connector setup failed")
        );
    }

    #[tokio::test]
    async fn cancel_marks_running_turn_interrupted() {
        #[derive(Default)]
        struct CancelAwareConnector {
            streams: Arc<
                TokioMutex<
                    HashMap<String, mpsc::Sender<Result<SessionNotification, ConnectorError>>>,
                >,
            >,
        }

        #[async_trait::async_trait]
        impl AgentConnector for CancelAwareConnector {
            fn connector_id(&self) -> &'static str {
                "cancel-aware"
            }
            fn connector_type(&self) -> agentdash_spi::ConnectorType {
                agentdash_spi::ConnectorType::LocalExecutor
            }
            fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
                agentdash_spi::ConnectorCapabilities::default()
            }
            fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
                Vec::new()
            }
            async fn discover_options_stream(
                &self,
                _executor: &str,
                _variant: Option<&str>,
                _working_dir: Option<PathBuf>,
            ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
            {
                Ok(Box::pin(stream::empty()))
            }
            async fn prompt(
                &self,
                session_id: &str,
                _follow_up_session_id: Option<&str>,
                _prompt: &PromptPayload,
                _context: agentdash_spi::ExecutionContext,
            ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
                let (tx, rx) = mpsc::channel(4);
                self.streams.lock().await.insert(session_id.to_string(), tx);
                Ok(Box::pin(ReceiverStream::new(rx)))
            }
            async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
                self.streams.lock().await.remove(session_id);
                Ok(())
            }
            async fn approve_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
            async fn reject_tool_call(
                &self,
                _session_id: &str,
                _tool_call_id: &str,
                _reason: Option<String>,
            ) -> Result<(), ConnectorError> {
                Ok(())
            }
        }

        let base = tempfile::tempdir().expect("tempdir");
        let connector = Arc::new(CancelAwareConnector::default());
        let hub = test_hub(base.path().to_path_buf(), connector, None);
        let session = hub.create_session("test").await.expect("create session");

        let turn_id = hub
            .start_prompt(&session.id, simple_prompt_request("hello"))
            .await
            .expect("prompt should start");
        hub.cancel(&session.id)
            .await
            .expect("cancel should succeed");
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;

        let state = hub
            .inspect_session_execution_state(&session.id)
            .await
            .expect("state should load");
        assert_eq!(
            state,
            SessionExecutionState::Interrupted {
                turn_id: Some(turn_id.clone()),
                message: Some("执行已取消".to_string())
            }
        );

        let history = hub
            .persistence
            .list_all_events(&session.id)
            .await
            .expect("history should load");
        let terminal = history
            .iter()
            .filter_map(|event| match &event.notification.update {
                SessionUpdate::SessionInfoUpdate(info) => {
                    parse_turn_terminal_event(info.meta.as_ref())
                }
                _ => None,
            })
            .last()
            .expect("terminal event should exist");
        assert_eq!(terminal.0, turn_id);
        assert_eq!(terminal.1, TurnTerminalKind::Interrupted);
        assert_eq!(terminal.2.as_deref(), Some("执行已取消"));
    }
}
