use std::path::PathBuf;
use std::sync::Arc;

use agentdash_domain::context_source::ContextSourceKind;
use agentdash_spi::MarketplaceSourceProvider;
use agentdash_spi::MemoryDiscoveryProvider;
use agentdash_spi::RoutineTriggerProvider;
use agentdash_spi::SkillDiscoveryProvider;
use agentdash_spi::platform::mount::MountProvider;
use agentdash_spi::{SourceResolver, VfsDiscoveryProvider};

use crate::auth::AuthProvider;
use crate::directory::IdentityDirectoryProvider;
use crate::external::ExternalServiceClient;
pub use agentdash_domain::shared_library::{IntegrationLibraryAssetSeed, LibraryAssetType};

/// Host Integration 错误
#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("Host Integration 初始化失败: {0}")]
    InitFailed(String),
    #[error("Host Integration 关闭失败: {0}")]
    ShutdownFailed(String),
}

/// AgentDash Host Integration 入口
///
/// 每个集成通过实现此 trait 向运行时注册自己的宿主级能力。
/// 所有方法均提供默认空实现，集成只需覆盖需要扩展的方法。
/// 但要注意：`trait` 上存在的方法并不等于该扩展点已经是稳定外部合同。
/// 当前宿主会优先保证已闭环、已验证的能力真实生效；其余扩展点可能仍处于实验阶段。
///
/// # 示例（企业私有仓库中）
///
/// ```rust,ignore
/// pub struct CorpIntegration;
///
/// impl AgentDashIntegration for CorpIntegration {
///     fn name(&self) -> &str { "corp" }
///
///     fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> {
///         Some(Box::new(CorpSsoAuthProvider::new()))
///     }
///
///     fn vfs_providers(&self) -> Vec<Box<dyn VfsProvider>> {
///         vec![Box::new(CorpKmProvider::new())]
///     }
/// }
/// ```
pub trait AgentDashIntegration: Send + Sync {
    /// 集成名称（用于日志和诊断）
    fn name(&self) -> &str;

    /// 贡献受信的 Complete Agent definition、instance、placement requirement 与 factory。
    ///
    /// Factory 只产出最终 `CompleteAgentService` 边界；Host 在 composition root 中归一
    /// placement、health、credential 与 offer evidence。集成不能声明默认成功或 fallback。
    fn complete_agent_registrations(&self) -> Vec<crate::CompleteAgentRegistrationContribution> {
        vec![]
    }

    /// 注册额外的寻址空间能力提供者。
    ///
    /// 注意：`VfsDiscoveryProvider` 仅负责 descriptor / discovery 层抽象，
    /// 不是统一 runtime I/O provider（后者为 `MountProvider`）。
    fn vfs_providers(&self) -> Vec<Box<dyn VfsDiscoveryProvider>> {
        vec![]
    }

    /// 注册额外的上下文来源解析器。
    ///
    /// 返回 `(kind, resolver)` 对，注册到 `SourceResolverRegistry`。
    /// 当前该扩展点仍处于实验阶段，宿主尚未将其纳入稳定运行时闭环。
    fn source_resolvers(&self) -> Vec<(ContextSourceKind, Box<dyn SourceResolver>)> {
        vec![]
    }

    /// 注册认证/授权提供者
    ///
    /// 若返回 `Some`，框架会在 HTTP 路由上挂载认证中间件。
    /// 同一时刻只能有一个活跃的 `AuthProvider`；若多个集成均返回 `Some`，
    /// 宿主应在启动阶段直接失败，而不是隐式覆盖。
    fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> {
        None
    }

    /// 注册身份目录 Provider。
    ///
    /// 目录 Provider 只负责把外部企业目录适配成通用 user/group/tree/resolve 能力。
    /// Project grants、业务权限判断和 projection 持久化仍由宿主负责。
    fn identity_directory_provider(&self) -> Option<Box<dyn IdentityDirectoryProvider>> {
        None
    }

    /// 注册外部服务客户端（企业 KM、文档中心等只读内容源）。
    ///
    /// 当前该扩展点仍处于实验阶段，尚未接入稳定宿主链路。
    fn external_service_clients(&self) -> Vec<Box<dyn ExternalServiceClient>> {
        vec![]
    }

    /// 注册 mount I/O provider（如 KM 桥接、云存储等）。
    ///
    /// 宿主会将返回的 provider 注册到 `MountProviderRegistry`，
    /// 使 `ExternalService` 类型的 context container 能通过标准 mount 链路
    /// 完成 read / write / list / search 操作。
    fn mount_providers(&self) -> Vec<Arc<dyn MountProvider>> {
        vec![]
    }

    /// 注册 Routine 触发器提供者（自定义事件源）。
    ///
    /// 宿主会在运行时启动阶段完成 `provider_key` 冲突检测。
    fn routine_trigger_providers(&self) -> Vec<Arc<dyn RoutineTriggerProvider>> {
        vec![]
    }

    /// 注册外部 Marketplace Source provider。
    ///
    /// 宿主启动时统一收集并校验 `source_key` 与支持的 Shared Library 资产类型。
    /// Provider 只负责外部目录发现、分页、详情和拉取候选 payload，不直接写数据库。
    fn marketplace_source_providers(&self) -> Vec<Arc<dyn MarketplaceSourceProvider>> {
        vec![]
    }

    /// 注册动态 Skill Discovery provider。
    ///
    /// Provider 可基于 session/workspace/user 等通用上下文贡献 skill inventory 与
    /// 默认上下文暴露列表。该扩展点只描述 context exposure，不表达权限控制。
    fn skill_discovery_providers(&self) -> Vec<Arc<dyn SkillDiscoveryProvider>> {
        vec![]
    }

    /// 注册动态 Memory Discovery provider。
    ///
    /// Provider 只贡献 active VFS 上可发现的 memory source inventory 与索引指针；
    /// 真实读写权限仍由对应 mount capability 决定。
    fn memory_discovery_providers(&self) -> Vec<Arc<dyn MemoryDiscoveryProvider>> {
        vec![]
    }

    /// 注册额外的 Skill 扫描目录（绝对路径）。
    ///
    /// 宿主会扫描这些目录下的 SKILL.md 文件，发现规则与 VFS mount 一致
    /// （一级子目录 + SKILL.md frontmatter 解析）。
    /// 集成提供的 skill 优先级低于 VFS mount 内发现的同名 skill。
    fn extra_skill_dirs(&self) -> Vec<PathBuf> {
        vec![]
    }

    /// 声明由 Host Integration 内嵌贡献的 Shared Library 资产。
    ///
    /// 宿主负责补齐 integration 名称、计算 digest、校验 payload，并以
    /// `source = integration_embedded` 写入 Shared Library。集成不得绕过
    /// LibraryAsset typed validator 直接修改 Project 运行配置。
    fn library_asset_seeds(&self) -> Vec<IntegrationLibraryAssetSeed> {
        vec![]
    }

    /// 集成初始化钩子 — 在框架完成 DI 组装后调用
    fn on_init(&self) -> Result<(), IntegrationError> {
        Ok(())
    }

    /// 集成关闭钩子 — 在服务退出前调用
    fn on_shutdown(&self) -> Result<(), IntegrationError> {
        Ok(())
    }
}
