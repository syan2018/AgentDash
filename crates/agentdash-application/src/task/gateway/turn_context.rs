use agentdash_domain::common::Vfs;
use agentdash_domain::task::Task;

use crate::context::{BuiltTaskAgentContext, ContextContributorRegistry};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::{SessionRequestAssembler, TaskRuntimePhase, TaskRuntimeSpec};
use crate::task::execution::{ExecutionPhase, TaskExecutionError};
use crate::task::gateway::repo_ops::{load_related_context, map_internal_error};
use crate::vfs::RelayVfsService;
use crate::workspace::BackendAvailability;
use agentdash_domain::common::AgentConfig;

/// 基础设施引用 — prepare_task_turn_context 中不因调用而变化的部分
pub struct TaskTurnServices<'a> {
    pub repos: &'a RepositorySet,
    pub availability: &'a dyn BackendAvailability,
    pub vfs_service: &'a RelayVfsService,
    pub contributor_registry: &'a ContextContributorRegistry,
    pub platform_config: &'a PlatformConfig,
}

/// 准备好的 turn 上下文 — 包含 dispatch 所需的所有数据
pub struct PreparedTurnContext {
    pub built: BuiltTaskAgentContext,
    pub vfs: Option<Vfs>,
    pub resolved_config: Option<AgentConfig>,
    pub use_cloud_native_agent: bool,
    pub workspace: Option<agentdash_domain::workspace::Workspace>,
    /// CapabilityResolver 产出的内置工具簇（dispatcher 直接使用，不再硬编码）。
    pub flow_capabilities: agentdash_spi::FlowCapabilities,
    /// CapabilityResolver 产出的有效 capability key（用于 hook runtime 追踪）。
    pub effective_capability_keys: std::collections::BTreeSet<String>,
    /// 发起本次 task 执行的用户身份（由 HTTP handler 注入）。
    pub identity: Option<agentdash_spi::auth::AuthIdentity>,
    /// Hook effect 回调（cloud-native 路径取代 TurnMonitor）。
    /// Relay 路径暂不使用此字段。
    pub post_turn_handler: Option<crate::session::post_turn_handler::DynPostTurnHandler>,
}

/// 从 Task / Story / Project / Workspace 等上下文中构建 turn 执行所需的完整信息。
///
/// 内部完全委托给 [`SessionRequestAssembler::compose_task_runtime`],
/// 这里只做 owner 上下文加载与 phase enum 映射。
pub async fn prepare_task_turn_context(
    svc: &TaskTurnServices<'_>,
    task: &Task,
    phase: ExecutionPhase,
    override_prompt: Option<&str>,
    additional_prompt: Option<&str>,
    connector_config: Option<&AgentConfig>,
) -> Result<PreparedTurnContext, TaskExecutionError> {
    let (story, project, workspace) = load_related_context(svc.repos, task)
        .await
        .map_err(map_internal_error)?;

    let assembler = SessionRequestAssembler::new(
        svc.vfs_service,
        svc.repos.canvas_repo.as_ref(),
        svc.availability,
        svc.repos,
        svc.platform_config,
        svc.contributor_registry,
    );

    let runtime_phase = match phase {
        ExecutionPhase::Start => TaskRuntimePhase::Start,
        ExecutionPhase::Continue => TaskRuntimePhase::Continue,
    };

    let out = assembler
        .compose_task_runtime(TaskRuntimeSpec {
            task,
            story: &story,
            project: &project,
            workspace: workspace.as_ref(),
            phase: runtime_phase,
            override_prompt,
            additional_prompt,
            explicit_executor_config: connector_config.cloned(),
            strict_config_resolution: true,
        })
        .await?;

    Ok(PreparedTurnContext {
        built: out.built,
        vfs: out.vfs,
        resolved_config: out.resolved_config,
        use_cloud_native_agent: out.use_cloud_native_agent,
        workspace: out.workspace,
        flow_capabilities: out.flow_capabilities,
        effective_capability_keys: out.effective_capability_keys,
        identity: None,
        post_turn_handler: None,
    })
}
