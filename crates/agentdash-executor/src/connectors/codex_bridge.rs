use std::{collections::HashMap, path::PathBuf, sync::Arc};

use futures::stream::BoxStream;
use tokio::sync::Mutex;

use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, PromptPayload,
};
use executors::{
    executors::{BaseCodingAgent, StandardCodingAgentExecutor as _},
    profile::{ExecutorConfigs, ExecutorProfileId},
};

use crate::{
    adapters::codex_config::to_codex_config, connectors::executor_session::spawn_executor_session,
};

const CODEX_EXECUTOR_ID: &str = "CODEX";

fn normalize_executor_id(executor: &str) -> String {
    executor.trim().replace('-', "_").to_ascii_uppercase()
}

fn is_codex_executor(executor: &str) -> bool {
    normalize_executor_id(executor) == CODEX_EXECUTOR_ID
}

pub struct CodexBridgeConnector {
    default_repo_root: PathBuf,
    cancel_by_session: Arc<Mutex<HashMap<String, executors::executors::CancellationToken>>>,
}

impl CodexBridgeConnector {
    /// 首阶段桥接：对外暴露独立 Codex connector，内部仍复用 executors 的 Codex runtime。
    /// 后续替换为纯原生 Codex SDK 时，仅需替换本模块实现。
    pub fn new(default_repo_root: PathBuf) -> Self {
        Self {
            default_repo_root,
            cancel_by_session: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl AgentConnector for CodexBridgeConnector {
    fn connector_id(&self) -> &'static str {
        "codex-bridge"
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
        let profile_id = ExecutorProfileId {
            executor: BaseCodingAgent::Codex,
            variant: None,
        };
        let available = configs
            .get_coding_agent(&profile_id)
            .map(|agent| agent.get_availability_info().is_available())
            .unwrap_or(false);

        let mut variants = configs
            .executors
            .get(&BaseCodingAgent::Codex)
            .map(|profile| profile.configurations.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        variants.sort();

        vec![AgentInfo {
            id: CODEX_EXECUTOR_ID.to_string(),
            name: "Codex".to_string(),
            variants,
            available,
        }]
    }

    async fn discover_options_stream(
        &self,
        executor: &str,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        if !is_codex_executor(executor) {
            return Err(ConnectorError::InvalidConfig(format!(
                "Codex bridge 不支持执行器: {executor}"
            )));
        }

        let profile_id = ExecutorProfileId {
            executor: BaseCodingAgent::Codex,
            variant: None,
        };
        let agent = ExecutorConfigs::get_cached()
            .get_coding_agent(&profile_id)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("找不到 Codex 执行器 profile".to_string())
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
        let codex_config = to_codex_config(&context.session.executor_config).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "执行器 '{}' 不是有效的 Codex bridge 执行器",
                context.session.executor_config.executor
            ))
        })?;

        let mut agent = ExecutorConfigs::get_cached()
            .get_coding_agent(&codex_config.profile_id())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("找不到 Codex 执行器 profile".to_string())
            })?;

        if codex_config.has_overrides() {
            agent.apply_overrides(&codex_config);
        }

        spawn_executor_session(
            self.connector_id(),
            self.connector_type(),
            self.cancel_by_session.clone(),
            agent,
            session_id,
            follow_up_session_id,
            prompt,
            context,
        )
        .await
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
            "当前 Codex bridge 尚未接入正式审批恢复链路".to_string(),
        ))
    }

    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(
            "当前 Codex bridge 尚未接入正式审批恢复链路".to_string(),
        ))
    }
}
