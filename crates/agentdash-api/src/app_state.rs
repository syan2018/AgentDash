use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

use crate::bootstrap::task_state_reconcile::reconcile_task_states_on_boot;
use agentdash_domain::backend::{BackendConfig, BackendRepository, BackendType};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workspace::WorkspaceRepository;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;
use agentdash_executor::{AgentConnector, ExecutorHub};
use agentdash_infrastructure::{
    SqliteBackendRepository, SqliteProjectRepository, SqliteSessionBindingRepository,
    SqliteStoryRepository, SqliteTaskRepository, SqliteWorkspaceRepository,
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

        let backend_repo = Arc::new(SqliteBackendRepository::new(pool));
        backend_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        ensure_default_backend(&backend_repo).await?;

        let workspace_root = std::env::current_dir()?;

        let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();
        sub_connectors.push(Arc::new(VibeKanbanExecutorsConnector::new(
            workspace_root.clone(),
        )));

        if let Some(pi_connector) = build_pi_agent_connector(&workspace_root) {
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

        let mcp_base_url = std::env::var("AGENTDASH_MCP_BASE_URL").ok();

        Ok(Self {
            project_repo,
            workspace_repo,
            story_repo,
            task_repo: task_repo.clone(),
            sqlite_task_repo: task_repo,
            session_binding_repo,
            backend_repo,
            executor_hub,
            connector,
            mcp_base_url,
        })
    }
}

/// 尝试构建 PiAgentConnector（需要有效的 LLM API Key）。
/// 按优先级依次检查：ANTHROPIC_API_KEY → OPENAI_API_KEY。
/// 若都未配置则返回 None，不影响服务启动。
fn build_pi_agent_connector(
    workspace_root: &std::path::Path,
) -> Option<agentdash_executor::connectors::pi_agent::PiAgentConnector> {
    use agentdash_agent::{LlmBridge, RigBridge};
    use rig::client::CompletionClient as _;
    use rig::providers::anthropic;

    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        let client = anthropic::Client::new(&api_key)
            .inspect_err(|e| tracing::warn!("Anthropic Client 初始化失败: {e}"))
            .ok()?;
        let model = client.completion_model(anthropic::completion::CLAUDE_4_SONNET);
        let bridge: Arc<dyn LlmBridge> = Arc::new(RigBridge::new(model));

        let system_prompt = std::env::var("PI_AGENT_SYSTEM_PROMPT").unwrap_or_else(|_| {
            "你是 AgentDash 内置 AI 助手，一个通用的编程与任务执行 Agent。请用中文回复用户。"
                .to_string()
        });

        let connector = agentdash_executor::connectors::pi_agent::PiAgentConnector::new(
            workspace_root.to_path_buf(),
            bridge,
            system_prompt,
        );

        tracing::info!("PiAgentConnector 已初始化（Anthropic）");
        return Some(connector);
    }

    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let client = rig::providers::openai::Client::new(&api_key)
            .inspect_err(|e| tracing::warn!("OpenAI Client 初始化失败: {e}"))
            .ok()?;
        let model = client.completion_model("gpt-4o");
        let bridge: Arc<dyn LlmBridge> = Arc::new(RigBridge::new(model));

        let system_prompt = std::env::var("PI_AGENT_SYSTEM_PROMPT").unwrap_or_else(|_| {
            "你是 AgentDash 内置 AI 助手，一个通用的编程与任务执行 Agent。请用中文回复用户。"
                .to_string()
        });

        let connector = agentdash_executor::connectors::pi_agent::PiAgentConnector::new(
            workspace_root.to_path_buf(),
            bridge,
            system_prompt,
        );

        tracing::info!("PiAgentConnector 已初始化（OpenAI）");
        return Some(connector);
    }

    tracing::info!(
        "未检测到 ANTHROPIC_API_KEY 或 OPENAI_API_KEY，PiAgentConnector 不可用"
    );
    None
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
