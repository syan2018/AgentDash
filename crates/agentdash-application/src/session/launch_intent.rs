//! Session 启动意图（LaunchIntent）统一表达。
//!
//! 目的：
//! - 把“来源 + strictness + 诊断标签”从调用点分散字符串收敛为类型化契约；
//! - 为后续全来源（HTTP/Task/Workflow/Routine/Companion/Local）单入口迁移提供
//!   稳定过渡层。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLaunchSource {
    HttpPrompt,
    HookAutoResume,
    CompanionParentResume,
    TaskService,
    WorkflowOrchestrator,
    RoutineExecutor,
    LocalRelayPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLaunchStrictness {
    Strict,
    Relaxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLaunchIntent {
    source: SessionLaunchSource,
    strictness: SessionLaunchStrictness,
}

impl SessionLaunchIntent {
    pub const fn new(source: SessionLaunchSource, strictness: SessionLaunchStrictness) -> Self {
        Self { source, strictness }
    }

    pub const fn source(self) -> SessionLaunchSource {
        self.source
    }

    pub const fn strictness(self) -> SessionLaunchStrictness {
        self.strictness
    }

    /// 统一用于日志/错误定位的 reason tag。
    pub const fn reason_tag(self) -> &'static str {
        match self.source {
            SessionLaunchSource::HttpPrompt => "http_prompt",
            SessionLaunchSource::HookAutoResume => "hook_auto_resume",
            SessionLaunchSource::CompanionParentResume => "companion_parent_resume",
            SessionLaunchSource::TaskService => "task_service",
            SessionLaunchSource::WorkflowOrchestrator => "workflow_orchestrator",
            SessionLaunchSource::RoutineExecutor => "routine_executor",
            SessionLaunchSource::LocalRelayPrompt => "local_relay_prompt",
        }
    }

    pub const fn http_prompt() -> Self {
        Self::new(SessionLaunchSource::HttpPrompt, SessionLaunchStrictness::Strict)
    }

    pub const fn hook_auto_resume() -> Self {
        Self::new(
            SessionLaunchSource::HookAutoResume,
            SessionLaunchStrictness::Strict,
        )
    }

    pub const fn companion_parent_resume() -> Self {
        Self::new(
            SessionLaunchSource::CompanionParentResume,
            SessionLaunchStrictness::Strict,
        )
    }

    pub const fn local_relay_prompt_relaxed() -> Self {
        Self::new(
            SessionLaunchSource::LocalRelayPrompt,
            SessionLaunchStrictness::Relaxed,
        )
    }
}

