//! AgentRun frame launch construction 输入契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! frame construction 逻辑才能拿到 context / MCP server 绑定 / flow capabilities /
//! context bundle 等运行时字段，否则会出现"通道漂移"。

use agentdash_application_ports::launch::LaunchCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRepositoryRehydrateMode {
    SystemContext,
    ExecutorState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptLaunchPath {
    Plain,
    OwnerBootstrap,
    RepositoryRehydrate(SessionRepositoryRehydrateMode),
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeTraceLaunchState {
    pub executor_session_id: Option<String>,
    pub last_event_seq: u64,
}

impl RuntimeTraceLaunchState {
    fn has_executor_follow_up(&self) -> bool {
        self.executor_session_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    }
}

pub fn resolve_prompt_launch_path(
    runtime_trace_state: &RuntimeTraceLaunchState,
    has_live_executor_session: bool,
    supports_repository_restore: bool,
    agent_needs_bootstrap: bool,
) -> PromptLaunchPath {
    if agent_needs_bootstrap {
        return PromptLaunchPath::OwnerBootstrap;
    }
    if !has_live_executor_session
        && runtime_trace_state.last_event_seq > 0
        && !runtime_trace_state.has_executor_follow_up()
    {
        return PromptLaunchPath::RepositoryRehydrate(if supports_repository_restore {
            SessionRepositoryRehydrateMode::ExecutorState
        } else {
            SessionRepositoryRehydrateMode::SystemContext
        });
    }
    PromptLaunchPath::Plain
}

#[derive(Clone)]
pub struct FrameLaunchEnvelopeConstructionInput {
    pub runtime_thread_id: String,
    pub command: LaunchCommand,
    pub runtime_trace_state: RuntimeTraceLaunchState,
    pub had_existing_runtime: bool,
    pub agent_needs_bootstrap: bool,
}
