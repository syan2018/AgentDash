use agentdash_spi::context_usage_kind;
use agentdash_spi::{ASSIGNMENT_CONTEXT_SLOTS, ContextFragment, HookInjection};

pub(crate) fn context_fragment_usage_kind(fragment: &ContextFragment) -> Option<String> {
    usage_kind_for_slot(&fragment.slot).map(str::to_string)
}

pub(crate) fn hook_injection_usage_kind(injection: &HookInjection) -> Option<String> {
    usage_kind_for_slot(&injection.slot).map(str::to_string)
}

fn usage_kind_for_slot(slot: &str) -> Option<&'static str> {
    if ASSIGNMENT_CONTEXT_SLOTS.contains(&slot) {
        return Some(context_usage_kind::SYSTEM_DEVELOPER);
    }
    None
}
