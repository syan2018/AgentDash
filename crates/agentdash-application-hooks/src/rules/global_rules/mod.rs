use super::NormalizedHookRule;

mod shell_exec_cwd_rewrite;
mod supervised_tool_approval;

static GLOBAL_RULES: &[NormalizedHookRule] = &[
    shell_exec_cwd_rewrite::REGISTRY_ITEM,
    supervised_tool_approval::REGISTRY_ITEM,
];

pub(super) fn registry_items() -> &'static [NormalizedHookRule] {
    GLOBAL_RULES
}
