use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 外部服务 Provider 错误 — 统一错误矩阵（对齐 03-19 PRD）
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// 凭证无效或权限不足（对应 HTTP 401/403）
    #[error("认证/权限错误: {0}")]
    AuthDenied(String),
    /// 目标资源不存在（对应 HTTP 404）
    #[error("资源不存在: {0}")]
    NotFound(String),
    /// Provider 请求超时（对应 HTTP 408/504）
    #[error("Provider 超时")]
    Timeout,
    /// Provider 状态冲突（对应 HTTP 409）
    #[error("Provider 状态冲突: {0}")]
    Conflict(String),
    /// 请求参数无效或选择器不支持（对应 HTTP 422）
    #[error("请求无效: {0}")]
    BadRequest(String),
    /// 能力不支持（对应 HTTP 501 或 capability miss）
    #[error("操作不支持: {0}")]
    OperationUnsupported(String),
    /// Provider 服务不可用（对应 HTTP 5xx）
    #[error("Provider 服务不可用: {0}")]
    ServiceUnavailable(String),
}

impl ProviderError {
    /// 此错误是否可以安全重试
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::Timeout | ProviderError::ServiceUnavailable(_)
        )
    }
}

/// Provider 能力声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider_id: String,
    pub display_name: String,
    /// 支持的操作列表（"list" / "read" / "stat" / "search"）
    pub supported_ops: Vec<String>,
    /// Provider 版本
    pub version: Option<String>,
}

/// 目录/文件列表选项
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// 最大返回条目数
    pub limit: Option<usize>,
    /// 分页游标
    pub cursor: Option<String>,
    /// 是否递归列出子目录
    pub recursive: bool,
}

/// 资源条目（file / directory 节点）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEntry {
    /// 相对路径（规范化，使用正斜杠）
    pub path: String,
    /// 节点类型
    pub entry_type: EntryType,
    /// 文件大小（字节）
    pub size: Option<u64>,
    /// 内容摘要标识（用于缓存验证）
    pub etag: Option<String>,
    /// 最后更新时间（ISO 8601）
    pub updated_at: Option<String>,
    /// 可选元数据
    pub metadata: Option<serde_json::Value>,
}

/// 节点类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    File,
    Directory,
}

/// 资源内容
#[derive(Debug, Clone)]
pub struct ResourceContent {
    /// 文本内容（与 `binary` 二选一）
    pub text: Option<String>,
    /// 二进制内容（与 `text` 二选一）
    pub binary: Option<Vec<u8>>,
    /// MIME 类型
    pub content_type: Option<String>,
    /// 内容摘要标识
    pub etag: Option<String>,
}

/// 资源元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStat {
    pub path: String,
    pub entry_type: EntryType,
    pub size: Option<u64>,
    pub etag: Option<String>,
    pub updated_at: Option<String>,
}

/// 搜索范围
#[derive(Debug, Clone, Default)]
pub struct SearchScope {
    /// 限定搜索的根路径（空表示全局）
    pub root_path: Option<String>,
    /// 最大返回结果数
    pub limit: Option<usize>,
}

/// 搜索命中条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub entry_type: EntryType,
    /// 匹配上下文摘要（可选）
    pub snippet: Option<String>,
    /// 相关度评分（0.0-1.0，可选）
    pub score: Option<f32>,
}

/// 外部服务客户端 — 对齐 03-19 PRD 的 provider service 契约
///
/// 企业通过实现此 trait 将 KM、文档中心、知识网关等接入统一 Address Space。
/// Agent 侧只看到 `mount + relative path`，不直接接触此接口。
///
/// # 首轮只读约束
///
/// 当前版本只定义读取能力（list/read/stat/search），不包含写入或执行操作。
#[async_trait]
pub trait ExternalServiceClient: Send + Sync {
    /// Provider 唯一标识（如 "corp-km"、"docs-center"）
    fn provider_id(&self) -> &str;

    /// 查询 Provider 能力声明
    async fn capabilities(&self) -> Result<ProviderCapabilities, ProviderError>;

    /// 列出指定路径下的资源条目
    async fn list(
        &self,
        path: &str,
        opts: &ListOptions,
    ) -> Result<Vec<ResourceEntry>, ProviderError>;

    /// 读取指定路径资源的内容
    async fn read(&self, path: &str) -> Result<ResourceContent, ProviderError>;

    /// 查询资源元信息（不读取内容）
    async fn stat(&self, path: &str) -> Result<ResourceStat, ProviderError>;

    /// 在指定范围内搜索资源
    async fn search(
        &self,
        query: &str,
        scope: &SearchScope,
    ) -> Result<Vec<SearchHit>, ProviderError>;
}
