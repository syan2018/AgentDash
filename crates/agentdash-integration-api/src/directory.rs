use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 通用身份目录中的用户投影。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryUser {
    pub user_id: String,
    pub subject: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub provider: Option<String>,
    pub source: Option<String>,
}

/// 通用身份目录中的用户组 / 组织投影。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryGroup {
    pub group_id: String,
    pub display_name: Option<String>,
    pub path: Option<String>,
    pub provider: Option<String>,
    pub source: Option<String>,
}

/// 目录分页搜索响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectorySearchResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub source: Option<String>,
    pub is_projection_only: bool,
}

/// 目录树节点，用于组织树逐层浏览。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryTreeNode {
    pub group_id: String,
    pub display_name: Option<String>,
    pub path: Option<String>,
    pub has_children: bool,
    pub children: Option<Vec<DirectoryTreeNode>>,
    pub provider: Option<String>,
    pub source: Option<String>,
}

/// 搜索目录的通用请求。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectorySearchRequest {
    pub query: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
}

/// 解析单个目录主体的请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryResolveRequest {
    pub key: String,
}

/// 组织树 children/listing 请求。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryTreeRequest {
    pub parent_group_id: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DirectoryProviderError {
    #[error("目录请求格式错误: {0}")]
    BadRequest(String),
    #[error("目录主体未找到: {kind} {key}")]
    NotFound { kind: &'static str, key: String },
    #[error("目录服务不可用: {0}")]
    Unavailable(String),
    #[error("目录服务错误: {0}")]
    Internal(String),
}

/// 企业目录 Provider。
///
/// 通用层只表达搜索、解析和组织树浏览能力；具体 IAM、LDAP、People API 等协议
/// 必须由私有集成层适配为这些中性 DTO。
#[async_trait]
pub trait IdentityDirectoryProvider: Send + Sync {
    async fn search_users(
        &self,
        request: DirectorySearchRequest,
    ) -> Result<DirectorySearchResponse<DirectoryUser>, DirectoryProviderError>;

    async fn search_groups(
        &self,
        request: DirectorySearchRequest,
    ) -> Result<DirectorySearchResponse<DirectoryGroup>, DirectoryProviderError>;

    async fn resolve_user(
        &self,
        request: DirectoryResolveRequest,
    ) -> Result<DirectoryUser, DirectoryProviderError>;

    async fn resolve_group(
        &self,
        request: DirectoryResolveRequest,
    ) -> Result<DirectoryGroup, DirectoryProviderError>;

    async fn list_group_children(
        &self,
        request: DirectoryTreeRequest,
    ) -> Result<DirectorySearchResponse<DirectoryTreeNode>, DirectoryProviderError>;
}
