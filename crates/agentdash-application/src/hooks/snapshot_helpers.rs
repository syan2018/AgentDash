use agentdash_domain::workflow::{LifecycleRunStatus, WorkflowHookRuleSpec};
use agentdash_spi::{
    ActiveWorkflowMeta, HookDiagnosticEntry, HookOwnerSummary, SessionHookSnapshot,
};

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

pub(crate) fn session_permission_policy(snapshot: &SessionHookSnapshot) -> Option<&str> {
    snapshot.metadata.as_ref()?.permission_policy.as_deref()
}

pub(crate) fn workflow_step_key(snapshot: &SessionHookSnapshot) -> Option<&str> {
    active_workflow(snapshot)?.step_key.as_deref()
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
