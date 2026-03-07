use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

use crate::bootstrap::task_state_reconcile::reconcile_task_states_on_boot;
use agentdash_domain::backend::{BackendConfig, BackendRepository, BackendType};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workspace::WorkspaceRepository;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;
use agentdash_executor::{AgentConnector, ExecutorHub};
use agentdash_infrastructure::{
    SqliteBackendRepository, SqliteProjectRepository, SqliteSessionBindingRepository,
    SqliteSettingsRepository, SqliteStoryRepository, SqliteTaskRepository,
    SqliteWorkspaceRepository,
};

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
pub struct AppState {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub sqlite_task_repo: Arc<SqliteTaskRepository>,
    pub session_binding_repo: Arc<dyn SessionBindingRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    pub settings_repo: Arc<dyn SettingsRepository>,
    pub executor_hub: ExecutorHub,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
    /// MCP 服务基础 URL（用于向 Agent 注入 MCP 端点信息）
    pub mcp_base_url: Option<String>,
}

impl AppState {
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        // 按依赖顺序初始化：projects → workspaces → stories → tasks
        let project_repo = Arc::new(SqliteProjectRepository::new(pool.clone()));
        project_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(pool.clone()));
        workspace_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let story_repo = Arc::new(SqliteStoryRepository::new(pool.clone()));
        story_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let task_repo = Arc::new(SqliteTaskRepository::new(pool.clone()));
        task_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        story_repo
            .reconcile_task_counts()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let session_binding_repo = Arc::new(SqliteSessionBindingRepository::new(pool.clone()));
        session_binding_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let backend_repo = Arc::new(SqliteBackendRepository::new(pool.clone()));
        backend_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        ensure_default_backend(&backend_repo).await?;

        let settings_repo = Arc::new(SqliteSettingsRepository::new(pool));
        settings_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let workspace_root = std::env::current_dir()?;

        let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();
        sub_connectors.push(Arc::new(VibeKanbanExecutorsConnector::new(
            workspace_root.clone(),
        )));

        if let Some(pi_connector) =
            build_pi_agent_connector(&workspace_root, settings_repo.as_ref()).await
        {
            sub_connectors.push(Arc::new(pi_connector));
        }

        let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
        let executor_hub = ExecutorHub::new(workspace_root, connector.clone());
        let project_repo_port: Arc<dyn ProjectRepository> = project_repo.clone();
        let story_repo_port: Arc<dyn StoryRepository> = story_repo.clone();
        let task_repo_port: Arc<dyn TaskRepository> = task_repo.clone();
        reconcile_task_states_on_boot(
            &project_repo_port,
            &story_repo_port,
            &task_repo_port,
            &executor_hub,
        )
        .await?;

        let mcp_base_url = std::env::var("AGENTDASH_MCP_BASE_URL").ok().or_else(|| {
            let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
            Some(format!("http://127.0.0.1:{port}"))
        });

        Ok(Self {
            project_repo,
            workspace_repo,
            story_repo,
            task_repo: task_repo.clone(),
            sqlite_task_repo: task_repo,
            session_binding_repo,
            backend_repo,
            settings_repo,
            executor_hub,
            connector,
            mcp_base_url,
        })
    }
}

/// 从 Settings 读取单个字符串值，如果 Settings 中没有则返回 None
async fn read_setting_str(repo: &dyn SettingsRepository, key: &str) -> Option<String> {
    repo.get(key)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.value.as_str().map(String::from))
        .filter(|s| !s.is_empty())
}

/// 尝试构建 PiAgentConnector。
///
/// 配置优先级：Settings DB > 环境变量 > 默认值。
/// 支持 OpenAI Responses API（默认）和 Chat Completions API。
async fn build_pi_agent_connector(
    workspace_root: &std::path::Path,
    settings: &dyn SettingsRepository,
) -> Option<agentdash_executor::connectors::pi_agent::PiAgentConnector> {
    use agentdash_agent::{LlmBridge, RigBridge};
    use rig::client::CompletionClient as _;

    // 从 Settings 读取 OpenAI 系列配置
    let api_key = read_setting_str(settings, "llm.openai.api_key")
        .await
        .or_else(|| std::env::var("OPENAI_API_KEY").ok());
    let base_url = read_setting_str(settings, "llm.openai.base_url")
        .await
        .or_else(|| std::env::var("OPENAI_BASE_URL").ok());
    let model_id = read_setting_str(settings, "llm.openai.default_model")
        .await
        .unwrap_or_else(|| "gpt-4o".to_string());
    let wire_api = read_setting_str(settings, "llm.openai.wire_api")
        .await
        .unwrap_or_else(|| "responses".to_string());

    // Pi Agent 参数
    let system_prompt = read_setting_str(settings, "agent.pi.system_prompt")
        .await
        .or_else(|| std::env::var("PI_AGENT_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| {
            "你是 AgentDash 内置 AI 助手，一个通用的编程与任务执行 Agent。请用中文回复用户。"
                .to_string()
        });

    // --- 尝试 Anthropic（Settings → 环境变量）---
    let anthropic_key = read_setting_str(settings, "llm.anthropic.api_key")
        .await
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    if let Some(api_key) = anthropic_key {
        let client = rig::providers::anthropic::Client::new(&api_key);
        let anthropic_model = rig::providers::anthropic::completion::CLAUDE_4_SONNET;
        let model = client.completion_model(anthropic_model);
        let bridge: Arc<dyn LlmBridge> = Arc::new(RigBridge::new(model));

        let connector = agentdash_executor::connectors::pi_agent::PiAgentConnector::new(
            workspace_root.to_path_buf(),
            bridge,
            system_prompt,
            anthropic_model,
        );
        tracing::info!("PiAgentConnector 已初始化（Anthropic）");
        return Some(connector);
    }

    // --- 尝试 OpenAI/兼容端点 ---
    let api_key = api_key?;

    if wire_api != "responses" {
        tracing::warn!(
            "Rig 发行版当前统一走 Responses API，忽略 llm.openai.wire_api={} 配置",
            wire_api
        );
    }

    let mut builder = rig::providers::openai::Client::builder(&api_key);
    if let Some(ref url) = base_url {
        builder = builder.base_url(url);
    }
    let client = builder.build();
    let model = client.completion_model(&model_id);
    tracing::info!(
        "OpenAI Responses Client 已就绪（base_url={}, model={model_id}）",
        base_url.as_deref().unwrap_or("default")
    );
    let bridge: Arc<dyn LlmBridge> = Arc::new(RigBridge::new(model));

    let connector = agentdash_executor::connectors::pi_agent::PiAgentConnector::new(
        workspace_root.to_path_buf(),
        bridge,
        system_prompt,
        &model_id,
    );
    tracing::info!("PiAgentConnector 已初始化（OpenAI 兼容）");
    Some(connector)
}

async fn ensure_default_backend(backend_repo: &Arc<SqliteBackendRepository>) -> Result<()> {
    let backends = backend_repo
        .list_backends()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if !backends.is_empty() {
        return Ok(());
    }

    let local = BackendConfig {
        id: "local-default".to_string(),
        name: "本地后端".to_string(),
        endpoint: "http://127.0.0.1:3001".to_string(),
        auth_token: None,
        enabled: true,
        backend_type: BackendType::Local,
    };
    backend_repo
        .add_backend(&local)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}
