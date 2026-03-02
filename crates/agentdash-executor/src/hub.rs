use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use agent_client_protocol::{
    ContentBlock, ContentChunk, SessionId, SessionNotification, SessionUpdate, TextContent,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

use agentdash_acp_meta::{merge_agentdash_meta, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1};

use crate::connector::{AgentConnector, ConnectorError, ExecutionContext};

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
}

impl PromptSessionRequest {
    /// 解析出有效的纯文本 prompt。
    /// 优先使用 prompt_blocks，若不存在则回退到 prompt 字段。
    /// 二者同时存在返回 Err。
    pub fn resolve_text_prompt(&self) -> Result<String, &'static str> {
        match (&self.prompt, &self.prompt_blocks) {
            (Some(_), Some(_)) => Err("prompt 与 promptBlocks 不能同时传入"),
            (None, None) => Err("必须提供 prompt 或 promptBlocks"),
            (Some(p), None) => {
                let trimmed = p.trim();
                if trimmed.is_empty() {
                    Err("prompt 不能为空")
                } else {
                    Ok(trimmed.to_string())
                }
            }
            (None, Some(blocks)) => {
                if blocks.is_empty() {
                    return Err("promptBlocks 不能为空数组");
                }
                let mut texts = Vec::new();
                for block in blocks {
                    if let Some(t) = block.get("type").and_then(|v| v.as_str()) {
                        match t {
                            "text" => {
                                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                                    texts.push(text.to_string());
                                }
                            }
                            "resource" => {
                                if let Some(res) = block.get("resource") {
                                    let uri = res.get("uri").and_then(|v| v.as_str()).unwrap_or("unknown");
                                    let text = res.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                    if !text.is_empty() {
                                        texts.push(format!("\n<file path=\"{uri}\">\n{text}\n</file>"));
                                    }
                                }
                            }
                            "resource_link" => {
                                let uri = block.get("uri").and_then(|v| v.as_str()).unwrap_or("unknown");
                                texts.push(format!("[引用文件: {uri}]"));
                            }
                            _ => {}
                        }
                    }
                }
                let result = texts.join("\n");
                if result.trim().is_empty() {
                    Err("promptBlocks 中没有有效内容")
                } else {
                    Ok(result)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
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
        let id = format!("sess-{}-{}", chrono::Utc::now().timestamp_millis(), &uuid::Uuid::new_v4().to_string()[..8]);
        let now = chrono::Utc::now().timestamp_millis();
        let meta = SessionMeta {
            id: id.clone(),
            title: title.to_string(),
            created_at: now,
            updated_at: now,
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

    pub async fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(session_id);
        }
        self.store.delete(session_id).await
    }

    pub async fn ensure_session(&self, session_id: &str) -> broadcast::Receiver<SessionNotification> {
        let mut sessions = self.sessions.lock().await;
        let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            SessionRuntime { tx, running: false }
        });
        runtime.tx.subscribe()
    }

    /// 多轮对话：同一 session 允许多次调用，但同一时间只允许一次活跃执行。
    /// 如果上一轮仍在执行中，返回 Err。
    pub async fn start_prompt(&self, session_id: &str, req: PromptSessionRequest) -> Result<(), ConnectorError> {
        let resolved_prompt = req.resolve_text_prompt()
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?;

        let tx = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
                SessionRuntime { tx, running: false }
            });
            if runtime.running {
                return Err(ConnectorError::Runtime("该会话有正在执行的 prompt，请等待完成或取消后再试".into()));
            }
            runtime.running = true;
            runtime.tx.clone()
        };

        let executor_config = req.executor_config.unwrap_or_else(|| {
            executors::profile::ExecutorConfig::new(executors::executors::BaseCodingAgent::ClaudeCode)
        });

        let working_directory = resolve_working_dir(&self.workspace_root, req.working_dir.as_deref());

        // 该 turn_id 必须在“用户消息注入”和“连接器流”之间保持一致，便于前端归并。
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());

        let context = ExecutionContext {
            turn_id: turn_id.clone(),
            working_directory,
            environment_variables: req.env,
            executor_config,
        };

        let title_hint = resolved_prompt.chars().take(30).collect::<String>();
        let store = self.store.clone();
        let sid = session_id.to_string();
        if let Ok(Some(mut meta)) = store.read_meta(&sid).await {
            meta.updated_at = chrono::Utc::now().timestamp_millis();
            let _ = store.write_meta(&meta).await;
        } else {
            let now = chrono::Utc::now().timestamp_millis();
            let meta = SessionMeta {
                id: sid.clone(),
                title: title_hint,
                created_at: now,
                updated_at: now,
            };
            let _ = store.write_meta(&meta).await;
        }

        // 注入用户消息到流和持久化存储（附带 `_meta.agentdash`）
        let connector_type = match self.connector.connector_type() {
            crate::connector::ConnectorType::LocalExecutor => "local_executor",
            crate::connector::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let mut source = AgentDashSourceV1::new(self.connector.connector_id(), connector_type);
        source.executor_id = Some(context.executor_config.executor.to_string());
        source.variant = context.executor_config.variant.clone();
        let mut trace = AgentDashTraceV1::new();
        trace.turn_id = Some(turn_id);
        let agentdash = AgentDashMetaV1::new()
            .source(Some(source))
            .trace(Some(trace));
        let meta = merge_agentdash_meta(None, &agentdash);

        let user_chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(&resolved_prompt))).meta(meta);
        let user_notification = SessionNotification::new(
            SessionId::new(session_id),
            SessionUpdate::UserMessageChunk(user_chunk),
        );
        let _ = store.append(&sid, &user_notification).await;
        let _ = tx.send(user_notification);

        let mut stream = self.connector.prompt(session_id, &resolved_prompt, context).await?;
        let sessions = self.sessions.clone();
        let session_id = session_id.to_string();

        tokio::spawn(async move {
            while let Some(next) = stream.next().await {
                match next {
                    Ok(notification) => {
                        let _ = store.append(&session_id, &notification).await;
                        let _ = tx.send(notification);
                    }
                    Err(e) => {
                        tracing::error!("执行流错误 session_id={}: {}", session_id, e);
                        break;
                    }
                }
            }
            // 执行完成后标记 running = false，允许下一轮
            let mut guard = sessions.lock().await;
            if let Some(runtime) = guard.get_mut(&session_id) {
                runtime.running = false;
            }
        });

        Ok(())
    }

    pub async fn subscribe_with_history(
        &self,
        session_id: &str,
    ) -> (Vec<SessionNotification>, broadcast::Receiver<SessionNotification>) {
        let history = self.store.read_all(session_id).await.unwrap_or_default();
        let rx = self.ensure_session(session_id).await;
        (history, rx)
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
