use std::collections::BTreeSet;

use agentdash_spi::{
    HookContributionSet, HookPolicyView, HookSourceLayer, HookSourceRef, SessionHookSnapshot,
};

use crate::workflow::ActiveWorkflowProjection;

mod completion;
mod helpers;
mod owner_resolver;
mod provider;
mod rules;
mod snapshot_helpers;
mod workflow_contribution;
mod workflow_snapshot;

pub use owner_resolver::SessionOwnerResolver;
pub use provider::AppExecutionHookProvider;
pub use workflow_snapshot::WorkflowSnapshotBuilder;

// Re-exports consumed by child modules (rules.rs, snapshot_helpers.rs, etc.)
// so that `super::xxx` references from those children remain valid.
use completion::ActiveWorkflowLocator;
use helpers::*;

fn workflow_scope_key(workflow: &ActiveWorkflowProjection) -> String {
    workflow
        .primary_workflow
        .as_ref()
        .map(|w| w.key.clone())
        .unwrap_or_else(|| workflow.lifecycle.key.clone())
}

fn lifecycle_step_advance_label(
    step: &agentdash_domain::workflow::LifecycleStepDefinition,
) -> &'static str {
    match step
        .workflow_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(_) => "auto",
        None => "manual",
    }
}

pub(super) fn global_builtin_sources() -> Vec<HookSourceRef> {
    vec![
        HookSourceRef {
            layer: HookSourceLayer::GlobalBuiltin,
            key: "runtime_trace_observability".to_string(),
            label: "Global Builtin / Runtime Trace".to_string(),
            priority: 100,
        },
        HookSourceRef {
            layer: HookSourceLayer::GlobalBuiltin,
            key: "workspace_path_safety".to_string(),
            label: "Global Builtin / Workspace Path Safety".to_string(),
            priority: 100,
        },
    ]
}

fn session_source_ref(session_id: &str) -> HookSourceRef {
    HookSourceRef {
        layer: HookSourceLayer::Session,
        key: session_id.to_string(),
        label: format!("Session / {session_id}"),
        priority: 500,
    }
}

fn workflow_source_refs(workflow: &ActiveWorkflowProjection) -> Vec<HookSourceRef> {
    let scope = workflow_scope_key(workflow);
    let label_name = workflow
        .primary_workflow
        .as_ref()
        .map(|w| w.name.as_str())
        .unwrap_or(workflow.lifecycle.name.as_str());
    vec![HookSourceRef {
        layer: HookSourceLayer::Workflow,
        key: format!("{}:{}", scope, workflow.active_step.key),
        label: format!("Workflow / {} / {}", label_name, workflow.active_step.key),
        priority: 300,
    }]
}

fn source_layer_tag(layer: HookSourceLayer) -> &'static str {
    match layer {
        HookSourceLayer::GlobalBuiltin => "global_builtin",
        HookSourceLayer::Workflow => "workflow",
        HookSourceLayer::Project => "project",
        HookSourceLayer::Story => "story",
        HookSourceLayer::Task => "task",
        HookSourceLayer::Session => "session",
    }
}

fn source_summary_from_refs(source_refs: &[HookSourceRef]) -> Vec<String> {
    source_refs
        .iter()
        .map(|source| format!("{}:{}", source_layer_tag(source.layer), source.key))
        .collect()
}

fn merge_hook_contribution(snapshot: &mut SessionHookSnapshot, contribution: HookContributionSet) {
    snapshot.sources.extend(contribution.sources);
    snapshot.tags.extend(contribution.tags);
    snapshot
        .context_fragments
        .extend(contribution.context_fragments);
    snapshot.constraints.extend(contribution.constraints);
    snapshot.policies.extend(contribution.policies);
    snapshot.diagnostics.extend(contribution.diagnostics);
    snapshot.sources = dedupe_source_refs(snapshot.sources.clone());
}

fn dedupe_source_refs(sources: Vec<HookSourceRef>) -> Vec<HookSourceRef> {
    let mut seen = BTreeSet::new();
    sources
        .into_iter()
        .filter(|source| {
            seen.insert((
                source_layer_tag(source.layer).to_string(),
                source.key.clone(),
            ))
        })
        .collect()
}

fn global_builtin_hook_contribution() -> HookContributionSet {
    let source_refs = global_builtin_sources();
    let source_summary = source_summary_from_refs(&source_refs);
    HookContributionSet {
        sources: source_refs.clone(),
        tags: vec![
            "hook_source:global_builtin".to_string(),
            "hook_builtin:runtime_trace".to_string(),
            "hook_builtin:workspace_path_safety".to_string(),
            "hook_builtin:supervised_tool_approval".to_string(),
        ],
        policies: vec![
            HookPolicyView {
                key: "global_builtin:runtime_trace_observable".to_string(),
                description:
                    "当前 session 的 hook 决策会被记录进 runtime trace / diagnostics 调试面。"
                        .to_string(),
                source_summary: source_summary.clone(),
                source_refs: source_refs.clone(),
                payload: None,
            },
            HookPolicyView {
                key: "global_builtin:workspace_path_safety".to_string(),
                description:
                    "shell_exec 在命中工作区内绝对 cwd 时，可由全局 builtin hook 自动 rewrite 为相对路径。"
                        .to_string(),
                source_summary,
                source_refs: source_refs.clone(),
                payload: None,
            },
            HookPolicyView {
                key: "global_builtin:supervised_tool_approval".to_string(),
                description:
                    "当当前会话 permission_policy=SUPERVISED 时，编辑/执行类工具会在运行前进入人工审批。"
                        .to_string(),
                source_summary: source_summary_from_refs(&source_refs),
                source_refs,
                payload: Some(serde_json::json!({
                    "permission_policy": "SUPERVISED",
                    "approval_tool_classes": ["execute", "edit", "delete", "move"],
                })),
            },
        ],
        ..HookContributionSet::default()
    }
}

fn dedupe_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.into_iter()
        .filter(|tag| seen.insert(tag.clone()))
        .collect()
}

fn map_hook_error(error: agentdash_domain::DomainError) -> agentdash_spi::HookError {
    agentdash_spi::HookError::Runtime(error.to_string())
}

#[cfg(test)]
mod test_fixtures;
