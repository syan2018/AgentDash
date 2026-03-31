//! 认证身份数据类型。
//!
//! `AuthIdentity` / `AuthMode` / `AuthGroup` 是纯数据结构，仅依赖 serde，
//! 定义在 SPI 层以便 `MountOperationContext`、`ExecutionContext` 等跨层契约
//! 能直接引用操作者身份，无需再从 plugin-api 拉取。
//!
//! `AuthProvider` trait 等行为契约仍留在 `agentdash-plugin-api`。

use std::str::FromStr;

use serde::{Deserialize, Serialize};

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
