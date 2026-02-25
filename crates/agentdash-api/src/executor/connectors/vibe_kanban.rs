use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};

use agent_client_protocol::{SessionId, SessionNotification};
use futures::StreamExt;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;
use workspace_utils::{log_msg::LogMsg, msg_store::MsgStore};

use executors::{
    approvals::NoopExecutorApprovalService,
    env::{ExecutionEnv, RepoContext},
    executors::StandardCodingAgentExecutor as _,
    logs::utils::patch::extract_normalized_entry_from_patch,
    profile::ExecutorConfigs,
};

use crate::executor::{
    adapters::normalized_to_acp::NormalizedToAcpConverter,
    connector::{AgentConnector, ConnectorError, ExecutionContext, ExecutionStream},
};

pub struct VibeKanbanExecutorsConnector {
    workspace_root: PathBuf,
    cancel_by_session: Arc<Mutex<HashMap<String, executors::executors::CancellationToken>>>,
}

impl VibeKanbanExecutorsConnector {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            cancel_by_session: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl AgentConnector for VibeKanbanExecutorsConnector {
    fn connector_id(&self) -> &'static str {
        "vibe-kanban-executors"
    }

    async fn prompt(
        &self,
        session_id: &str,
        prompt: &str,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        let mut agent = ExecutorConfigs::get_cached()
            .get_coding_agent(&context.executor_config.profile_id())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "找不到执行器 profile: {}",
                    context.executor_config
                ))
            })?;

        if context.executor_config.has_overrides() {
            agent.apply_overrides(&context.executor_config);
        }

        agent.use_approvals(Arc::new(NoopExecutorApprovalService));

        let repo_context = RepoContext::new(self.workspace_root.clone(), vec![".".to_string()]);
        let mut env = ExecutionEnv::new(
            repo_context,
            false,
            "请在提交前完成 pnpm lint/type-check/test 等自检".to_string(),
        );
        env.merge(&context.environment_variables);

        let mut spawned = agent
            .spawn(&context.working_directory, prompt, &env)
            .await
            .map_err(|e| ConnectorError::SpawnFailed(e.to_string()))?;

        if let Some(cancel) = spawned.cancel.clone() {
            self.cancel_by_session
                .lock()
                .await
                .insert(session_id.to_string(), cancel);
        }

        let msg_store = Arc::new(MsgStore::new());

        // Start normalizers (they listen to msg_store streams).
        agent.normalize_logs(msg_store.clone(), &context.working_directory);

        // Forward child stdout/stderr into msg_store
        if let Some(stdout) = spawned.child.inner().stdout.take() {
            let ms = msg_store.clone();
            tokio::spawn(async move {
                let mut stream = ReaderStream::new(stdout);
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(bytes) => ms.push_stdout(String::from_utf8_lossy(&bytes).into_owned()),
                        Err(e) => {
                            ms.push_stderr(format!("stdout 读取失败: {e}"));
                            break;
                        }
                    }
                }
            });
        }

        if let Some(stderr) = spawned.child.inner().stderr.take() {
            let ms = msg_store.clone();
            tokio::spawn(async move {
                let mut stream = ReaderStream::new(stderr);
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(bytes) => ms.push_stderr(String::from_utf8_lossy(&bytes).into_owned()),
                        Err(e) => {
                            ms.push_stderr(format!("stderr 读取失败: {e}"));
                            break;
                        }
                    }
                }
            });
        }

        // Wait for completion and then mark store as finished.
        let ms = msg_store.clone();
        let cancel_map = self.cancel_by_session.clone();
        let session_id_owned = session_id.to_string();
        tokio::spawn(async move {
            let _ = spawned.child.wait().await;
            ms.push_finished();
            cancel_map.lock().await.remove(&session_id_owned);
        });

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<SessionNotification, ConnectorError>>(256);
        let mut converter = NormalizedToAcpConverter::new(SessionId::new(session_id.to_string()));

        tokio::spawn(async move {
            let mut stream = msg_store.history_plus_stream();
            while let Some(next) = stream.next().await {
                match next {
                    Ok(LogMsg::JsonPatch(patch)) => {
                        if let Some((idx, entry)) = extract_normalized_entry_from_patch(&patch) {
                            for n in converter.apply(idx, entry) {
                                if tx.send(Ok(n)).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Ok(LogMsg::Finished) => break,
                    Ok(_) => {}
                    Err(e) => {
                        let _ = tx
                            .send(Err(ConnectorError::Runtime(format!("日志流错误: {e}"))))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        if let Some(token) = self.cancel_by_session.lock().await.get(session_id).cloned() {
            token.cancel();
        }
        Ok(())
    }
}

