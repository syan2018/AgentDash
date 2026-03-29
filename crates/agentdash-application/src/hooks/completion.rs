use uuid::Uuid;

use agentdash_spi::{
    HookCompletionStatus, HookDiagnosticEntry, HookError, HookResolution, HookStepAdvanceRequest,
    SessionHookSnapshot,
};

use crate::workflow::{
    ActiveWorkflowProjection, WorkflowCompletionDecision, build_step_completion_artifact_drafts,
    execution_log as workflow_recording,
};

use super::snapshot_helpers::*;

pub(super) struct ActiveWorkflowLocator {
    pub(super) run_id: Uuid,
    pub(super) step_key: String,
}

pub(super) struct ActiveWorkflowChecklistEvidenceSummary {
    pub(super) artifact_type: agentdash_domain::workflow::WorkflowRecordArtifactType,
    pub(super) count: usize,
    pub(super) artifact_ids: Vec<Uuid>,
    pub(super) titles: Vec<String>,
}

impl super::provider::AppExecutionHookProvider {
    pub(super) async fn apply_completion_decision(
        &self,
        snapshot: &SessionHookSnapshot,
        decision: WorkflowCompletionDecision,
        resolution: &mut HookResolution,
    ) -> Result<(), HookError> {
        let source_summary = active_workflow_source_summary(snapshot);
        let source_refs = active_workflow_source_refs(snapshot);
        resolution
            .diagnostics
            .extend(
                decision
                    .evidence
                    .iter()
                    .map(|evidence| HookDiagnosticEntry {
                        code: evidence.code.clone(),
                        summary: evidence.summary.clone(),
                        detail: evidence.detail.clone(),
                        source_summary: source_summary.clone(),
                        source_refs: source_refs.clone(),
                    }),
            );

        let Some(locator) = active_workflow_locator(snapshot) else {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy.clone(),
                satisfied: decision.satisfied,
                advanced: false,
                reason: decision
                    .blocking_reason
                    .or(decision.summary)
                    .unwrap_or_else(|| "当前没有可推进的 active workflow".to_string()),
            });
            return Ok(());
        };

        if !decision.should_complete_step {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy,
                satisfied: decision.satisfied,
                advanced: false,
                reason: decision
                    .blocking_reason
                    .or(decision.summary)
                    .unwrap_or_else(|| "completion 条件尚未满足".to_string()),
            });
            return Ok(());
        }

        let run = self
            .workflow_builder
            .get_lifecycle_run(locator.run_id)
            .await?;
        let Some(run) = run else {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy,
                satisfied: true,
                advanced: false,
                reason: format!("workflow run {} 已不存在，无法推进", locator.run_id),
            });
            resolution.diagnostics.push(HookDiagnosticEntry {
                code: "workflow_run_missing_for_completion".to_string(),
                summary: "Hook 发现 workflow run 已不存在，无法写回 completion".to_string(),
                detail: Some(locator.run_id.to_string()),
                source_summary,
                source_refs,
            });
            return Ok(());
        };

        if run.current_step_key.as_deref() != Some(locator.step_key.as_str()) {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy,
                satisfied: true,
                advanced: false,
                reason: format!(
                    "workflow 已离开当前 step（当前为 {:?}），无需重复推进",
                    run.current_step_key
                ),
            });
            return Ok(());
        }

        let record_artifacts = build_completion_record_artifacts_from_snapshot(snapshot, &decision);
        let completion_summary = decision.summary.clone();

        resolution.completion = Some(HookCompletionStatus {
            mode: decision.transition_policy.clone(),
            satisfied: true,
            advanced: false,
            reason: completion_summary
                .clone()
                .unwrap_or_else(|| "completion 条件满足，等待 post-evaluate 推进".to_string()),
        });
        let run_id_str = locator.run_id.to_string();
        let step_key_str = locator.step_key.clone();

        resolution.pending_advance = Some(HookStepAdvanceRequest {
            run_id: run_id_str.clone(),
            step_key: step_key_str.clone(),
            completion_mode: decision.transition_policy,
            summary: completion_summary.clone(),
            record_artifacts: record_artifacts
                .into_iter()
                .map(|a| {
                    serde_json::json!({
                        "title": a.title,
                        "artifact_type": a.artifact_type,
                        "content": a.content,
                    })
                })
                .collect(),
        });

        resolution.pending_execution_log.push(
            workflow_recording::completion_evaluated_entry(
                &run_id_str,
                &step_key_str,
                true,
                completion_summary
                    .as_deref()
                    .unwrap_or("completion satisfied"),
            ),
        );
        resolution.pending_execution_log.push(
            workflow_recording::step_completed_entry(
                &run_id_str,
                &step_key_str,
                completion_summary
                    .as_deref()
                    .unwrap_or("step completed by hook"),
            ),
        );

        resolution.diagnostics.push(HookDiagnosticEntry {
            code: "workflow_step_advance_requested".to_string(),
            summary: format!(
                "Hook 产出 step 推进信号：run={}, step=`{}`",
                locator.run_id, locator.step_key
            ),
            detail: None,
            source_summary,
            source_refs,
        });

        Ok(())
    }
}

pub(super) fn active_workflow_checklist_evidence_summary(
    workflow: &ActiveWorkflowProjection,
) -> ActiveWorkflowChecklistEvidenceSummary {
    let artifact_type = workflow
        .effective_contract
        .completion
        .default_artifact_type
        .unwrap_or(agentdash_domain::workflow::WorkflowRecordArtifactType::PhaseNote);
    let matching = workflow
        .run
        .record_artifacts
        .iter()
        .filter(|artifact| {
            artifact.step_key == workflow.active_step.key
                && artifact.artifact_type == artifact_type
                && !artifact.content.trim().is_empty()
        })
        .collect::<Vec<_>>();

    ActiveWorkflowChecklistEvidenceSummary {
        artifact_type,
        count: matching.len(),
        artifact_ids: matching.iter().map(|artifact| artifact.id).collect(),
        titles: matching
            .iter()
            .map(|artifact| artifact.title.trim())
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect(),
    }
}

pub(super) fn workflow_record_artifact_type_tag(
    artifact_type: agentdash_domain::workflow::WorkflowRecordArtifactType,
) -> &'static str {
    match artifact_type {
        agentdash_domain::workflow::WorkflowRecordArtifactType::SessionSummary => "session_summary",
        agentdash_domain::workflow::WorkflowRecordArtifactType::JournalUpdate => "journal_update",
        agentdash_domain::workflow::WorkflowRecordArtifactType::ArchiveSuggestion => {
            "archive_suggestion"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::PhaseNote => "phase_note",
        agentdash_domain::workflow::WorkflowRecordArtifactType::ChecklistEvidence => {
            "checklist_evidence"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::ExecutionTrace => {
            "execution_trace"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::DecisionRecord => {
            "decision_record"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::ContextSnapshot => {
            "context_snapshot"
        }
    }
}

fn build_completion_record_artifacts_from_snapshot(
    snapshot: &SessionHookSnapshot,
    decision: &WorkflowCompletionDecision,
) -> Vec<crate::workflow::WorkflowRecordArtifactDraft> {
    build_step_completion_artifact_drafts(
        workflow_step_key(snapshot).unwrap_or("workflow_step"),
        active_workflow_default_artifact_type(snapshot),
        active_workflow_default_artifact_title(snapshot),
        decision,
    )
}
