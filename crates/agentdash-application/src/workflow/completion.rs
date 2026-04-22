use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use agentdash_domain::workflow::{
    WorkflowCheckKind, WorkflowCompletionSpec, WorkflowSessionTerminalState,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowCompletionSignalSet {
    #[serde(default)]
    pub checklist_evidence_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_terminal_state: Option<WorkflowSessionTerminalState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_terminal_message: Option<String>,
    #[serde(default)]
    pub artifact_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub explicit_actions: Vec<String>,
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
    /// `"manual"` when the step has no attached workflow; `"auto"` when checks drive completion.
    pub transition_policy: String,
    pub satisfied: bool,
    pub should_complete_step: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking_reason: Option<String>,
    #[serde(default)]
    pub evidence: Vec<WorkflowCompletionEvidence>,
}

/// `auto_completion` is [`Some`] when the lifecycle step has a `workflow_key` (auto-driven step);
/// pass that workflow's `contract.completion`.
pub fn evaluate_step_completion(
    auto_completion: Option<&WorkflowCompletionSpec>,
    signals: &WorkflowCompletionSignalSet,
) -> WorkflowCompletionDecision {
    let Some(completion) = auto_completion else {
        return WorkflowCompletionDecision {
            transition_policy: "manual".to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some("Manual step — requires explicit advancement".to_string()),
            evidence: vec![],
        };
    };

    let checks = &completion.checks;
    if checks.is_empty() {
        return WorkflowCompletionDecision {
            transition_policy: "manual".to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some("workflow 未声明自动完成 check，等待显式推进".to_string()),
            evidence: vec![],
        };
    }

    let results: Vec<_> = checks
        .iter()
        .map(|check| evaluate_check(check, signals))
        .collect();
    let all_satisfied = results.iter().all(|r| r.satisfied);
    let evidence: Vec<_> = results
        .iter()
        .map(|r| WorkflowCompletionEvidence {
            code: r.code.clone(),
            summary: r.summary.clone(),
            detail: r.detail.clone(),
        })
        .collect();

    if all_satisfied {
        WorkflowCompletionDecision {
            transition_policy: "auto".to_string(),
            satisfied: true,
            should_complete_step: true,
            summary: Some("All completion checks passed".to_string()),
            blocking_reason: None,
            evidence,
        }
    } else {
        WorkflowCompletionDecision {
            transition_policy: "auto".to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some("Completion checks not yet satisfied".to_string()),
            evidence,
        }
    }
}

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

struct CheckEvaluationResult {
    code: String,
    summary: String,
    detail: Option<String>,
    satisfied: bool,
}

fn evaluate_check(
    check: &agentdash_domain::workflow::WorkflowCheckSpec,
    signals: &WorkflowCompletionSignalSet,
) -> CheckEvaluationResult {
    match check.kind {
        WorkflowCheckKind::ArtifactExists => {
            let artifact_type = require_string(check, "artifact_type");
            let count = signals
                .artifact_counts
                .get(&artifact_type)
                .copied()
                .unwrap_or_default();
            let satisfied = count > 0;
            CheckEvaluationResult {
                code: if satisfied {
                    "artifact_exists_satisfied".to_string()
                } else {
                    "artifact_exists_missing".to_string()
                },
                summary: if satisfied {
                    "已检测到要求的记录产物".to_string()
                } else {
                    "尚未检测到要求的记录产物".to_string()
                },
                detail: Some(format!("artifact_type={artifact_type}")),
                satisfied,
            }
        }
        WorkflowCheckKind::ArtifactCountGte => {
            let artifact_type = require_string(check, "artifact_type");
            let min_count = require_u64(check, "min_count") as usize;
            let count = signals
                .artifact_counts
                .get(&artifact_type)
                .copied()
                .unwrap_or_default();
            let satisfied = count >= min_count;
            CheckEvaluationResult {
                code: if satisfied {
                    "artifact_count_gte_satisfied".to_string()
                } else {
                    "artifact_count_gte_pending".to_string()
                },
                summary: if satisfied {
                    "记录产物数量满足 workflow check".to_string()
                } else {
                    "记录产物数量尚未满足 workflow check".to_string()
                },
                detail: Some(format!(
                    "artifact_type={artifact_type}, count={count}, min={min_count}"
                )),
                satisfied,
            }
        }
        WorkflowCheckKind::SessionTerminalIn => {
            let accepted = require_string_list(check, "states");
            let terminal_state = signals.session_terminal_state;
            let satisfied = terminal_state
                .map(session_terminal_state_tag)
                .map(|state| accepted.iter().any(|candidate| candidate == state))
                .unwrap_or(false);
            CheckEvaluationResult {
                code: if satisfied {
                    "session_terminal_in_satisfied".to_string()
                } else {
                    "session_terminal_in_pending".to_string()
                },
                summary: if satisfied {
                    "关联 session 终态满足 workflow check".to_string()
                } else {
                    "关联 session 终态尚未满足 workflow check".to_string()
                },
                detail: terminal_state
                    .map(|state| format!("terminal_state={}", session_terminal_state_tag(state))),
                satisfied,
            }
        }
        WorkflowCheckKind::ChecklistEvidencePresent => CheckEvaluationResult {
            code: if signals.checklist_evidence_present {
                "checklist_evidence_present".to_string()
            } else {
                "checklist_evidence_missing".to_string()
            },
            summary: if signals.checklist_evidence_present {
                "已检测到 checklist evidence".to_string()
            } else {
                "尚未检测到 checklist evidence".to_string()
            },
            detail: None,
            satisfied: signals.checklist_evidence_present,
        },
        WorkflowCheckKind::ExplicitActionReceived => {
            let action_key = require_string(check, "action_key");
            let satisfied = signals
                .explicit_actions
                .iter()
                .any(|candidate| candidate == &action_key);
            CheckEvaluationResult {
                code: if satisfied {
                    "explicit_action_received".to_string()
                } else {
                    "explicit_action_missing".to_string()
                },
                summary: if satisfied {
                    "已收到显式动作".to_string()
                } else {
                    "尚未收到显式动作".to_string()
                },
                detail: Some(action_key),
                satisfied,
            }
        }
        WorkflowCheckKind::Custom => CheckEvaluationResult {
            code: "custom_check_pending".to_string(),
            summary: format!(
                "自定义 check `{}` 尚未内置 evaluator，默认视为未满足",
                check.key
            ),
            detail: None,
            satisfied: false,
        },
    }
}

fn read_string(payload: Option<&serde_json::Value>, key: &str) -> Option<String> {
    payload
        .and_then(|payload| payload.get(key))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn read_string_list(payload: Option<&serde_json::Value>, key: &str) -> Vec<String> {
    payload
        .and_then(|payload| payload.get(key))
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn read_u64(payload: Option<&serde_json::Value>, key: &str) -> Option<u64> {
    payload
        .and_then(|payload| payload.get(key))
        .and_then(serde_json::Value::as_u64)
}

fn require_string(check: &agentdash_domain::workflow::WorkflowCheckSpec, key: &str) -> String {
    read_string(check.payload.as_ref(), key).unwrap_or_else(|| {
        panic!(
            "workflow check `{}` 缺少必填字符串字段 `{}`",
            check.key, key
        )
    })
}

fn require_string_list(
    check: &agentdash_domain::workflow::WorkflowCheckSpec,
    key: &str,
) -> Vec<String> {
    let items = read_string_list(check.payload.as_ref(), key);
    if items.is_empty() {
        panic!(
            "workflow check `{}` 缺少必填字符串列表 `{}`",
            check.key, key
        );
    }
    items
}

fn require_u64(check: &agentdash_domain::workflow::WorkflowCheckSpec, key: &str) -> u64 {
    read_u64(check.payload.as_ref(), key)
        .unwrap_or_else(|| panic!("workflow check `{}` 缺少必填整数 `{}`", check.key, key))
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workflow::WorkflowCheckSpec;
    use std::panic;

    use super::*;

    fn contract_with_checks() -> WorkflowCompletionSpec {
        WorkflowCompletionSpec {
            checks: vec![WorkflowCheckSpec {
                key: "evidence".to_string(),
                kind: WorkflowCheckKind::ChecklistEvidencePresent,
                description: "evidence".to_string(),
                payload: None,
            }],
        }
    }

    #[test]
    fn auto_checks_require_all_checks() {
        let decision = evaluate_step_completion(
            Some(&contract_with_checks()),
            &WorkflowCompletionSignalSet {
                checklist_evidence_present: false,
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(!decision.satisfied);
        assert!(!decision.should_complete_step);
        assert_eq!(decision.transition_policy, "auto");
    }

    #[test]
    fn auto_checks_succeeds_when_contract_checks_are_met() {
        let decision = evaluate_step_completion(
            Some(&contract_with_checks()),
            &WorkflowCompletionSignalSet {
                checklist_evidence_present: true,
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(decision.satisfied);
        assert!(decision.should_complete_step);
    }

    #[test]
    fn session_terminal_signal_does_not_force_satisfy_unrelated_checks() {
        // 旧行为：terminal 信号一来就强制短路 satisfied=true，无视 contract checks。
        // 新行为：terminal 信号必须经 SessionTerminalIn check 评估；
        // 这里 check 只接受 completed，但收到 Failed → 不满足。
        let spec = WorkflowCompletionSpec {
            checks: vec![WorkflowCheckSpec {
                key: "term".to_string(),
                kind: WorkflowCheckKind::SessionTerminalIn,
                description: "terminal".to_string(),
                payload: Some(serde_json::json!({ "states": ["completed"] })),
            }],
            ..WorkflowCompletionSpec::default()
        };
        let decision = evaluate_step_completion(
            Some(&spec),
            &WorkflowCompletionSignalSet {
                session_terminal_state: Some(WorkflowSessionTerminalState::Failed),
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(!decision.satisfied);
        assert!(!decision.should_complete_step);
    }

    #[test]
    fn empty_checks_yields_manual_decision_not_implicit_session_terminal_wait() {
        // 空 checks = workflow 没声明自动门禁 → 视同 manual，由 agent/编排显式推进，
        // 不再隐式"等 session terminal"。
        let spec = WorkflowCompletionSpec::default();
        let decision =
            evaluate_step_completion(Some(&spec), &WorkflowCompletionSignalSet::default());

        assert_eq!(decision.transition_policy, "manual");
        assert!(!decision.satisfied);
        assert!(!decision.should_complete_step);
    }

    #[test]
    fn empty_checks_ignores_session_terminal_signal() {
        // 即便有 session terminal 信号，空 checks 也保持 manual——
        // 没有声明就不会被任何信号强行通关。
        let spec = WorkflowCompletionSpec::default();
        let decision = evaluate_step_completion(
            Some(&spec),
            &WorkflowCompletionSignalSet {
                session_terminal_state: Some(WorkflowSessionTerminalState::Completed),
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert_eq!(decision.transition_policy, "manual");
        assert!(!decision.should_complete_step);
    }

    #[test]
    fn session_terminal_in_check_without_runtime_terminal_is_pending() {
        let spec = WorkflowCompletionSpec {
            checks: vec![WorkflowCheckSpec {
                key: "term".to_string(),
                kind: WorkflowCheckKind::SessionTerminalIn,
                description: "terminal".to_string(),
                payload: Some(serde_json::json!({ "states": ["completed"] })),
            }],
            ..WorkflowCompletionSpec::default()
        };
        let decision =
            evaluate_step_completion(Some(&spec), &WorkflowCompletionSignalSet::default());

        assert!(!decision.satisfied);
    }

    #[test]
    fn malformed_artifact_count_check_panics_instead_of_silently_pending() {
        let spec = WorkflowCompletionSpec {
            checks: vec![WorkflowCheckSpec {
                key: "artifact-count".to_string(),
                kind: WorkflowCheckKind::ArtifactCountGte,
                description: "artifact count".to_string(),
                payload: Some(serde_json::json!({ "artifact_type": "phase_note" })),
            }],
            ..WorkflowCompletionSpec::default()
        };

        assert!(
            panic::catch_unwind(|| {
                let _ =
                    evaluate_step_completion(Some(&spec), &WorkflowCompletionSignalSet::default());
            })
            .is_err()
        );
    }
}
