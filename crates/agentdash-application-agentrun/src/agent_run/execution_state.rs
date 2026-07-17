#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunExecutionState {
    Idle,
    Running {
        turn_id: Option<String>,
    },
    Cancelling {
        turn_id: Option<String>,
    },
    Completed {
        turn_id: String,
    },
    Failed {
        turn_id: String,
        message: Option<String>,
    },
    Interrupted {
        turn_id: Option<String>,
        message: Option<String>,
    },
    Lost {
        turn_id: Option<String>,
        message: Option<String>,
    },
}
