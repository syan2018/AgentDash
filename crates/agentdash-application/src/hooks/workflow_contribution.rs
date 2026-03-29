use agentdash_spi::{HookContextFragment, HookPolicyView, HookSourceRef};

use crate::workflow::ActiveWorkflowProjection;

use super::{lifecycle_step_advance_label, workflow_scope_key};

pub(super) fn build_step_summary_markdown(workflow: &ActiveWorkflowProjection) -> String {
    let wf_line = match workflow.primary_workflow.as_ref() {
        Some(w) => format!("- workflow: {} (`{}`)", w.name, w.key),
        None => "- workflow: (none)".to_string(),
    };
    format!(
        "## Active Workflow Step\n- lifecycle: {} (`{}`)\n- step: `{}`\n{}\n- advance: `{}`\n- status: `{}`\n\n{}",
        workflow.lifecycle.name,
        workflow.lifecycle.key,
        workflow.active_step.key,
        wf_line,
        lifecycle_step_advance_label(&workflow.active_step),
        super::snapshot_helpers::workflow_run_status_tag(workflow.run.status),
        workflow.active_step.description
    )
}

pub(super) fn build_workflow_step_fragments(
    workflow: &ActiveWorkflowProjection,
    source_summary: &[String],
    source_refs: &[HookSourceRef],
) -> Vec<HookContextFragment> {
    let mut fragments = vec![HookContextFragment {
        slot: "workflow".to_string(),
        label: "active_workflow_step".to_string(),
        content: build_step_summary_markdown(workflow),
        source_summary: source_summary.to_vec(),
        source_refs: source_refs.to_vec(),
    }];

    if !workflow
        .effective_contract
        .injection
        .instructions
        .is_empty()
    {
        fragments.push(HookContextFragment {
            slot: "workflow".to_string(),
            label: "active_workflow_instructions".to_string(),
            content: build_instruction_injection_markdown(
                &workflow.effective_contract.injection.instructions,
            ),
            source_summary: source_summary.to_vec(),
            source_refs: source_refs.to_vec(),
        });
    }

    fragments
}

fn build_instruction_injection_markdown(instructions: &[String]) -> String {
    let body = instructions
        .iter()
        .map(|instruction| format!("- {instruction}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("## Workflow Instructions\n{body}")
}

pub(super) fn build_workflow_policies(
    workflow: &ActiveWorkflowProjection,
    source_summary: &[String],
    source_refs: &[HookSourceRef],
) -> Vec<HookPolicyView> {
    let scope = workflow_scope_key(workflow);
    let step_advance = lifecycle_step_advance_label(&workflow.active_step);
    let mut policies = vec![HookPolicyView {
        key: format!(
            "workflow:{}:{}:step_advance",
            scope, workflow.active_step.key
        ),
        description: format!(
            "当前 step 推进模式为 `{step_advance}`（有 workflow_key 时为 auto，否则为 manual）。",
        ),
        source_summary: source_summary.to_vec(),
        source_refs: source_refs.to_vec(),
        payload: Some(serde_json::json!({
            "lifecycle_key": workflow.lifecycle.key,
            "step_key": workflow.active_step.key,
            "step_advance": step_advance,
            "workflow_key": workflow.active_step.workflow_key,
        })),
    }];

    for constraint in &workflow.effective_contract.constraints {
        policies.push(HookPolicyView {
            key: format!(
                "workflow:{}:{}:constraint:{}",
                scope, workflow.active_step.key, constraint.key
            ),
            description: constraint.description.clone(),
            source_summary: source_summary.to_vec(),
            source_refs: source_refs.to_vec(),
            payload: Some(serde_json::json!({
                "kind": constraint.kind,
                "payload": constraint.payload.clone(),
            })),
        });
    }

    if step_advance == "auto" && !workflow.effective_contract.completion.checks.is_empty() {
        policies.push(HookPolicyView {
            key: format!(
                "workflow:{}:{}:check_gate",
                scope, workflow.active_step.key
            ),
            description:
                "当前 step 会基于 contract checks 自动推进；在满足所有检查前，不应提前结束当前 loop。"
                    .to_string(),
            source_summary: source_summary.to_vec(),
            source_refs: source_refs.to_vec(),
            payload: Some(serde_json::json!({
                "check_count": workflow.effective_contract.completion.checks.len(),
                "step_key": workflow.active_step.key,
            })),
        });
    }

    policies
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    use crate::workflow::{ActiveWorkflowProjection, WorkflowBindingSummary};
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowBindingKind,
        WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource, WorkflowInjectionSpec,
        build_effective_contract,
    };
    use agentdash_spi::HookSourceLayer;

    fn workflow_projection_with_instructions(
        instructions: Vec<String>,
    ) -> ActiveWorkflowProjection {
        let contract = WorkflowContract {
            injection: WorkflowInjectionSpec {
                instructions,
                ..WorkflowInjectionSpec::default()
            },
            ..WorkflowContract::default()
        };
        let definition = WorkflowDefinition::new(
            "trellis_dev_task_implement",
            "Trellis Dev Workflow / Implement",
            "workflow desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow definition should build");
        let active_step = LifecycleStepDefinition {
            key: "implement".to_string(),
            description: "实现并记录结果".to_string(),
            workflow_key: Some(definition.key.clone()),
        };
        let lifecycle = LifecycleDefinition::new(
            "trellis_dev_task",
            "Trellis Dev Lifecycle",
            "lifecycle desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            "implement",
            vec![active_step.clone()],
        )
        .expect("lifecycle definition should build");
        let project_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let mut run = LifecycleRun::new(
            project_id,
            lifecycle.id,
            WorkflowBindingKind::Task,
            binding_id,
            &lifecycle.steps,
            &lifecycle.entry_step_key,
        )
        .expect("workflow run should build");
        run.activate_step("implement")
            .expect("implement step should activate");
        let effective_contract =
            build_effective_contract(&lifecycle.key, &active_step.key, Some(&definition));
        ActiveWorkflowProjection {
            run,
            lifecycle,
            active_step,
            primary_workflow: Some(definition),
            effective_contract,
            binding: WorkflowBindingSummary {
                binding_kind: WorkflowBindingKind::Task,
                binding_id,
                binding_label: Some("Task A".to_string()),
            },
        }
    }

    #[test]
    fn workflow_step_fragments_do_not_duplicate_constraints_fragment() {
        let workflow = workflow_projection_with_instructions(vec![
            "先补齐检查证据，再结束 session".to_string(),
        ]);
        let source_refs = vec![HookSourceRef {
            layer: HookSourceLayer::Workflow,
            key: "trellis_dev_task:implement".to_string(),
            label: "Workflow / Trellis Dev Workflow / implement".to_string(),
            priority: 300,
        }];
        let source_summary = super::super::source_summary_from_refs(&source_refs);

        let fragments = build_workflow_step_fragments(&workflow, &source_summary, &source_refs);

        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0].label, "active_workflow_step");
        assert_eq!(fragments[1].label, "active_workflow_instructions");
        assert!(
            !fragments
                .iter()
                .any(|fragment| fragment.label == "workflow_step_constraints")
        );
    }
}
