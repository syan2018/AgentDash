use agentdash_platform_spi::HookInjection;

use agentdash_application_ports::lifecycle_surface_projection::ActiveWorkflowProjection;

pub(super) fn build_step_summary_markdown(workflow: &ActiveWorkflowProjection) -> String {
    let wf_line = match workflow.primary_workflow.as_ref() {
        Some(w) => format!("- workflow: {} (`{}`)", w.name, w.key),
        None => "- workflow: (none)".to_string(),
    };
    format!(
        "## Active Workflow Step\n- lifecycle: {} (`{}`)\n- step: `{}`\n{}\n- advance: `{}`\n- status: `{}`\n\n{}",
        workflow.lifecycle_name,
        workflow.lifecycle_key,
        workflow.active_activity.key,
        wf_line,
        workflow.advance_label(),
        super::snapshot_helpers::workflow_run_status_tag(workflow.run.status),
        workflow.active_activity.description
    )
}

pub(super) fn build_active_workflow_step_fragments(
    workflow: &ActiveWorkflowProjection,
    source: &str,
) -> Vec<HookInjection> {
    let mut injections = vec![HookInjection {
        slot: "workflow".to_string(),
        content: build_step_summary_markdown(workflow),
        source: source.to_string(),
    }];

    let guidance = workflow
        .active_contract()
        .and_then(|c| c.injection.guidance.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(guidance) = guidance {
        injections.push(HookInjection {
            slot: "workflow".to_string(),
            content: build_guidance_injection_markdown(guidance),
            source: source.to_string(),
        });
    }

    injections
}

fn build_guidance_injection_markdown(guidance: &str) -> String {
    format!("## Workflow Guidance\n{guidance}")
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentdash_application_ports::lifecycle_surface_projection::ActiveWorkflowProjection;
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentProcedure,
        AgentProcedureContract, DefinitionSource, LifecycleNodeType, LifecycleRun,
        RuntimeNodeState, RuntimeNodeStatus, WorkflowInjectionSpec,
    };

    fn workflow_projection_with_guidance(guidance: Option<String>) -> ActiveWorkflowProjection {
        let project_id = uuid::Uuid::new_v4();
        let contract = AgentProcedureContract {
            injection: WorkflowInjectionSpec {
                guidance,
                ..WorkflowInjectionSpec::default()
            },
            ..AgentProcedureContract::default()
        };
        let procedure = AgentProcedure::new(
            uuid::Uuid::new_v4(),
            "trellis_dev_task_implement",
            "Trellis Dev Workflow / Implement",
            "workflow desc",
            DefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow definition");
        ActiveWorkflowProjection {
            run: LifecycleRun::new_control(project_id),
            orchestration_id: uuid::Uuid::new_v4(),
            node_path: "implement".to_string(),
            lifecycle_graph_id: None,
            lifecycle_key: "trellis_dev_task".to_string(),
            lifecycle_name: "Trellis Dev Lifecycle".to_string(),
            active_activity: ActivityDefinition {
                key: "implement".to_string(),
                description: "实现并记录结果".to_string(),
                executor: ActivityExecutorSpec::Agent(
                    AgentActivityExecutorSpec::create_activity_agent(procedure.key.clone()),
                ),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: Default::default(),
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            },
            active_attempt: RuntimeNodeState {
                node_id: "implement".to_string(),
                node_path: "implement".to_string(),
                kind: agentdash_domain::workflow::PlanNodeKind::AgentCall,
                status: RuntimeNodeStatus::Running,
                attempt: 1,
                inputs: Vec::new(),
                outputs: Vec::new(),
                executor_run_ref: None,
                children: Vec::new(),
                phase_path: Vec::new(),
                started_at: None,
                completed_at: None,
                error: None,
                trace_refs: Vec::new(),
                cache: None,
            },
            active_node_type: LifecycleNodeType::AgentNode,
            active_procedure_key: Some(procedure.key.clone()),
            snapshot_contract: None,
            primary_workflow: Some(procedure),
        }
    }

    #[test]
    fn workflow_step_fragments_do_not_duplicate_constraints_fragment() {
        let workflow =
            workflow_projection_with_guidance(Some("先补齐检查证据，再结束 session".to_string()));
        let source = super::super::workflow_source(&workflow);

        let injections = build_active_workflow_step_fragments(&workflow, &source);

        assert_eq!(injections.len(), 2);
        assert_eq!(injections[0].slot, "workflow");
        assert!(injections[0].content.contains("Active Workflow Step"));
        assert_eq!(injections[1].slot, "workflow");
        assert!(injections[1].content.contains("Workflow Guidance"));
    }
}
