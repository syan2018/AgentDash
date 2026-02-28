use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};

use agent_client_protocol::{SessionId, SessionNotification};
use futures::stream::BoxStream;
use futures::StreamExt;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;
use workspace_utils::{log_msg::LogMsg, msg_store::MsgStore};

use agentdash_acp_meta::AgentDashSourceV1;

use executors::{
    approvals::NoopExecutorApprovalService,
    env::{ExecutionEnv, RepoContext},
    executors::StandardCodingAgentExecutor as _,
    logs::utils::patch::extract_normalized_entry_from_patch,
    profile::ExecutorConfigs,
};

use crate::{
    adapters::normalized_to_acp::NormalizedToAcpConverter,
    connector::{
        AgentConnector, ConnectorCapabilities, ConnectorError, ConnectorType, ExecutorInfo,
        ExecutionContext, ExecutionStream,
    },
};

pub struct VibeKanbanExecutorsConnector {
    workspace_root: PathBuf,
    cancel_by_session: Arc<Mutex<HashMap<String, executors::executors::CancellationToken>>>,
}

fn humanize_executor_id(id: &str) -> String {
    id.split('_')
        .filter(|s| !s.is_empty())
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

    fn connector_type(&self) -> ConnectorType {
        ConnectorType::LocalExecutor
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            supports_cancel: true,
            supports_discovery: true,
            supports_variants: true,
            supports_model_override: true,
            supports_permission_policy: true,
        }
    }

    fn list_executors(&self) -> Vec<ExecutorInfo> {
        let configs = ExecutorConfigs::get_cached();
        let mut out: Vec<ExecutorInfo> = configs
            .executors
            .iter()
            .map(|(&agent, profile)| {
                let id = agent.to_string();
                let available = profile
                    .get_default()
                    .map(|a| a.get_availability_info().is_available())
                    .unwrap_or(false);

                let mut variants: Vec<String> = profile.configurations.keys().cloned().collect();
                variants.sort();

                ExecutorInfo {
                    id: id.clone(),
                    name: humanize_executor_id(&id),
                    variants,
                    available,
                }
            })
            .collect();

        out.sort_by(|a, b| b.available.cmp(&a.available).then(a.name.cmp(&b.name)));
        out
    }

    async fn discover_options_stream(
        &self,
        executor: &str,
        variant: Option<&str>,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        use std::str::FromStr as _;

        let base = executors::executors::BaseCodingAgent::from_str(executor)
            .map_err(|_| ConnectorError::InvalidConfig(format!("未知执行器: {executor}")))?;

        let v = variant
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter(|s| !s.eq_ignore_ascii_case("DEFAULT"))
            .map(|s| s.to_string());

        let profile_id = executors::profile::ExecutorProfileId {
            executor: base,
            variant: v,
        };

        let agent = ExecutorConfigs::get_cached()
            .get_coding_agent(&profile_id)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("找不到执行器 profile: {profile_id}"))
            })?;

        let wd = working_dir.unwrap_or_else(|| self.workspace_root.clone());
        agent.discover_options(Some(&wd), Some(&self.workspace_root))
            .await
            .map_err(|e| ConnectorError::Runtime(format!("discover_options 失败: {e}")))
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

        agent.normalize_logs(msg_store.clone(), &context.working_directory);

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

        let ms = msg_store.clone();
        let cancel_map = self.cancel_by_session.clone();
        let session_id_owned = session_id.to_string();
        tokio::spawn(async move {
            let _ = spawned.child.wait().await;
            ms.push_finished();
            cancel_map.lock().await.remove(&session_id_owned);
        });

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<SessionNotification, ConnectorError>>(256);
        let connector_type = match self.connector_type() {
            ConnectorType::LocalExecutor => "local_executor",
            ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let mut source = AgentDashSourceV1::new(self.connector_id(), connector_type);
        source.executor_id = Some(context.executor_config.executor.to_string());
        source.variant = context.executor_config.variant.clone();
        let mut converter = NormalizedToAcpConverter::new(
            SessionId::new(session_id.to_string()),
            source,
            context.turn_id.clone(),
        );

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
