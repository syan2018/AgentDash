use std::collections::HashMap;
use std::str::FromStr;

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

/// 当前认证模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// 个人使用模式，通常对应单个固定用户
    Personal,
    /// 企业接入模式，身份通常来自企业 SSO / 代理头 / Token 校验
    Enterprise,
}

impl AuthMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Enterprise => "enterprise",
        }
    }
}

impl std::fmt::Display for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AuthMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "personal" => Ok(Self::Personal),
            "enterprise" => Ok(Self::Enterprise),
            other => Err(format!(
                "不支持的认证模式 `{other}`，仅支持 personal / enterprise"
            )),
        }
    }
}

/// 用户组声明
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthGroup {
    /// 用户组唯一标识
    pub group_id: String,
    /// 可读名称（可选）
    pub display_name: Option<String>,
}

/// 认证后的用户身份
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthIdentity {
    /// 当前认证模式
    pub auth_mode: AuthMode,
    /// 用户唯一标识
    pub user_id: String,
    /// Provider 返回的原始主体标识（通常来自 claim / token subject）
    pub subject: String,
    /// 可读显示名称
    pub display_name: Option<String>,
    /// 用户邮箱
    pub email: Option<String>,
    /// claim 投影得到的用户组列表
    #[serde(default)]
    pub groups: Vec<AuthGroup>,
    /// 是否具备管理员旁路权限
    pub is_admin: bool,
    /// 产生该身份的 provider 标识
    pub provider: Option<String>,
    /// 扩展字段（由具体 Provider 自定义）
    #[serde(default)]
    pub extra: serde_json::Value,
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
