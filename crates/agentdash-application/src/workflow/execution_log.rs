//! Lifecycle execution log recording pipeline.
//!
//! Provides helpers to build `PendingExecutionLogEntry` items and flush them
//! to `LifecycleRun.execution_log` after each hook evaluation cycle.

use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleExecutionEntry, LifecycleExecutionEventKind, LifecycleRunRepository,
};
use agentdash_spi::hooks::PendingExecutionLogEntry;

use super::error::WorkflowApplicationError;

fn parse_event_kind(s: &str) -> Option<LifecycleExecutionEventKind> {
    match s {
        "step_activated" => Some(LifecycleExecutionEventKind::StepActivated),
        "step_completed" => Some(LifecycleExecutionEventKind::StepCompleted),
        "constraint_blocked" => Some(LifecycleExecutionEventKind::ConstraintBlocked),
        "completion_evaluated" => Some(LifecycleExecutionEventKind::CompletionEvaluated),
        "artifact_appended" => Some(LifecycleExecutionEventKind::ArtifactAppended),
        "context_injected" => Some(LifecycleExecutionEventKind::ContextInjected),
        _ => None,
    }
}

fn to_domain_entry(entry: &PendingExecutionLogEntry) -> Option<LifecycleExecutionEntry> {
    Some(LifecycleExecutionEntry {
        timestamp: Utc::now(),
        step_key: entry.step_key.clone(),
        event_kind: parse_event_kind(&entry.event_kind)?,
        summary: entry.summary.clone(),
        detail: entry.detail.clone(),
    })
}

/// Flush pending entries grouped by `run_id`.
pub async fn flush_execution_log_entries(
    repo: &dyn LifecycleRunRepository,
    entries: Vec<PendingExecutionLogEntry>,
) -> Result<(), WorkflowApplicationError> {
    let mut by_run: HashMap<String, Vec<LifecycleExecutionEntry>> = HashMap::new();
    for entry in &entries {
        if let Some(domain_entry) = to_domain_entry(entry) {
            by_run
                .entry(entry.run_id.clone())
                .or_default()
                .push(domain_entry);
        }
    }

    for (run_id_str, domain_entries) in by_run {
        let run_id = Uuid::parse_str(&run_id_str).map_err(|e| {
            WorkflowApplicationError::Internal(format!("invalid run_id in execution log: {e}"))
        })?;

        let Some(mut run) = repo.get_by_id(run_id).await? else {
            continue;
        };

        run.append_execution_log(domain_entries);

        repo.update(&run).await?;
    }

    Ok(())
}

/// Build a `PendingExecutionLogEntry` for a step-completed event.
pub fn step_completed_entry(
    run_id: &str,
    step_key: &str,
    summary: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        step_key: step_key.to_string(),
        event_kind: "step_completed".to_string(),
        summary: summary.to_string(),
        detail: None,
    }
}

/// Build a `PendingExecutionLogEntry` for a completion-evaluated event.
pub fn completion_evaluated_entry(
    run_id: &str,
    step_key: &str,
    satisfied: bool,
    summary: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        step_key: step_key.to_string(),
        event_kind: "completion_evaluated".to_string(),
        summary: summary.to_string(),
        detail: Some(serde_json::json!({ "satisfied": satisfied })),
    }
}

/// Build a `PendingExecutionLogEntry` for a constraint-blocked event.
pub fn constraint_blocked_entry(
    run_id: &str,
    step_key: &str,
    reason: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        step_key: step_key.to_string(),
        event_kind: "constraint_blocked".to_string(),
        summary: reason.to_string(),
        detail: None,
    }
}

/// Build a `PendingExecutionLogEntry` for a context-injected event.
pub fn context_injected_entry(
    run_id: &str,
    step_key: &str,
    summary: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        step_key: step_key.to_string(),
        event_kind: "context_injected".to_string(),
        summary: summary.to_string(),
        detail: None,
    }
}

/// Build a `PendingExecutionLogEntry` for an artifact-appended event.
pub fn artifact_appended_entry(
    run_id: &str,
    step_key: &str,
    artifact_type: &str,
    title: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        step_key: step_key.to_string(),
        event_kind: "artifact_appended".to_string(),
        summary: format!("{artifact_type}: {title}"),
        detail: Some(serde_json::json!({
            "artifact_type": artifact_type,
            "title": title,
        })),
    }
}
