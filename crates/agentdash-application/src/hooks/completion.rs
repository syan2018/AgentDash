use agentdash_spi::{
    HookCompletionStatus, HookDiagnosticEntry, HookError, HookResolution, HookStepAdvanceRequest,
    SessionHookSnapshot,
};

use crate::workflow::{
    WorkflowCompletionDecision,
    execution_log as workflow_recording,
};

use super::snapshot_helpers::*;

impl super::provider::AppExecutionHookProvider {
    pub(super) async fn apply_completion_decision(
        &self,
        snapshot: &SessionHookSnapshot,
        decision: WorkflowCompletionDecision,
        resolution: &mut HookResolution,
    ) -> Result<(), HookError> {
        resolution
            .diagnostics
            .extend(
                decision
                    .evidence
                    .iter()
                    .map(|evidence| HookDiagnosticEntry {
                        code: evidence.code.clone(),
                        message: evidence.summary.clone(),
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
                message: "Hook 发现 workflow run 已不存在，无法写回 completion".to_string(),
            });
            return Ok(());
        };

        if run.current_step_key() != Some(locator.step_key.as_str()) {
            resolution.completion = Some(HookCompletionStatus {
                mode: decision.transition_policy,
                satisfied: true,
                advanced: false,
                reason: format!(
                    "workflow 已离开当前 step（当前为 {:?}），无需重复推进",
                    run.current_step_key()
                ),
            });
            return Ok(());
        }

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
            record_artifacts: vec![],
        });

        resolution
            .pending_execution_log
            .push(workflow_recording::completion_evaluated_entry(
                &run_id_str,
                &step_key_str,
                true,
                completion_summary
                    .as_deref()
                    .unwrap_or("completion satisfied"),
            ));
        resolution
            .pending_execution_log
            .push(workflow_recording::step_completed_entry(
                &run_id_str,
                &step_key_str,
                completion_summary
                    .as_deref()
                    .unwrap_or("step completed by hook"),
            ));

        resolution.diagnostics.push(HookDiagnosticEntry {
            code: "workflow_step_advance_requested".to_string(),
            message: format!(
                "Hook 产出 step 推进信号：run={}, step=`{}`",
                locator.run_id, locator.step_key
            ),
        });

        Ok(())
    }
}

