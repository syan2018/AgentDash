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

use crate::authz::{McpProjectPermission, require_project_permission};
use crate::error::McpError;
use crate::servers::{RelayMcpServer, StoryMcpServer, TaskMcpServer, WorkflowMcpServer};
use crate::services::McpServices;
use agentdash_spi::platform::auth::AuthIdentity;

type McpHttpService<S> = StreamableHttpService<S, LocalSessionManager>;
type McpServiceCache<K, S> = Arc<Mutex<HashMap<K, McpHttpService<S>>>>;
type UserServiceKey = String;
type ProjectScopedServiceKey = (String, Uuid);

/// 创建 Relay 层的 Streamable HTTP 服务
pub fn create_relay_http_service(
    services: Arc<McpServices>,
    identity: AuthIdentity,
) -> McpHttpService<RelayMcpServer> {
    StreamableHttpService::new(
        move || Ok(RelayMcpServer::new(services.clone(), identity.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}

fn create_story_http_service(
    services: Arc<McpServices>,
    project_id: Uuid,
    story_id: Uuid,
    identity: AuthIdentity,
) -> McpHttpService<StoryMcpServer> {
    StreamableHttpService::new(
        move || {
            Ok(StoryMcpServer::new(
                services.clone(),
                project_id,
                story_id,
                identity.clone(),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}

fn create_task_http_service(
    services: Arc<McpServices>,
    project_id: Uuid,
    story_id: Uuid,
    task_id: Uuid,
    identity: AuthIdentity,
) -> McpHttpService<TaskMcpServer> {
    StreamableHttpService::new(
        move || {
            Ok(TaskMcpServer::new(
                services.clone(),
                project_id,
                story_id,
                task_id,
                identity.clone(),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}

/// 通过 stdio 服务 StoryMcpServer
pub async fn serve_story_via_stdio(
    services: Arc<McpServices>,
    project_id: Uuid,
    story_id: Uuid,
    identity: AuthIdentity,
) -> Result<(), rmcp::RmcpError> {
    use rmcp::{ServiceExt, transport::stdio};

    let server = StoryMcpServer::new(services, project_id, story_id, identity);
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
    identity: AuthIdentity,
) -> Result<(), rmcp::RmcpError> {
    use rmcp::{ServiceExt, transport::stdio};

    let server = TaskMcpServer::new(services, project_id, story_id, task_id, identity);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[derive(Clone)]
struct McpHttpRouterState {
    services: Arc<McpServices>,
    relay_services: McpServiceCache<UserServiceKey, RelayMcpServer>,
    story_services: McpServiceCache<ProjectScopedServiceKey, StoryMcpServer>,
    task_services: McpServiceCache<ProjectScopedServiceKey, TaskMcpServer>,
    workflow_services: McpServiceCache<ProjectScopedServiceKey, WorkflowMcpServer>,
}

impl McpHttpRouterState {
    fn new(services: Arc<McpServices>) -> Self {
        Self {
            services,
            relay_services: Arc::new(Mutex::new(HashMap::new())),
            story_services: Arc::new(Mutex::new(HashMap::new())),
            task_services: Arc::new(Mutex::new(HashMap::new())),
            workflow_services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn relay_service(
        &self,
        identity: &AuthIdentity,
    ) -> Result<McpHttpService<RelayMcpServer>, (StatusCode, String)> {
        let key = identity.user_id.clone();
        if let Some(service) = self
            .relay_services
            .lock()
            .expect("relay service cache lock poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(service);
        }

        let service = create_relay_http_service(self.services.clone(), identity.clone());
        let mut guard = self
            .relay_services
            .lock()
            .expect("relay service cache lock poisoned");

        Ok(guard.entry(key).or_insert_with(|| service.clone()).clone())
    }

    async fn story_service(
        &self,
        identity: &AuthIdentity,
        story_id: Uuid,
    ) -> Result<McpHttpService<StoryMcpServer>, (StatusCode, String)> {
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
        require_project_permission(
            &self.services,
            identity,
            story.project_id,
            McpProjectPermission::View,
        )
        .await
        .map_err(mcp_error_response)?;

        let key = (identity.user_id.clone(), story_id);
        if let Some(service) = self
            .story_services
            .lock()
            .expect("story service cache lock poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(service);
        }

        let service = create_story_http_service(
            self.services.clone(),
            story.project_id,
            story_id,
            identity.clone(),
        );
        let mut guard = self
            .story_services
            .lock()
            .expect("story service cache lock poisoned");

        Ok(guard.entry(key).or_insert_with(|| service.clone()).clone())
    }

    async fn task_service(
        &self,
        identity: &AuthIdentity,
        task_id: Uuid,
    ) -> Result<McpHttpService<TaskMcpServer>, (StatusCode, String)> {
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
        require_project_permission(
            &self.services,
            identity,
            story.project_id,
            McpProjectPermission::View,
        )
        .await
        .map_err(mcp_error_response)?;

        let key = (identity.user_id.clone(), task_id);
        if let Some(service) = self
            .task_services
            .lock()
            .expect("task service cache lock poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(service);
        }

        let service = create_task_http_service(
            self.services.clone(),
            story.project_id,
            task.story_id,
            task.id,
            identity.clone(),
        );
        let mut guard = self
            .task_services
            .lock()
            .expect("task service cache lock poisoned");

        Ok(guard.entry(key).or_insert_with(|| service.clone()).clone())
    }

    async fn workflow_service(
        &self,
        identity: &AuthIdentity,
        project_id: Uuid,
    ) -> Result<McpHttpService<WorkflowMcpServer>, (StatusCode, String)> {
        require_project_permission(
            &self.services,
            identity,
            project_id,
            McpProjectPermission::View,
        )
        .await
        .map_err(mcp_error_response)?;

        let key = (identity.user_id.clone(), project_id);
        if let Some(service) = self
            .workflow_services
            .lock()
            .expect("workflow service cache lock poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(service);
        }

        let service = StreamableHttpService::new(
            {
                let services = self.services.clone();
                let identity = identity.clone();
                move || {
                    Ok(WorkflowMcpServer::new(
                        services.clone(),
                        project_id,
                        identity.clone(),
                    ))
                }
            },
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );

        let mut guard = self
            .workflow_services
            .lock()
            .expect("workflow service cache lock poisoned");

        Ok(guard.entry(key).or_insert_with(|| service.clone()).clone())
    }
}

fn request_identity(request: &Request<Body>) -> Result<AuthIdentity, (StatusCode, String)> {
    request
        .extensions()
        .get::<AuthIdentity>()
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "MCP 请求缺少有效认证身份".to_string(),
            )
        })
}

fn mcp_error_response(error: McpError) -> (StatusCode, String) {
    let status = match &error {
        McpError::NotFound { .. } => StatusCode::NOT_FOUND,
        McpError::Forbidden { .. } | McpError::ScopeMismatch { .. } => StatusCode::FORBIDDEN,
        McpError::InvalidParam { .. } => StatusCode::BAD_REQUEST,
        McpError::Domain(_) | McpError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}

async fn handle_relay_mcp(
    state: Arc<McpHttpRouterState>,
    request: Request<Body>,
) -> impl IntoResponse {
    let identity = match request_identity(&request) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };

    match state.relay_service(&identity).await {
        Ok(service) => service.handle(request).await.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_story_mcp(
    state: Arc<McpHttpRouterState>,
    story_id: Uuid,
    request: Request<Body>,
) -> impl IntoResponse {
    let identity = match request_identity(&request) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };

    match state.story_service(&identity, story_id).await {
        Ok(service) => service.handle(request).await.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_task_mcp(
    state: Arc<McpHttpRouterState>,
    task_id: Uuid,
    request: Request<Body>,
) -> impl IntoResponse {
    let identity = match request_identity(&request) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };

    match state.task_service(&identity, task_id).await {
        Ok(service) => service.handle(request).await.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_workflow_mcp(
    state: Arc<McpHttpRouterState>,
    project_id: Uuid,
    request: Request<Body>,
) -> impl IntoResponse {
    let identity = match request_identity(&request) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };

    match state.workflow_service(&identity, project_id).await {
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
        let http_state = Arc::new(McpHttpRouterState::new(self.services));

        axum::Router::new()
            .route("/mcp/health", get(mcp_health_check))
            .route(
                "/mcp/relay",
                any({
                    let state = http_state.clone();
                    move |request: Request<Body>| {
                        let state = state.clone();
                        async move { handle_relay_mcp(state, request).await }
                    }
                }),
            )
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
