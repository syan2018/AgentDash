use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use agentdash_domain::workflow::{
    EffectiveSessionContract, LifecycleTransitionPolicyKind, LifecycleTransitionSpec,
    WorkflowCheckKind, WorkflowRecordArtifactType, WorkflowSessionTerminalState,
};

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

pub fn evaluate_step_transition(
    transition: &LifecycleTransitionSpec,
    contract: &EffectiveSessionContract,
    signals: &WorkflowCompletionSignalSet,
) -> WorkflowCompletionDecision {
    match transition.policy.kind {
        LifecycleTransitionPolicyKind::Manual => WorkflowCompletionDecision {
            transition_policy: transition_policy_tag(transition).to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some("manual step 需要显式推进，不会由 runtime 自动完成".to_string()),
            evidence: vec![WorkflowCompletionEvidence {
                code: "manual_step_requires_explicit_completion".to_string(),
                summary: "当前 step 使用 manual transition".to_string(),
                detail: None,
            }],
        },
        LifecycleTransitionPolicyKind::SessionTerminalMatches => {
            evaluate_session_terminal_transition(transition, signals)
        }
        LifecycleTransitionPolicyKind::AllChecksPass => {
            evaluate_contract_checks(transition, contract, signals, true)
        }
        LifecycleTransitionPolicyKind::AnyChecksPass => {
            evaluate_contract_checks(transition, contract, signals, false)
        }
        LifecycleTransitionPolicyKind::ExplicitAction => {
            let action_key = transition
                .policy
                .action_key
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string();
            let satisfied = !action_key.is_empty()
                && signals
                    .explicit_actions
                    .iter()
                    .any(|action| action == &action_key);
            if satisfied {
                WorkflowCompletionDecision {
                    transition_policy: transition_policy_tag(transition).to_string(),
                    satisfied: true,
                    should_complete_step: true,
                    summary: Some(format!("收到显式动作 `{action_key}`，允许推进 step")),
                    blocking_reason: None,
                    evidence: vec![WorkflowCompletionEvidence {
                        code: "explicit_action_received".to_string(),
                        summary: "收到生命周期显式推进动作".to_string(),
                        detail: Some(action_key),
                    }],
                }
            } else {
                WorkflowCompletionDecision {
                    transition_policy: transition_policy_tag(transition).to_string(),
                    satisfied: false,
                    should_complete_step: false,
                    summary: None,
                    blocking_reason: Some(format!(
                        "当前 step 依赖显式动作 `{action_key}` 才能推进"
                    )),
                    evidence: vec![WorkflowCompletionEvidence {
                        code: "explicit_action_missing".to_string(),
                        summary: "尚未收到生命周期显式推进动作".to_string(),
                        detail: if action_key.is_empty() {
                            None
                        } else {
                            Some(action_key)
                        },
                    }],
                }
            }
        }
    }
}

pub fn transition_policy_tag(transition: &LifecycleTransitionSpec) -> &'static str {
    match transition.policy.kind {
        LifecycleTransitionPolicyKind::Manual => "manual",
        LifecycleTransitionPolicyKind::AllChecksPass => "all_checks_pass",
        LifecycleTransitionPolicyKind::AnyChecksPass => "any_checks_pass",
        LifecycleTransitionPolicyKind::SessionTerminalMatches => "session_terminal_matches",
        LifecycleTransitionPolicyKind::ExplicitAction => "explicit_action",
    }
}

pub fn session_terminal_state_tag(state: WorkflowSessionTerminalState) -> &'static str {
    match state {
        WorkflowSessionTerminalState::Completed => "completed",
        WorkflowSessionTerminalState::Failed => "failed",
        WorkflowSessionTerminalState::Interrupted => "interrupted",
    }
}

fn evaluate_session_terminal_transition(
    transition: &LifecycleTransitionSpec,
    signals: &WorkflowCompletionSignalSet,
) -> WorkflowCompletionDecision {
    match signals.session_terminal_state {
        Some(state)
            if transition
                .policy
                .session_terminal_states
                .iter()
                .any(|candidate| candidate == &state) =>
        {
            WorkflowCompletionDecision {
                transition_policy: transition_policy_tag(transition).to_string(),
                satisfied: true,
                should_complete_step: true,
                summary: Some(session_terminal_summary(
                    state,
                    signals.session_terminal_message.as_deref(),
                )),
                blocking_reason: None,
                evidence: vec![WorkflowCompletionEvidence {
                    code: "session_terminal_detected".to_string(),
                    summary: "关联 session 已进入允许的终态，满足 step transition".to_string(),
                    detail: Some(format!(
                        "terminal_state={}",
                        session_terminal_state_tag(state)
                    )),
                }],
            }
        }
        Some(state) => WorkflowCompletionDecision {
            transition_policy: transition_policy_tag(transition).to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some(format!(
                "关联 session 已结束，但终态 `{}` 不在允许列表内",
                session_terminal_state_tag(state)
            )),
            evidence: vec![WorkflowCompletionEvidence {
                code: "session_terminal_state_rejected".to_string(),
                summary: "关联 session 终态不符合当前 step transition".to_string(),
                detail: Some(format!(
                    "terminal_state={}",
                    session_terminal_state_tag(state)
                )),
            }],
        },
        None => WorkflowCompletionDecision {
            transition_policy: transition_policy_tag(transition).to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some(
                "当前 step 依赖 session_terminal_matches，必须等待关联 session 进入终态"
                    .to_string(),
            ),
            evidence: vec![WorkflowCompletionEvidence {
                code: "session_terminal_missing".to_string(),
                summary: "关联 session 仍未结束".to_string(),
                detail: None,
            }],
        },
    }
}

fn evaluate_contract_checks(
    transition: &LifecycleTransitionSpec,
    contract: &EffectiveSessionContract,
    signals: &WorkflowCompletionSignalSet,
    require_all: bool,
) -> WorkflowCompletionDecision {
    if contract.completion.checks.is_empty() {
        return WorkflowCompletionDecision {
            transition_policy: transition_policy_tag(transition).to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some(
                "当前 step 需要基于 checks 推进，但 effective contract 未定义 checks".to_string(),
            ),
            evidence: vec![WorkflowCompletionEvidence {
                code: "workflow_checks_missing".to_string(),
                summary: "effective contract 未定义 checks，无法自动推进".to_string(),
                detail: None,
            }],
        };
    }

    let results = contract
        .completion
        .checks
        .iter()
        .map(|check| evaluate_check(check, signals))
        .collect::<Vec<_>>();

    let satisfied = if require_all {
        results.iter().all(|result| result.satisfied)
    } else {
        results.iter().any(|result| result.satisfied)
    };

    let evidence = results
        .iter()
        .map(|result| WorkflowCompletionEvidence {
            code: result.code.clone(),
            summary: result.summary.clone(),
            detail: result.detail.clone(),
        })
        .collect::<Vec<_>>();

    if satisfied {
        WorkflowCompletionDecision {
            transition_policy: transition_policy_tag(transition).to_string(),
            satisfied: true,
            should_complete_step: true,
            summary: Some(if require_all {
                "当前 step 的所有 checks 均已满足，可推进生命周期".to_string()
            } else {
                "当前 step 已满足至少一个 check，可推进生命周期".to_string()
            }),
            blocking_reason: None,
            evidence,
        }
    } else {
        WorkflowCompletionDecision {
            transition_policy: transition_policy_tag(transition).to_string(),
            satisfied: false,
            should_complete_step: false,
            summary: None,
            blocking_reason: Some(if require_all {
                "当前 step 仍有 checks 未满足，不能推进生命周期".to_string()
            } else {
                "当前 step 还没有任何 check 满足，不能推进生命周期".to_string()
            }),
            evidence,
        }
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
        WorkflowCheckKind::TaskStatusIn => {
            let accepted = read_string_list(check.payload.as_ref(), "statuses");
            let task_status = signals.task_status.clone();
            let satisfied = task_status
                .as_deref()
                .map(|status| accepted.iter().any(|candidate| candidate == status))
                .unwrap_or(false);
            CheckEvaluationResult {
                code: if satisfied {
                    "task_status_in_satisfied".to_string()
                } else {
                    "task_status_in_pending".to_string()
                },
                summary: if satisfied {
                    "Task 状态满足 workflow check".to_string()
                } else {
                    "Task 状态尚未满足 workflow check".to_string()
                },
                detail: task_status.map(|status| format!("task_status={status}")),
                satisfied,
            }
        }
        WorkflowCheckKind::ArtifactExists => {
            let artifact_type = read_string(check.payload.as_ref(), "artifact_type");
            let count = artifact_type
                .as_deref()
                .and_then(|key| signals.artifact_counts.get(key))
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
                detail: artifact_type.map(|artifact_type| format!("artifact_type={artifact_type}")),
                satisfied,
            }
        }
        WorkflowCheckKind::ArtifactCountGte => {
            let artifact_type = read_string(check.payload.as_ref(), "artifact_type");
            let min_count = read_u64(check.payload.as_ref(), "min_count").unwrap_or(1) as usize;
            let count = artifact_type
                .as_deref()
                .and_then(|key| signals.artifact_counts.get(key))
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
                detail: artifact_type.map(|artifact_type| {
                    format!("artifact_type={artifact_type}, count={count}, min={min_count}")
                }),
                satisfied,
            }
        }
        WorkflowCheckKind::SessionTerminalIn => {
            let accepted = read_string_list(check.payload.as_ref(), "states");
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
            let action_key = read_string(check.payload.as_ref(), "action_key");
            let satisfied = action_key
                .as_deref()
                .map(|action_key| {
                    signals
                        .explicit_actions
                        .iter()
                        .any(|candidate| candidate == action_key)
                })
                .unwrap_or(false);
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
                detail: action_key,
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

pub fn workflow_artifact_type_tag(artifact_type: WorkflowRecordArtifactType) -> &'static str {
    match artifact_type {
        WorkflowRecordArtifactType::SessionSummary => "session_summary",
        WorkflowRecordArtifactType::JournalUpdate => "journal_update",
        WorkflowRecordArtifactType::ArchiveSuggestion => "archive_suggestion",
        WorkflowRecordArtifactType::PhaseNote => "phase_note",
        WorkflowRecordArtifactType::ChecklistEvidence => "checklist_evidence",
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
    use agentdash_domain::workflow::{
        LifecycleTransitionPolicy, WorkflowCheckSpec, WorkflowCompletionSpec,
    };

    use super::*;

    fn contract_with_checks() -> EffectiveSessionContract {
        EffectiveSessionContract {
            completion: WorkflowCompletionSpec {
                checks: vec![
                    WorkflowCheckSpec {
                        key: "task_ready".to_string(),
                        kind: WorkflowCheckKind::TaskStatusIn,
                        description: "task ready".to_string(),
                        payload: Some(serde_json::json!({
                            "statuses": ["awaiting_verification", "completed"]
                        })),
                    },
                    WorkflowCheckSpec {
                        key: "evidence".to_string(),
                        kind: WorkflowCheckKind::ChecklistEvidencePresent,
                        description: "evidence".to_string(),
                        payload: None,
                    },
                ],
                default_artifact_type: Some(WorkflowRecordArtifactType::ChecklistEvidence),
                default_artifact_title: Some("summary".to_string()),
            },
            ..EffectiveSessionContract::default()
        }
    }

    fn checks_transition() -> LifecycleTransitionSpec {
        LifecycleTransitionSpec {
            policy: LifecycleTransitionPolicy {
                kind: LifecycleTransitionPolicyKind::AllChecksPass,
                next_step_key: Some("record".to_string()),
                session_terminal_states: vec![],
                action_key: None,
            },
            on_failure: None,
        }
    }

    #[test]
    fn all_checks_pass_requires_all_checks() {
        let decision = evaluate_step_transition(
            &checks_transition(),
            &contract_with_checks(),
            &WorkflowCompletionSignalSet {
                task_status: Some("awaiting_verification".to_string()),
                checklist_evidence_present: false,
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(!decision.satisfied);
        assert!(!decision.should_complete_step);
        assert_eq!(decision.transition_policy, "all_checks_pass");
    }

    #[test]
    fn all_checks_pass_succeeds_when_contract_checks_are_met() {
        let decision = evaluate_step_transition(
            &checks_transition(),
            &contract_with_checks(),
            &WorkflowCompletionSignalSet {
                task_status: Some("completed".to_string()),
                checklist_evidence_present: true,
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(decision.satisfied);
        assert!(decision.should_complete_step);
    }

    #[test]
    fn session_terminal_transition_requires_matching_state() {
        let decision = evaluate_step_transition(
            &LifecycleTransitionSpec {
                policy: LifecycleTransitionPolicy {
                    kind: LifecycleTransitionPolicyKind::SessionTerminalMatches,
                    next_step_key: Some("check".to_string()),
                    session_terminal_states: vec![WorkflowSessionTerminalState::Completed],
                    action_key: None,
                },
                on_failure: None,
            },
            &EffectiveSessionContract::default(),
            &WorkflowCompletionSignalSet {
                session_terminal_state: Some(WorkflowSessionTerminalState::Failed),
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(!decision.satisfied);
        assert!(!decision.should_complete_step);
    }

    #[test]
    fn workflow_artifact_type_tag_is_stable() {
        assert_eq!(
            workflow_artifact_type_tag(WorkflowRecordArtifactType::ChecklistEvidence),
            "checklist_evidence"
        );
    }
}
