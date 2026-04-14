use std::{collections::HashMap, io, path::PathBuf, sync::Arc};

use agent_client_protocol::{SessionNotification, SessionUpdate};
use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_agent_types::AgentMessage;
use tokio::sync::{Mutex, broadcast};

use super::companion_wait::CompanionWaitRegistry;
use super::continuation::{
    build_companion_human_response_notification, build_continuation_system_context_from_events,
    build_restored_session_messages_from_events,
};
use super::hook_messages as msg;
use super::hook_runtime::HookSessionRuntime;
use super::hub_support::*;
use super::persistence::SessionPersistence;
pub use super::types::*;
use agentdash_spi::hooks::{
    ExecutionHookProvider, SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
use agentdash_spi::{AddressSpace, AgentConnector, ConnectorError};

#[derive(Clone)]
pub struct SessionHub {
    /// 当 `PromptSessionRequest.address_space` 为 None 时回退使用（如云宿主 cwd、本机首个 accessible root）。
    pub(super) default_address_space: Option<AddressSpace>,
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
    pub(super) sessions: Arc<Mutex<HashMap<String, SessionRuntime>>>,
    pub(super) persistence: Arc<dyn SessionPersistence>,
    pub(crate) address_space_service: Option<Arc<crate::address_space::RelayAddressSpaceService>>,
    pub(super) extra_skill_dirs: Vec<PathBuf>,
    pub companion_wait_registry: CompanionWaitRegistry,
    pub(super) title_generator: Option<Arc<dyn super::title_generator::SessionTitleGenerator>>,
}

impl SessionHub {
    pub fn new_with_hooks_and_persistence(
        default_address_space: Option<AddressSpace>,
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
        persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self {
            default_address_space,
            connector,
            hook_provider,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            persistence,
            address_space_service: None,
            extra_skill_dirs: Vec::new(),
            companion_wait_registry: CompanionWaitRegistry::default(),
            title_generator: None,
        }
    }

    /// 注入 Address Space 访问服务（用于 skill 扫描等需要跨 mount 读取的场景）
    pub fn with_address_space_service(
        mut self,
        service: Arc<crate::address_space::RelayAddressSpaceService>,
    ) -> Self {
        self.address_space_service = Some(service);
        self
    }

    /// 注入插件提供的额外 Skill 扫描目录
    pub fn with_extra_skill_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.extra_skill_dirs = dirs;
        self
    }

    /// 注入会话标题自动生成器（可选；未注入时不触发自动标题生成）
    pub fn with_title_generator(
        mut self,
        generator: Arc<dyn super::title_generator::SessionTitleGenerator>,
    ) -> Self {
        self.title_generator = Some(generator);
        self
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

    pub async fn create_session(&self, title: &str) -> std::io::Result<SessionMeta> {
        self.create_session_with_title_source(title, super::types::TitleSource::Auto)
            .await
    }

    /// 创建会话并显式指定标题来源。
    /// Task 绑定的会话应使用 `TitleSource::User` 以阻止自动覆盖。
    pub async fn create_session_with_title_source(
        &self,
        title: &str,
        title_source: super::types::TitleSource,
    ) -> std::io::Result<SessionMeta> {
        let id = format!(
            "sess-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let now = chrono::Utc::now().timestamp_millis();
        let meta = SessionMeta {
            id: id.clone(),
            title: title.to_string(),
            title_source,
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
            bootstrap_state: SessionBootstrapState::Plain,
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
    ) -> std::io::Result<std::collections::HashMap<String, SessionExecutionState>> {
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
                let meta = self
                    .persistence
                    .get_session_meta(id)
                    .await?
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, format!("session {id} 不存在"))
                    })?;
                let status = meta_to_execution_state(&meta, id)?;
                result.insert(id.clone(), status);
            }
        }
        Ok(result)
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
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            ));
        };

        meta_to_execution_state(&meta, session_id)
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

    pub async fn has_live_runtime(&self, session_id: &str) -> bool {
        self.connector.has_live_session(session_id).await
    }

    pub async fn mark_owner_bootstrap_pending(&self, session_id: &str) -> std::io::Result<()> {
        let _ = self
            .update_session_meta(session_id, |meta| {
                meta.bootstrap_state = SessionBootstrapState::Pending;
            })
            .await?;
        Ok(())
    }

    pub async fn build_continuation_system_context(
        &self,
        session_id: &str,
        owner_context: Option<&str>,
    ) -> std::io::Result<Option<String>> {
        let events = self.persistence.list_all_events(session_id).await?;
        Ok(build_continuation_system_context_from_events(
            owner_context,
            &events,
        ))
    }

    pub async fn build_restored_session_messages(
        &self,
        session_id: &str,
    ) -> std::io::Result<Vec<AgentMessage>> {
        let events = self.persistence.list_all_events(session_id).await?;
        Ok(build_restored_session_messages_from_events(&events))
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

    /// 向指定 session 主动注入补充通知（bridge 事件 / companion / canvas 等）。
    /// 直接 persist + broadcast，不经过 turn processor。
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
            runtime.last_activity_at = chrono::Utc::now().timestamp_millis();
            runtime.tx.clone()
        };
        let _ = tx.send(persisted.clone());
        Ok(persisted)
    }

    /// 查找所有超过指定超时时间无活动的 running session，返回其 session_id 列表。
    pub async fn find_stalled_sessions(&self, stall_timeout_ms: u64) -> Vec<String> {
        let now = chrono::Utc::now().timestamp_millis();
        let threshold = stall_timeout_ms as i64;
        let sessions = self.sessions.lock().await;
        sessions
            .iter()
            .filter(|(_, runtime)| runtime.running && (now - runtime.last_activity_at) > threshold)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Hook auto-resume: schedule a delayed follow-up prompt in a separate task.
    /// Uses fire-and-forget to avoid awaiting `start_prompt` directly inside
    /// the stream-processing spawn block (whose Future is not Send).
    pub(super) fn schedule_hook_auto_resume(&self, session_id: String) {
        let hub = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let resume_req = PromptSessionRequest::from_user_input(UserPromptInput::from_text(
                msg::AUTO_RESUME_PROMPT,
            ));
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

    /// 人通过 API 回应 companion 请求。
    /// 若命中 wait registry，则恢复挂起的工具调用；
    /// 无论是否命中，都把回应写入 session 事件流，保证历史可回放。
    pub async fn respond_companion_request(
        &self,
        session_id: &str,
        request_id: &str,
        payload: serde_json::Value,
    ) -> Result<(), ConnectorError> {
        let resolved = self
            .companion_wait_registry
            .resolve(session_id, request_id, payload.clone())
            .await;

        let fallback_turn_id = self
            .persistence
            .get_session_meta(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?
            .and_then(|meta| meta.last_turn_id);
        let turn_id = resolved
            .as_ref()
            .map(|result| result.turn_id.as_str())
            .or_else(|| fallback_turn_id.as_deref());

        let request_type = resolved
            .as_ref()
            .and_then(|result| result.request_type.as_deref());

        let notification = build_companion_human_response_notification(
            session_id,
            turn_id,
            request_id,
            &payload,
            request_type,
            resolved.is_some(),
        );
        let _ = self.inject_notification(session_id, notification).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MemorySessionPersistence;
    use crate::session::local_workspace_address_space;
    use agent_client_protocol::{
        ContentBlock, ContentChunk, SessionId, SessionInfoUpdate, SessionNotification,
        SessionUpdate, TextContent, ToolCall, ToolCallId, ToolCallStatus, ToolCallUpdate,
        ToolCallUpdateFields,
    };
    use agentdash_acp_meta::{
        AgentDashEventV1, AgentDashMetaV1, AgentDashTraceV1, merge_agentdash_meta,
    };
    use agentdash_spi::PromptPayload;
    use agentdash_spi::StopReason;
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
        mount_root: PathBuf,
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn agentdash_spi::hooks::ExecutionHookProvider>>,
    ) -> SessionHub {
        SessionHub::new_with_hooks_and_persistence(
            Some(local_workspace_address_space(&mount_root)),
            connector,
            hook_provider,
            Arc::new(MemorySessionPersistence::default()),
        )
    }

    fn simple_prompt_request(prompt: &str) -> PromptSessionRequest {
        PromptSessionRequest {
            user_input: UserPromptInput {
                executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
                ..UserPromptInput::from_text(prompt)
            },
            mcp_servers: vec![],
            relay_mcp_server_names: Default::default(),
            address_space: None,
            flow_capabilities: None,
            system_context: None,
            bootstrap_action: SessionBootstrapAction::None,
            identity: None,
            post_turn_handler: None,
        }
    }

    fn owner_bootstrap_request(prompt: &str, system_context: &str) -> PromptSessionRequest {
        let mut req = simple_prompt_request(prompt);
        req.system_context = Some(system_context.to_string());
        req.bootstrap_action = SessionBootstrapAction::OwnerContext;
        req
    }

    fn test_meta(
        source: &AgentDashSourceV1,
        turn_id: &str,
        entry_index: u32,
    ) -> agent_client_protocol::Meta {
        let mut trace = AgentDashTraceV1::new();
        trace.turn_id = Some(turn_id.to_string());
        trace.entry_index = Some(entry_index);

        merge_agentdash_meta(
            None,
            &AgentDashMetaV1::new()
                .source(Some(source.clone()))
                .trace(Some(trace)),
        )
        .expect("test meta should build")
    }

    fn test_event_meta(
        source: &AgentDashSourceV1,
        turn_id: &str,
        entry_index: u32,
        event_type: &str,
        data: serde_json::Value,
    ) -> agent_client_protocol::Meta {
        let mut trace = AgentDashTraceV1::new();
        trace.turn_id = Some(turn_id.to_string());
        trace.entry_index = Some(entry_index);

        let mut event = AgentDashEventV1::new(event_type);
        event.severity = Some("info".to_string());
        event.data = Some(data);

        merge_agentdash_meta(
            None,
            &AgentDashMetaV1::new()
                .source(Some(source.clone()))
                .trace(Some(trace))
                .event(Some(event)),
        )
        .expect("test event meta should build")
    }

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

    #[derive(Default)]
    struct RepositoryRestoreRecordingConnector {
        contexts: Arc<TokioMutex<Vec<agentdash_spi::ExecutionContext>>>,
    }

    #[async_trait::async_trait]
    impl AgentConnector for RepositoryRestoreRecordingConnector {
        fn connector_id(&self) -> &'static str {
            "repository-restore-recording"
        }
        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }
        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }
        fn supports_repository_restore(&self, executor: &str) -> bool {
            executor == "PI_AGENT"
        }
        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            vec![agentdash_spi::AgentInfo {
                id: "PI_AGENT".to_string(),
                name: "Pi Agent".to_string(),
                variants: Vec::new(),
                available: true,
            }]
        }
        async fn discover_options_stream(
            &self,
            _executor: &str,
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

    #[test]
    fn resolve_prompt_payload_from_text_block() {
        let input = UserPromptInput::from_text("  hello world  ");

        let payload = input
            .resolve_prompt_payload()
            .expect("resolve should succeed");
        assert_eq!(payload.text_prompt, "hello world");
        assert_eq!(payload.user_blocks.len(), 1);
        assert!(matches!(payload.prompt_payload, PromptPayload::Blocks(_)));

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
    async fn respond_companion_request_resolves_waiting_tool_and_persists_response_event() {
        struct NoopConnector;

        #[async_trait::async_trait]
        impl AgentConnector for NoopConnector {
            fn connector_id(&self) -> &'static str {
                "noop"
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
        let hub = test_hub(base.path().to_path_buf(), Arc::new(NoopConnector), None);
        let session = hub.create_session("test").await.expect("create session");
        let payload = json!({
            "type": "decision",
            "status": "approved",
            "choice": "YES",
            "summary": "YES"
        });

        let rx = hub
            .companion_wait_registry
            .register(&session.id, "req-1", "turn-1", Some("approval".to_string()))
            .await;

        hub.respond_companion_request(&session.id, "req-1", payload.clone())
            .await
            .expect("respond should succeed");

        assert_eq!(rx.await.expect("wait registry should resolve"), payload);

        let events = hub
            .persistence
            .list_all_events(&session.id)
            .await
            .expect("events should load");
        let response = events
            .iter()
            .find(|event| {
                let event_type = serde_json::to_value(&event.notification)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("update")
                            .and_then(|update| update.get("_meta"))
                            .and_then(|meta| meta.get("agentdash"))
                            .and_then(|agentdash| agentdash.get("event"))
                            .and_then(|event| event.get("type"))
                            .and_then(|value| value.as_str().map(ToString::to_string))
                    });
                event_type.as_deref() == Some("companion_human_response")
            })
            .expect("response event should exist");

        assert_eq!(response.turn_id.as_deref(), Some("turn-1"));

        let notification = serde_json::to_value(&response.notification).expect("serialize");
        let event_data = notification
            .get("update")
            .and_then(|update| update.get("_meta"))
            .and_then(|meta| meta.get("agentdash"))
            .and_then(|agentdash| agentdash.get("event"))
            .and_then(|event| event.get("data"))
            .expect("response event data");
        assert_eq!(
            event_data
                .get("request_id")
                .and_then(|value| value.as_str()),
            Some("req-1")
        );
        assert_eq!(
            event_data
                .get("resumed_waiting_tool")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            event_data
                .get("request_type")
                .and_then(|value| value.as_str()),
            Some("approval")
        );
    }

    #[tokio::test]
    async fn start_prompt_triggers_session_start_before_connector_prompt() {
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
        hub.mark_owner_bootstrap_pending(&session.id)
            .await
            .expect("should mark pending");

        hub.start_prompt(&session.id, owner_bootstrap_request("hello", "ctx"))
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
    async fn owner_bootstrap_marks_session_meta_bootstrapped() {
        let base = tempfile::tempdir().expect("tempdir");
        let connector = Arc::new(SessionStartAwareConnector::default());
        let queries = Arc::new(TokioMutex::new(Vec::new()));
        let hook_provider = Arc::new(RecordingHookProvider {
            queries: queries.clone(),
        });
        let hub = test_hub(base.path().to_path_buf(), connector, Some(hook_provider));
        let session = hub.create_session("test").await.expect("create session");
        hub.mark_owner_bootstrap_pending(&session.id)
            .await
            .expect("should mark pending");

        hub.start_prompt(&session.id, owner_bootstrap_request("hello", "ctx"))
            .await
            .expect("prompt should start");

        let meta = hub
            .get_session_meta(&session.id)
            .await
            .expect("meta should load")
            .expect("session should exist");
        assert_eq!(meta.bootstrap_state, SessionBootstrapState::Bootstrapped);
    }

    #[tokio::test]
    async fn build_continuation_system_context_strips_owner_resource_blocks() {
        let persistence = Arc::new(MemorySessionPersistence::default());
        let base = tempfile::tempdir().expect("tempdir");
        let hub = SessionHub::new_with_hooks_and_persistence(
            Some(local_workspace_address_space(&base.path().to_path_buf())),
            Arc::new(SessionStartAwareConnector::default()),
            None,
            persistence,
        );
        let session = hub.create_session("test").await.expect("create session");

        let source = AgentDashSourceV1::new("test", "unit");
        let user_blocks = vec![
            serde_json::from_value::<ContentBlock>(serde_json::json!({
                "type": "resource",
                "resource": {
                    "uri": "agentdash://project-context/project-1",
                    "mimeType": "text/markdown",
                    "text": "## Project\nhidden"
                }
            }))
            .expect("resource block"),
            ContentBlock::Text(TextContent::new("继续分析 session 生命周期")),
        ];
        for notification in
            build_user_message_notifications(&session.id, &source, "t-1", &user_blocks)
        {
            hub.inject_notification(&session.id, notification)
                .await
                .expect("inject user notification");
        }

        let assistant_chunk = ContentChunk::new(ContentBlock::Text(TextContent::new("已记录历史")))
            .message_id(Some("assistant-msg-1".to_string()))
            .meta(
                merge_agentdash_meta(
                    None,
                    &AgentDashMetaV1::new()
                        .source(Some(source.clone()))
                        .trace(Some({
                            let mut trace = AgentDashTraceV1::new();
                            trace.turn_id = Some("t-1".to_string());
                            trace.entry_index = Some(99);
                            trace
                        })),
                )
                .expect("assistant meta"),
            );
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::AgentMessageChunk(assistant_chunk),
            ),
        )
        .await
        .expect("inject assistant notification");

        let context = hub
            .build_continuation_system_context(&session.id, Some("## Owner\nproject"))
            .await
            .expect("context should build")
            .expect("continuation context should exist");
        assert!(context.contains("继续分析 session 生命周期"));
        assert!(context.contains("已记录历史"));
        assert!(context.contains("## Owner"));
        assert!(!context.contains("agentdash://project-context/"));
        assert!(!context.contains("hidden"));
    }

    #[tokio::test]
    async fn build_restored_session_messages_reconstructs_tool_history_without_owner_blocks() {
        let persistence = Arc::new(MemorySessionPersistence::default());
        let base = tempfile::tempdir().expect("tempdir");
        let hub = SessionHub::new_with_hooks_and_persistence(
            Some(local_workspace_address_space(&base.path().to_path_buf())),
            Arc::new(SessionStartAwareConnector::default()),
            None,
            persistence,
        );
        let session = hub.create_session("test").await.expect("create session");

        let source = AgentDashSourceV1::new("test", "unit");
        let user_blocks = vec![
            serde_json::from_value::<ContentBlock>(serde_json::json!({
                "type": "resource",
                "resource": {
                    "uri": "agentdash://project-context/project-1",
                    "mimeType": "text/markdown",
                    "text": "## Project\nhidden"
                }
            }))
            .expect("resource block"),
            ContentBlock::Text(TextContent::new("继续分析 session 生命周期")),
        ];
        for notification in
            build_user_message_notifications(&session.id, &source, "t-1", &user_blocks)
        {
            hub.inject_notification(&session.id, notification)
                .await
                .expect("inject user notification");
        }

        let assistant_chunk = ContentChunk::new(ContentBlock::Text(TextContent::new("已记录历史")))
            .message_id(Some("assistant-msg-1".to_string()))
            .meta(Some(test_meta(&source, "t-1", 1)));
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::AgentMessageChunk(assistant_chunk),
            ),
        )
        .await
        .expect("inject assistant notification");

        let tool_call = ToolCall::new(ToolCallId::new("tool-1"), "shell_exec")
            .status(ToolCallStatus::Pending)
            .raw_input(serde_json::json!({ "command": "pwd" }))
            .meta(Some(test_meta(&source, "t-1", 1)));
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::ToolCall(tool_call),
            ),
        )
        .await
        .expect("inject tool call");

        let raw_result = serde_json::to_value(agentdash_spi::AgentToolResult {
            content: vec![agentdash_spi::ContentPart::text("workspace root")],
            is_error: false,
            details: Some(serde_json::json!({ "exit_code": 0 })),
        })
        .expect("serialize tool result");
        let mut fields = ToolCallUpdateFields::default();
        fields.title = Some("shell_exec".to_string());
        fields.status = Some(ToolCallStatus::Completed);
        fields.raw_output = Some(raw_result);
        let tool_update = ToolCallUpdate::new(ToolCallId::new("tool-1"), fields)
            .meta(Some(test_meta(&source, "t-1", 1)));
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::ToolCallUpdate(tool_update),
            ),
        )
        .await
        .expect("inject tool update");

        let messages = hub
            .build_restored_session_messages(&session.id)
            .await
            .expect("messages should build");
        assert_eq!(messages.len(), 3);

        match &messages[0] {
            agentdash_spi::AgentMessage::User { content, .. } => {
                assert_eq!(content.len(), 1);
                assert_eq!(messages[0].first_text(), Some("继续分析 session 生命周期"));
                assert_ne!(messages[0].first_text(), Some("## Project\nhidden"));
            }
            other => panic!("unexpected first message: {other:?}"),
        }

        match &messages[1] {
            agentdash_spi::AgentMessage::Assistant {
                tool_calls,
                stop_reason,
                ..
            } => {
                assert_eq!(messages[1].first_text(), Some("已记录历史"));
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "shell_exec");
                assert_eq!(stop_reason.clone(), Some(StopReason::ToolUse));
            }
            other => panic!("unexpected assistant message: {other:?}"),
        }

        match &messages[2] {
            agentdash_spi::AgentMessage::ToolResult {
                tool_call_id,
                tool_name,
                details,
                is_error,
                ..
            } => {
                assert_eq!(tool_call_id, "tool-1");
                assert_eq!(tool_name.as_deref(), Some("shell_exec"));
                assert_eq!(messages[2].first_text(), Some("workspace root"));
                assert_eq!(
                    details
                        .as_ref()
                        .and_then(|value| value.get("exit_code"))
                        .and_then(serde_json::Value::as_i64),
                    Some(0)
                );
                assert!(!*is_error);
            }
            other => panic!("unexpected tool result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn build_restored_session_messages_applies_latest_compaction_checkpoint() {
        let persistence = Arc::new(MemorySessionPersistence::default());
        let base = tempfile::tempdir().expect("tempdir");
        let hub = SessionHub::new_with_hooks_and_persistence(
            Some(local_workspace_address_space(&base.path().to_path_buf())),
            Arc::new(SessionStartAwareConnector::default()),
            None,
            persistence,
        );
        let session = hub.create_session("test").await.expect("create session");
        let source = AgentDashSourceV1::new("test", "unit");

        for (turn_id, entry_index, text) in [
            ("t-1", 0_u32, "历史用户消息 1"),
            ("t-2", 0_u32, "历史用户消息 2"),
            ("t-3", 0_u32, "最近用户消息"),
        ] {
            hub.inject_notification(
                &session.id,
                SessionNotification::new(
                    SessionId::new(session.id.clone()),
                    SessionUpdate::UserMessageChunk(
                        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                            .meta(Some(test_meta(&source, turn_id, entry_index))),
                    ),
                ),
            )
            .await
            .expect("inject user notification");
        }

        let compaction_meta = test_event_meta(
            &source,
            "t-3",
            0,
            "context_compacted",
            serde_json::json!({
                "summary": "## 历史摘要\n- 已完成旧分析",
                "tokens_before": 42000,
                "messages_compacted": 2,
                "newly_compacted_messages": 2,
                "timestamp_ms": 1710000000000_u64,
            }),
        );
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(compaction_meta)),
            ),
        )
        .await
        .expect("inject compaction checkpoint");

        let restored = hub
            .build_restored_session_messages(&session.id)
            .await
            .expect("messages should build");

        assert_eq!(restored.len(), 2);
        match &restored[0] {
            agentdash_spi::AgentMessage::CompactionSummary {
                summary,
                tokens_before,
                messages_compacted,
                ..
            } => {
                assert!(summary.contains("历史摘要"));
                assert_eq!(*tokens_before, 42_000);
                assert_eq!(*messages_compacted, 2);
            }
            other => panic!("unexpected first message: {other:?}"),
        }
        assert_eq!(restored[1].first_text(), Some("最近用户消息"));
    }

    #[tokio::test]
    async fn build_continuation_system_context_uses_compacted_projection() {
        let persistence = Arc::new(MemorySessionPersistence::default());
        let base = tempfile::tempdir().expect("tempdir");
        let hub = SessionHub::new_with_hooks_and_persistence(
            Some(local_workspace_address_space(&base.path().to_path_buf())),
            Arc::new(SessionStartAwareConnector::default()),
            None,
            persistence,
        );
        let session = hub.create_session("test").await.expect("create session");
        let source = AgentDashSourceV1::new("test", "unit");

        for (turn_id, entry_index, text) in [
            ("t-1", 0_u32, "第一段旧历史"),
            ("t-2", 0_u32, "第二段旧历史"),
            ("t-3", 0_u32, "保留的新历史"),
        ] {
            hub.inject_notification(
                &session.id,
                SessionNotification::new(
                    SessionId::new(session.id.clone()),
                    SessionUpdate::UserMessageChunk(
                        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                            .meta(Some(test_meta(&source, turn_id, entry_index))),
                    ),
                ),
            )
            .await
            .expect("inject user notification");
        }

        let compaction_meta = test_event_meta(
            &source,
            "t-3",
            0,
            "context_compacted",
            serde_json::json!({
                "summary": "压缩后的历史摘要",
                "tokens_before": 38000,
                "messages_compacted": 2,
                "newly_compacted_messages": 2,
                "timestamp_ms": 1710000000000_u64,
            }),
        );
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(compaction_meta)),
            ),
        )
        .await
        .expect("inject compaction checkpoint");

        let context = hub
            .build_continuation_system_context(&session.id, None)
            .await
            .expect("context should build")
            .expect("continuation context should exist");

        assert!(context.contains("压缩后的历史摘要"));
        assert!(context.contains("保留的新历史"));
        assert!(!context.contains("第一段旧历史"));
        assert!(!context.contains("第二段旧历史"));
    }

    #[tokio::test]
    async fn start_prompt_passes_restored_session_state_when_connector_supports_repository_restore()
    {
        let base = tempfile::tempdir().expect("tempdir");
        let connector = Arc::new(RepositoryRestoreRecordingConnector::default());
        let hub = test_hub(base.path().to_path_buf(), connector.clone(), None);
        let session = hub.create_session("test").await.expect("create session");

        let source = AgentDashSourceV1::new("test", "unit");
        for notification in build_user_message_notifications(
            &session.id,
            &source,
            "t-1",
            &[ContentBlock::Text(TextContent::new("历史用户消息"))],
        ) {
            hub.inject_notification(&session.id, notification)
                .await
                .expect("inject user notification");
        }
        let assistant_chunk =
            ContentChunk::new(ContentBlock::Text(TextContent::new("历史助手消息")))
                .message_id(Some("assistant-msg-restore".to_string()))
                .meta(Some(test_meta(&source, "t-1", 1)));
        hub.inject_notification(
            &session.id,
            SessionNotification::new(
                SessionId::new(session.id.clone()),
                SessionUpdate::AgentMessageChunk(assistant_chunk),
            ),
        )
        .await
        .expect("inject assistant notification");

        assert!(
            !hub.has_live_runtime(&session.id).await,
            "仅有被动 session 条目时不应视为 live runtime"
        );

        let mut req = simple_prompt_request("新的用户消息");
        req.user_input.executor_config = Some(agentdash_spi::AgentConfig::new("PI_AGENT"));
        hub.start_prompt(&session.id, req)
            .await
            .expect("prompt should start");

        let contexts = connector.contexts.lock().await;
        let context = contexts.last().expect("context should be recorded");
        let restored = context
            .restored_session_state
            .as_ref()
            .expect("restored session state should exist");
        assert_eq!(restored.messages.len(), 2);
        assert_eq!(restored.messages[0].first_text(), Some("历史用户消息"));
        assert_eq!(restored.messages[1].first_text(), Some("历史助手消息"));
        assert!(context.system_context.is_none());
    }

    #[tokio::test]
    async fn start_prompt_uses_request_address_space_override() {
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
                    prompt_blocks: Some(vec![json!({
                        "type": "text",
                        "text": "hello",
                    })]),
                    working_dir: Some("src".to_string()),
                    env: HashMap::new(),
                    executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
                },
                mcp_servers: vec![],
                relay_mcp_server_names: Default::default(),
                address_space: Some(local_workspace_address_space(workspace.path())),
                flow_capabilities: None,
                system_context: None,
                bootstrap_action: SessionBootstrapAction::None,
                identity: None,
                post_turn_handler: None,
            },
        )
        .await
        .expect("prompt should start");

        let contexts = connector.contexts.lock().await;
        let context = contexts.last().expect("context should be recorded");
        let ws_path = agentdash_spi::workspace_path_from_context(context).expect("default mount");
        assert_eq!(ws_path, workspace.path().to_path_buf());
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
        // 等待 adapter task（检测 stream 关闭 → drop processor_tx）
        // 和 processor task（检测 channel 关闭 → 清理 runtime）完成
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

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
