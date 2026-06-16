use std::collections::BTreeSet;

use crate::lifecycle::ActiveWorkflowProjection;

mod fragment_bridge;
mod helpers;
mod owner_resolver;
pub mod presets;
mod provider;
mod rules;
pub(crate) mod script_engine;
mod snapshot_helpers;
mod active_workflow_contribution;
mod active_workflow_snapshot;

pub use active_workflow_snapshot::ActiveWorkflowSnapshotBuilder;
pub(crate) use fragment_bridge::hook_injection_to_fragment;
pub use owner_resolver::SessionOwnerResolver;
pub use presets::{HookRulePreset, PresetSource, hook_rule_preset_registry};
pub use provider::{AppExecutionHookProvider, AppExecutionHookProviderRepos};

// Re-exports consumed by child modules (rules.rs, snapshot_helpers.rs, etc.)
// so that `super::xxx` references from those children remain valid.
use helpers::shell_exec_rewritten_args;

fn workflow_scope_key(workflow: &ActiveWorkflowProjection) -> String {
    workflow
        .primary_workflow
        .as_ref()
        .map(|w| w.key.clone())
        .unwrap_or_else(|| workflow.lifecycle_key.clone())
}

pub(super) fn global_builtin_source() -> &'static str {
    "builtin:global"
}

pub(super) fn workflow_source(workflow: &ActiveWorkflowProjection) -> String {
    let scope = workflow_scope_key(workflow);
    format!("workflow:{}:{}", scope, workflow.active_activity.key)
}

fn dedupe_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.into_iter()
        .filter(|tag| seen.insert(tag.clone()))
        .collect()
}

#[cfg(test)]
mod test_fixtures;
