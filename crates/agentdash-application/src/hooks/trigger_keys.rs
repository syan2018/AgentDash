use agentdash_spi::HookTrigger;

pub(crate) fn hook_trigger_key(trigger: &HookTrigger) -> &'static str {
    match trigger {
        HookTrigger::SessionStart => "session_start",
        HookTrigger::UserPromptSubmit => "user_prompt_submit",
        HookTrigger::BeforeTool => "before_tool",
        HookTrigger::AfterTool => "after_tool",
        HookTrigger::AfterTurn => "after_turn",
        HookTrigger::BeforeStop => "before_stop",
        HookTrigger::SessionTerminal => "session_terminal",
        HookTrigger::BeforeSubagentDispatch => "before_subagent_dispatch",
        HookTrigger::AfterSubagentDispatch => "after_subagent_dispatch",
        HookTrigger::SubagentResult => "subagent_result",
        HookTrigger::BeforeCompact => "before_compact",
        HookTrigger::AfterCompact => "after_compact",
        HookTrigger::BeforeProviderRequest => "before_provider_request",
        HookTrigger::CapabilityChanged => "capability_changed",
    }
}
