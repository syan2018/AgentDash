use crate::workflow::{
    WorkflowCompletionDecision, WorkflowCompletionSignalSet, evaluate_step_completion,
};
use agentdash_domain::workflow::{
    EffectiveSessionContract, LifecycleRunStatus, WorkflowConstraintKind, WorkflowHookRuleSpec,
    WorkflowHookTrigger, WorkflowSessionTerminalState,
};
use agentdash_spi::{
    ActiveWorkflowMeta, HookDiagnosticEntry, HookOwnerSummary, SessionHookSnapshot,
};

use super::ActiveWorkflowLocator;

pub struct ResolvedOwnerSummary {
    pub summary: HookOwnerSummary,
    pub diagnostics: Vec<HookDiagnosticEntry>,
}

pub(crate) fn workflow_run_status_tag(status: LifecycleRunStatus) -> &'static str {
    match status {
        LifecycleRunStatus::Draft => "draft",
        LifecycleRunStatus::Ready => "ready",
        LifecycleRunStatus::Running => "running",
        LifecycleRunStatus::Blocked => "blocked",
        LifecycleRunStatus::Completed => "completed",
        LifecycleRunStatus::Failed => "failed",
        LifecycleRunStatus::Cancelled => "cancelled",
    }
}

fn active_workflow(snapshot: &SessionHookSnapshot) -> Option<&ActiveWorkflowMeta> {
    snapshot.metadata.as_ref()?.active_workflow.as_ref()
}

pub(crate) fn workflow_transition_policy(snapshot: &SessionHookSnapshot) -> Option<&str> {
    active_workflow(snapshot)?.transition_policy.as_deref()
}

pub(crate) fn workflow_auto_completion_snapshot(snapshot: &SessionHookSnapshot) -> bool {
    matches!(
        workflow_transition_policy(snapshot),
        Some("auto" | "all_checks_pass" | "any_checks_pass" | "session_terminal_matches",)
    )
}

pub(crate) fn active_workflow_checklist_evidence(snapshot: &SessionHookSnapshot) -> bool {
    active_workflow(snapshot)
        .and_then(|aw| aw.checklist_evidence_present)
        .unwrap_or(false)
}

pub(crate) fn active_workflow_default_artifact_type(
    snapshot: &SessionHookSnapshot,
) -> Option<agentdash_domain::workflow::WorkflowRecordArtifactType> {
    active_workflow(snapshot)?.default_artifact_type
}

pub(crate) fn active_workflow_default_artifact_title(
    snapshot: &SessionHookSnapshot,
) -> Option<&str> {
    active_workflow(snapshot)?.default_artifact_title.as_deref()
}

pub(crate) fn session_permission_policy(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot.metadata.as_ref()?.permission_policy.as_deref()
}

pub(crate) fn requires_supervised_tool_approval(tool_name: &str) -> bool {
    let normalized = tool_name.to_ascii_lowercase();
    normalized.ends_with("shell_exec")
        || normalized.ends_with("shell")
        || normalized.ends_with("write_file")
        || normalized.ends_with("fs_apply_patch")
        || normalized.contains("delete")
        || normalized.contains("remove")
        || normalized.contains("move")
        || normalized.contains("rename")
}

pub(crate) fn workflow_step_key(snapshot: &SessionHookSnapshot) -> Option<&str> {
    active_workflow(snapshot)?.step_key.as_deref()
}

pub(crate) fn snapshot_default_mount_root_ref(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()?
        .default_mount_root_ref
        .as_deref()
}

pub(crate) fn active_workflow_locator(
    snapshot: &SessionHookSnapshot,
) -> Option<ActiveWorkflowLocator> {
    let aw = active_workflow(snapshot)?;
    Some(ActiveWorkflowLocator {
        run_id: aw.run_id?,
        step_key: aw.step_key.clone()?,
    })
}

pub(crate) fn active_workflow_contract(
    snapshot: &SessionHookSnapshot,
) -> Option<EffectiveSessionContract> {
    active_workflow(snapshot)?.effective_contract.clone()
}

pub(crate) fn completion_decision_for_active_workflow_snapshot(
    snapshot: &SessionHookSnapshot,
    signals: &WorkflowCompletionSignalSet,
) -> Option<WorkflowCompletionDecision> {
    let contract = active_workflow_contract(snapshot)?;
    Some(evaluate_step_completion(
        workflow_auto_completion_snapshot(snapshot).then_some(&contract.completion),
        signals,
    ))
}

pub(crate) fn active_workflow_constraints(
    snapshot: &SessionHookSnapshot,
) -> Vec<agentdash_domain::workflow::WorkflowConstraintSpec> {
    active_workflow_contract(snapshot)
        .map(|contract| contract.constraints)
        .unwrap_or_default()
}

pub(crate) fn active_workflow_denied_record_artifact_types(
    snapshot: &SessionHookSnapshot,
) -> Vec<String> {
    active_workflow_constraints(snapshot)
        .into_iter()
        .filter(|constraint| constraint.kind == WorkflowConstraintKind::Custom)
        .flat_map(|constraint| {
            let payload = constraint.payload.as_ref();
            let is_record_gate = payload
                .and_then(|value| value.get("policy"))
                .and_then(serde_json::Value::as_str)
                == Some("deny_record_artifact_types");
            if !is_record_gate {
                return Vec::new();
            }
            payload
                .and_then(|value| value.get("artifact_types"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(crate) fn checklist_evidence_present(snapshot: &SessionHookSnapshot) -> bool {
    active_workflow_checklist_evidence(snapshot)
}

pub(crate) fn parse_session_terminal_state(
    payload: Option<&serde_json::Value>,
) -> Option<WorkflowSessionTerminalState> {
    match payload
        .and_then(|value| value.get("terminal_state"))
        .and_then(serde_json::Value::as_str)
    {
        Some("completed") => Some(WorkflowSessionTerminalState::Completed),
        Some("failed") => Some(WorkflowSessionTerminalState::Failed),
        Some("interrupted") => Some(WorkflowSessionTerminalState::Interrupted),
        _ => None,
    }
}

/// 检查 snapshot 是否关联了 task owner
pub(crate) fn snapshot_has_task_owner(snapshot: &SessionHookSnapshot) -> bool {
    snapshot
        .owners
        .iter()
        .any(|o| o.owner_type == "task" && o.task_id.is_some())
}

/// 基于 owner type 返回默认 hook rules。
///
/// 当 session 关联了某种 owner 但该 owner 没有自己的 Workflow（或 Workflow 中
/// 未定义某些阶段的 lifecycle rules）时，由此函数提供 owner 级别的"内置默认"。
/// 调用方在 `apply_hook_rules` 中统一评估，与 workflow contract rules 合并。
pub(crate) fn owner_default_hook_rules(
    snapshot: &SessionHookSnapshot,
) -> Vec<WorkflowHookRuleSpec> {
    let mut rules = Vec::new();

    if snapshot_has_task_owner(snapshot) {
        rules.push(WorkflowHookRuleSpec {
            key: "builtin:task_session_terminal".to_string(),
            trigger: WorkflowHookTrigger::SessionTerminal,
            description: "Task 默认 lifecycle: session 终止时根据 execution_mode 转换 task 状态"
                .to_string(),
            preset: Some("task_session_terminal".to_string()),
            params: None,
            script: None,
            enabled: true,
        });
    }

    // 后续可为 story / project 等 owner type 追加各自的默认 rules

    rules
}

pub(crate) fn active_workflow_hook_rules(
    snapshot: &SessionHookSnapshot,
) -> &[WorkflowHookRuleSpec] {
    active_workflow(snapshot)
        .and_then(|aw| aw.effective_contract.as_ref())
        .map(|c| c.hook_rules.as_slice())
        .unwrap_or_default()
}

/// Build a source string from the snapshot's active workflow metadata.
pub(crate) fn active_workflow_source_from_snapshot(snapshot: &SessionHookSnapshot) -> String {
    let lifecycle_key = active_workflow(snapshot)
        .and_then(|aw| aw.lifecycle_key.as_deref())
        .unwrap_or("unknown");
    let step_key = workflow_step_key(snapshot).unwrap_or("unknown");
    format!("workflow:{lifecycle_key}:{step_key}")
}
