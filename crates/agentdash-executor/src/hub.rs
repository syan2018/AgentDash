use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use agent_client_protocol::{
    ContentBlock, ContentChunk, Meta, SessionId, SessionInfoUpdate, SessionNotification,
    SessionUpdate, TextContent,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
    parse_agentdash_meta,
};

use agent_client_protocol::McpServer;

use crate::connector::ExecutionAddressSpace;
use crate::connector::{AgentConnector, ConnectorError, ExecutionContext, PromptPayload};
use crate::hook_events::build_hook_trace_notification;
use crate::hooks::{
    ExecutionHookProvider, HookSessionRuntime, HookTraceEntry, HookTrigger,
    SessionHookRefreshQuery, SessionHookSnapshotQuery, SharedHookSessionRuntime,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSessionRequest {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub executor_config: Option<crate::connector::AgentDashExecutorConfig>,
    /// ACP per-session MCP Server 列表（不走 serde — 仅由后端代码填充）
    #[serde(skip)]
    pub mcp_servers: Vec<McpServer>,
    /// 可选的工作空间根目录覆盖。用于 relay `workspace_root` 或云端原生 Agent 的 Task 绑定 workspace。
    #[serde(skip)]
    pub workspace_root: Option<PathBuf>,
    /// 可选的会话级 Address Space 视图。
    #[serde(skip)]
    pub address_space: Option<ExecutionAddressSpace>,
    /// 流程工具能力裁剪。由 session plan 层根据 owner type 填充。
    #[serde(skip)]
    pub flow_capabilities: Option<crate::connector::FlowCapabilities>,
    /// 会话级 owner 上下文（project/story 的 markdown 摘要）。
    /// 由 session binding 层在每次 prompt 前填充，注入到 system prompt 头部。
    /// 不出现在用户消息流中，对用户透明。
    #[serde(skip)]
    pub system_context: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPromptPayload {
    pub text_prompt: String,
    pub prompt_payload: PromptPayload,
    pub user_blocks: Vec<ContentBlock>,
}

impl PromptSessionRequest {
    /// 解析出有效的 prompt payload。
    /// - `text_prompt`：当前本地执行器仍使用的文本 prompt（由 block 降级拼接）
    /// - `user_blocks`：注入会话流时保留的原始 ACP ContentBlock
    ///
    /// 优先使用 `prompt_blocks`，若不存在则回退到 `prompt` 字段。
    /// 二者同时存在返回 Err。
    pub fn resolve_prompt_payload(&self) -> Result<ResolvedPromptPayload, String> {
        match (&self.prompt, &self.prompt_blocks) {
            (Some(_), Some(_)) => Err("prompt 与 promptBlocks 不能同时传入".to_string()),
            (None, None) => Err("必须提供 prompt 或 promptBlocks".to_string()),
            (Some(p), None) => {
                let trimmed = p.trim();
                if trimmed.is_empty() {
                    Err("prompt 不能为空".to_string())
                } else {
                    let text_prompt = trimmed.to_string();
                    Ok(ResolvedPromptPayload {
                        text_prompt: text_prompt.clone(),
                        prompt_payload: PromptPayload::Text(text_prompt),
                        user_blocks: vec![ContentBlock::Text(TextContent::new(trimmed))],
                    })
                }
            }
            (None, Some(blocks)) => {
                if blocks.is_empty() {
                    return Err("promptBlocks 不能为空数组".to_string());
                }
                let mut user_blocks = Vec::with_capacity(blocks.len());
                for (index, block) in blocks.iter().enumerate() {
                    let parsed =
                        serde_json::from_value::<ContentBlock>(block.clone()).map_err(|e| {
                            format!("promptBlocks[{index}] 不是有效 ACP ContentBlock: {e}")
                        })?;
                    user_blocks.push(parsed);
                }
                let prompt_payload = PromptPayload::Blocks(user_blocks.clone());
                let text_prompt = prompt_payload.to_fallback_text();
                if text_prompt.trim().is_empty() {
                    Err("promptBlocks 中没有有效内容".to_string())
                } else {
                    Ok(ResolvedPromptPayload {
                        text_prompt,
                        prompt_payload,
                        user_blocks,
                    })
                }
            }
        }
    }
}

fn build_user_message_notifications(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    user_blocks: &[ContentBlock],
) -> Vec<SessionNotification> {
    user_blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let mut trace = AgentDashTraceV1::new();
            trace.turn_id = Some(turn_id.to_string());
            trace.entry_index = Some(index as u32);

            let agentdash = AgentDashMetaV1::new()
                .source(Some(source.clone()))
                .trace(Some(trace));
            let meta = merge_agentdash_meta(None, &agentdash);

            let chunk = ContentChunk::new(block.clone()).meta(meta);
            SessionNotification::new(
                SessionId::new(session_id),
                SessionUpdate::UserMessageChunk(chunk),
            )
        })
        .collect()
}

fn build_turn_lifecycle_notification(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    event_type: &str,
    severity: &str,
    message: Option<String>,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some(severity.to_string());
    event.message = message;

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace))
        .event(Some(event));
    let meta = merge_agentdash_meta(None, &agentdash);

    let info = SessionInfoUpdate::new().meta(meta);
    SessionNotification::new(
        SessionId::new(session_id),
        SessionUpdate::SessionInfoUpdate(info),
    )
}

fn build_turn_terminal_notification(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    terminal_kind: TurnTerminalKind,
    message: Option<String>,
) -> SessionNotification {
    build_turn_lifecycle_notification(
        session_id,
        source,
        turn_id,
        terminal_kind.event_type(),
        terminal_kind.severity(),
        message,
    )
}

fn parse_executor_session_bound(meta: Option<&Meta>, expected_turn_id: &str) -> Option<String> {
    let parsed = parse_agentdash_meta(meta?)?;
    let trace = parsed.trace?;
    let turn_id = trace.turn_id?;
    if turn_id != expected_turn_id {
        return None;
    }

    let event = parsed.event?;
    if event.r#type != "executor_session_bound" {
        return None;
    }

    if let Some(data) = event.data {
        if let Some(session_id) = data
            .get("executor_session_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(session_id.to_string());
        }
    }

    event
        .message
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_turn_id(meta: Option<&Meta>) -> Option<String> {
    parse_agentdash_meta(meta?)
        .and_then(|parsed| parsed.trace.and_then(|trace| trace.turn_id))
        .map(|turn_id| turn_id.trim().to_string())
        .filter(|turn_id| !turn_id.is_empty())
}

fn parse_turn_terminal_event(
    meta: Option<&Meta>,
) -> Option<(String, TurnTerminalKind, Option<String>)> {
    let parsed = parse_agentdash_meta(meta?)?;
    let trace = parsed.trace?;
    let turn_id = trace.turn_id?;
    let event = parsed.event?;

    match event.r#type.as_str() {
        "turn_completed" => Some((turn_id, TurnTerminalKind::Completed, event.message)),
        "turn_failed" => Some((turn_id, TurnTerminalKind::Failed, event.message)),
        "turn_interrupted" => Some((turn_id, TurnTerminalKind::Interrupted, event.message)),
        _ => None,
    }
}

fn build_session_runtime(tx: broadcast::Sender<SessionNotification>) -> SessionRuntime {
    SessionRuntime {
        tx,
        running: false,
        current_turn_id: None,
        cancel_requested: false,
        hook_session: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSessionContext {
    pub dispatch_id: String,
    pub parent_session_id: String,
    pub parent_turn_id: String,
    pub companion_label: String,
    pub slice_mode: String,
    pub adoption_mode: String,
    #[serde(default)]
    pub inherited_fragment_labels: Vec<String>,
    #[serde(default)]
    pub inherited_constraint_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// 最后一次执行的终态：idle | running | completed | failed | interrupted
    /// 由 ExecutorHub 在 turn 结束时写入，是执行状态的持久化 source of truth。
    #[serde(default = "SessionMeta::default_status")]
    pub last_execution_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_config: Option<crate::connector::AgentDashExecutorConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_context: Option<CompanionSessionContext>,
}

impl SessionMeta {
    fn default_status() -> String {
        "idle".to_string()
    }
}

#[derive(Clone)]
pub struct ExecutorHub {
    workspace_root: PathBuf,
    connector: Arc<dyn AgentConnector>,
    hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
    sessions: Arc<Mutex<HashMap<String, SessionRuntime>>>,
    store: SessionStore,
}

struct SessionRuntime {
    tx: broadcast::Sender<SessionNotification>,
    running: bool,
    current_turn_id: Option<String>,
    cancel_requested: bool,
    hook_session: Option<SharedHookSessionRuntime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecutionState {
    Idle,
    Running {
        turn_id: Option<String>,
    },
    Completed {
        turn_id: String,
    },
    Failed {
        turn_id: String,
        message: Option<String>,
    },
    Interrupted {
        turn_id: Option<String>,
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnTerminalKind {
    Completed,
    Failed,
    Interrupted,
}

impl TurnTerminalKind {
    fn event_type(self) -> &'static str {
        match self {
            Self::Completed => "turn_completed",
            Self::Failed => "turn_failed",
            Self::Interrupted => "turn_interrupted",
        }
    }

    fn severity(self) -> &'static str {
        match self {
            Self::Completed => "info",
            Self::Failed => "error",
            Self::Interrupted => "warning",
        }
    }

    fn state_tag(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

impl ExecutorHub {
    pub fn new(workspace_root: PathBuf, connector: Arc<dyn AgentConnector>) -> Self {
        Self::new_with_hooks(workspace_root, connector, None)
    }

    pub fn new_with_hooks(
        workspace_root: PathBuf,
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
    ) -> Self {
        let store = SessionStore::new(workspace_root.join(".agentdash").join("sessions"));
        Self {
            workspace_root,
            connector,
            hook_provider,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            store,
        }
    }

    /// 启动时调用：将上次进程异常退出时残留的 `running` 状态修正为 `interrupted`。
    ///
    /// 进程正常退出时，所有 session 的终态已经由 prompt 流写入 meta。
    /// 若进程被 kill，内存状态丢失，meta 里的 `running` 就是孤儿状态，需要在下次启动时修正。
    pub async fn recover_interrupted_sessions(&self) -> std::io::Result<()> {
        let sessions = self.store.list_sessions().await?;
        for mut meta in sessions {
            if meta.last_execution_status == "running" {
                tracing::warn!(
                    session_id = %meta.id,
                    "启动恢复：session 上次未正常结束，标记为 interrupted"
                );
                meta.last_execution_status = "interrupted".to_string();
                meta.updated_at = chrono::Utc::now().timestamp_millis();
                self.store.write_meta(&meta).await?;
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
            last_execution_status: "idle".to_string(),
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
        };
        self.store.write_meta(&meta).await?;
        Ok(meta)
    }

    pub async fn list_sessions(&self) -> std::io::Result<Vec<SessionMeta>> {
        self.store.list_sessions().await
    }

    pub async fn get_session_meta(&self, session_id: &str) -> std::io::Result<Option<SessionMeta>> {
        self.store.read_meta(session_id).await
    }

    /// 批量获取多个 session 的 meta，并发读取，返回 (session_id → SessionMeta) 映射。
    /// 不存在的 session_id 不出现在结果 map 中。
    pub async fn get_session_metas_bulk(
        &self,
        session_ids: &[String],
    ) -> std::io::Result<std::collections::HashMap<String, SessionMeta>> {
        use futures::future::join_all;

        let futures: Vec<_> = session_ids
            .iter()
            .map(|id| {
                let store = self.store.clone();
                let id = id.clone();
                async move {
                    let meta = store.read_meta(&id).await?;
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
    /// 不扫 JSONL 历史。
    pub async fn inspect_execution_states_bulk(
        &self,
        session_ids: &[String],
    ) -> std::collections::HashMap<String, SessionExecutionState> {
        // 单次 lock 读内存运行状态
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
                // 读 meta 中持久化的终态
                let status = self
                    .store
                    .read_meta(id)
                    .await
                    .ok()
                    .flatten()
                    .map(|meta| match meta.last_execution_status.as_str() {
                        "idle" => SessionExecutionState::Idle,
                        "completed" => SessionExecutionState::Completed {
                            turn_id: String::new(),
                        },
                        "failed" => SessionExecutionState::Failed {
                            turn_id: String::new(),
                            message: None,
                        },
                        "interrupted" => SessionExecutionState::Interrupted {
                            turn_id: None,
                            message: None,
                        },
                        // "running" 在启动恢复时已被修正为 "interrupted"，
                        // 正常运行时由内存 map 已经处理，不应落到这里
                        "running" => {
                            tracing::warn!(session_id = %id, "bulk 查询遇到 running 状态但内存 map 无记录，视为 interrupted");
                            SessionExecutionState::Interrupted { turn_id: None, message: None }
                        }
                        other => unreachable!("last_execution_status 出现了非法值: {other:?}，这是 ExecutorHub 的 bug"),
                    })
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
        let Some(mut meta) = self.store.read_meta(session_id).await? else {
            return Ok(None);
        };
        updater(&mut meta);
        meta.updated_at = chrono::Utc::now().timestamp_millis();
        self.store.write_meta(&meta).await?;
        Ok(Some(meta))
    }

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

        let history = self.store.read_all(session_id).await?;
        let mut latest_turn_id: Option<String> = None;
        let mut terminal_by_turn: HashMap<String, (TurnTerminalKind, Option<String>)> =
            HashMap::new();

        for notification in history {
            match &notification.update {
                SessionUpdate::UserMessageChunk(chunk) => {
                    if let Some(turn_id) = parse_turn_id(chunk.meta.as_ref()) {
                        latest_turn_id = Some(turn_id);
                    }
                }
                SessionUpdate::SessionInfoUpdate(info) => {
                    if let Some((turn_id, success, message)) =
                        parse_turn_terminal_event(info.meta.as_ref())
                    {
                        terminal_by_turn.insert(turn_id, (success, message));
                    }
                }
                _ => {}
            }
        }

        if running {
            return Ok(SessionExecutionState::Running {
                turn_id: live_turn_id.or(latest_turn_id),
            });
        }

        if let Some(turn_id) = latest_turn_id {
            if let Some((terminal_kind, message)) = terminal_by_turn.remove(&turn_id) {
                match terminal_kind {
                    TurnTerminalKind::Completed => {
                        return Ok(SessionExecutionState::Completed { turn_id });
                    }
                    TurnTerminalKind::Failed => {
                        return Ok(SessionExecutionState::Failed { turn_id, message });
                    }
                    TurnTerminalKind::Interrupted => {
                        return Ok(SessionExecutionState::Interrupted {
                            turn_id: Some(turn_id),
                            message,
                        });
                    }
                }
            }
            return Ok(SessionExecutionState::Interrupted {
                turn_id: Some(turn_id),
                message: None,
            });
        }

        Ok(SessionExecutionState::Idle)
    }

    pub async fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(session_id);
        }
        self.store.delete(session_id).await
    }

    pub async fn ensure_session(
        &self,
        session_id: &str,
    ) -> broadcast::Receiver<SessionNotification> {
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

        if self.store.read_meta(session_id).await?.is_none() {
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
    /// 如果上一轮仍在执行中，返回 Err。
    pub async fn start_prompt(
        &self,
        session_id: &str,
        req: PromptSessionRequest,
    ) -> Result<String, ConnectorError> {
        self.start_prompt_with_follow_up(session_id, None, req)
            .await
    }

    /// 多轮对话（支持底层执行器 follow-up 会话续跑）。
    pub async fn start_prompt_with_follow_up(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        req: PromptSessionRequest,
    ) -> Result<String, ConnectorError> {
        let resolved_payload = req
            .resolve_prompt_payload()
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());

        let tx = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
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
        let working_directory = resolve_working_dir(&workspace_root, req.working_dir.as_deref());

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
                // session_id 不存在：调用方没有先调 create_session，这是 API 层的 bug
                return Err(ConnectorError::Runtime(format!(
                    "session {sid} 不存在，请先调用 create_session 再 prompt"
                )));
            }
            Err(e) => {
                // 文件 IO 失败：不能静默创建一个空 meta 继续，会导致状态丢失
                return Err(ConnectorError::Runtime(format!(
                    "读取 session {sid} meta 失败: {e}"
                )));
            }
        };
        let executor_config = req
            .executor_config
            .clone()
            .or_else(|| session_meta.executor_config.clone())
            .unwrap_or_else(crate::connector::AgentDashExecutorConfig::default);

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

        let context = ExecutionContext {
            turn_id: turn_id.clone(),
            workspace_root,
            working_directory,
            environment_variables: req.env,
            executor_config,
            mcp_servers: req.mcp_servers,
            address_space: req.address_space,
            hook_session: hook_session.clone(),
            flow_capabilities: req.flow_capabilities.unwrap_or_default(),
            system_context: req.system_context,
        };

        session_meta.updated_at = now;
        session_meta.last_execution_status = "running".to_string();
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

        // 注入用户消息到流和持久化存储（附带 `_meta.agentdash`）
        let connector_type = match self.connector.connector_type() {
            crate::connector::ConnectorType::LocalExecutor => "local_executor",
            crate::connector::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
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
                // 持久化终态到 meta
                if let Ok(Some(mut meta)) = store.read_meta(&sid).await {
                    meta.last_execution_status = "failed".to_string();
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
                        {
                            if last_executor_session_id.as_deref()
                                != Some(executor_session_id.as_str())
                            {
                                last_executor_session_id = Some(executor_session_id.clone());
                                if let Ok(Some(mut meta)) = store.read_meta(&session_id).await {
                                    if meta.executor_session_id.as_deref()
                                        != Some(executor_session_id.as_str())
                                    {
                                        meta.executor_session_id = Some(executor_session_id);
                                        meta.updated_at = chrono::Utc::now().timestamp_millis();
                                        let _ = store.write_meta(&meta).await;
                                    }
                                }
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
                            runtime.current_turn_id.as_deref() == Some(turn_id_for_spawn.as_str()),
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

            // 持久化终态到 meta — 这是执行状态的 source of truth，后续查询直接读这个字段
            let status_str = terminal_kind.state_tag().to_string();
            if let Ok(Some(mut meta)) = store.read_meta(&session_id).await {
                meta.last_execution_status = status_str;
                meta.updated_at = chrono::Utc::now().timestamp_millis();
                let _ = store.write_meta(&meta).await;
            }

            if let Some(hook_session) = hook_session_for_spawn {
                match hook_session
                    .evaluate(crate::hooks::HookEvaluationQuery {
                        session_id: session_id.clone(),
                        trigger: HookTrigger::SessionTerminal,
                        turn_id: Some(turn_id_for_spawn.clone()),
                        tool_name: None,
                        tool_call_id: None,
                        subagent_type: None,
                        snapshot: Some(hook_session.snapshot()),
                        payload: Some(serde_json::json!({
                            "terminal_state": terminal_kind.state_tag(),
                            "message": terminal_message,
                        })),
                    })
                    .await
                {
                    Ok(resolution) => {
                        if resolution.refresh_snapshot {
                            let _ = hook_session
                                .refresh(SessionHookRefreshQuery {
                                    session_id: session_id.clone(),
                                    turn_id: Some(turn_id_for_spawn.clone()),
                                    reason: Some("trigger:session_terminal".to_string()),
                                })
                                .await;
                        }
                        let trace = HookTraceEntry {
                            sequence: hook_session.next_trace_sequence(),
                            timestamp_ms: chrono::Utc::now().timestamp_millis(),
                            revision: hook_session.revision(),
                            trigger: HookTrigger::SessionTerminal,
                            decision: if resolution
                                .completion
                                .as_ref()
                                .is_some_and(|completion| completion.advanced)
                            {
                                "phase_advanced".to_string()
                            } else {
                                "terminal_observed".to_string()
                            },
                            tool_name: None,
                            tool_call_id: None,
                            subagent_type: None,
                            matched_rule_keys: resolution.matched_rule_keys,
                            refresh_snapshot: resolution.refresh_snapshot,
                            block_reason: resolution.block_reason,
                            completion: resolution.completion,
                            diagnostics: resolution.diagnostics,
                        };
                        hook_session.append_trace(trace.clone());
                        if let Some(notification) = build_hook_trace_notification(
                            &session_id,
                            Some(&turn_id_for_spawn),
                            source.clone(),
                            &trace,
                        ) {
                            let _ = store.append(&session_id, &notification).await;
                            let _ = tx.send(notification);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            session_id = %session_id,
                            error = %error,
                            "session terminal hook 评估失败"
                        );
                    }
                }
            }

            // 执行完成后标记 running = false，允许下一轮
            let mut guard = sessions.lock().await;
            if let Some(runtime) = guard.get_mut(&session_id) {
                runtime.running = false;
                runtime.current_turn_id = None;
                runtime.cancel_requested = false;
            }
        });

        Ok(turn_id)
    }

    pub async fn subscribe_with_history(
        &self,
        session_id: &str,
    ) -> (
        Vec<SessionNotification>,
        broadcast::Receiver<SessionNotification>,
    ) {
        let history = self.store.read_all(session_id).await.unwrap_or_default();
        let rx = self.ensure_session(session_id).await;
        (history, rx)
    }

    /// 向指定 session 主动注入通知：
    /// - 先持久化到会话历史
    /// - 再广播给当前订阅者
    pub async fn inject_notification(
        &self,
        session_id: &str,
        notification: SessionNotification,
    ) -> std::io::Result<()> {
        self.store.append(session_id, &notification).await?;

        let tx = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
                build_session_runtime(tx)
            });
            runtime.tx.clone()
        };

        let _ = tx.send(notification);
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
            .store
            .read_all(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let mut latest_turn_id = current_turn_id;
        let mut terminal_by_turn: HashMap<String, (TurnTerminalKind, Option<String>)> =
            HashMap::new();
        for notification in history {
            match &notification.update {
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
        self.store
            .append(session_id, &interrupted)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let _ = tx.send(interrupted);
        Ok(())
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

impl ExecutorHub {
    async fn load_session_hook_runtime(
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

#[derive(Clone)]
struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn jsonl_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{session_id}.jsonl"))
    }

    fn meta_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{session_id}.meta.json"))
    }

    async fn write_meta(&self, meta: &SessionMeta) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.meta_path(&meta.id);
        let json = serde_json::to_string_pretty(meta).unwrap_or_else(|_| "{}".into());
        tokio::fs::write(path, json).await
    }

    async fn read_meta(&self, session_id: &str) -> std::io::Result<Option<SessionMeta>> {
        let path = self.meta_path(session_id);
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let meta = serde_json::from_str::<SessionMeta>(&content)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Some(meta))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn list_sessions(&self) -> std::io::Result<Vec<SessionMeta>> {
        let mut entries = match tokio::fs::read_dir(&self.base_dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut sessions = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.ends_with(".meta.json") {
                continue;
            }
            let content = tokio::fs::read_to_string(entry.path()).await?;
            if let Ok(meta) = serde_json::from_str::<SessionMeta>(&content) {
                sessions.push(meta);
            }
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    async fn delete(&self, session_id: &str) -> std::io::Result<()> {
        let jsonl = self.jsonl_path(session_id);
        let meta = self.meta_path(session_id);
        let _ = tokio::fs::remove_file(jsonl).await;
        let _ = tokio::fs::remove_file(meta).await;
        Ok(())
    }

    async fn append(&self, session_id: &str, n: &SessionNotification) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.jsonl_path(session_id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        let line = serde_json::to_string(n).unwrap_or_else(|_| "{}".to_string());
        use tokio::io::AsyncWriteExt as _;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        Ok(())
    }

    async fn read_all(&self, session_id: &str) -> std::io::Result<Vec<SessionNotification>> {
        let path = self.jsonl_path(session_id);
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut out = Vec::new();
        for line in content.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if let Ok(n) = serde_json::from_str::<SessionNotification>(t) {
                out.push(n);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::{Mutex as TokioMutex, mpsc};
    use tokio_stream::wrappers::ReceiverStream;

    #[test]
    fn resolve_prompt_payload_from_text_prompt() {
        let req = PromptSessionRequest {
            prompt: Some("  hello world  ".to_string()),
            prompt_blocks: None,
            working_dir: None,
            env: std::collections::HashMap::new(),
            executor_config: None,
            mcp_servers: vec![],
            workspace_root: None,
            address_space: None,
            flow_capabilities: None,
            system_context: None,
        };

        let payload = req
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
        let req = PromptSessionRequest {
            prompt: None,
            prompt_blocks: Some(vec![
                json!({ "type": "text", "text": "请分析 @src/main.ts" }),
                json!({ "type": "resource_link", "uri": "file:///workspace/src/main.ts", "name": "src/main.ts" }),
                json!({ "type": "image", "mimeType": "image/png", "data": "AAAA" }),
            ]),
            working_dir: None,
            env: std::collections::HashMap::new(),
            executor_config: None,
            mcp_servers: vec![],
            workspace_root: None,
            address_space: None,
            flow_capabilities: None,
            system_context: None,
        };

        let payload = req
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
    async fn start_prompt_uses_request_workspace_root_override() {
        #[derive(Default)]
        struct RecordingConnector {
            contexts: Arc<TokioMutex<Vec<ExecutionContext>>>,
        }

        #[async_trait::async_trait]
        impl AgentConnector for RecordingConnector {
            fn connector_id(&self) -> &'static str {
                "recording"
            }

            fn connector_type(&self) -> crate::connector::ConnectorType {
                crate::connector::ConnectorType::LocalExecutor
            }

            fn capabilities(&self) -> crate::connector::ConnectorCapabilities {
                crate::connector::ConnectorCapabilities::default()
            }

            fn list_executors(&self) -> Vec<crate::connector::ExecutorInfo> {
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
                context: ExecutionContext,
            ) -> Result<crate::connector::ExecutionStream, ConnectorError> {
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
        let hub = ExecutorHub::new(base.path().to_path_buf(), connector.clone());
        let session = hub.create_session("test").await.expect("create session");

        hub.start_prompt(
            &session.id,
            PromptSessionRequest {
                prompt: Some("hello".to_string()),
                prompt_blocks: None,
                working_dir: Some("src".to_string()),
                env: HashMap::new(),
                executor_config: None,
                mcp_servers: vec![],
                workspace_root: Some(workspace.path().to_path_buf()),
                address_space: None,
                flow_capabilities: None,
                system_context: None,
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
            contexts: Arc<TokioMutex<Vec<ExecutionContext>>>,
        }

        #[async_trait::async_trait]
        impl AgentConnector for RecordingConnector {
            fn connector_id(&self) -> &'static str {
                "recording"
            }

            fn connector_type(&self) -> crate::connector::ConnectorType {
                crate::connector::ConnectorType::LocalExecutor
            }

            fn capabilities(&self) -> crate::connector::ConnectorCapabilities {
                crate::connector::ConnectorCapabilities::default()
            }

            fn list_executors(&self) -> Vec<crate::connector::ExecutorInfo> {
                vec![crate::connector::ExecutorInfo {
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
                context: ExecutionContext,
            ) -> Result<crate::connector::ExecutionStream, ConnectorError> {
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
        let hub = ExecutorHub::new(base.path().to_path_buf(), connector.clone());

        let session = hub
            .create_session("reuse existing executor")
            .await
            .expect("create session");

        hub.update_session_meta(&session.id, |meta| {
            meta.executor_config = Some(crate::connector::AgentDashExecutorConfig::new("PI_AGENT"));
        })
        .await
        .expect("update meta should succeed");

        hub.start_prompt(
            &session.id,
            PromptSessionRequest {
                prompt: Some("hello".to_string()),
                prompt_blocks: None,
                working_dir: None,
                env: HashMap::new(),
                executor_config: None,
                mcp_servers: vec![],
                workspace_root: None,
                address_space: None,
                flow_capabilities: None,
                system_context: None,
            },
        )
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

            fn connector_type(&self) -> crate::connector::ConnectorType {
                crate::connector::ConnectorType::LocalExecutor
            }

            fn capabilities(&self) -> crate::connector::ConnectorCapabilities {
                crate::connector::ConnectorCapabilities::default()
            }

            fn list_executors(&self) -> Vec<crate::connector::ExecutorInfo> {
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
                _context: ExecutionContext,
            ) -> Result<crate::connector::ExecutionStream, ConnectorError> {
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
        let hub = ExecutorHub::new(base.path().to_path_buf(), Arc::new(FailingConnector));
        let session = hub.create_session("test").await.expect("create session");

        let error = hub
            .start_prompt(
                &session.id,
                PromptSessionRequest {
                    prompt: Some("hello".to_string()),
                    prompt_blocks: None,
                    working_dir: None,
                    env: HashMap::new(),
                    executor_config: None,
                    mcp_servers: vec![],
                    workspace_root: None,
                    address_space: None,
                    flow_capabilities: None,
                    system_context: None,
                },
            )
            .await
            .expect_err("prompt should fail");
        assert!(error.to_string().contains("connector setup failed"));

        let history = hub
            .store
            .read_all(&session.id)
            .await
            .expect("history should load");
        let terminal = history
            .iter()
            .filter_map(|notification| match &notification.update {
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

            fn connector_type(&self) -> crate::connector::ConnectorType {
                crate::connector::ConnectorType::LocalExecutor
            }

            fn capabilities(&self) -> crate::connector::ConnectorCapabilities {
                crate::connector::ConnectorCapabilities::default()
            }

            fn list_executors(&self) -> Vec<crate::connector::ExecutorInfo> {
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
                _context: ExecutionContext,
            ) -> Result<crate::connector::ExecutionStream, ConnectorError> {
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
        let hub = ExecutorHub::new(base.path().to_path_buf(), connector);
        let session = hub.create_session("test").await.expect("create session");

        let turn_id = hub
            .start_prompt(
                &session.id,
                PromptSessionRequest {
                    prompt: Some("hello".to_string()),
                    prompt_blocks: None,
                    working_dir: None,
                    env: HashMap::new(),
                    executor_config: None,
                    mcp_servers: vec![],
                    workspace_root: None,
                    address_space: None,
                    flow_capabilities: None,
                    system_context: None,
                },
            )
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
                message: Some("执行已取消".to_string()),
            }
        );

        let history = hub
            .store
            .read_all(&session.id)
            .await
            .expect("history should load");
        let terminal = history
            .iter()
            .filter_map(|notification| match &notification.update {
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
