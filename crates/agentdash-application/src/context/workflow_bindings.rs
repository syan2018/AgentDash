use crate::vfs::ResolveBindingsOutput;
use crate::workflow::ActiveWorkflowProjection;
use agentdash_spi::{ContextFragment, MergeStrategy};

use super::Contribution;

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
            workflow.lifecycle.name,
            workflow.lifecycle.key,
            workflow.active_step.key,
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
        let heading = binding
            .title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                let reason = binding.reason.trim();
                if reason.is_empty() {
                    binding.locator.as_str()
                } else {
                    reason
                }
            });
        let body = binding.content.trim();
        if body.is_empty() {
            continue;
        }
        fragments.push(ContextFragment {
            slot: "workflow_context".to_string(),
            label: "workflow_context_binding".to_string(),
            order: 84 + index as i32,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:workflow_bindings".to_string(),
            content: format!(
                "## {}\n- locator: `{}`\n- reason: {}\n\n{}",
                heading, binding.locator, binding.reason, body
            ),
        });
    }

    if !resolved_bindings.warnings.is_empty() {
        fragments.push(ContextFragment {
            slot: "workflow_context".to_string(),
            label: "workflow_context_warnings".to_string(),
            order: 89,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:workflow_bindings".to_string(),
            content: format!(
                "## Workflow Binding Warnings\n{}",
                resolved_bindings
                    .warnings
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
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
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowBindingKind,
        WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource, WorkflowInjectionSpec,
    };
    use uuid::Uuid;

    fn sample_workflow() -> ActiveWorkflowProjection {
        let contract = WorkflowContract {
            injection: WorkflowInjectionSpec {
                context_bindings: vec![agentdash_domain::workflow::WorkflowContextBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow 总规则".to_string(),
                    required: true,
                    title: Some("Workflow 总规则".to_string()),
                }],
                ..Default::default()
            },
            ..WorkflowContract::default()
        };
        let definition = WorkflowDefinition::new(
            Uuid::new_v4(),
            "wf_impl",
            "Workflow Implement",
            "desc",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow definition");
        let step = LifecycleStepDefinition {
            key: "implement".to_string(),
            description: "执行实现工作".to_string(),
            workflow_key: Some(definition.key.clone()),
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
        };
        let lifecycle = LifecycleDefinition::new(
            Uuid::new_v4(),
            "trellis_dev_task",
            "Trellis Dev Task",
            "desc",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "implement",
            vec![step.clone()],
            vec![],
        )
        .expect("lifecycle definition");
        let run = LifecycleRun::new(
            Uuid::new_v4(),
            lifecycle.id,
            "sess-test-bindings",
            &lifecycle.steps,
            &lifecycle.entry_step_key,
            &lifecycle.edges,
        )
        .expect("run");
        ActiveWorkflowProjection {
            run,
            lifecycle,
            active_step: step,
            primary_workflow: Some(definition),
        }
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
