use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use executors::{executors::StandardCodingAgentExecutor as _, profile::ExecutorConfigs};
use futures::stream::BoxStream;
use tokio::sync::Mutex;

use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, PromptPayload,
};
use std::str::FromStr as _;

use crate::connectors::executor_session::spawn_executor_session;

pub struct VibeKanbanExecutorsConnector {
    default_repo_root: PathBuf,
    cancel_by_session: Arc<Mutex<HashMap<String, executors::executors::CancellationToken>>>,
    excluded_executors: HashSet<String>,
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

fn normalize_executor_id(id: &str) -> String {
    id.trim().replace('-', "_").to_ascii_uppercase()
}

impl VibeKanbanExecutorsConnector {
    pub fn new(default_repo_root: PathBuf) -> Self {
        Self::new_with_exclusions(default_repo_root, std::iter::empty::<&str>())
    }

    pub fn new_with_exclusions<I, S>(default_repo_root: PathBuf, excluded_executors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            default_repo_root,
            cancel_by_session: Arc::new(Mutex::new(HashMap::new())),
            excluded_executors: excluded_executors
                .into_iter()
                .map(|id| normalize_executor_id(id.as_ref()))
                .collect(),
        }
    }

    fn is_excluded(&self, executor_id: &str) -> bool {
        self.excluded_executors
            .contains(&normalize_executor_id(executor_id))
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
            .filter(|(agent, _)| !self.is_excluded(&agent.to_string()))
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
        if self.is_excluded(executor) {
            return Err(ConnectorError::InvalidConfig(format!(
                "执行器 '{executor}' 已从 vibe-kanban 桥接中排除"
            )));
        }

        let normalized = normalize_executor_id(executor);
        let base = executors::executors::BaseCodingAgent::from_str(&normalized)
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
        if self.is_excluded(&context.session.executor_config.executor) {
            return Err(ConnectorError::InvalidConfig(format!(
                "执行器 '{}' 已从 vibe-kanban 桥接中排除",
                context.session.executor_config.executor
            )));
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
