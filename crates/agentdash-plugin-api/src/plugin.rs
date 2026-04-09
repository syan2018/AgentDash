use std::path::PathBuf;
use std::sync::Arc;

use agentdash_domain::context_source::ContextSourceKind;
use agentdash_spi::AgentConnector;
use agentdash_spi::mount::MountProvider;
use agentdash_spi::{AddressSpaceDiscoveryProvider, SourceResolver};

use crate::auth::AuthProvider;
use crate::external::ExternalServiceClient;

/// 插件错误
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("插件初始化失败: {0}")]
    InitFailed(String),
    #[error("插件关闭失败: {0}")]
    ShutdownFailed(String),
}

/// AgentDash 插件入口
///
/// 每个插件通过实现此 trait 向运行时注册自己的扩展。
/// 所有方法均提供默认空实现，插件只需覆盖需要扩展的方法。
/// 但要注意：`trait` 上存在的方法并不等于该扩展点已经是稳定外部合同。
/// 当前宿主会优先保证已闭环、已验证的能力真实生效；其余扩展点可能仍处于实验阶段。
///
/// # 示例（企业私有仓库中）
///
/// ```rust,ignore
/// pub struct CorpPlugin;
///
/// impl AgentDashPlugin for CorpPlugin {
///     fn name(&self) -> &str { "corp" }
///
///     fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> {
///         Some(Box::new(CorpSsoAuthProvider::new()))
///     }
///
///     fn address_space_providers(&self) -> Vec<Box<dyn AddressSpaceProvider>> {
///         vec![Box::new(CorpKmProvider::new())]
///     }
/// }
/// ```
pub trait AgentDashPlugin: Send + Sync {
    /// 插件名称（用于日志和诊断）
    fn name(&self) -> &str;

    /// 注册额外的寻址空间能力提供者。
    ///
    /// 注意：`AddressSpaceDiscoveryProvider` 仅负责 descriptor / discovery 层抽象，
    /// 不是统一 runtime I/O provider（后者为 `MountProvider`）。
    fn address_space_providers(&self) -> Vec<Box<dyn AddressSpaceDiscoveryProvider>> {
        vec![]
    }

    /// 注册额外的上下文来源解析器。
    ///
    /// 返回 `(kind, resolver)` 对，注册到 `SourceResolverRegistry`。
    /// 当前该扩展点仍处于实验阶段，宿主尚未将其纳入稳定运行时闭环。
    fn source_resolvers(&self) -> Vec<(ContextSourceKind, Box<dyn SourceResolver>)> {
        vec![]
    }

    /// 注册额外的 Agent 连接器。
    ///
    /// 宿主会在运行时构建前完成冲突检测；若多个插件声明同一执行器 ID，应启动失败。
    fn agent_connectors(&self) -> Vec<Arc<dyn AgentConnector>> {
        vec![]
    }

    /// 注册认证/授权提供者
    ///
    /// 若返回 `Some`，框架会在 HTTP 路由上挂载认证中间件。
    /// 同一时刻只能有一个活跃的 `AuthProvider`；若多个插件均返回 `Some`，
    /// 宿主应在启动阶段直接失败，而不是隐式覆盖。
    fn auth_provider(&self) -> Option<Box<dyn AuthProvider>> {
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

    /// 注册额外的 Skill 扫描目录（绝对路径）。
    ///
    /// 宿主会扫描这些目录下的 SKILL.md 文件，发现规则与 address space mount 一致
    /// （一级子目录 + SKILL.md frontmatter 解析）。
    /// 插件提供的 skill 优先级低于 address space mount 内发现的同名 skill。
    fn extra_skill_dirs(&self) -> Vec<PathBuf> {
        vec![]
    }

    /// 插件初始化钩子 — 在框架完成 DI 组装后调用
    fn on_init(&self) -> Result<(), PluginError> {
        Ok(())
    }

    /// 插件关闭钩子 — 在服务退出前调用
    fn on_shutdown(&self) -> Result<(), PluginError> {
        Ok(())
    }
}
