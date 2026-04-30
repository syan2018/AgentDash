use std::{collections::HashMap, path::PathBuf, sync::Arc};

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;
use workspace_utils::{log_msg::LogMsg, msg_store::MsgStore};

use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};

use executors::{
    approvals::NoopExecutorApprovalService,
    env::{ExecutionEnv, RepoContext},
    executors::StandardCodingAgentExecutor as _,
    logs::utils::patch::extract_normalized_entry_from_patch,
    profile::ExecutorConfigs,
};

use crate::adapters::normalized_to_acp::NormalizedToAcpConverter;
use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, PromptPayload, workspace_path_from_context,
};

pub struct VibeKanbanExecutorsConnector {
    default_repo_root: PathBuf,
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

fn build_executor_session_bound_notification(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    executor_session_id: &str,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let event = AgentDashEventV1::new("executor_session_bound")
        .message(Some(executor_session_id.to_string()))
        .data(Some(json!({ "executor_session_id": executor_session_id })));

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new().meta(merge_agentdash_meta(None, &agentdash)),
        ),
    )
}

impl VibeKanbanExecutorsConnector {
    pub fn new(default_repo_root: PathBuf) -> Self {
        Self {
            default_repo_root,
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

    fn list_executors(&self) -> Vec<AgentInfo> {
        let configs = ExecutorConfigs::get_cached();
        let mut out: Vec<AgentInfo> = configs
            .executors
            .iter()
            .map(|(&agent, profile)| {
                let id = agent.to_string();
                let available = profile
                    .get_variant("DEFAULT")
                    .map(|a| a.get_availability_info().is_available())
                    .unwrap_or(false);

                let mut variants: Vec<String> = profile.configurations.keys().cloned().collect();
                variants.sort();

                AgentInfo {
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
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        use std::str::FromStr as _;

        let base = executors::executors::BaseCodingAgent::from_str(executor)
            .map_err(|_| ConnectorError::InvalidConfig(format!("未知执行器: {executor}")))?;

        let profile_id = executors::profile::ExecutorProfileId {
            executor: base,
            variant: None,
        };

        let agent = ExecutorConfigs::get_cached()
            .get_coding_agent(&profile_id)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("找不到执行器 profile: {profile_id}"))
            })?;

        let wd = working_dir.unwrap_or_else(|| self.default_repo_root.clone());
        agent
            .discover_options(Some(&wd), Some(&self.default_repo_root))
            .await
            .map_err(|e| ConnectorError::Runtime(format!("discover_options 失败: {e}")))
    }

    async fn has_live_session(&self, session_id: &str) -> bool {
        self.cancel_by_session.lock().await.contains_key(session_id)
    }

    async fn prompt(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        let user_text = prompt.to_fallback_text();
        // vibe_kanban 通过 stdio 把 prompt 前置拼接给外部进程，暂未支持结构化
        // Bundle。PR 3 期间仍消费 deprecated `assembled_system_prompt` 作为兜底，
        // 待 PR 8 删除该字段时此 connector 需要自渲染或协议升级。
        #[allow(deprecated)]
        let prompt_text = if let Some(ref sys_prompt) = context.turn.assembled_system_prompt {
            format!("{sys_prompt}\n\n{user_text}")
        } else {
            user_text
        };
        let prompt_text = prompt_text.trim().to_string();
        if prompt_text.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "prompt payload 解析后为空".to_string(),
            ));
        }

        let vk_config = crate::adapters::vibe_kanban_config::to_vibe_kanban_config(
            &context.session.executor_config,
        )
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "执行器 '{}' 不是有效的 vibe-kanban 执行器",
                context.session.executor_config.executor
            ))
        })?;

        let mut agent = ExecutorConfigs::get_cached()
            .get_coding_agent(&vk_config.profile_id())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("找不到执行器 profile: {vk_config}"))
            })?;

        if vk_config.has_overrides() {
            agent.apply_overrides(&vk_config);
        }

        agent.use_approvals(Arc::new(NoopExecutorApprovalService));

        let repo_root = workspace_path_from_context(&context)?;
        let repo_context = RepoContext::new(repo_root, vec![".".to_string()]);
        let mut env = ExecutionEnv::new(
            repo_context,
            false,
            "请在提交前完成 pnpm lint/type-check/test 等自检".to_string(),
        );
        env.merge(&context.session.environment_variables);

        let follow_up_session_id = follow_up_session_id
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let mut spawned = if let Some(follow_up_session_id) = follow_up_session_id {
            agent
                .spawn_follow_up(
                    &context.session.working_directory,
                    &prompt_text,
                    follow_up_session_id,
                    None,
                    &env,
                )
                .await
                .map_err(|e| ConnectorError::SpawnFailed(e.to_string()))?
        } else {
            agent
                .spawn(&context.session.working_directory, &prompt_text, &env)
                .await
                .map_err(|e| ConnectorError::SpawnFailed(e.to_string()))?
        };

        if let Some(cancel) = spawned.cancel.clone() {
            self.cancel_by_session
                .lock()
                .await
                .insert(session_id.to_string(), cancel);
        }

        let msg_store = Arc::new(MsgStore::new());

        agent.normalize_logs(msg_store.clone(), &context.session.working_directory);

        if let Some(stdout) = spawned.child.inner().stdout.take() {
            let ms = msg_store.clone();
            tokio::spawn(async move {
                let mut stream = ReaderStream::new(stdout);
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(bytes) => ms.push_stdout(String::from_utf8_lossy(&bytes).into_owned()),
                        Err(e) => {
                            ms.push_stdout(format!("stdout 读取失败: {e}"));
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
                        Ok(bytes) => ms.push_stdout(String::from_utf8_lossy(&bytes).into_owned()),
                        Err(e) => {
                            ms.push_stdout(format!("stderr 读取失败: {e}"));
                            break;
                        }
                    }
                }
            });
        }

        let ms = msg_store.clone();
        let cancel_map = self.cancel_by_session.clone();
        let session_id_owned = session_id.to_string();
        let session_id_for_wait = session_id_owned.clone();
        tokio::spawn(async move {
            let _ = spawned.child.wait().await;
            ms.push_finished();
            cancel_map.lock().await.remove(&session_id_for_wait);
        });

        let (tx, rx) =
            tokio::sync::mpsc::channel::<Result<SessionNotification, ConnectorError>>(256);
        let connector_type = match self.connector_type() {
            ConnectorType::LocalExecutor => "local_executor",
            ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let mut source = AgentDashSourceV1::new(self.connector_id(), connector_type);
        source.executor_id = Some(context.session.executor_config.executor.to_string());
        let turn_id = context.session.turn_id.clone();
        let mut converter = NormalizedToAcpConverter::new(
            SessionId::new(session_id.to_string()),
            source.clone(),
            turn_id.clone(),
        );

        tokio::spawn(async move {
            let mut stream = msg_store.history_plus_stream();
            let mut last_executor_session_id: Option<String> = None;
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
                    Ok(LogMsg::SessionId(executor_session_id)) => {
                        if last_executor_session_id.as_deref() == Some(executor_session_id.as_str())
                        {
                            continue;
                        }
                        last_executor_session_id = Some(executor_session_id.clone());
                        let notification = build_executor_session_bound_notification(
                            &session_id_owned,
                            &source,
                            &turn_id,
                            &executor_session_id,
                        );
                        if tx.send(Ok(notification)).await.is_err() {
                            return;
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

    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(
            "当前 vibe-kanban 执行器尚未接入正式审批恢复链路".to_string(),
        ))
    }

    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(
            "当前 vibe-kanban 执行器尚未接入正式审批恢复链路".to_string(),
        ))
    }
}
