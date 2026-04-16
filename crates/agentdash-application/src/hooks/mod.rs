use std::collections::BTreeSet;

use crate::workflow::ActiveWorkflowProjection;

mod completion;
mod helpers;
mod owner_resolver;
pub mod presets;
mod provider;
mod rules;
pub(crate) mod script_engine;
mod snapshot_helpers;
mod workflow_contribution;
mod workflow_snapshot;

pub use owner_resolver::SessionOwnerResolver;
pub use presets::{HookRulePreset, PresetSource, hook_rule_preset_registry};
pub use provider::AppExecutionHookProvider;
pub use workflow_snapshot::WorkflowSnapshotBuilder;

// Re-exports consumed by child modules (rules.rs, snapshot_helpers.rs, etc.)
// so that `super::xxx` references from those children remain valid.
use completion::ActiveWorkflowLocator;
use helpers::shell_exec_rewritten_args;

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

pub(super) fn global_builtin_source() -> &'static str {
    "builtin:global"
}

pub(super) fn workflow_source(workflow: &ActiveWorkflowProjection) -> String {
    let scope = workflow_scope_key(workflow);
    format!("workflow:{}:{}", scope, workflow.active_step.key)
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
