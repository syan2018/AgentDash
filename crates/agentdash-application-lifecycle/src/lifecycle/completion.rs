use agentdash_domain::workflow::WorkflowSessionTerminalState;

pub fn session_terminal_summary(
    state: WorkflowSessionTerminalState,
    message: Option<&str>,
) -> String {
    match (
        state,
        message.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (WorkflowSessionTerminalState::Completed, _) => "关联 session 已自然结束".to_string(),
        (WorkflowSessionTerminalState::Failed, Some(message)) => {
            format!("关联 session 以失败终态结束：{message}")
        }
        (WorkflowSessionTerminalState::Failed, None) => "关联 session 以失败终态结束".to_string(),
        (WorkflowSessionTerminalState::Interrupted, Some(message)) => {
            format!("关联 session 已中断：{message}")
        }
        (WorkflowSessionTerminalState::Interrupted, None) => "关联 session 已中断".to_string(),
    }
}

pub fn session_terminal_state_tag(state: WorkflowSessionTerminalState) -> &'static str {
    match state {
        WorkflowSessionTerminalState::Completed => "completed",
        WorkflowSessionTerminalState::Failed => "failed",
        WorkflowSessionTerminalState::Interrupted => "interrupted",
    }
}
