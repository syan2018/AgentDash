use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use agent_client_protocol::SessionNotification;
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::{Mutex, broadcast};

use crate::executor::connector::{AgentConnector, ConnectorError, ExecutionContext};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSessionRequest {
    pub prompt: String,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub executor_config: Option<executors::profile::ExecutorConfig>,
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
    started: bool,
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

    pub async fn ensure_session(&self, session_id: &str) -> broadcast::Receiver<SessionNotification> {
        let mut sessions = self.sessions.lock().await;
        let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            SessionRuntime { tx, started: false }
        });
        runtime.tx.subscribe()
    }

    pub async fn start_prompt(&self, session_id: &str, req: PromptSessionRequest) -> Result<(), ConnectorError> {
        let (tx, should_start) = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
                SessionRuntime { tx, started: false }
            });
            if runtime.started {
                (runtime.tx.clone(), false)
            } else {
                runtime.started = true;
                (runtime.tx.clone(), true)
            }
        };

        if !should_start {
            return Ok(());
        }

        self.store.reset(session_id).await.ok();

        let executor_config = req.executor_config.unwrap_or_else(|| {
            executors::profile::ExecutorConfig::new(executors::executors::BaseCodingAgent::ClaudeCode)
        });

        let working_directory = resolve_working_dir(&self.workspace_root, req.working_dir.as_deref());

        let context = ExecutionContext {
            working_directory,
            environment_variables: req.env,
            executor_config,
        };

        let mut stream = self.connector.prompt(session_id, &req.prompt, context).await?;
        let store = self.store.clone();
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

    fn file_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{session_id}.jsonl"))
    }

    async fn reset(&self, session_id: &str) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.file_path(session_id);
        tokio::fs::write(path, "").await
    }

    async fn append(&self, session_id: &str, n: &SessionNotification) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.file_path(session_id);
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
        let path = self.file_path(session_id);
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

