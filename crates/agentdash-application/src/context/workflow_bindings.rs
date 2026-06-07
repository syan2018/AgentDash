use crate::vfs::ResolveBindingsOutput;
use crate::workflow::ActiveWorkflowProjection;
use agentdash_spi::{ContextFragment, MergeStrategy};

use super::Contribution;
use super::rendering::{render_resolved_binding_section, render_resolved_binding_warnings};

/// Workflow context_bindings 片段。
///
/// 把已解析的 workflow bindings 渲染为 task owner context 片段，并附带一段绑定摘要。
pub fn contribute_workflow_binding(
    workflow: &ActiveWorkflowProjection,
    resolved_bindings: &ResolveBindingsOutput,
) -> Contribution {
    let mut fragments = vec![ContextFragment {
        slot: "workflow_context".to_string(),
        label: "workflow_projection_snapshot".to_string(),
        order: 83,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:contributor:workflow_bindings".to_string(),
        content: format!(
            "## Workflow Projection Snapshot\n- lifecycle: {} (`{}`)\n- step: `{}`\n- primary_workflow: {}\n- run_status: `{}`\n- binding_count: {}\n- resolved_binding_count: {}",
            workflow.lifecycle_name,
            workflow.lifecycle_key,
            workflow.active_activity.key,
            workflow
                .primary_workflow
                .as_ref()
                .map(|item| format!("{} (`{}`)", item.name, item.key))
                .unwrap_or_else(|| "(none)".to_string()),
            enum_tag(&workflow.run.status),
            workflow
                .active_contract()
                .map(|c| c.injection.context_bindings.len())
                .unwrap_or(0),
            resolved_bindings.resolved.len()
        ),
    }];

    for (index, binding) in resolved_bindings.resolved.iter().enumerate() {
        let Some(section) = render_resolved_binding_section(binding) else {
            continue;
        };
        fragments.push(ContextFragment {
            slot: "workflow_context".to_string(),
            label: "workflow_context_binding".to_string(),
            order: 84 + index as i32,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:workflow_bindings".to_string(),
            content: section,
        });
    }

    if let Some(warning_section) = render_resolved_binding_warnings(resolved_bindings) {
        fragments.push(ContextFragment {
            slot: "workflow_context".to_string(),
            label: "workflow_context_warnings".to_string(),
            order: 89,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:workflow_bindings".to_string(),
            content: warning_section,
        });
    }

    Contribution::fragments_only(fragments)
}

fn enum_tag<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .map(|raw| raw.trim_matches('"').to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::{ResolveBindingsOutput, ResolvedBinding};
    use crate::workflow::activity_projection;

    fn sample_workflow() -> ActiveWorkflowProjection {
        activity_projection(None)
    }

    #[test]
    fn contribute_workflow_binding_renders_summary_and_bindings() {
        let contribution = contribute_workflow_binding(
            &sample_workflow(),
            &ResolveBindingsOutput {
                resolved: vec![ResolvedBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    title: Some("Workflow 总规则".to_string()),
                    reason: "workflow 总规则".to_string(),
                    content: "## Workflow\n- rule: read before write".to_string(),
                }],
                warnings: vec![],
            },
        );

        assert_eq!(contribution.fragments.len(), 2);
        assert!(
            contribution.fragments[0]
                .content
                .contains("resolved_binding_count: 1")
        );
        assert!(
            contribution.fragments[1]
                .content
                .contains("Workflow 总规则")
        );
    }
}
