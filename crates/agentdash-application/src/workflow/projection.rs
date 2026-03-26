use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleStepDefinition, WorkflowDefinition, WorkflowDefinitionRepository, WorkflowTargetKind,
    build_effective_contract,
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub lifecycle: LifecycleDefinition,
    pub active_step: LifecycleStepDefinition,
    pub primary_workflow: Option<WorkflowDefinition>,
    pub effective_contract: agentdash_domain::workflow::EffectiveSessionContract,
    pub target: WorkflowTargetSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTargetSummary {
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    pub target_label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowProjectionSnapshot {
    pub run_id: Uuid,
    pub lifecycle_id: Uuid,
    pub lifecycle_key: String,
    pub lifecycle_name: String,
    pub run_status: String,
    pub step_key: String,
    pub step_title: String,
    pub primary_workflow_id: Option<Uuid>,
    pub primary_workflow_key: Option<String>,
    pub primary_workflow_name: Option<String>,
    pub target: WorkflowTargetSummary,
    pub instruction_count: usize,
    pub binding_count: usize,
    pub resolved_binding_count: usize,
    pub attachment_count: usize,
    pub constraint_count: usize,
    pub check_count: usize,
}

impl ActiveWorkflowProjection {
    pub fn to_snapshot(&self) -> WorkflowProjectionSnapshot {
        let step_title = if self.active_step.description.trim().is_empty() {
            self.active_step.key.clone()
        } else {
            self.active_step.description.clone()
        };
        WorkflowProjectionSnapshot {
            run_id: self.run.id,
            lifecycle_id: self.lifecycle.id,
            lifecycle_key: self.lifecycle.key.clone(),
            lifecycle_name: self.lifecycle.name.clone(),
            run_status: lifecycle_run_status_tag(self.run.status).to_string(),
            step_key: self.active_step.key.clone(),
            step_title,
            primary_workflow_id: self.primary_workflow.as_ref().map(|w| w.id),
            primary_workflow_key: self.primary_workflow.as_ref().map(|w| w.key.clone()),
            primary_workflow_name: self.primary_workflow.as_ref().map(|w| w.name.clone()),
            target: self.target.clone(),
            instruction_count: self.effective_contract.injection.instructions.len(),
            binding_count: self.effective_contract.injection.context_bindings.len(),
            resolved_binding_count: 0,
            attachment_count: 0,
            constraint_count: self.effective_contract.constraints.len(),
            check_count: self.effective_contract.completion.checks.len(),
        }
    }
}

fn lifecycle_run_status_tag(status: agentdash_domain::workflow::LifecycleRunStatus) -> &'static str {
    use agentdash_domain::workflow::LifecycleRunStatus;

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

pub async fn resolve_active_workflow_projection(
    target_kind: WorkflowTargetKind,
    target_id: Uuid,
    target_label: Option<String>,
    definition_repo: &dyn WorkflowDefinitionRepository,
    lifecycle_repo: &dyn LifecycleDefinitionRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let runs = run_repo
        .list_by_target(target_kind, target_id)
        .await
        .map_err(|e| format!("加载 lifecycle runs 失败: {e}"))?;

    let Some(run) = super::run::select_active_run(runs) else {
        return Ok(None);
    };
    let Some(current_step_key) = run.current_step_key.as_deref() else {
        return Ok(None);
    };

    let lifecycle = lifecycle_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(|e| format!("加载 lifecycle definition 失败: {e}"))?
        .filter(|definition| definition.is_active());
    let Some(lifecycle) = lifecycle else {
        return Ok(None);
    };

    let Some(active_step) = lifecycle
        .steps
        .iter()
        .find(|step| step.key == current_step_key)
        .cloned()
    else {
        return Ok(None);
    };

    let primary_workflow = match active_step
        .workflow_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(wk) => {
            let Some(wf) = definition_repo
                .get_by_key(wk)
                .await
                .map_err(|e| format!("加载 workflow 失败: {e}"))?
                .filter(|definition| definition.is_active())
            else {
                return Ok(None);
            };
            Some(wf)
        }
        None => None,
    };

    let effective_contract = build_effective_contract(
        &lifecycle.key,
        &active_step.key,
        primary_workflow.as_ref(),
    );

    Ok(Some(ActiveWorkflowProjection {
        run,
        lifecycle,
        active_step,
        primary_workflow,
        effective_contract,
        target: WorkflowTargetSummary {
            target_kind,
            target_id,
            target_label,
        },
    }))
}
