//! 传输层集成
//!
//! 提供将 MCP Server 挂载到不同传输通道的辅助函数：
//! - Streamable HTTP（集成到现有 Axum 服务，面向 Relay / Story / Task 层）
//! - Stdio（面向 Agent 子进程，可用于后续独立进程模式）
//!
//! ## 架构
//!
//! ```text
//!                    ┌─────────────────────────────────────────────────┐
//!                    │              agentdash-api (Axum)               │
//!                    │                                                 │
//!  用户 / IDE  ──────┤  POST /mcp/relay        → RelayMcpServer       │
//!                    │  POST /mcp/story/{id}   → StoryMcpServer       │
//!                    │  POST /mcp/task/{id}    → TaskMcpServer        │
//!                    └─────────────────────────────────────────────────┘
//! ```

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    extract::Path,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::{any, get},
};
use rmcp::transport::{
    StreamableHttpServerConfig,
    streamable_http_server::{session::local::LocalSessionManager, tower::StreamableHttpService},
};
use uuid::Uuid;

use crate::servers::{RelayMcpServer, StoryMcpServer, TaskMcpServer, WorkflowMcpServer};
use crate::services::McpServices;

/// 创建 Relay 层的 Streamable HTTP 服务
pub fn create_relay_http_service(
    services: Arc<McpServices>,
) -> StreamableHttpService<RelayMcpServer> {
    StreamableHttpService::new(
        move || Ok(RelayMcpServer::new(services.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
}

fn create_story_http_service(
    services: Arc<McpServices>,
    project_id: Uuid,
    story_id: Uuid,
) -> StreamableHttpService<StoryMcpServer> {
    StreamableHttpService::new(
        move || Ok(StoryMcpServer::new(services.clone(), project_id, story_id)),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
}

fn create_task_http_service(
    services: Arc<McpServices>,
    project_id: Uuid,
    story_id: Uuid,
    task_id: Uuid,
) -> StreamableHttpService<TaskMcpServer> {
    StreamableHttpService::new(
        move || {
            Ok(TaskMcpServer::new(
                services.clone(),
                project_id,
                story_id,
                task_id,
            ))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
}

/// 通过 stdio 服务 StoryMcpServer
pub async fn serve_story_via_stdio(
    services: Arc<McpServices>,
    project_id: Uuid,
    story_id: Uuid,
) -> Result<(), rmcp::RmcpError> {
    use rmcp::{ServiceExt, transport::stdio};

    let server = StoryMcpServer::new(services, project_id, story_id);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// 通过 stdio 服务 TaskMcpServer
pub async fn serve_task_via_stdio(
    services: Arc<McpServices>,
    project_id: Uuid,
    story_id: Uuid,
    task_id: Uuid,
) -> Result<(), rmcp::RmcpError> {
    use rmcp::{ServiceExt, transport::stdio};

    let server = TaskMcpServer::new(services, project_id, story_id, task_id);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[derive(Clone)]
struct McpHttpRouterState {
    services: Arc<McpServices>,
    story_services: Arc<Mutex<HashMap<Uuid, StreamableHttpService<StoryMcpServer>>>>,
    task_services: Arc<Mutex<HashMap<Uuid, StreamableHttpService<TaskMcpServer>>>>,
    workflow_services: Arc<Mutex<HashMap<Uuid, StreamableHttpService<WorkflowMcpServer>>>>,
}

impl McpHttpRouterState {
    fn new(services: Arc<McpServices>) -> Self {
        Self {
            services,
            story_services: Arc::new(Mutex::new(HashMap::new())),
            task_services: Arc::new(Mutex::new(HashMap::new())),
            workflow_services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn story_service(
        &self,
        story_id: Uuid,
    ) -> Result<StreamableHttpService<StoryMcpServer>, (StatusCode, String)> {
        if let Some(service) = self
            .story_services
            .lock()
            .expect("story service cache lock poisoned")
            .get(&story_id)
            .cloned()
        {
            return Ok(service);
        }

        let story = self
            .services
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(|error| {
                tracing::error!(%story_id, ?error, "加载 Story 以建立 MCP HTTP 服务失败");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("加载 Story 失败: {error}"),
                )
            })?
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Story 不存在: {story_id}")))?;

        let service = create_story_http_service(self.services.clone(), story.project_id, story_id);
        let mut guard = self
            .story_services
            .lock()
            .expect("story service cache lock poisoned");

        Ok(guard
            .entry(story_id)
            .or_insert_with(|| service.clone())
            .clone())
    }

    async fn task_service(
        &self,
        task_id: Uuid,
    ) -> Result<StreamableHttpService<TaskMcpServer>, (StatusCode, String)> {
        if let Some(service) = self
            .task_services
            .lock()
            .expect("task service cache lock poisoned")
            .get(&task_id)
            .cloned()
        {
            return Ok(service);
        }

        // M1-b：Task 查询经 Story aggregate（find_by_task_id 一步拿到 Story）
        let story = self
            .services
            .story_repo
            .find_by_task_id(task_id)
            .await
            .map_err(|error| {
                tracing::error!(%task_id, ?error, "加载 Task 以建立 MCP HTTP 服务失败");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("加载 Task 失败: {error}"),
                )
            })?
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Task 不存在: {task_id}")))?;
        let task = story
            .find_task(task_id)
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Task 不存在: {task_id}")))?;

        let service = create_task_http_service(
            self.services.clone(),
            story.project_id,
            task.story_id,
            task.id,
        );
        let mut guard = self
            .task_services
            .lock()
            .expect("task service cache lock poisoned");

        Ok(guard
            .entry(task_id)
            .or_insert_with(|| service.clone())
            .clone())
    }

    fn workflow_service(
        &self,
        project_id: Uuid,
    ) -> Result<StreamableHttpService<WorkflowMcpServer>, (StatusCode, String)> {
        if let Some(service) = self
            .workflow_services
            .lock()
            .expect("workflow service cache lock poisoned")
            .get(&project_id)
            .cloned()
        {
            return Ok(service);
        }

        let service = StreamableHttpService::new(
            {
                let services = self.services.clone();
                move || Ok(WorkflowMcpServer::new(services.clone(), project_id))
            },
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );

        let mut guard = self
            .workflow_services
            .lock()
            .expect("workflow service cache lock poisoned");

        Ok(guard
            .entry(project_id)
            .or_insert_with(|| service.clone())
            .clone())
    }
}

async fn handle_story_mcp(
    state: Arc<McpHttpRouterState>,
    story_id: Uuid,
    request: Request<Body>,
) -> impl IntoResponse {
    match state.story_service(story_id).await {
        Ok(service) => service.handle(request).await.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_task_mcp(
    state: Arc<McpHttpRouterState>,
    task_id: Uuid,
    request: Request<Body>,
) -> impl IntoResponse {
    match state.task_service(task_id).await {
        Ok(service) => service.handle(request).await.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_workflow_mcp(
    state: Arc<McpHttpRouterState>,
    project_id: Uuid,
    request: Request<Body>,
) -> impl IntoResponse {
    match state.workflow_service(project_id) {
        Ok(service) => service.handle(request).await.into_response(),
        Err(error) => error.into_response(),
    }
}

/// MCP 路由构建器
pub struct McpRouterBuilder {
    services: Arc<McpServices>,
}

impl McpRouterBuilder {
    pub fn new(services: Arc<McpServices>) -> Self {
        Self { services }
    }

    /// 构建 MCP 路由子树
    pub fn build(self) -> axum::Router {
        let relay_service = create_relay_http_service(self.services.clone());
        let http_state = Arc::new(McpHttpRouterState::new(self.services));

        axum::Router::new()
            .nest_service("/mcp/relay", relay_service)
            .route("/mcp/health", get(mcp_health_check))
            .route(
                "/mcp/story/{story_id}",
                any({
                    let state = http_state.clone();
                    move |Path(story_id): Path<Uuid>, request: Request<Body>| {
                        let state = state.clone();
                        async move { handle_story_mcp(state, story_id, request).await }
                    }
                }),
            )
            .route(
                "/mcp/task/{task_id}",
                any({
                    let state = http_state.clone();
                    move |Path(task_id): Path<Uuid>, request: Request<Body>| {
                        let state = state.clone();
                        async move { handle_task_mcp(state, task_id, request).await }
                    }
                }),
            )
            .route(
                "/mcp/workflow/{project_id}",
                any({
                    let state = http_state.clone();
                    move |Path(project_id): Path<Uuid>, request: Request<Body>| {
                        let state = state.clone();
                        async move { handle_workflow_mcp(state, project_id, request).await }
                    }
                }),
            )
    }
}

async fn mcp_health_check() -> &'static str {
    "MCP OK"
}
