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

use crate::connector::{AgentConnector, ConnectorError, ExecutionContext, PromptPayload};

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
    pub executor_config: Option<executors::profile::ExecutorConfig>,
    /// ACP per-session MCP Server 列表（不走 serde — 仅由后端代码填充）
    #[serde(skip)]
    pub mcp_servers: Vec<McpServer>,
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
    success: bool,
    message: Option<String>,
) -> SessionNotification {
    build_turn_lifecycle_notification(
        session_id,
        source,
        turn_id,
        if success {
            "turn_completed"
        } else {
            "turn_failed"
        },
        if success { "info" } else { "error" },
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

fn parse_turn_terminal_event(meta: Option<&Meta>) -> Option<(String, bool, Option<String>)> {
    let parsed = parse_agentdash_meta(meta?)?;
    let trace = parsed.trace?;
    let turn_id = trace.turn_id?;
    let event = parsed.event?;

    match event.r#type.as_str() {
        "turn_completed" => Some((turn_id, true, event.message)),
        "turn_failed" => Some((turn_id, false, event.message)),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_session_id: Option<String>,
}

#[derive(Clone)]
pub struct ExecutorHub {
    workspace_root: PathBuf,
    connector: Arc<dyn AgentConnector>,
    sessions: Arc<Mutex<HashMap<String, SessionRuntime>>>,
    store: SessionStore,
}

struct SessionRuntime {
    tx: broadcast::Sender<SessionNotification>,
    running: bool,
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
    },
}

impl ExecutorHub {
    pub fn new(workspace_root: PathBuf, connector: Arc<dyn AgentConnector>) -> Self {
        let store = SessionStore::new(workspace_root.join(".agentdash").join("sessions"));
        Self {
            workspace_root,
            connector,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            store,
        }
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
            executor_session_id: None,
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

    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> std::io::Result<SessionExecutionState> {
        let running = {
            let sessions = self.sessions.lock().await;
            sessions.get(session_id).map(|runtime| runtime.running)
        }
        .unwrap_or(false);

        let history = self.store.read_all(session_id).await?;
        let mut latest_turn_id: Option<String> = None;
        let mut terminal_by_turn: HashMap<String, (bool, Option<String>)> = HashMap::new();

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
                turn_id: latest_turn_id,
            });
        }

        if let Some(turn_id) = latest_turn_id {
            if let Some((success, message)) = terminal_by_turn.remove(&turn_id) {
                if success {
                    return Ok(SessionExecutionState::Completed { turn_id });
                }
                return Ok(SessionExecutionState::Failed { turn_id, message });
            }
            return Ok(SessionExecutionState::Interrupted {
                turn_id: Some(turn_id),
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
            SessionRuntime { tx, running: false }
        });
        runtime.tx.subscribe()
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

        let tx = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
                SessionRuntime { tx, running: false }
            });
            if runtime.running {
                return Err(ConnectorError::Runtime(
                    "该会话有正在执行的 prompt，请等待完成或取消后再试".into(),
                ));
            }
            runtime.running = true;
            runtime.tx.clone()
        };

        let executor_config = req.executor_config.unwrap_or_else(|| {
            executors::profile::ExecutorConfig::new(
                executors::executors::BaseCodingAgent::ClaudeCode,
            )
        });

        let working_directory =
            resolve_working_dir(&self.workspace_root, req.working_dir.as_deref());

        // 该 turn_id 必须在“用户消息注入”和“连接器流”之间保持一致，便于前端归并。
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());

        let context = ExecutionContext {
            turn_id: turn_id.clone(),
            working_directory,
            environment_variables: req.env,
            executor_config,
            mcp_servers: req.mcp_servers,
        };

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
            Ok(None) | Err(_) => SessionMeta {
                id: sid.clone(),
                title: title_hint.clone(),
                created_at: now,
                updated_at: now,
                executor_session_id: None,
            },
        };
        session_meta.updated_at = now;
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

        let mut stream = self
            .connector
            .prompt(
                session_id,
                resolved_follow_up_session_id.as_deref(),
                &resolved_payload.prompt_payload,
                context,
            )
            .await?;
        let sessions = self.sessions.clone();
        let session_id = session_id.to_string();

        let turn_id_for_spawn = turn_id.clone();
        tokio::spawn(async move {
            let mut terminal_notification: Option<SessionNotification> = None;
            let mut last_executor_session_id: Option<String> = None;
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
                        terminal_notification = Some(build_turn_terminal_notification(
                            &session_id,
                            &source,
                            &turn_id_for_spawn,
                            false,
                            Some(e.to_string()),
                        ));
                        break;
                    }
                }
            }

            if terminal_notification.is_none() {
                terminal_notification = Some(build_turn_terminal_notification(
                    &session_id,
                    &source,
                    &turn_id_for_spawn,
                    true,
                    None,
                ));
            }

            if let Some(done) = terminal_notification {
                let _ = store.append(&session_id, &done).await;
                let _ = tx.send(done);
            }

            // 执行完成后标记 running = false，允许下一轮
            let mut guard = sessions.lock().await;
            if let Some(runtime) = guard.get_mut(&session_id) {
                runtime.running = false;
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
                SessionRuntime { tx, running: false }
            });
            runtime.tx.clone()
        };

        let _ = tx.send(notification);
        Ok(())
    }

    pub async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        self.connector.cancel(session_id).await
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
    use serde_json::json;

    #[test]
    fn resolve_prompt_payload_from_text_prompt() {
        let req = PromptSessionRequest {
            prompt: Some("  hello world  ".to_string()),
            prompt_blocks: None,
            working_dir: None,
            env: std::collections::HashMap::new(),
            executor_config: None,
            mcp_servers: vec![],
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
}
