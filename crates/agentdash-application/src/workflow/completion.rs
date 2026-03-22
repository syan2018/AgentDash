use serde::{Deserialize, Serialize};

use agentdash_domain::workflow::WorkflowPhaseCompletionMode;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSessionTerminalState {
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowCompletionSignalSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    #[serde(default)]
    pub checklist_evidence_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_terminal_state: Option<WorkflowSessionTerminalState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_terminal_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowCompletionEvidence {
    pub code: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowCompletionDecision {
    pub completion_mode: String,
    pub satisfied: bool,
    pub should_complete_phase: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking_reason: Option<String>,
    #[serde(default)]
    pub evidence: Vec<WorkflowCompletionEvidence>,
}

pub fn evaluate_phase_completion(
    mode: WorkflowPhaseCompletionMode,
    signals: &WorkflowCompletionSignalSet,
) -> WorkflowCompletionDecision {
    match mode {
        WorkflowPhaseCompletionMode::Manual => WorkflowCompletionDecision {
            completion_mode: completion_mode_tag(mode).to_string(),
            satisfied: false,
            should_complete_phase: false,
            summary: None,
            blocking_reason: Some("manual phase 需要显式推进，不会由 runtime 自动完成".to_string()),
            evidence: vec![WorkflowCompletionEvidence {
                code: "manual_phase_requires_explicit_completion".to_string(),
                summary: "当前 phase 使用 manual completion".to_string(),
                detail: None,
            }],
        },
        WorkflowPhaseCompletionMode::SessionEnded => match signals.session_terminal_state {
            Some(terminal_state) => WorkflowCompletionDecision {
                completion_mode: completion_mode_tag(mode).to_string(),
                satisfied: true,
                should_complete_phase: true,
                summary: Some(session_terminal_summary(
                    terminal_state,
                    signals.session_terminal_message.as_deref(),
                )),
                blocking_reason: None,
                evidence: vec![WorkflowCompletionEvidence {
                    code: "session_terminal_detected".to_string(),
                    summary: "关联 session 已进入终态，满足 session_ended completion".to_string(),
                    detail: Some(format!(
                        "terminal_state={}",
                        session_terminal_state_tag(terminal_state)
                    )),
                }],
            },
            None => WorkflowCompletionDecision {
                completion_mode: completion_mode_tag(mode).to_string(),
                satisfied: false,
                should_complete_phase: false,
                summary: None,
                blocking_reason: Some(
                    "当前 phase 依赖 session_ended completion，必须等待关联 session 进入终态"
                        .to_string(),
                ),
                evidence: vec![WorkflowCompletionEvidence {
                    code: "session_terminal_missing".to_string(),
                    summary: "关联 session 仍未结束，session_ended completion 尚未满足".to_string(),
                    detail: None,
                }],
            },
        },
        WorkflowPhaseCompletionMode::ChecklistPassed => {
            let task_ready = matches!(
                signals.task_status.as_deref(),
                Some("awaiting_verification" | "completed")
            );
            if task_ready && signals.checklist_evidence_present {
                let task_status = signals.task_status.clone().unwrap_or_default();
                WorkflowCompletionDecision {
                    completion_mode: completion_mode_tag(mode).to_string(),
                    satisfied: true,
                    should_complete_phase: true,
                    summary: Some(format!(
                        "Task 状态已进入 `{task_status}`，且存在 checklist evidence，满足 checklist_passed completion"
                    )),
                    blocking_reason: None,
                    evidence: vec![
                        WorkflowCompletionEvidence {
                            code: "checklist_task_status_satisfied".to_string(),
                            summary: "Task 状态满足 checklist completion 条件".to_string(),
                            detail: Some(format!("task_status={task_status}")),
                        },
                        WorkflowCompletionEvidence {
                            code: "checklist_evidence_present".to_string(),
                            summary: "已检测到 checklist evidence".to_string(),
                            detail: None,
                        },
                    ],
                }
            } else {
                let blocking_reason = if !task_ready {
                    "当前 phase 尚未满足 checklist_passed completion；请先完成验证并把 Task 更新为 awaiting_verification 或 completed"
                        .to_string()
                } else {
                    "当前 phase 尚未满足 checklist_passed completion；请先产出包含检查结论的 phase note / checklist evidence，再结束 session"
                        .to_string()
                };
                let mut evidence = vec![WorkflowCompletionEvidence {
                    code: if task_ready {
                        "checklist_task_status_satisfied".to_string()
                    } else {
                        "checklist_task_status_pending".to_string()
                    },
                    summary: if task_ready {
                        "Task 状态满足 checklist completion 条件".to_string()
                    } else {
                        "Task 状态尚未满足 checklist completion 条件".to_string()
                    },
                    detail: signals
                        .task_status
                        .as_ref()
                        .map(|status| format!("task_status={status}")),
                }];
                if !signals.checklist_evidence_present {
                    evidence.push(WorkflowCompletionEvidence {
                        code: "checklist_evidence_missing".to_string(),
                        summary: "尚未检测到 checklist evidence".to_string(),
                        detail: None,
                    });
                }
                WorkflowCompletionDecision {
                    completion_mode: completion_mode_tag(mode).to_string(),
                    satisfied: false,
                    should_complete_phase: false,
                    summary: None,
                    blocking_reason: Some(blocking_reason),
                    evidence,
                }
            }
        }
    }
}

pub fn completion_mode_tag(mode: WorkflowPhaseCompletionMode) -> &'static str {
    match mode {
        WorkflowPhaseCompletionMode::Manual => "manual",
        WorkflowPhaseCompletionMode::SessionEnded => "session_ended",
        WorkflowPhaseCompletionMode::ChecklistPassed => "checklist_passed",
    }
}

pub fn session_terminal_state_tag(state: WorkflowSessionTerminalState) -> &'static str {
    match state {
        WorkflowSessionTerminalState::Completed => "completed",
        WorkflowSessionTerminalState::Failed => "failed",
        WorkflowSessionTerminalState::Interrupted => "interrupted",
    }
}

fn session_terminal_summary(state: WorkflowSessionTerminalState, message: Option<&str>) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checklist_completion_satisfied_by_ready_task_status_and_evidence() {
        let decision = evaluate_phase_completion(
            WorkflowPhaseCompletionMode::ChecklistPassed,
            &WorkflowCompletionSignalSet {
                task_status: Some("awaiting_verification".to_string()),
                checklist_evidence_present: true,
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(decision.satisfied);
        assert!(decision.should_complete_phase);
        assert_eq!(decision.completion_mode, "checklist_passed");
    }

    #[test]
    fn checklist_completion_requires_evidence_even_when_task_ready() {
        let decision = evaluate_phase_completion(
            WorkflowPhaseCompletionMode::ChecklistPassed,
            &WorkflowCompletionSignalSet {
                task_status: Some("awaiting_verification".to_string()),
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(!decision.satisfied);
        assert!(!decision.should_complete_phase);
        assert_eq!(
            decision.blocking_reason.as_deref(),
            Some(
                "当前 phase 尚未满足 checklist_passed completion；请先产出包含检查结论的 phase note / checklist evidence，再结束 session"
            )
        );
        assert!(
            decision
                .evidence
                .iter()
                .any(|entry| entry.code == "checklist_evidence_missing")
        );
    }

    #[test]
    fn session_ended_completion_requires_terminal_signal() {
        let decision = evaluate_phase_completion(
            WorkflowPhaseCompletionMode::SessionEnded,
            &WorkflowCompletionSignalSet::default(),
        );

        assert!(!decision.satisfied);
        assert!(!decision.should_complete_phase);
        assert!(decision.blocking_reason.is_some());
    }

    #[test]
    fn session_ended_completion_builds_terminal_summary() {
        let decision = evaluate_phase_completion(
            WorkflowPhaseCompletionMode::SessionEnded,
            &WorkflowCompletionSignalSet {
                session_terminal_state: Some(WorkflowSessionTerminalState::Failed),
                session_terminal_message: Some("tool runtime panic".to_string()),
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(decision.satisfied);
        assert!(decision.should_complete_phase);
        assert_eq!(
            decision.summary.as_deref(),
            Some("关联 session 以失败终态结束：tool runtime panic")
        );
    }
}
