use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 认证错误
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("凭证无效或已过期")]
    InvalidCredentials,
    #[error("权限不足: {0}")]
    Forbidden(String),
    #[error("认证服务不可用: {0}")]
    ServiceUnavailable(String),
    #[error("认证请求格式错误: {0}")]
    BadRequest(String),
}

/// 认证请求 — 从 HTTP 请求中提取的原始信息
///
/// 刻意不依赖 axum/http 类型，由中间件适配层完成转换。
#[derive(Debug, Clone)]
pub struct AuthRequest {
    /// 请求头（key 已规范化为小写）
    pub headers: HashMap<String, String>,
    /// 查询参数
    pub query_params: HashMap<String, String>,
    /// 请求路径
    pub path: String,
    /// HTTP 方法
    pub method: String,
}

/// 认证后的用户身份
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthIdentity {
    /// 用户唯一标识
    pub user_id: String,
    /// 可读显示名称
    pub display_name: Option<String>,
    /// 角色列表（用于粗粒度权限判断）
    pub roles: Vec<String>,
    /// 租户标识（多租户场景）
    pub tenant_id: Option<String>,
    /// 扩展字段（由具体 Provider 自定义）
    pub extra: serde_json::Value,
}

/// 认证与授权提供者
///
/// 企业版通过实现此 trait 接入 SSO/LDAP/OAuth2 等认证体系。
/// 框架在挂载认证中间件时调用 `authenticate`，在路由处理前调用 `authorize`。
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// 从请求中提取并验证身份
    ///
    /// 返回 `Err(AuthError::InvalidCredentials)` 表示身份验证失败，框架返回 401。
    async fn authenticate(&self, req: &AuthRequest) -> Result<AuthIdentity, AuthError>;

    /// 检查已验证身份是否有权访问指定资源
    ///
    /// - `resource`: 资源标识，如 `"/api/projects"` 或 `"project:uuid"`
    /// - `action`: 操作标识，如 `"read"` / `"write"`
    ///
    /// 返回 `Ok(false)` 时框架返回 403。
    async fn authorize(
        &self,
        identity: &AuthIdentity,
        resource: &str,
        action: &str,
    ) -> Result<bool, AuthError>;
}
