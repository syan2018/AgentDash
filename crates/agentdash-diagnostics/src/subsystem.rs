//! 子系统取值约定。
//!
//! `Subsystem` 是 `diag!` 宏的必填第二参数，渲染进 event 的 `subsystem`
//! 字段。取值为稳定的小写字符串（如 `"relay"`），供查询端点按列过滤。

/// 平台过程诊断的子系统归类。
///
/// 取值口径：按"调用点所属子系统"赋值，不强求与 crate/模块路径一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subsystem {
    /// Relay 消息路由 / 后端注册（`agentdash-relay`、`relay/ws_handler`）。
    Relay,
    /// 会话启动链路（`session/launch/*`）。
    SessionLaunch,
    /// AgentRun 执行（`agentdash-application-agentrun`）。
    AgentRun,
    /// 生命周期调度与状态转换（`agentdash-application-lifecycle`）。
    Lifecycle,
    /// 工作流编排（`agentdash-application-workflow`）。
    Workflow,
    /// Hook 触发与失败（`agentdash-application-hooks`）。
    Hooks,
    /// 技能发现与装配（`agentdash-application-skill`）。
    Skill,
    /// 对账 / 状态收敛（`reconcile/*`）。
    Reconcile,
    /// 定时任务 / Cron。
    Cron,
    /// 鉴权 / 认证。
    Auth,
    /// 虚拟文件系统（`agentdash-application-vfs`）。
    Vfs,
    /// 基础设施 / 通用（DB、配置、启动等无更具体归类时）。
    Infra,
    /// MCP 协议相关。
    Mcp,
    /// HTTP API 层（路由、中间件）。
    Api,
}

impl Subsystem {
    /// 渲染进 event `subsystem` 字段的稳定小写字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Subsystem::Relay => "relay",
            Subsystem::SessionLaunch => "session_launch",
            Subsystem::AgentRun => "agent_run",
            Subsystem::Lifecycle => "lifecycle",
            Subsystem::Workflow => "workflow",
            Subsystem::Hooks => "hooks",
            Subsystem::Skill => "skill",
            Subsystem::Reconcile => "reconcile",
            Subsystem::Cron => "cron",
            Subsystem::Auth => "auth",
            Subsystem::Vfs => "vfs",
            Subsystem::Infra => "infra",
            Subsystem::Mcp => "mcp",
            Subsystem::Api => "api",
        }
    }
}

impl std::fmt::Display for Subsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
