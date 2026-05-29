use agentdash_spi::HookInjection;

use crate::workflow::ActiveWorkflowProjection;

pub(super) fn build_step_summary_markdown(workflow: &ActiveWorkflowProjection) -> String {
    let wf_line = match workflow.primary_workflow.as_ref() {
        Some(w) => format!("- workflow: {} (`{}`)", w.name, w.key),
        None => "- workflow: (none)".to_string(),
    };
    format!(
        "## Active Workflow Step\n- lifecycle: {} (`{}`)\n- step: `{}`\n{}\n- advance: `{}`\n- status: `{}`\n\n{}",
        workflow.lifecycle.name,
        workflow.lifecycle.key,
        workflow.active_activity.key,
        wf_line,
        workflow.advance_label(),
        super::snapshot_helpers::workflow_run_status_tag(workflow.run.status),
        workflow.active_activity.description
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

    use crate::workflow::{ActiveWorkflowProjection, activity_projection};

    fn workflow_projection_with_guidance(guidance: Option<String>) -> ActiveWorkflowProjection {
        activity_projection(guidance)
    }

    #[test]
    fn workflow_step_fragments_do_not_duplicate_constraints_fragment() {
        let workflow =
            workflow_projection_with_guidance(Some("先补齐检查证据，再结束 session".to_string()));
        let source = super::super::workflow_source(&workflow);

        let injections = build_workflow_step_fragments(&workflow, &source);

        assert_eq!(injections.len(), 2);
        assert_eq!(injections[0].slot, "workflow");
        assert!(injections[0].content.contains("Active Workflow Step"));
        assert_eq!(injections[1].slot, "workflow");
        assert!(injections[1].content.contains("Workflow Guidance"));
    }
}
