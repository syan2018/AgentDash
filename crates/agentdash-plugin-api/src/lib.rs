//! AgentDash 插件 SPI
//!
//! 本 crate 是开源核心仓库与企业私有扩展仓库之间的**唯一契约面**。
//! 只包含 trait 定义、类型声明和错误枚举，零业务实现。
//!
//! # 设计原则
//!
//! - 零运行时依赖（不引入 tokio/axum/sqlx）
//! - 不重新定义已有 trait，直接 re-export 已有抽象
//! - 企业扩展仓库只需依赖本 crate
//!
//! # 扩展点总览
//!
//! | 扩展点 | Trait | 说明 |
//! |--------|-------|------|
//! | 寻址空间 | `AddressSpaceProvider` | 新增可寻址资源类型 |
//! | 来源解析器 | `SourceResolver` | 新增 ContextSourceKind 解析逻辑 |
//! | Agent 连接器 | `AgentConnector` | 接入自定义 Agent 运行时 |
//! | 认证/授权 | `AuthProvider` | 企业 SSO/LDAP 等 |
//! | 外部服务 | `ExternalServiceClient` | 企业 KM、文档中心等只读内容源 |

pub mod auth;
pub mod external;
pub mod plugin;

// 复用已有 trait，不重新定义
pub use agentdash_domain::context_source::ContextSourceKind;
pub use agentdash_injection::{AddressSpaceDiscoveryProvider, SourceResolver};
pub use agentdash_spi::AgentConnector;

pub use auth::{
    AuthError, AuthGroup, AuthIdentity, AuthMode, AuthProvider, AuthRequest, LoginCredentials,
    LoginFieldDescriptor, LoginMetadata, LoginResponse,
};
pub use external::{
    ExternalServiceClient, ListOptions, ProviderCapabilities, ProviderError, ResourceContent,
    ResourceEntry, ResourceStat, SearchHit, SearchScope,
};
pub use plugin::{AgentDashPlugin, PluginError};

/// Mount I/O SPI — 供插件实现文件系统级操作。
///
/// 用法：`use agentdash_plugin_api::mount::{MountProvider, ReadResult, ...};`
pub use agentdash_spi::mount;
