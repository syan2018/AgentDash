use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use agentdash_spi::auth::{AuthGroup, AuthIdentity, AuthMode};

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

impl AuthRequest {
    /// 按小写 key 读取请求头。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// 读取查询参数。
    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query_params.get(name).map(String::as_str)
    }
}

/// 登录凭证 — 用户向 AuthProvider 提交的认证信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
    /// 扩展字段（如 MFA code、client_ip 等）
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// 登录成功的响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginResponse {
    /// 用于后续请求认证的 token
    pub access_token: String,
    /// 已认证的身份信息
    pub identity: AuthIdentity,
}

/// 登录表单字段描述 — 供前端渲染
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginFieldDescriptor {
    /// 字段名（如 "username"、"password"）
    pub name: String,
    /// 显示标签（如 "域账号"、"密码"）
    pub label: String,
    /// 输入类型（"text" / "password" / "email"）
    #[serde(default = "default_field_type")]
    pub field_type: String,
    /// 占位文本
    pub placeholder: Option<String>,
    /// 是否必填
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_field_type() -> String {
    "text".to_string()
}
fn default_true() -> bool {
    true
}

/// 登录元数据 — 描述认证方式，供前端渲染登录页面
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginMetadata {
    /// 认证方式标识（如 "ldap"、"oauth2"、"personal"）
    pub provider_type: String,
    /// 显示名称（如 "LDAP 域账号登录"）
    pub display_name: String,
    /// 描述文字
    pub description: Option<String>,
    /// 登录表单需要的字段列表
    pub fields: Vec<LoginFieldDescriptor>,
    /// 是否需要交互式登录（false = 无需登录页，如 personal 模式）
    pub requires_login: bool,
}

/// 认证与授权提供者
///
/// 企业版通过实现此 trait 接入 SSO/LDAP/OAuth2 等认证体系。
/// 个人模式也应通过该 trait 提供固定/本地身份，而不是绕过请求链路。
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// 从请求中提取并验证身份
    ///
    /// 返回 `Err(AuthError::InvalidCredentials)` 表示身份验证失败，框架返回 401。
    async fn authenticate(&self, req: &AuthRequest) -> Result<AuthIdentity, AuthError>;

    /// 对已认证身份执行 Provider 级粗粒度授权
    ///
    /// 该方法适合做 claim / provider 级的粗粒度限制，例如：
    /// - 企业代理头已声明该用户不是有效组织成员
    /// - 某身份仅允许访问一部分顶层入口
    ///
    /// 领域级授权（Project grants、共享、owner/editor/viewer 等）应由宿主应用层负责。
    ///
    /// 返回 `Ok(false)` 时框架返回 403。
    async fn authorize(
        &self,
        identity: &AuthIdentity,
        resource: &str,
        action: &str,
    ) -> Result<bool, AuthError>;

    /// 执行交互式登录（用户提交凭证换取 token + 身份信息）
    ///
    /// 对于不需要交互式登录的 provider（如 personal 模式），
    /// 默认实现返回 `BadRequest`。宿主根据 `login_metadata().requires_login`
    /// 判断是否挂载登录路由。
    async fn login(&self, _credentials: &LoginCredentials) -> Result<LoginResponse, AuthError> {
        Err(AuthError::BadRequest(
            "该认证模式不支持交互式登录".to_string(),
        ))
    }

    /// 返回登录方式元数据，供前端渲染登录表单
    ///
    /// 默认实现返回 personal 模式描述（不需要登录页）。
    fn login_metadata(&self) -> LoginMetadata {
        LoginMetadata {
            provider_type: "none".to_string(),
            display_name: "个人模式".to_string(),
            description: Some("无需登录，使用固定本地身份".to_string()),
            fields: vec![],
            requires_login: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_auth_modes() {
        assert_eq!("personal".parse::<AuthMode>().unwrap(), AuthMode::Personal);
        assert_eq!(
            "enterprise".parse::<AuthMode>().unwrap(),
            AuthMode::Enterprise
        );
    }

    #[test]
    fn rejects_unknown_auth_mode() {
        let err = "unknown".parse::<AuthMode>().expect_err("未知模式应失败");
        assert!(err.contains("personal / enterprise"));
    }

    #[test]
    fn reads_headers_case_insensitively() {
        let mut headers = HashMap::new();
        headers.insert("x-user-id".to_string(), "alice".to_string());

        let request = AuthRequest {
            headers,
            query_params: HashMap::new(),
            path: "/api/me".to_string(),
            method: "GET".to_string(),
        };

        assert_eq!(request.header("X-User-Id"), Some("alice"));
    }
}
