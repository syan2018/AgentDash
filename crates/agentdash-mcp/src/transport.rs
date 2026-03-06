//! 传输层集成
//!
//! 提供将 MCP Server 挂载到不同传输通道的辅助函数：
//! - Streamable HTTP（集成到现有 Axum 服务，面向 Relay 层）
//! - Stdio（面向 Agent 子进程，用于 Story/Task 层）
//!
//! ## 架构
//!
//! ```text
//!                    ┌─────────────────────────────────────────────────┐
//!                    │              agentdash-api (Axum)               │
//!                    │                                                 │
//!  用户 / IDE  ──────┤  POST /mcp/relay  → StreamableHttpService      │
//!                    │                      (RelayMcpServer)           │
//!                    │                                                 │
//!                    │  内部 spawn ─────→ StoryMcpServer.serve(stdio)  │
//!                    │                     ↕ Agent 子进程              │
//!                    │                                                 │
//!                    │  内部 spawn ─────→ TaskMcpServer.serve(stdio)   │
//!                    │                     ↕ Agent 子进程              │
//!                    └─────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;

use rmcp::transport::{
    StreamableHttpServerConfig,
    streamable_http_server::{session::local::LocalSessionManager, tower::StreamableHttpService},
};
use uuid::Uuid;

use crate::servers::{RelayMcpServer, StoryMcpServer, TaskMcpServer};
use crate::services::McpServices;

/// 创建 Relay 层的 Streamable HTTP 服务
///
/// 返回的 `StreamableHttpService` 实现了 Tower `Service` trait，
/// 可直接通过 `axum::Router::nest_service` 挂载到路由树。
///
/// ```rust,ignore
/// let relay_service = create_relay_http_service(services.clone());
/// let router = Router::new()
///     .nest_service("/mcp/relay", relay_service);
/// ```
pub fn create_relay_http_service(
    services: Arc<McpServices>,
) -> StreamableHttpService<RelayMcpServer> {
    StreamableHttpService::new(
        move || Ok(RelayMcpServer::new(services.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
}

/// 通过 stdio 服务 StoryMcpServer
///
/// 阻塞当前 async 上下文直到连接关闭。
/// 典型用法：Agent 子进程启动时，在 main 中调用此函数。
///
/// ```rust,ignore
/// serve_story_via_stdio(services, project_id, story_id).await?;
/// ```
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
///
/// 阻塞当前 async 上下文直到连接关闭。
/// 典型用法：Agent 子进程启动时，在 main 中调用此函数。
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

/// MCP 路由构建器
///
/// 辅助将 MCP 服务挂载到现有 Axum Router。
/// 提供统一的路由前缀和中间件配置入口。
///
/// ## 路由结构
///
/// ```text
/// /mcp/
///   ├── relay    → Streamable HTTP (RelayMcpServer)
///   └── health   → MCP 服务健康检查
/// ```
///
/// Story/Task 层的 MCP Server 通过 stdio 传输提供给 Agent 子进程，
/// 不直接挂载到 HTTP 路由。
pub struct McpRouterBuilder {
    services: Arc<McpServices>,
}

impl McpRouterBuilder {
    pub fn new(services: Arc<McpServices>) -> Self {
        Self { services }
    }

    /// 构建 MCP 路由子树
    ///
    /// 返回一个可以通过 `Router::merge` 或 `Router::nest` 挂载的路由。
    /// 调用方负责添加认证中间件等。
    pub fn build(self) -> axum::Router {
        use axum::{Router, routing::get};

        let relay_service = create_relay_http_service(self.services);

        Router::new()
            .nest_service("/mcp/relay", relay_service)
            .route("/mcp/health", get(mcp_health_check))
    }
}

async fn mcp_health_check() -> &'static str {
    "MCP OK"
}
