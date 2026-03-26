use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    WorkflowAgentRole, WorkflowAssignmentRepository, WorkflowDefinitionRepository,
    WorkflowSessionBinding, WorkflowTargetKind,
};

use super::error::WorkflowApplicationError;
use super::run::{merge_session_binding, select_active_run};

#[derive(Debug, Clone)]
pub struct ResolvedAssignment {
    pub lifecycle: LifecycleDefinition,
    pub run: LifecycleRun,
    pub newly_created: bool,
}

pub struct ResolveAssignmentInput {
    pub project_id: Uuid,
    pub role: WorkflowAgentRole,
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    pub session_binding_id: Option<Uuid>,
}

pub async fn resolve_assignment_and_ensure_run<D, L, A, R>(
    definition_repo: &D,
    lifecycle_repo: &L,
    assignment_repo: &A,
    run_repo: &R,
    input: ResolveAssignmentInput,
) -> Result<Option<ResolvedAssignment>, WorkflowApplicationError>
where
    D: WorkflowDefinitionRepository + ?Sized,
    L: LifecycleDefinitionRepository + ?Sized,
    A: WorkflowAssignmentRepository + ?Sized,
    R: LifecycleRunRepository + ?Sized,
{
    let assignments = assignment_repo
        .list_by_project_and_role(input.project_id, input.role)
        .await?;
    let default_assignment = assignments.into_iter().find(|a| a.enabled && a.is_default);

    let assignment = match default_assignment {
        Some(a) => a,
        None => return Ok(None),
    };

    let lifecycle = lifecycle_repo
        .get_by_id(assignment.lifecycle_id)
        .await?
        .ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!(
                "assignment 引用的 lifecycle_definition 不存在: {}",
                assignment.lifecycle_id
            ))
        })?;

    if !lifecycle.is_active() {
        return Ok(None);
    }
    if lifecycle.target_kind != input.target_kind {
        return Err(WorkflowApplicationError::Conflict(format!(
            "assignment 引用的 lifecycle `{}` target_kind={:?} 与请求的 {:?} 不匹配",
            lifecycle.key, lifecycle.target_kind, input.target_kind
        )));
    }

    let existing_runs = run_repo
        .list_by_target(input.target_kind, input.target_id)
        .await?;
    if let Some(active_run) = select_active_run(existing_runs) {
        return Ok(Some(ResolvedAssignment {
            lifecycle,
            run: active_run,
            newly_created: false,
        }));
    }

    let mut run = LifecycleRun::new(
        input.project_id,
        lifecycle.id,
        input.target_kind,
        input.target_id,
        &lifecycle.steps,
        &lifecycle.entry_step_key,
    )
    .map_err(WorkflowApplicationError::BadRequest)?;

    if let Some(first_step) = lifecycle.steps.first() {
        let workflow = definition_repo
            .get_by_key(&first_step.primary_workflow_key)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle 首步引用的 workflow_definition 不存在: {}",
                    first_step.primary_workflow_key
                ))
            })?;
        let session_requirement = merge_session_binding(
            first_step.session_binding,
            workflow.contract.injection.session_binding,
        );
        if matches!(
            session_requirement,
            WorkflowSessionBinding::Required | WorkflowSessionBinding::Optional
        ) {
            if let Some(binding_id) = input.session_binding_id {
                let _ = run.attach_session_binding(&first_step.key, binding_id);
            }
        }
        if session_requirement != WorkflowSessionBinding::Required
            || input.session_binding_id.is_some()
        {
            let _ = run.activate_step(&first_step.key);
        }
    }

    run_repo.create(&run).await?;

    Ok(Some(ResolvedAssignment {
        lifecycle,
        run,
        newly_created: true,
    }))
}
