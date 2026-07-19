use super::NormalizedHookRule;

mod shell_exec_cwd_rewrite;

static GLOBAL_RULES: &[NormalizedHookRule] = &[shell_exec_cwd_rewrite::REGISTRY_ITEM];

pub(super) fn registry_items() -> &'static [NormalizedHookRule] {
    GLOBAL_RULES
}
