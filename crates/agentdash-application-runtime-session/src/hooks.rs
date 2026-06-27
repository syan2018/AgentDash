use agentdash_spi::{AgentFrameHookSnapshot, ContextFragment, HookInjection, MergeStrategy};

use crate::context::Contribution;

const HOOK_WORKFLOW: i32 = 83;
const HOOK_CONSTRAINT: i32 = 84;
const HOOK_DEFAULT: i32 = 200;

const HOOK_SLOT_ORDERS: &[(&str, i32)] =
    &[("workflow", HOOK_WORKFLOW), ("constraint", HOOK_CONSTRAINT)];

fn default_hook_order(slot: &str) -> i32 {
    HOOK_SLOT_ORDERS
        .iter()
        .find(|(name, _)| *name == slot)
        .map(|(_, order)| *order)
        .unwrap_or(HOOK_DEFAULT)
}

pub(crate) fn hook_injection_to_fragment(injection: HookInjection) -> ContextFragment {
    let order = default_hook_order(&injection.slot);
    ContextFragment {
        slot: injection.slot,
        label: injection.source.clone(),
        order,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: injection.source,
        content: injection.content,
    }
}

impl From<&AgentFrameHookSnapshot> for Contribution {
    fn from(snapshot: &AgentFrameHookSnapshot) -> Self {
        Contribution::fragments_only(
            snapshot
                .injections
                .iter()
                .cloned()
                .map(hook_injection_to_fragment)
                .collect(),
        )
    }
}
