// manual_step_notice — 通知 Agent 当前 step 使用手动推进策略

if ctx.workflow.transition_policy != "manual" {
    return #{};
}

#{
    completion: #{
        mode: "manual",
        satisfied: false,
        reason: "manual step 需要显式推进"
    },
    diagnostics: [
        make_diagnostic(
            "before_stop_manual_phase",
            "当前 workflow step 使用 manual transition，不会由 Hook 自动推进 step"
        )
    ]
}
