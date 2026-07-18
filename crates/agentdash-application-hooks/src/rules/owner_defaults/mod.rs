use agentdash_domain::workflow::WorkflowHookRuleSpec;
use agentdash_platform_spi::AgentFrameHookSnapshot;

pub(super) fn owner_default_hook_rules(
    snapshot: &AgentFrameHookSnapshot,
) -> Vec<WorkflowHookRuleSpec> {
    let _ = snapshot;
    Vec::new()
}
