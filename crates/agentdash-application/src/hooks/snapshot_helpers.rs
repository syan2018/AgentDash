use uuid::Uuid;

use crate::workflow::{
    evaluate_step_completion, WorkflowCompletionDecision, WorkflowCompletionSignalSet,
};
use agentdash_domain::task::TaskStatus;
use agentdash_domain::workflow::{
    EffectiveSessionContract, LifecycleRunStatus, WorkflowCheckKind, WorkflowConstraintKind,
    WorkflowSessionTerminalState,
};
use agentdash_executor::{
    HookDiagnosticEntry, HookOwnerSummary, HookSourceLayer, HookSourceRef, SessionHookSnapshot,
};

use super::ActiveWorkflowLocator;

pub(crate) struct ResolvedOwnerSummary {
    pub(crate) summary: HookOwnerSummary,
    pub(crate) diagnostics: Vec<HookDiagnosticEntry>,
    pub(crate) task_status: Option<String>,
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

pub(crate) fn task_status_tag(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Assigned => "assigned",
        TaskStatus::Running => "running",
        TaskStatus::AwaitingVerification => "awaiting_verification",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
    }
}

pub(crate) fn workflow_transition_policy(snapshot: &SessionHookSnapshot) -> Option<&str> {
    let aw = snapshot.metadata.as_ref()?.get("active_workflow")?;
    aw.get("step_advance")
        .or_else(|| aw.get("step_completion_mode"))
        .or_else(|| aw.get("transition_policy"))
        .and_then(serde_json::Value::as_str)
}

pub(crate) fn workflow_auto_completion_snapshot(snapshot: &SessionHookSnapshot) -> bool {
    matches!(
        workflow_transition_policy(snapshot),
        Some(
            "auto" | "all_checks_pass" | "any_checks_pass" | "session_terminal_matches",
        )
    )
}

pub(crate) fn active_workflow_checklist_evidence(snapshot: &SessionHookSnapshot) -> bool {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("checklist_evidence_present"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn parse_workflow_record_artifact_type_tag(
    value: &str,
) -> Option<agentdash_domain::workflow::WorkflowRecordArtifactType> {
    match value {
        "session_summary" => {
            Some(agentdash_domain::workflow::WorkflowRecordArtifactType::SessionSummary)
        }
        "journal_update" => {
            Some(agentdash_domain::workflow::WorkflowRecordArtifactType::JournalUpdate)
        }
        "archive_suggestion" => {
            Some(agentdash_domain::workflow::WorkflowRecordArtifactType::ArchiveSuggestion)
        }
        "phase_note" => Some(agentdash_domain::workflow::WorkflowRecordArtifactType::PhaseNote),
        "checklist_evidence" => {
            Some(agentdash_domain::workflow::WorkflowRecordArtifactType::ChecklistEvidence)
        }
        "execution_trace" => {
            Some(agentdash_domain::workflow::WorkflowRecordArtifactType::ExecutionTrace)
        }
        "decision_record" => {
            Some(agentdash_domain::workflow::WorkflowRecordArtifactType::DecisionRecord)
        }
        "context_snapshot" => {
            Some(agentdash_domain::workflow::WorkflowRecordArtifactType::ContextSnapshot)
        }
        _ => None,
    }
}

pub(crate) fn active_workflow_default_artifact_type(
    snapshot: &SessionHookSnapshot,
) -> Option<agentdash_domain::workflow::WorkflowRecordArtifactType> {
    parse_workflow_record_artifact_type_tag(
        snapshot
            .metadata
            .as_ref()
            .and_then(|value| value.get("active_workflow"))
            .and_then(|value| value.get("default_artifact_type"))
            .and_then(serde_json::Value::as_str)?,
    )
}

pub(crate) fn active_workflow_default_artifact_title(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("default_artifact_title"))
        .and_then(serde_json::Value::as_str)
}

pub(crate) fn session_permission_policy(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("permission_policy"))
        .and_then(serde_json::Value::as_str)
}

pub(crate) fn requires_supervised_tool_approval(tool_name: &str) -> bool {
    let normalized = tool_name.to_ascii_lowercase();
    normalized.ends_with("shell_exec")
        || normalized.ends_with("shell")
        || normalized.ends_with("write_file")
        || normalized.ends_with("fs_write")
        || normalized.contains("delete")
        || normalized.contains("remove")
        || normalized.contains("move")
        || normalized.contains("rename")
}

pub(crate) fn workflow_step_key(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("step_key"))
        .and_then(serde_json::Value::as_str)
}

pub(crate) fn active_task_status(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_task"))
        .and_then(|value| value.get("status"))
        .and_then(serde_json::Value::as_str)
}

pub(crate) fn snapshot_workspace_root(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("workspace_root"))
        .and_then(serde_json::Value::as_str)
}

pub(crate) fn active_workflow_source_summary(snapshot: &SessionHookSnapshot) -> Vec<String> {
    let mut summary = Vec::new();
    if let Some(lifecycle_key) = snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("lifecycle_key"))
        .and_then(serde_json::Value::as_str)
    {
        summary.push(format!("lifecycle:{lifecycle_key}"));
    }
    if let Some(step_key) = workflow_step_key(snapshot) {
        summary.push(format!("workflow_step:{step_key}"));
    }
    summary
}

pub(crate) fn active_workflow_source_refs(snapshot: &SessionHookSnapshot) -> Vec<HookSourceRef> {
    snapshot
        .sources
        .iter()
        .filter(|source| source.layer == HookSourceLayer::Workflow)
        .cloned()
        .collect()
}

pub(crate) fn active_workflow_locator(snapshot: &SessionHookSnapshot) -> Option<ActiveWorkflowLocator> {
    let run_id = snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("run_id"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())?;
    let step_key = workflow_step_key(snapshot)?.to_string();
    Some(ActiveWorkflowLocator { run_id, step_key })
}

pub(crate) fn active_workflow_contract(snapshot: &SessionHookSnapshot) -> Option<EffectiveSessionContract> {
    serde_json::from_value(
        snapshot
            .metadata
            .as_ref()?
            .get("active_workflow")?
            .get("effective_contract")?
            .clone(),
    )
    .ok()
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

pub(crate) fn active_workflow_checks(
    snapshot: &SessionHookSnapshot,
) -> Vec<agentdash_domain::workflow::WorkflowCheckSpec> {
    active_workflow_contract(snapshot)
        .map(|contract| contract.completion.checks)
        .unwrap_or_default()
}

pub(crate) fn active_workflow_denied_task_statuses(snapshot: &SessionHookSnapshot) -> Vec<String> {
    active_workflow_constraints(snapshot)
        .into_iter()
        .filter(|constraint| constraint.kind == WorkflowConstraintKind::DenyTaskStatusTransition)
        .flat_map(|constraint| {
            constraint
                .payload
                .as_ref()
                .and_then(|payload| payload.get("to"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(crate) fn active_workflow_denied_record_artifact_types(snapshot: &SessionHookSnapshot) -> Vec<String> {
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

pub(crate) fn active_workflow_task_status_check_statuses(snapshot: &SessionHookSnapshot) -> Vec<String> {
    active_workflow_checks(snapshot)
        .into_iter()
        .filter(|check| check.kind == WorkflowCheckKind::TaskStatusIn)
        .flat_map(|check| {
            check
                .payload
                .as_ref()
                .and_then(|payload| payload.get("statuses"))
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
