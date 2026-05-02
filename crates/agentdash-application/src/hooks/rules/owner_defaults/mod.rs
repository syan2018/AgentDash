use agentdash_domain::workflow::WorkflowHookRuleSpec;
use agentdash_spi::SessionHookSnapshot;

mod task_owner_session_terminal;

type OwnerDefaultRuleBuilder = fn(&SessionHookSnapshot) -> Option<WorkflowHookRuleSpec>;

static OWNER_DEFAULT_RULE_BUILDERS: &[OwnerDefaultRuleBuilder] =
    &[task_owner_session_terminal::REGISTRY_ITEM];

pub(super) fn owner_default_hook_rules(
    snapshot: &SessionHookSnapshot,
) -> Vec<WorkflowHookRuleSpec> {
    OWNER_DEFAULT_RULE_BUILDERS
        .iter()
        .filter_map(|build| build(snapshot))
        .collect()
}
