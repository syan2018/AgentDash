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
    CompanionDispatch,
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
pub enum SessionLaunchPreparation {
    /// 入口传入的是 bare request，需要先经过 augmenter 补齐运行时字段。
    RequiresAugment,
    /// 入口已经完成 req 组装（compose + finalize），可直接 start_prompt。
    PreAssembled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLaunchIntent {
    source: SessionLaunchSource,
    strictness: SessionLaunchStrictness,
    preparation: SessionLaunchPreparation,
    /// 底层执行器的 follow-up 会话 ID（仅 relay / companion dispatch 需要）。
    follow_up_session_id: Option<String>,
}

impl SessionLaunchIntent {
    pub fn new(
        source: SessionLaunchSource,
        strictness: SessionLaunchStrictness,
        preparation: SessionLaunchPreparation,
    ) -> Self {
        Self {
            source,
            strictness,
            preparation,
            follow_up_session_id: None,
        }
    }

    /// 链式设置 follow_up_session_id。
    pub fn with_follow_up(mut self, session_id: Option<impl Into<String>>) -> Self {
        self.follow_up_session_id = session_id.map(Into::into);
        self
    }

    pub fn source(&self) -> SessionLaunchSource {
        self.source
    }

    pub fn strictness(&self) -> SessionLaunchStrictness {
        self.strictness
    }

    pub fn preparation(&self) -> SessionLaunchPreparation {
        self.preparation
    }

    pub fn follow_up_session_id(&self) -> Option<&str> {
        self.follow_up_session_id.as_deref()
    }

    pub fn reason_tag(&self) -> &'static str {
        match self.source {
            SessionLaunchSource::HttpPrompt => "http_prompt",
            SessionLaunchSource::HookAutoResume => "hook_auto_resume",
            SessionLaunchSource::CompanionDispatch => "companion_dispatch",
            SessionLaunchSource::CompanionParentResume => "companion_parent_resume",
            SessionLaunchSource::TaskService => "task_service",
            SessionLaunchSource::WorkflowOrchestrator => "workflow_orchestrator",
            SessionLaunchSource::RoutineExecutor => "routine_executor",
            SessionLaunchSource::LocalRelayPrompt => "local_relay_prompt",
        }
    }

    pub fn http_prompt() -> Self {
        Self::new(
            SessionLaunchSource::HttpPrompt,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::RequiresAugment,
        )
    }

    pub fn hook_auto_resume() -> Self {
        Self::new(
            SessionLaunchSource::HookAutoResume,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::RequiresAugment,
        )
    }

    pub fn companion_parent_resume() -> Self {
        Self::new(
            SessionLaunchSource::CompanionParentResume,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::RequiresAugment,
        )
    }

    pub fn companion_dispatch() -> Self {
        Self::new(
            SessionLaunchSource::CompanionDispatch,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::PreAssembled,
        )
    }

    pub fn task_service() -> Self {
        Self::new(
            SessionLaunchSource::TaskService,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::PreAssembled,
        )
    }

    pub fn workflow_orchestrator() -> Self {
        Self::new(
            SessionLaunchSource::WorkflowOrchestrator,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::PreAssembled,
        )
    }

    pub fn routine_executor() -> Self {
        Self::new(
            SessionLaunchSource::RoutineExecutor,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::PreAssembled,
        )
    }

    pub fn local_relay_prompt() -> Self {
        Self::new(
            SessionLaunchSource::LocalRelayPrompt,
            SessionLaunchStrictness::Strict,
            SessionLaunchPreparation::PreAssembled,
        )
    }

    pub fn local_relay_prompt_relaxed() -> Self {
        Self::new(
            SessionLaunchSource::LocalRelayPrompt,
            SessionLaunchStrictness::Relaxed,
            SessionLaunchPreparation::PreAssembled,
        )
    }
}

