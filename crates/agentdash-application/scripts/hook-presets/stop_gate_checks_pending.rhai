// stop_gate_checks_pending — 在 completion checks 全部满足前阻止 Agent 结束 session

if !ctx.signals.auto_completion {
    return #{};
}
if !ctx.signals.has_block_stop_constraint {
    return #{};
}
if ctx.signals.completion_satisfied {
    return #{};
}

let src = ctx.workflow.source;
let evidence = ctx.signals.checklist_evidence_present;

#{
    inject: [
        make_injection("workflow",
            "## Session Stop Gate\n- 当前 workflow step 通过 contract checks 自动推进。\n- 结束前请先补齐验证结论、剩余风险与必要证据，让 checks 真正满足。\n- 如果 contract 依赖 checklist evidence 或其它显式信号，请先补齐对应条件。\n- 只要 checks 尚未满足，就不要直接结束本轮 session。",
            src),
        make_injection("constraint",
            "当前 step 的 contract checks 还未满足；请先补齐验证结论、必要证据与状态信号，再结束 session。",
            src)
    ],
    diagnostics: [
        make_diagnostic(
            "before_stop_workflow_checks_pending",
            "当前 workflow step 尚未满足 contract checks，Hook 要求继续 loop (checklist_evidence_present=" + evidence + ")"
        )
    ]
}
