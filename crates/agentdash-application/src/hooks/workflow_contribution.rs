use agentdash_spi::HookInjection;

use crate::workflow::ActiveWorkflowProjection;

use super::lifecycle_step_advance_label;

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
    source: &str,
) -> Vec<HookInjection> {
    let mut injections = vec![HookInjection {
        slot: "workflow".to_string(),
        content: build_step_summary_markdown(workflow),
        source: source.to_string(),
    }];

    let instructions = workflow
        .active_contract()
        .map(|c| c.injection.instructions.as_slice())
        .unwrap_or(&[]);
    if !instructions.is_empty() {
        injections.push(HookInjection {
            slot: "workflow".to_string(),
            content: build_instruction_injection_markdown(instructions),
            source: source.to_string(),
        });
    }

    injections
}

fn build_instruction_injection_markdown(instructions: &[String]) -> String {
    let body = instructions
        .iter()
        .map(|instruction| format!("- {instruction}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("## Workflow Instructions\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    use crate::workflow::ActiveWorkflowProjection;
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowBindingKind,
        WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource, WorkflowInjectionSpec,
    };

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
            Uuid::new_v4(),
            "trellis_dev_task_implement",
            "Trellis Dev Workflow / Implement",
            "workflow desc",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow definition should build");
        let active_step = LifecycleStepDefinition {
            key: "implement".to_string(),
            description: "实现并记录结果".to_string(),
            workflow_key: Some(definition.key.clone()),
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
            task_id: None,
        };
        let project_id = Uuid::new_v4();
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "trellis_dev_task",
            "Trellis Dev Lifecycle",
            "lifecycle desc",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "implement",
            vec![active_step.clone()],
            vec![],
        )
        .expect("lifecycle definition should build");
        let mut run = LifecycleRun::new(
            project_id,
            lifecycle.id,
            "sess-test-contribution",
            &lifecycle.steps,
            &lifecycle.entry_step_key,
            &lifecycle.edges,
        )
        .expect("workflow run should build");
        run.activate_step("implement")
            .expect("implement step should activate");
        ActiveWorkflowProjection {
            run,
            lifecycle,
            active_step,
            primary_workflow: Some(definition),
        }
    }

    #[test]
    fn workflow_step_fragments_do_not_duplicate_constraints_fragment() {
        let workflow = workflow_projection_with_instructions(vec![
            "先补齐检查证据，再结束 session".to_string(),
        ]);
        let source = super::super::workflow_source(&workflow);

        let injections = build_workflow_step_fragments(&workflow, &source);

        assert_eq!(injections.len(), 2);
        assert_eq!(injections[0].slot, "workflow");
        assert!(injections[0].content.contains("Active Workflow Step"));
        assert_eq!(injections[1].slot, "workflow");
        assert!(injections[1].content.contains("Workflow Instructions"));
    }
}
