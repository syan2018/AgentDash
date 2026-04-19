use crate::vfs::ResolveBindingsOutput;
use crate::workflow::ActiveWorkflowProjection;
use agentdash_spi::{ContextFragment, MergeStrategy};

use super::contributor::{ContextContributor, Contribution, ContributorInput};

/// Workflow context_bindings 注入 Contributor
///
/// 负责把已解析的 workflow bindings 渲染为 task owner context 片段，
/// 并附带一段 workflow 绑定解析摘要。
pub struct WorkflowContextBindingsContributor {
    workflow: ActiveWorkflowProjection,
    resolved_bindings: ResolveBindingsOutput,
}

impl WorkflowContextBindingsContributor {
    pub fn new(
        workflow: ActiveWorkflowProjection,
        resolved_bindings: ResolveBindingsOutput,
    ) -> Self {
        Self {
            workflow,
            resolved_bindings,
        }
    }
}

impl ContextContributor for WorkflowContextBindingsContributor {
    fn contribute(&self, _input: &ContributorInput<'_>) -> Contribution {
        let mut fragments = vec![ContextFragment {
            slot: "workflow_context",
            label: "workflow_projection_snapshot",
            order: 83,
            strategy: MergeStrategy::Append,
            content: format!(
                "## Workflow Projection Snapshot\n- lifecycle: {} (`{}`)\n- step: `{}`\n- primary_workflow: {}\n- run_status: `{}`\n- binding_count: {}\n- resolved_binding_count: {}",
                self.workflow.lifecycle.name,
                self.workflow.lifecycle.key,
                self.workflow.active_step.key,
                self.workflow
                    .primary_workflow
                    .as_ref()
                    .map(|item| format!("{} (`{}`)", item.name, item.key))
                    .unwrap_or_else(|| "(none)".to_string()),
                enum_tag(&self.workflow.run.status),
                self.workflow
                    .effective_contract
                    .injection
                    .context_bindings
                    .len(),
                self.resolved_bindings.resolved.len()
            ),
        }];

        for (index, binding) in self.resolved_bindings.resolved.iter().enumerate() {
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
                slot: "workflow_context",
                label: "workflow_context_binding",
                order: 84 + index as i32,
                strategy: MergeStrategy::Append,
                content: format!(
                    "## {}\n- locator: `{}`\n- reason: {}\n\n{}",
                    heading, binding.locator, binding.reason, body
                ),
            });
        }

        if !self.resolved_bindings.warnings.is_empty() {
            fragments.push(ContextFragment {
                slot: "workflow_context",
                label: "workflow_context_warnings",
                order: 89,
                strategy: MergeStrategy::Append,
                content: format!(
                    "## Workflow Binding Warnings\n{}",
                    self.resolved_bindings
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
}

fn enum_tag<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .map(|raw| raw.trim_matches('"').to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::super::contributor::TaskExecutionPhase;
    use super::*;
    use crate::vfs::{ResolveBindingsOutput, ResolvedBinding};
    use agentdash_domain::workflow::{
        EffectiveSessionContract, LifecycleDefinition, LifecycleRun, LifecycleStepDefinition,
        WorkflowBindingKind, WorkflowDefinition, WorkflowDefinitionSource,
    };
    use uuid::Uuid;

    fn sample_workflow() -> ActiveWorkflowProjection {
        let definition = WorkflowDefinition::new(
            Uuid::new_v4(),
            "wf_impl",
            "Workflow Implement",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            Default::default(),
        )
        .expect("workflow definition");
        let step = LifecycleStepDefinition {
            key: "implement".to_string(),
            description: "执行实现工作".to_string(),
            workflow_key: Some(definition.key.clone()),
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
            capabilities: vec![],
        };
        let lifecycle = LifecycleDefinition::new(
            Uuid::new_v4(),
            "trellis_dev_task",
            "Trellis Dev Task",
            "desc",
            WorkflowBindingKind::Task,
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
            primary_workflow: Some(definition.clone()),
            effective_contract: EffectiveSessionContract {
                lifecycle_key: Some("trellis_dev_task".to_string()),
                active_step_key: Some("implement".to_string()),
                injection: agentdash_domain::workflow::WorkflowInjectionSpec {
                    context_bindings: vec![agentdash_domain::workflow::WorkflowContextBinding {
                        locator: ".trellis/workflow.md".to_string(),
                        reason: "workflow 总规则".to_string(),
                        required: true,
                        title: Some("Workflow 总规则".to_string()),
                    }],
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    #[test]
    fn workflow_context_bindings_contributor_renders_summary_and_bindings() {
        let contributor = WorkflowContextBindingsContributor::new(
            sample_workflow(),
            ResolveBindingsOutput {
                resolved: vec![ResolvedBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    title: Some("Workflow 总规则".to_string()),
                    reason: "workflow 总规则".to_string(),
                    content: "## Workflow\n- rule: read before write".to_string(),
                }],
                warnings: vec![],
            },
        );

        let contribution = contributor.contribute(&ContributorInput {
            task: &agentdash_domain::task::Task::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "Task".to_string(),
                "desc".to_string(),
            ),
            story: &agentdash_domain::story::Story::new(
                Uuid::new_v4(),
                "Story".to_string(),
                "desc".to_string(),
            ),
            project: &agentdash_domain::project::Project::new(
                "Project".to_string(),
                "desc".to_string(),
            ),
            workspace: None,
            phase: TaskExecutionPhase::Start,
            override_prompt: None,
            additional_prompt: None,
        });

        assert_eq!(contribution.context_fragments.len(), 2);
        assert!(
            contribution.context_fragments[0]
                .content
                .contains("resolved_binding_count: 1")
        );
        assert!(
            contribution.context_fragments[1]
                .content
                .contains("Workflow 总规则")
        );
    }
}
